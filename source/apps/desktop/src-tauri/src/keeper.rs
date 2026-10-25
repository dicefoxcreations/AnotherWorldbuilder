//! The keeper of a group's worlds: who comes in, who may ask to, and what
//! each one gets — the same rules wherever it runs. On a server (see
//! `server`) it holds no world's keys; on the GM's own device (`here`) it
//! answers for the GM's worlds and holds theirs.

use super::*;
use crate::store::JoinRequest;
use aw_sync::world::{Ask, BackupPlan, HeldWorld, Host, Offer, Order, Person, Reply, ServerBackups, ServerState};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

mod transactions;

/// Someone seen this long ago is noted again on disk.
const NOTE_AGAIN: u64 = 10 * 60;

pub(crate) fn is_id(id: &str) -> bool {
    id.strip_prefix("id:").is_some_and(|body| body.len() == 16 && body.chars().all(|c| c.is_ascii_hexdigit()))
}

/// What the server keeps of whom: who may bring worlds, which are closed,
/// who came and who is turned away.
#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Book {
    /// Besides the admin.
    gms: Vec<String>,
    /// GMs who would bring a world.
    asking: Vec<String>,
    /// By world: its name, for when it is gone.
    closed: BTreeMap<String, String>,
    blocked: Vec<String>,
    /// By id: name, and when last seen.
    users: BTreeMap<String, (String, u64)>,
    /// By world: how it is backed up, where its GM said otherwise than daily, 7 kept.
    #[serde(default)]
    backups: BTreeMap<String, BackupPlan>,
}

/// Unless its GM says otherwise, a world here is backed up daily and the last
/// seven stay.
fn default_plan() -> BackupPlan {
    BackupPlan { every: "daily".into(), keep: 7 }
}

pub(crate) struct Keeper {
    store: Store,
    /// In the app: the GM's own device, answering for its worlds as a server
    /// does — under the same rules. It holds their keys: whom the GM let in
    /// ahead of time it lets in at once. It keeps no one else's world.
    own: bool,
    admin: String,
    /// Where the backups of the worlds lie: `backups/` in the data folder.
    backups: PathBuf,
    me: IdentityPublic,
    book: Mutex<Book>,
    path: PathBuf,
    /// Worlds being written right now: asked about meanwhile, the server
    /// says it is busy instead of waiting — and lists them as last known.
    writing: Mutex<HashSet<String>>,
    known: Mutex<HashMap<String, Known>>,
}

/// What the server last read of a world: its GM, and whether encrypted.
#[derive(Clone, Default)]
struct Known {
    gm: String,
    gm_id: String,
    encrypted: bool,
    at: Option<std::time::Instant>,
}

/// How long what is known of a world holds before it is read again.
const KNOWN_FOR: std::time::Duration = std::time::Duration::from_secs(60);

/// A world being written, until it is done — whatever the outcome.
struct Writing<'a> {
    keeper: &'a Keeper,
    world: String,
}

impl Drop for Writing<'_> {
    fn drop(&mut self) {
        if let Ok(mut writing) = self.keeper.writing.lock() {
            writing.remove(&self.world);
        }
    }
}

/// Whether an ask brings records or image parts along.
/// Requests to join one world may wait at once: more are not taken.
const MAX_REQUESTS: usize = 50;

/// How long a write waits for the one before it.
const WRITE_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

fn writes(ask: &Ask) -> bool {
    match ask {
        Ask::Swap { give, .. } => !give.is_empty(),
        Ask::Give { records } => !records.is_empty(),
        Ask::Parts { give, .. } => !give.is_empty(),
        Ask::Commit { .. } | Ask::Abort { .. } | Ask::Release { .. } => true,
        _ => false,
    }
}

impl Keeper {
    pub(crate) fn open(store: Store, data: &Path, admin: String) -> Answer<Self> {
        let path = data.join("server.json");
        let book = match std::fs::read(&path) {
            Ok(raw) => serde_json::from_slice(&raw)?,
            Err(_) => Book::default(),
        };
        let me = store.load_identity()?.public();
        let keeper = Keeper { store, own: false, admin, backups: data.join("backups"), me, book: Mutex::new(book), path, writing: Mutex::default(), known: Mutex::default() };
        keeper.recover_online_commands()?;
        Ok(keeper)
    }

    fn book(&self) -> Answer<std::sync::MutexGuard<'_, Book>> {
        self.book.lock().map_err(AppError::from)
    }

    fn save(&self, book: &Book) -> Answer<()> {
        aw_store::durable::write(&self.path, &serde_json::to_vec_pretty(book)?)?;
        Ok(())
    }

    /// Notes who came — on disk when new, or not seen for a while.
    fn note(&self, who: &IdentityPublic) -> Answer<()> {
        let mut book = self.book()?;
        let now = network::now();
        let id = who.id.to_string();
        let again = book.users.get(&id).is_none_or(|(name, last)| *name != who.name || now.saturating_sub(*last) > NOTE_AGAIN);
        book.users.insert(id, (who.name.clone(), now));
        if again {
            self.save(&book)?;
        }
        Ok(())
    }

    /// Asking about a world: `None` while it is being written; otherwise
    /// the claim to write it, if the ask writes.
    fn claim(&self, world: &str, writes: bool) -> Answer<Option<Option<Writing<'_>>>> {
        let mut writing = self.writing.lock().map_err(AppError::from)?;
        if writing.contains(world) {
            return Ok(None);
        }
        if !writes {
            return Ok(Some(None));
        }
        writing.insert(world.to_string());
        Ok(Some(Some(Writing { keeper: self, world: world.to_string() })))
    }

    /// A write's turn: waits a moment for the write before it, then says
    /// busy — the app asks again.
    fn claim_soon(&self, world: &str) -> Answer<Option<Option<Writing<'_>>>> {
        let until = std::time::Instant::now() + WRITE_WAIT;
        loop {
            if let Some(writing) = self.claim(world, true)? {
                return Ok(Some(writing));
            }
            if std::time::Instant::now() >= until {
                return Ok(None);
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// GM and form of a world — as last known while it is being written,
    /// and read again now and then.
    fn known(&self, world: &str) -> Answer<Known> {
        let last = self.known.lock().map_err(AppError::from)?.get(world).cloned();
        let fresh = last.as_ref().and_then(|k| k.at).is_some_and(|at| at.elapsed() < KNOWN_FOR);
        if fresh || self.writing.lock().map_err(AppError::from)?.contains(world) {
            return Ok(last.unwrap_or_default());
        }
        let gm = self.store.with_journal(world, |j| Ok(j.world_item().map(|(.., gm)| gm.clone())))?;
        let known = Known {
            gm: gm.as_ref().map(|g| g.name.clone()).unwrap_or_default(),
            gm_id: gm.map(|g| g.id.to_string()).unwrap_or_default(),
            encrypted: self.store.is_encrypted(world)?,
            at: Some(std::time::Instant::now()),
        };
        self.known.lock().map_err(AppError::from)?.insert(world.to_string(), known.clone());
        Ok(known)
    }

    fn may_bring(&self, id: &str) -> Answer<bool> {
        Ok(!self.own && (id == self.admin || self.book()?.gms.iter().any(|g| g == id)))
    }

    /// Whether anyone may learn of a world and ask to join it: one its GM put
    /// on the list — on the GM's own device, one they offer on the network.
    fn listed(&self, world: &str) -> bool {
        if self.own {
            return network::offered(&self.store, world);
        }
        self.store.on_the_list(world).unwrap_or(false)
    }

    /// The worlds held and open: name, icon, GM.
    /// The worlds named to whoever asks — `id`: also those they are GM or a
    /// member of, all of them for the admin; none: only those on the list.
    fn offers(&self, id: Option<&str>) -> Answer<Vec<Offer>> {
        let closed: Vec<String> = self.book()?.closed.keys().cloned().collect();
        let mut out = Vec::new();
        for meta in self.store.list_worlds()? {
            let world = meta.public.world_id.to_string();
            if closed.contains(&world) || (self.own && !network::answered_here(&self.store, &world)) {
                continue;
            }
            let theirs = id.is_some_and(|id| {
                id == self.admin
                    || self.store.gm_of(&world).ok().flatten().as_deref() == Some(id)
                    || self.store.has_grant(&world, id).unwrap_or(false)
                    || self.store.expects(&world, id).unwrap_or(false)
            });
            if !theirs && !self.listed(&world) {
                continue;
            }
            let gm = self.known(&world)?.gm;
            out.push(Offer { world, name: meta.public.name.clone(), icon: meta.icon.clone(), gm });
        }
        Ok(out)
    }

    fn reply(&self, world: &str, who: &IdentityPublic, ask: Ask) -> Answer<Reply> {
        let id = who.id.to_string();
        self.note(who)?;
        if self.book()?.blocked.contains(&id) {
            return Ok(Reply::NotOffered);
        }
        if self.own {
            // The GM's own worlds alone — none on its way to another folder,
            // nothing brought, no one managing.
            match ask {
                Ask::Worlds => {}
                Ask::Manage { .. } | Ask::Backups { .. } | Ask::HostWorlds => return Ok(Reply::NotOffered),
                _ if !self.store.is_gm(world) || crate::online::requires_online(&self.store, world) => return Ok(Reply::NotOffered),
                _ if store::moving(world) => return Ok(Reply::Busy),
                _ => {}
            }
        }
        match ask {
            Ask::Worlds if self.own => {
                return Ok(Reply::Worlds { worlds: self.offers(Some(&id))?, host: None, admin: false, may_bring: false, asked: false });
            }
            Ask::Worlds => {
                let asked = self.book()?.asking.contains(&id);
                return Ok(Reply::Worlds {
                    worlds: self.offers(Some(&id))?,
                    host: Some(self.me.clone()),
                    admin: id == self.admin,
                    may_bring: self.may_bring(&id)?,
                    asked,
                });
            }
            // Its GM takes their own world away — to another server, say; the admin too,
            // without closing it first.
            Ask::Manage { order: Some(Order::Delete { world }) } if self.store.holds(&world) && self.store.gm_of(&world)?.as_deref() == Some(id.as_str()) => {
                let Some(_writing) = self.claim(&world, true)? else { return Ok(Reply::Busy) };
                self.store.delete_world(&world)?;
                println!("{world}: taken away by {}", who.name);
                return Ok(Reply::Managed { state: ServerState::default() });
            }
            Ask::Manage { order } if id == self.admin => {
                let _writing = if let Some(Order::Delete { world }) = &order {
                    let Some(writing) = self.claim(world, true)? else { return Ok(Reply::Busy) };
                    writing
                } else { None };
                if let Some(order) = order {
                    self.carry_out(order)?;
                }
                return Ok(Reply::Managed { state: self.state()? });
            }
            Ask::Manage { .. } => return Ok(Reply::NotOffered),
            // Its GM, or the admin: how the server backs the world up.
            Ask::Backups { plan } => {
                let gm = self.store.holds(world) && self.store.gm_of(world)?.as_deref() == Some(id.as_str());
                if !(gm || (id == self.admin && self.store.holds(world))) {
                    return Ok(Reply::NotOffered);
                }
                if let Some(plan) = plan {
                    if !["off", "daily", "weekly"].contains(&plan.every.as_str()) {
                        return Err(AppError("Off, daily or weekly.".into()));
                    }
                    let mut book = self.book()?;
                    book.backups.insert(world.to_string(), BackupPlan { keep: plan.keep.clamp(1, 365), ..plan });
                    self.save(&book)?;
                }
                return Ok(Reply::Backups { backups: self.backups_of(world)? });
            }
            Ask::HostWorlds => {
                if !self.may_bring(&id)? {
                    let mut book = self.book()?;
                    if !book.asking.contains(&id) {
                        book.asking.push(id);
                        self.save(&book)?;
                    }
                }
                return Ok(Reply::Requested);
            }
            _ => {}
        }
        if world.is_empty() || self.book()?.closed.contains_key(world) {
            return Ok(Reply::NotOffered);
        }
        // One write at a time; a write waits a moment for the one before
        // it, reading never waits.
        let _writing = if writes(&ask) {
            let Some(writing) = self.claim_soon(world)? else { return Ok(Reply::Busy) };
            writing
        } else {
            None
        };
        if !self.store.holds(world) {
            return self.bring(world, who, ask);
        }
        let store = &self.store;
        if matches!(ask, Ask::Commit { .. } | Ask::Abort { .. } | Ask::Release { .. }) {
            if store.gm_of(world)?.as_deref() != Some(id.as_str()) && !store.has_grant(world, &id)? {
                return Ok(Reply::NotMember);
            }
            return self.online_command(world, who, ask);
        }
        // Its GM: the whole of it, and who asked to join.
        if store.gm_of(world)?.as_deref() == Some(id.as_str()) {
            let mut reply = network::answer_member(store, world, &id, ask)?;
            if let Reply::Digest { requests, no_copy, .. } = &mut reply {
                *requests = self.requests(world)?;
                *no_copy = !store.copies(world)?;
            }
            return Ok(reply);
        }
        // A member by the grant the GM wrote. Anyone else may ask to join a
        // world on the list; one that is not is as if it were not here — its
        // GM adds a new player by their id.
        // Let in ahead of time, on the GM's own device: in at once — the keys
        // for the id are here now.
        if self.own && !store.has_grant(world, &id)? && who.is_genuine() {
            let meta = store.load_world_meta(world)?;
            if meta.member(&id).is_none() && meta.invited.contains(&id) {
                crate::admit(store, world, who.clone(), Vec::new())?;
            }
        }
        if !store.has_grant(world, &id)? {
            let expected = store.expects(world, &id)?;
            let may_ask = expected || self.listed(world);
            // Let in ahead of time by their id: whatever they ask, they ask
            // to join — a device that still takes them for a member, too.
            return Ok(if may_ask && (expected || matches!(ask, Ask::Join)) {
                let mut requests = store.load_requests(world)?;
                if who.is_genuine() && !requests.iter().any(|r| r.identity.id == who.id) && requests.len() < MAX_REQUESTS {
                    requests.push(JoinRequest { identity: who.clone(), at: network::now() });
                    store.save_requests(world, &requests)?;
                }
                Reply::Requested
            } else if may_ask || store.load_requests(world)?.iter().any(|r| r.identity.id == who.id) {
                // Whoever asked already knows of it, and waits for the GM.
                Reply::NotMember
            } else {
                Reply::NotOffered
            });
        }
        // On the GM's own device: online where the world stays here, else
        // their copy.
        if self.own {
            if crate::host::online_only(store, world) {
                return Ok(network::online(store, world, who, ask));
            }
            return network::answer_member(store, world, &id, ask);
        }
        // Unencrypted: online, each member with their view and rights.
        if !store.is_encrypted(world)? {
            return Ok(network::online(store, world, who, ask));
        }
        // Encrypted: what only their keys open — kept while online where
        // the GM keeps players from copies.
        let mut reply = network::answer_member(store, world, &id, ask)?;
        if let Reply::Digest { no_copy, .. } = &mut reply {
            *no_copy = !store.copies(world)?;
        }
        Ok(reply)
    }

    /// A world new here: brought by a GM the admin lets bring worlds — and
    /// only their own. Anyone else's waits for the admin.
    fn bring(&self, world: &str, who: &IdentityPublic, ask: Ask) -> Answer<Reply> {
        let id = who.id.to_string();
        if !self.may_bring(&id)? {
            let mut book = self.book()?;
            if matches!(ask, Ask::Digest { .. }) && !book.asking.contains(&id) {
                book.asking.push(id);
                self.save(&book)?;
            }
            return Ok(Reply::Requested);
        }
        Ok(match ask {
            Ask::Digest { .. } => Reply::Digest {
                digest: aw_store::journal::Journal::digest(&[]),
                presence: Vec::new(),
                rolls: Vec::new(),
                handover: false,
                requests: Vec::new(),
                no_copy: false,
                mark: None,
            },
            // What it has, asked for: all of it.
            Ask::Swap { have, give } if give.is_empty() => Reply::Records { records: Vec::new(), wanted: have, digest: aw_store::journal::Journal::digest(&[]) },
            Ask::Swap { have, give } => {
                if !self.take_brought(world, who, &give)? {
                    return Ok(Reply::NotOffered);
                }
                let here: HashSet<String> = self.store.inventory(world)?.into_iter().collect();
                let wanted = have.into_iter().filter(|r| !here.contains(r)).collect();
                Reply::Records { records: Vec::new(), wanted, digest: aw_store::journal::Journal::digest(&here.into_iter().collect::<Vec<_>>()) }
            }
            Ask::Give { records } if !records.is_empty() => {
                if !self.take_brought(world, who, &records)? {
                    return Ok(Reply::NotOffered);
                }
                Reply::Taken { digest: aw_store::journal::Journal::digest(&self.store.inventory(world)?) }
            }
            _ => Reply::NotOffered,
        })
    }

    /// The first records of a world brought here. They have to name the one
    /// bringing it GM — a world of someone else's goes again.
    fn take_brought(&self, world: &str, who: &IdentityPublic, records: &[String]) -> Answer<bool> {
        self.store.receive(world, records)?;
        if self.store.gm_of(world)?.as_deref() != Some(who.id.as_str()) {
            self.store.delete_world(world)?;
            return Ok(false);
        }
        println!("{world}: brought by {}", who.name);
        Ok(true)
    }

    /// Who asked to join a world here — for its GM, who decides. Those let
    /// in meanwhile are done.
    fn requests(&self, world: &str) -> Answer<Vec<IdentityPublic>> {
        let store = &self.store;
        let mut requests = store.load_requests(world)?;
        let before = requests.len();
        requests.retain(|r| !store.has_grant(world, r.identity.id.as_str()).unwrap_or(false));
        if requests.len() != before {
            store.save_requests(world, &requests)?;
        }
        Ok(requests.into_iter().map(|r| r.identity).collect())
    }

    fn plan(&self, world: &str) -> Answer<BackupPlan> {
        Ok(self.book()?.backups.get(world).cloned().unwrap_or_else(default_plan))
    }

    /// The folder of a world's backups here.
    fn backups_dir(&self, world: &str) -> PathBuf {
        self.backups.join(store::dir_name(world))
    }

    fn backups_of(&self, world: &str) -> Answer<ServerBackups> {
        let there = backup::list(&self.backups_dir(world));
        Ok(ServerBackups { plan: self.plan(world)?, count: there.len() as u32, last: there.first().map(|(a, _)| a.at).unwrap_or(0) })
    }

    /// Backs up the worlds whose time has come — not one being written right
    /// now; it comes the next time round.
    pub(crate) fn due(&self) {
        self.release_receipts(std::time::Duration::from_secs(10 * 60));
        let Ok(worlds) = self.store.list_worlds() else { return };
        for meta in worlds {
            let world = meta.public.world_id.to_string();
            let Ok(plan) = self.plan(&world) else { continue };
            let Some(period) = backup::period(&plan.every) else { continue };
            let into = self.backups_dir(&world);
            let last = backup::list(&into).first().map(|(a, _)| a.at).unwrap_or(0);
            if store::now().saturating_sub(last) < period {
                continue;
            }
            let Ok(Some(Some(_writing))) = self.claim(&world, true) else { continue };
            let gm = self.known(&world).map(|k| k.gm_id).unwrap_or_default();
            match backup::copy_world(&self.store.world_dir(&world), &into, &world, &meta.public.name, &gm, Some(plan.keep), |_| Ok(())) {
                Ok(at) => println!("{world}: backed up to {}", at.display()),
                Err(e) => eprintln!("{world}: backup failed: {}", e.0),
            }
        }
    }

    fn carry_out(&self, order: Order) -> Answer<()> {
        let mut book = self.book()?;
        match order {
            Order::Allow { id } => {
                if !is_id(&id) {
                    return Err(AppError("That is no id.".into()));
                }
                book.asking.retain(|a| a != &id);
                if id != self.admin && !book.gms.contains(&id) {
                    book.gms.push(id);
                }
            }
            Order::Refuse { id } => {
                book.asking.retain(|a| a != &id);
                book.gms.retain(|g| g != &id);
            }
            Order::Close { world } => {
                let name = self.store.load_world_meta(&world).map(|m| m.public.name).unwrap_or_default();
                book.closed.insert(world, name);
            }
            Order::Open { world } => {
                book.closed.remove(&world);
            }
            Order::Delete { world } => {
                if !book.closed.contains_key(&world) {
                    return Err(AppError("Close the world first.".into()));
                }
                if self.store.holds(&world) {
                    self.store.delete_world(&world)?;
                }
            }
            Order::Block { id } if id != self.admin => {
                if !book.blocked.contains(&id) {
                    book.blocked.push(id);
                }
            }
            Order::Block { .. } => return Err(AppError("The admin cannot be turned away.".into())),
            Order::Unblock { id } => book.blocked.retain(|b| b != &id),
        }
        self.save(&book)
    }

    fn state(&self) -> Answer<ServerState> {
        let book = self.book()?;
        let person = |id: &str| {
            let (name, last) = book.users.get(id).cloned().unwrap_or_default();
            Person { id: id.to_string(), name, last, blocked: book.blocked.iter().any(|b| b == id) }
        };
        let mut worlds = Vec::new();
        for meta in self.store.list_worlds()? {
            let world = meta.public.world_id.to_string();
            let known = self.known(&world)?;
            worlds.push(HeldWorld {
                name: meta.public.name.clone(),
                gm: known.gm,
                gm_id: known.gm_id,
                bytes: size(&self.store.world_dir(&world)),
                encrypted: known.encrypted,
                closed: book.closed.contains_key(&world),
                world,
            });
        }
        // Closed and gone: by the name it had.
        for (world, name) in book.closed.iter().filter(|(w, _)| !self.store.holds(w)) {
            worlds.push(HeldWorld { world: world.clone(), name: name.clone(), gm: String::new(), gm_id: String::new(), bytes: 0, encrypted: true, closed: true });
        }
        worlds.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        let mut users: Vec<Person> = book.users.keys().map(|id| person(id)).collect();
        users.sort_by(|a, b| b.last.cmp(&a.last));
        Ok(ServerState {
            worlds,
            gms: std::iter::once(&self.admin).chain(book.gms.iter()).map(|id| person(id)).collect(),
            asking: book.asking.iter().map(|id| person(id)).collect(),
            users,
        })
    }
}

/// The app's own keeper, by profile: the GM's device answering others.
static HERE: std::sync::OnceLock<Mutex<HashMap<PathBuf, Arc<Keeper>>>> = std::sync::OnceLock::new();

/// The keeper of the GM's worlds on this device — what answers whoever asks
/// this app, under a server's rules. Its notes (who came, who is turned
/// away) lie in the profile's `host/`.
pub(crate) fn here(store: &Store) -> Answer<Arc<Keeper>> {
    let mut all = HERE.get_or_init(Default::default).lock().map_err(AppError::from)?;
    if let Some(keeper) = all.get(store.profile_dir()) {
        return Ok(keeper.clone());
    }
    let admin = store.load_identity()?.id().to_string();
    let mut keeper = Keeper::open(store.clone(), &store.profile_dir().join("host"), admin)?;
    keeper.own = true;
    let keeper = Arc::new(keeper);
    all.insert(store.profile_dir().to_path_buf(), keeper.clone());
    Ok(keeper)
}

impl Keeper {
    /// Answers someone whose identity is proven.
    pub(crate) fn answer_to(&self, world: &str, who: &IdentityPublic, ask: Ask) -> Answer<Reply> {
        self.reply(world, who, ask)
    }

}

impl Host for Keeper {
    fn offers(&self) -> Vec<Offer> {
        Keeper::offers(self, None).unwrap_or_default()
    }
    /// Every world: one not here yet may be brought.
    fn serves(&self, _world: &str) -> bool {
        true
    }
    fn answer(&self, world: &str, who: &IdentityPublic, ask: Ask) -> Reply {
        self.reply(world, who, ask).unwrap_or(Reply::NotOffered)
    }
}
/// What a folder takes on disk.
fn size(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| match e.file_type() {
                    Ok(t) if t.is_dir() => size(&e.path()),
                    _ => e.metadata().map(|m| m.len()).unwrap_or(0),
                })
                .sum()
        })
        .unwrap_or(0)
}
#[cfg(test)]
mod tests {
    use super::*;
    use aw_sync::world::{Reach, Server, WorldNet};
    use aw_model::Level;
    use aw_store::journal::{Access, Plain};
    use store::NewPart;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct OnlineWire<'a> {
        keeper: &'a Keeper,
        who: &'a Identity,
        alive: AtomicBool,
        lose_ack: AtomicBool,
        stop_before_commit: AtomicBool,
        record_limit: usize,
        asks: std::sync::atomic::AtomicUsize,
        swaps: std::sync::atomic::AtomicUsize,
    }

    impl crate::online::Transport for OnlineWire<'_> {
        fn ask<'a>(&'a self, world: &'a str, ask: Ask) -> std::pin::Pin<Box<dyn std::future::Future<Output = Answer<Reply>> + Send + 'a>> {
            Box::pin(async move {
                if !self.alive.load(Ordering::SeqCst) {
                    return Err(AppError(crate::online::UNAVAILABLE.into()));
                }
                let final_commit = matches!(ask, Ask::Commit { finish: true, .. });
                if final_commit && self.stop_before_commit.swap(false, Ordering::SeqCst) {
                    self.alive.store(false, Ordering::SeqCst);
                    return Err(AppError(crate::online::UNAVAILABLE.into()));
                }
                if matches!(ask, Ask::Swap { .. }) { self.swaps.fetch_add(1, Ordering::SeqCst); }
                let mut reply = crate::online::patiently(std::time::Duration::from_secs(10), || {
                    self.asks.fetch_add(1, Ordering::SeqCst);
                    std::future::ready(Ok(self.keeper.answer(world, &self.who.public(), ask.clone())))
                }).await?;
                if let Reply::Records { records, .. } = &mut reply {
                    records.truncate(self.record_limit);
                }
                if final_commit && self.lose_ack.swap(false, Ordering::SeqCst) {
                    self.alive.store(false, Ordering::SeqCst);
                    return Err(AppError(crate::online::UNAVAILABLE.into()));
                }
                Ok(reply)
            })
        }
    }

    fn online_wire<'a>(keeper: &'a Keeper, who: &'a Identity) -> OnlineWire<'a> {
        OnlineWire { keeper, who, alive: AtomicBool::new(true), lose_ack: AtomicBool::new(false), stop_before_commit: AtomicBool::new(false), record_limit: usize::MAX,
                     asks: Default::default(), swaps: Default::default() }
    }

    fn on_server(gm: &Someone, world: &str) -> Server {
        let server = Server { endpoint: "1".repeat(64), addrs: vec!["127.0.0.1:7447".into()] };
        let mut settings = gm.store.load_network().unwrap();
        settings.servers = vec![server.code()];
        settings.homes.insert(world.into(), server.endpoint.clone());
        gm.store.save_network(&settings).unwrap();
        server
    }

    async fn run_online<T, F, R>(base: &Store, world: &str, server: &Server, wire: &T, run: F) -> Answer<R>
    where T: crate::online::Transport, F: FnOnce(Store) -> Answer<R> {
        crate::online::execute(base, world, server, wire, |cache| std::future::ready(run(cache))).await
    }

    fn staged_control(keeper: &Keeper, gm: &Someone, world: &str, transaction: &str) -> Ask {
        let before = gm.store.inventory(world).unwrap();
        gm.store.control(world, aw_store::journal::Item::Copies { players: false }, "staged").unwrap();
        let added: Vec<_> = gm.store.inventory(world).unwrap().into_iter().filter(|id| !before.contains(id)).collect();
        Ask::Commit { transaction: transaction.into(), base: aw_store::journal::Journal::digest(&keeper.store.inventory(world).unwrap()),
            records: gm.store.records(world, &added).unwrap(), parts: Vec::new(), finish: false }
    }

    #[test]
    fn a_staged_control_conflicts_instead_of_overwriting_a_newer_server_revision() {
        let dir = tempfile::tempdir().unwrap();
        let gm = someone(dir.path(), "Marla");
        let keeper = keeper(dir.path(), &gm);
        let (world, _) = world(&gm, "Eldermere");
        swap(&keeper, &gm, &world);
        let original = keeper.store.inventory(&world).unwrap();
        let staged = staged_control(&keeper, &gm, &world, &"a".repeat(64));
        let Ask::Commit { transaction, base, .. } = &staged else { unreachable!() };
        let finish = Ask::Commit { transaction: transaction.clone(), base: base.clone(), records: Vec::new(), parts: Vec::new(), finish: true };
        assert_eq!(keeper.answer(&world, &gm.me.public(), staged), Reply::Staged);
        assert_eq!(keeper.store.inventory(&world).unwrap(), original);
        assert!(keeper.store.copies(&world).unwrap());
        // A second, independently acknowledged command changes the revision.
        let before = gm.store.inventory(&world).unwrap();
        gm.store.control(&world, aw_store::journal::Item::Copies { players: true }, "another-command").unwrap();
        let added: Vec<_> = gm.store.inventory(&world).unwrap().into_iter().filter(|id| !before.contains(id)).collect();
        keeper.store.receive(&world, &gm.store.records(&world, &added).unwrap()).unwrap();
        assert_eq!(keeper.answer(&world, &gm.me.public(), finish), Reply::Conflict);
        assert!(keeper.store.copies(&world).unwrap());
    }

    #[test]
    fn a_durable_commit_intent_recovers_after_restart_and_its_receipt_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let gm = someone(dir.path(), "Marla");
        let server = keeper(dir.path(), &gm);
        let (world, _) = world(&gm, "Eldermere");
        swap(&server, &gm, &world);
        let staged = staged_control(&server, &gm, &world, &"b".repeat(64));
        let Ask::Commit { transaction, base, .. } = &staged else { unreachable!() };
        let finish = Ask::Commit { transaction: transaction.clone(), base: base.clone(), records: Vec::new(), parts: Vec::new(), finish: true };
        let scope = blake3::hash(format!("{world}\n{}", gm.me.id()).as_bytes()).to_hex().to_string();
        let stage = dir.path().join("online-transactions").join(scope).join(transaction);
        assert_eq!(server.answer(&world, &gm.me.public(), staged), Reply::Staged);
        aw_store::durable::write(&stage.join("commit"), b"recover").unwrap();
        drop(server);
        let server = keeper(dir.path(), &gm);
        assert!(!server.store.copies(&world).unwrap());
        let reply = server.answer(&world, &gm.me.public(), finish.clone());
        assert!(matches!(reply, Reply::Committed { .. }));
        assert_eq!(server.answer(&world, &gm.me.public(), finish), reply);
        assert!(!stage.join("records").exists());
    }

    #[test]
    fn unsigned_online_records_never_reach_the_canonical_journal() {
        let dir = tempfile::tempdir().unwrap();
        let gm = someone(dir.path(), "Marla");
        let keeper = keeper(dir.path(), &gm);
        let (world, _) = world(&gm, "Eldermere");
        swap(&keeper, &gm, &world);
        let before = keeper.store.inventory(&world).unwrap();
        let mut staged = staged_control(&keeper, &gm, &world, &"c".repeat(64));
        if let Ask::Commit { records, finish, .. } = &mut staged {
            let mut raw: serde_json::Value = serde_json::from_str(&records[0]).unwrap();
            raw["signature"] = "invalid".into();
            records[0] = serde_json::to_string(&raw).unwrap();
            *finish = true;
        }
        assert!(keeper.reply(&world, &gm.me.public(), staged).is_err());
        assert_eq!(keeper.store.inventory(&world).unwrap(), before);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires loopback UDP sockets"]
    async fn encrypted_online_commands_use_a_real_server_and_fail_closed_after_disconnect() {
        struct Nobody;
        impl Host for Nobody {
            fn offers(&self) -> Vec<Offer> { Vec::new() }
            fn answer(&self, _: &str, _: &IdentityPublic, _: Ask) -> Reply { Reply::NotOffered }
        }
        let dir = tempfile::tempdir().unwrap();
        let (gm, bob) = (someone(dir.path(), "Marla"), someone(dir.path(), "Bob"));
        let keeper = Arc::new(keeper(dir.path(), &gm));
        let (world, article) = world(&gm, "Online separation test");
        gm.store.control(&world, aw_store::journal::Item::Copies { players: false }, "").unwrap();
        admit(&gm.store, &world, bob.me.public(), Vec::new()).unwrap();
        swap(&keeper, &gm, &world);
        let reach = Reach { only: Some(std::net::Ipv4Addr::LOCALHOST), ..Default::default() };
        let mut endpoint = Some(WorldNet::start(keeper.clone(), reach.clone()).await.unwrap());
        let server = Server { endpoint: endpoint.as_ref().unwrap().id(), addrs: endpoint.as_ref().unwrap().loopback_addr().ip_addrs().map(ToString::to_string).collect() };
        let client = WorldNet::start(Arc::new(Nobody), reach).await.unwrap();
        let authoritative = keeper.store.inventory(&world).unwrap();
        // A pre-0.1.56 GM folder may contain stale, unsent edits. Upgrading
        // must read the server, rather than silently publishing this folder.
        let old = document::code_of(&gm.store, &gm.me, &world, &article).unwrap();
        document::save(&gm.store, &gm.me, &world, &article, &old.code.replace("Content of Rumours", "Stale local copy must not upload."), &old.base, "old-client").unwrap();
        for who in [&gm, &bob] {
            let mut settings = who.store.load_network().unwrap();
            settings.servers = vec![server.code()];
            settings.homes.insert(world.clone(), server.endpoint.clone());
            who.store.save_network(&settings).unwrap();
        }
        network::with_servers(&gm.store.for_sync(), &client, &gm.me, &HashSet::new()).await.unwrap();
        assert_eq!(keeper.store.inventory(&world).unwrap(), authoritative, "migration cannot upload an old working copy");
        let wire = crate::online::Connection { net: &client, server: &server, me: &gm.me };
        run_online(&gm.store, &world, &server, &wire, |cache| {
            let code = document::code_of(&cache, &gm.me, &world, &article)?;
            assert!(!code.code.contains("Stale local copy must not upload."));
            document::save(&cache, &gm.me, &world, &article, &code.code.replace("Content of Rumours", "Saved through real encrypted transport."), &code.base, "real-wire")
        }).await.unwrap();
        let player = crate::online::Connection { net: &client, server: &server, me: &bob.me };
        let code = run_online(&bob.store, &world, &server, &player, |cache| {
            document::code_of(&cache, &bob.me, &world, &article).map(|d| d.code)
        }).await.unwrap();
        assert!(code.contains("Saved through real encrypted transport.") && !code.contains("Content of Truth"));
        assert!(!gm.store.holds(&world) && !bob.store.holds(&world));
        assert!(!keeper.store.holds_token(&world));
        assert!(keeper.store.load_keyring(&world).unwrap().attributes().next().is_none());
        // An explicit move/create on a server still provisions a new world.
        let fresh = new_world(&gm.store, &gm.me, "Explicit new upload".into()).unwrap().public.world_id.to_string();
        gm.store.control(&fresh, aw_store::journal::Item::Copies { players: false }, "").unwrap();
        network::live_on(&gm.store, &fresh, Some(server.code())).unwrap();
        assert!(gm.store.load_network().unwrap().uploading.contains(&fresh));
        for _ in 0..20 {
            network::with_servers(&gm.store.for_sync(), &client, &gm.me, &HashSet::new()).await.unwrap();
            if network::online_worlds(&gm.store).iter().any(|w| w.world == fresh) { break }
        }
        assert!(keeper.store.holds(&fresh));
        assert!(!gm.store.holds(&fresh));
        assert!(!gm.store.load_network().unwrap().uploading.contains(&fresh));
        crate::online::close_session(&gm.store, &fresh).await.unwrap();
        crate::online::close_session(&gm.store, &world).await.unwrap();
        crate::online::close_session(&bob.store, &world).await.unwrap();
        // Optional disposable profiles for the native UI smoke test. No real
        // profile, world, or server is used by this fixture.
        let fixture = std::env::var_os("AW_ONLINE_FIXTURE").map(PathBuf::from);
        if let Some(path) = &fixture {
            for who in [&gm, &bob] {
                let config = dir.path().join(format!("{}-config", who.me.public().name));
                std::fs::create_dir_all(&config).unwrap();
                std::fs::write(config.join("locations.json"), serde_json::to_vec(&serde_json::json!({ "profile": who.store.profile_dir(), "worlds": who.store.worlds_dir(), "additional": [] })).unwrap()).unwrap();
            }
            std::fs::write(path, serde_json::to_vec(&serde_json::json!({ "root": dir.path(), "gm_config": dir.path().join("Marla-config"), "bob_config": dir.path().join("Bob-config"), "world": world, "article": article, "server": server.code() })).unwrap()).unwrap();
            while !path.with_extension("offline").exists() { tokio::time::sleep(std::time::Duration::from_millis(200)).await; }
        }
        drop(endpoint.take());
        let failed = run_online(&gm.store, &world, &server, &wire, |_| {
            panic!("the real server is gone; no local operation may run");
            #[allow(unreachable_code)] Ok(())
        }).await;
        assert!(failed.is_err());
        assert!(!gm.store.holds(&world));
        if let Some(path) = &fixture {
            while !path.with_extension("finish").exists() { tokio::time::sleep(std::time::Duration::from_millis(200)).await; }
        }
    }

    #[tokio::test]
    async fn an_online_gm_has_no_retained_world_and_never_opens_a_copy_when_the_server_is_off() {
        let dir = tempfile::tempdir().unwrap();
        let gm = someone(dir.path(), "Marla");
        let keeper = keeper(dir.path(), &gm);
        let (world, article) = world(&gm, "Eldermere");
        gm.store.control(&world, aw_store::journal::Item::Copies { players: false }, "").unwrap();
        swap(&keeper, &gm, &world);
        let server = on_server(&gm, &world);
        let token = std::fs::read(gm.store.private_world_dir(&world).join("owner.key")).unwrap();
        crate::online::adopt_uploaded(&gm.store, &world, &server).await.unwrap();
        assert!(!gm.store.holds(&world), "copies off applies to the GM too");
        let wire = online_wire(&keeper, &gm.me);
        let opened = run_online(&gm.store, &world, &server, &wire, |cache| {
            document::code_of(&cache, &gm.me, &world, &article).map(|d| d.code)
        }).await.unwrap();
        assert!(opened.contains("Content of Truth"));
        assert!(!keeper.store.holds_token(&world));
        assert!(keeper.store.load_keyring(&world).unwrap().attributes().next().is_none());
        assert!(keeper.store.view_keys(&world, &keeper.store.load_identity().unwrap()).is_err(), "the server cannot decrypt the world");
        wire.alive.store(false, Ordering::SeqCst);
        // What the server said a moment ago still counts; then it asks again.
        tokio::time::sleep(crate::online::FRESH).await;
        let refused = run_online(&gm.store, &world, &server, &wire, |_| {
            panic!("a disconnected server world must not execute a local command");
            #[allow(unreachable_code)] Ok(())
        }).await;
        assert!(refused.is_err());
        assert!(!gm.store.holds(&world));
        assert_eq!(std::fs::read(gm.store.private_world_dir(&world).join("owner.key")).unwrap(), token);
        assert!(gm.store.private_world_dir(&world).join("keyring.json").exists());
        crate::online::close_session(&gm.store, &world).await.unwrap();
    }

    #[tokio::test]
    async fn a_world_on_a_server_leaves_no_copy_even_where_copies_are_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let gm = someone(dir.path(), "Marla");
        let keeper = keeper(dir.path(), &gm);
        let (world, article) = world(&gm, "Eldermere");
        assert!(gm.store.copies(&world).unwrap(), "the world allows copies");
        swap(&keeper, &gm, &world);
        let server = on_server(&gm, &world);
        crate::online::adopt_uploaded(&gm.store, &world, &server).await.unwrap();
        assert!(!gm.store.holds(&world), "uploaded, it lies on the server only");
        let wire = online_wire(&keeper, &gm.me);
        run_online(&gm.store, &world, &server, &wire, |cache| document::code_of(&cache, &gm.me, &world, &article).map(|d| d.code)).await.unwrap();
        assert!(!gm.store.holds(&world), "working on it leaves nothing here");
        wire.alive.store(false, Ordering::SeqCst);
        tokio::time::sleep(crate::online::FRESH).await;
        let refused = run_online(&gm.store, &world, &server, &wire, |_| {
            panic!("no server, no world");
            #[allow(unreachable_code)] Ok(())
        }).await;
        assert!(refused.is_err());
        crate::online::close_session(&gm.store, &world).await.unwrap();
    }

    #[tokio::test]
    async fn a_player_downloads_every_piece_before_opening_and_copies_off_is_respected() {
        let dir = tempfile::tempdir().unwrap();
        let (gm, bob) = (someone(dir.path(), "Marla"), someone(dir.path(), "Bob"));
        let keeper = keeper(dir.path(), &gm);
        let (world, article) = world(&gm, "Eldermere");
        gm.store.control(&world, aw_store::journal::Item::Copies { players: false }, "").unwrap();
        admit(&gm.store, &world, bob.me.public(), Vec::new()).unwrap();
        swap(&keeper, &gm, &world);
        let server = on_server(&bob, &world);
        let mut wire = online_wire(&keeper, &bob.me);
        wire.record_limit = 2;
        let code = run_online(&bob.store, &world, &server, &wire, |cache| {
            document::code_of(&cache, &bob.me, &world, &article).map(|d| d.code)
        }).await.unwrap();
        assert!(code.contains("Content of Rumours") && !code.contains("Content of Truth"));
        assert!(!bob.store.holds(&world));
        assert!(!bob.store.worlds_dir().join(store::dir_name(&world)).exists());
        crate::online::close_session(&bob.store, &world).await.unwrap();
        let cache = bob.store.profile_dir().join("online-sessions");
        assert!(std::fs::read_dir(cache).unwrap().next().is_none(), "closing releases the disposable data");
    }

    #[tokio::test]
    async fn an_edit_is_on_the_server_before_the_command_reports_success() {
        let dir = tempfile::tempdir().unwrap();
        let gm = someone(dir.path(), "Marla");
        let keeper = keeper(dir.path(), &gm);
        let (world, article) = world(&gm, "Eldermere");
        gm.store.control(&world, aw_store::journal::Item::Copies { players: false }, "").unwrap();
        swap(&keeper, &gm, &world);
        let server = on_server(&gm, &world);
        crate::online::adopt_uploaded(&gm.store, &world, &server).await.unwrap();
        let wire = online_wire(&keeper, &gm.me);
        let before = keeper.store.inventory(&world).unwrap();
        run_online(&gm.store, &world, &server, &wire, |cache| {
            let code = document::code_of(&cache, &gm.me, &world, &article)?;
            document::save(&cache, &gm.me, &world, &article, &code.code.replace("Content of Rumours", "Saved on the server."), &code.base, "online-edit")
        }).await.unwrap();
        assert_ne!(keeper.store.inventory(&world).unwrap(), before);
        crate::online::close_session(&gm.store, &world).await.unwrap();
        let code = run_online(&gm.store, &world, &server, &wire, |cache| {
            document::code_of(&cache, &gm.me, &world, &article).map(|d| d.code)
        }).await.unwrap();
        assert!(code.contains("Saved on the server."), "a new session reads the confirmed edit");
        crate::online::close_session(&gm.store, &world).await.unwrap();
    }

    #[tokio::test]
    async fn a_connection_lost_before_commit_never_becomes_a_later_offline_upload() {
        let dir = tempfile::tempdir().unwrap();
        let gm = someone(dir.path(), "Marla");
        let keeper = keeper(dir.path(), &gm);
        let (world, article) = world(&gm, "Eldermere");
        gm.store.control(&world, aw_store::journal::Item::Copies { players: false }, "").unwrap();
        swap(&keeper, &gm, &world);
        let server = on_server(&gm, &world);
        crate::online::adopt_uploaded(&gm.store, &world, &server).await.unwrap();
        let wire = online_wire(&keeper, &gm.me);
        let before = keeper.store.inventory(&world).unwrap();
        wire.stop_before_commit.store(true, Ordering::SeqCst);
        let failed = run_online(&gm.store, &world, &server, &wire, |cache| {
            let code = document::code_of(&cache, &gm.me, &world, &article)?;
            document::save(&cache, &gm.me, &world, &article, &code.code.replace("Content of Rumours", "Must not appear later."), &code.base, "lost-connection")
        }).await;
        assert!(failed.is_err());
        assert_eq!(keeper.store.inventory(&world).unwrap(), before, "staging changes no canonical records");
        wire.alive.store(true, Ordering::SeqCst);
        let code = run_online(&gm.store, &world, &server, &wire, |cache| {
            document::code_of(&cache, &gm.me, &world, &article).map(|d| d.code)
        }).await.unwrap();
        assert!(!code.contains("Must not appear later."));
        assert_eq!(keeper.store.inventory(&world).unwrap(), before);
        crate::online::close_session(&gm.store, &world).await.unwrap();
    }

    #[tokio::test]
    async fn a_read_right_after_the_server_asks_nothing_and_a_failed_save_downloads_nothing_again() {
        let dir = tempfile::tempdir().unwrap();
        let gm = someone(dir.path(), "Marla");
        let keeper = keeper(dir.path(), &gm);
        let (world, article) = world(&gm, "Eldermere");
        gm.store.control(&world, aw_store::journal::Item::Copies { players: false }, "").unwrap();
        swap(&keeper, &gm, &world);
        let server = on_server(&gm, &world);
        crate::online::adopt_uploaded(&gm.store, &world, &server).await.unwrap();
        let wire = online_wire(&keeper, &gm.me);
        let read = |cache: Store| document::code_of(&cache, &gm.me, &world, &article).map(|d| d.code);
        run_online(&gm.store, &world, &server, &wire, read).await.unwrap();
        let asked = wire.asks.load(Ordering::SeqCst);
        assert!(asked > 0, "the first command reads the server");
        for _ in 0..5 {
            run_online(&gm.store, &world, &server, &wire, read).await.unwrap();
        }
        assert_eq!(wire.asks.load(Ordering::SeqCst), asked, "reads right after read nothing from the network");

        wire.stop_before_commit.store(true, Ordering::SeqCst);
        let failed = run_online(&gm.store, &world, &server, &wire, |cache| {
            let code = document::code_of(&cache, &gm.me, &world, &article)?;
            document::save(&cache, &gm.me, &world, &article, &code.code.replace("Content of Rumours", "Not sent."), &code.base, "lost")
        }).await;
        assert!(failed.is_err());
        wire.alive.store(true, Ordering::SeqCst);
        let swaps = wire.swaps.load(Ordering::SeqCst);
        let code = run_online(&gm.store, &world, &server, &wire, read).await.unwrap();
        assert!(!code.contains("Not sent."), "what was not acknowledged is gone here too");
        assert_eq!(wire.swaps.load(Ordering::SeqCst), swaps, "the acknowledged world stays on this device, not downloaded again");
        crate::online::close_session(&gm.store, &world).await.unwrap();
    }

    #[tokio::test]
    async fn what_another_member_wrote_comes_by_itself_without_naming_everything() {
        let dir = tempfile::tempdir().unwrap();
        let (gm, bob) = (someone(dir.path(), "Marla"), someone(dir.path(), "Bob"));
        let keeper = keeper(dir.path(), &gm);
        let (world, article) = world(&gm, "Eldermere");
        admit(&gm.store, &world, bob.me.public(), Vec::new()).unwrap();
        swap(&keeper, &gm, &world);
        let server = on_server(&gm, &world);
        crate::online::adopt_uploaded(&gm.store, &world, &server).await.unwrap();
        let (marla_wire, bob_wire) = (online_wire(&keeper, &gm.me), online_wire(&keeper, &bob.me));
        let read = |cache: Store| document::code_of(&cache, &bob.me, &world, &article).map(|d| d.code);
        let first = run_online(&bob.store, &world, &server, &bob_wire, read).await.unwrap();
        assert!(first.contains("Content of Rumours"));
        let swaps = bob_wire.swaps.load(Ordering::SeqCst);

        run_online(&gm.store, &world, &server, &marla_wire, |cache| {
            let code = document::code_of(&cache, &gm.me, &world, &article)?;
            document::save(&cache, &gm.me, &world, &article, &code.code.replace("Content of Rumours", "Written by Marla."), &code.base, "marla")
        }).await.unwrap();
        tokio::time::sleep(crate::online::FRESH).await;
        let after = run_online(&bob.store, &world, &server, &bob_wire, read).await.unwrap();
        assert!(after.contains("Written by Marla."), "{after}");
        assert_eq!(bob_wire.swaps.load(Ordering::SeqCst), swaps, "only what is new, not everything by name");
        crate::online::close_session(&bob.store, &world).await.unwrap();
        crate::online::close_session(&gm.store, &world).await.unwrap();
    }

    #[test]
    fn a_world_not_listed_shows_itself_to_the_id_its_gm_let_in_and_nobody_else() {
        let dir = tempfile::tempdir().unwrap();
        let (marla, bob, eve) = (someone(dir.path(), "Marla"), someone(dir.path(), "Bob"), someone(dir.path(), "Eve"));
        let keeper = keeper(dir.path(), &marla);
        let (world, _) = world(&marla, "Eldermere");
        // The GM lets Bob in ahead of time, by his id alone.
        let mut meta = marla.store.load_world_meta(&world).unwrap();
        meta.invited.push(bob.me.id().to_string());
        marla.store.save_world_meta(&meta).unwrap();
        swap(&keeper, &marla, &world);

        let named = |who: &Someone| {
            let Reply::Worlds { worlds, .. } = keeper.answer("", &who.me.public(), Ask::Worlds) else { panic!() };
            worlds.into_iter().map(|w| w.name).collect::<Vec<_>>()
        };
        assert_eq!(named(&bob), vec!["Eldermere"]);
        assert!(named(&eve).is_empty(), "nobody else learns of it");
        assert_eq!(keeper.answer(&world, &eve.me.public(), Ask::Join), Reply::NotOffered);
        assert_eq!(keeper.answer(&world, &bob.me.public(), Ask::Join), Reply::Requested);

        // The GM's device takes him in by itself; the server passes the world on.
        let Reply::Digest { requests, .. } = keeper.answer(&world, &marla.me.public(), Ask::Digest { presence: None, rolls: Vec::new() }) else { panic!() };
        crate::network::take_requests(&marla.store, &world, requests).unwrap();
        assert!(marla.store.load_world_meta(&world).unwrap().member(bob.me.id().as_str()).is_some());
        swap(&keeper, &marla, &world);
        assert!(matches!(swap(&keeper, &bob, &world), Reply::Records { .. }));
        assert!(named(&eve).is_empty());
    }

    #[test]
    fn a_player_taken_out_of_a_world_not_listed_loses_it_and_comes_back_by_id() {
        let dir = tempfile::tempdir().unwrap();
        let (marla, bob) = (someone(dir.path(), "Marla"), someone(dir.path(), "Bob"));
        let keeper = keeper(dir.path(), &marla);
        let (world, _) = world(&marla, "Eldermere");
        let named = |who: &Someone| {
            let Reply::Worlds { worlds, .. } = keeper.answer("", &who.me.public(), Ask::Worlds) else { panic!() };
            worlds.into_iter().map(|w| w.name).collect::<Vec<_>>()
        };
        let let_in = || {
            let Reply::Digest { requests, .. } = keeper.answer(&world, &marla.me.public(), Ask::Digest { presence: None, rolls: Vec::new() }) else { panic!() };
            crate::network::take_requests(&marla.store, &world, requests).unwrap();
            swap(&keeper, &marla, &world);
        };
        crate::network::invite(&marla.store, &world, bob.me.id().as_str()).unwrap();
        swap(&keeper, &marla, &world);
        assert_eq!(keeper.answer(&world, &bob.me.public(), Ask::Join), Reply::Requested);
        let_in();
        assert!(matches!(swap(&keeper, &bob, &world), Reply::Records { .. }));

        // Taken out: the world is gone for him, and asking again goes nowhere.
        marla.store.remove_member(&world, bob.me.id().as_str()).unwrap();
        swap(&keeper, &marla, &world);
        assert!(named(&bob).is_empty(), "{:?}", named(&bob));
        assert_eq!(keeper.answer(&world, &bob.me.public(), Ask::Join), Reply::NotOffered);
        assert_eq!(swap(&keeper, &bob, &world), Reply::NotOffered);

        // Let in by id again: he sees it, asks, and is in.
        crate::network::invite(&marla.store, &world, bob.me.id().as_str()).unwrap();
        swap(&keeper, &marla, &world);
        assert_eq!(named(&bob), vec!["Eldermere"]);
        // His device, still taking him for a member, asks for news: that asks to join.
        assert_eq!(keeper.answer(&world, &bob.me.public(), Ask::Digest { presence: None, rolls: Vec::new() }), Reply::Requested);
        let_in();
        assert!(marla.store.load_world_meta(&world).unwrap().member(bob.me.id().as_str()).is_some());
        assert!(matches!(swap(&keeper, &bob, &world), Reply::Records { .. }));
    }

    #[tokio::test]
    async fn a_save_while_someone_else_writes_waits_its_turn() {
        let dir = tempfile::tempdir().unwrap();
        let gm = someone(dir.path(), "Marla");
        let keeper = keeper(dir.path(), &gm);
        let (world, article) = world(&gm, "Eldermere");
        swap(&keeper, &gm, &world);
        let server = on_server(&gm, &world);
        crate::online::adopt_uploaded(&gm.store, &world, &server).await.unwrap();
        let wire = online_wire(&keeper, &gm.me);
        let other = async {
            let writing = keeper.claim(&world, true).unwrap().unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            drop(writing);
        };
        let save = run_online(&gm.store, &world, &server, &wire, |cache| {
            let code = document::code_of(&cache, &gm.me, &world, &article)?;
            document::save(&cache, &gm.me, &world, &article, &code.code.replace("Content of Rumours", "After the other one."), &code.base, "busy")
        });
        let ((), saved) = tokio::join!(other, save);
        saved.unwrap();
        let read = run_online(&gm.store, &world, &server, &wire, |cache| document::code_of(&cache, &gm.me, &world, &article).map(|d| d.code)).await.unwrap();
        assert!(read.contains("After the other one."));
        crate::online::close_session(&gm.store, &world).await.unwrap();
    }

    #[tokio::test]
    async fn new_gm_keys_survive_a_lost_commit_acknowledgement() {
        let dir = tempfile::tempdir().unwrap();
        let gm = someone(dir.path(), "Marla");
        let keeper = keeper(dir.path(), &gm);
        let (world, _) = world(&gm, "Eldermere");
        gm.store.control(&world, aw_store::journal::Item::Copies { players: false }, "").unwrap();
        swap(&keeper, &gm, &world);
        let server = on_server(&gm, &world);
        crate::online::adopt_uploaded(&gm.store, &world, &server).await.unwrap();
        let wire = online_wire(&keeper, &gm.me);
        wire.lose_ack.store(true, Ordering::SeqCst);
        let created = std::sync::Mutex::new(String::new());
        let failed = run_online(&gm.store, &world, &server, &wire, |cache| {
            let id = cache.create_article(&world, "place".into(), "new-knowledge".into(),
                vec![part("knowledge:new", "gm", "New knowledge", true)], "new-key")?;
            *created.lock().unwrap() = id.to_string();
            Ok(())
        }).await;
        assert!(failed.is_err(), "no success is reported without acknowledgement");
        assert!(gm.store.load_keyring(&world).unwrap().current_epoch(&"knowledge:new".parse().unwrap()).is_some());
        wire.alive.store(true, Ordering::SeqCst);
        let article = created.lock().unwrap().clone();
        let code = run_online(&gm.store, &world, &server, &wire, |cache| {
            document::code_of(&cache, &gm.me, &world, &article).map(|d| d.code)
        }).await.unwrap();
        assert!(code.contains("New knowledge"), "the published ciphertext still opens after reconnecting");
        assert!(!keeper.store.holds_token(&world));
        assert!(keeper.store.load_keyring(&world).unwrap().attributes().next().is_none());
        crate::online::close_session(&gm.store, &world).await.unwrap();
    }

    #[test]
    fn removing_a_server_address_does_not_turn_its_world_into_a_local_world() {
        let dir = tempfile::tempdir().unwrap();
        let gm = someone(dir.path(), "Marla");
        let (world, _) = world(&gm, "Eldermere");
        on_server(&gm, &world);
        let mut settings = gm.store.load_network().unwrap();
        settings.servers.clear();
        gm.store.save_network(&settings).unwrap();
        assert!(crate::online::requires_online(&gm.store, &world));
        assert!(crate::online::server_for(&gm.store, &world).is_err());
        assert!(!crate::online::direct_world_command_allowed("document_save"));
        assert!(!crate::online::direct_world_command_allowed("view"));
        assert!(!crate::online::direct_world_command_allowed("world_delete"));
    }

    struct Someone {
        store: Store,
        me: Identity,
    }

    fn someone(dir: &Path, name: &str) -> Someone {
        let store = Store::new(dir.join(name));
        let me = Identity::create(name).unwrap();
        store.save_identity(&me).unwrap();
        Someone { store, me }
    }

    fn keeper(dir: &Path, admin: &Someone) -> Keeper {
        let store = Store::new(dir.join("server"));
        store.save_identity(&Identity::create("aw-server").unwrap()).unwrap();
        Keeper::open(store, dir, admin.me.id().to_string()).unwrap()
    }

    fn part(view: &str, edit: &str, title: &str, is_base: bool) -> NewPart {
        NewPart {
            is_base,
            level: Level::Open,
            view: view.parse().unwrap(),
            access: Access { section: title.into(), own: view.into(), edit: edit.into() },
            plain: Plain { title: title.into(), text: format!("Content of {title}"), ..Plain::default() },
        }
    }

    /// A world with a document: the players' rumours, the GM's truth.
    fn world(gm: &Someone, name: &str) -> (String, String) {
        let world = new_world(&gm.store, &gm.me, name.into()).unwrap().public.world_id.to_string();
        let article = gm
            .store
            .create_article(
                &world,
                "place".into(),
                "tavern".into(),
                vec![part("players", "gm", "Tavern", true), part("players", "players", "Rumours", false), part("gm", "gm", "Truth", false)],
                "",
            )
            .unwrap()
            .to_string();
        (world, article)
    }

    /// What `swap` does over the network, without it: one round both ways.
    fn swap(keeper: &Keeper, who: &Someone, world: &str) -> Reply {
        match keeper.answer(world, &who.me.public(), Ask::Digest { presence: None, rolls: Vec::new() }) {
            Reply::Digest { no_copy, .. } => {
                if !who.store.is_gm(world) {
                    who.store.hold_fleeting(world, no_copy).unwrap();
                }
            }
            other => return other,
        }
        let have = who.store.inventory(world).unwrap();
        let reply = keeper.answer(world, &who.me.public(), Ask::Swap { have, give: Vec::new() });
        let Reply::Records { records, wanted, .. } = &reply else { return reply };
        who.store.receive(world, records).unwrap();
        let give = who.store.records(world, wanted).unwrap();
        let have = who.store.inventory(world).unwrap();
        keeper.answer(world, &who.me.public(), Ask::Swap { have, give })
    }

    fn manage(keeper: &Keeper, who: &Someone, order: Option<Order>) -> Reply {
        keeper.answer("", &who.me.public(), Ask::Manage { order })
    }

    fn state(keeper: &Keeper, admin: &Someone) -> ServerState {
        let Reply::Managed { state } = manage(keeper, admin, None) else { panic!("the admin manages") };
        state
    }

    fn titles(who: &Someone, world: &str) -> Vec<String> {
        let held = who.store.view_keys(world, &who.me).unwrap();
        let mut out: Vec<String> =
            build_view(&who.store.load_articles(world).unwrap(), &held).into_iter().flat_map(|a| a.layers.into_iter().map(|l| l.title)).collect();
        out.sort();
        out
    }

    #[test]
    fn a_world_comes_a_little_at_a_time_and_while_it_is_written_nobody_waits() {
        let dir = tempfile::tempdir().unwrap();
        let marla = someone(dir.path(), "Marla");
        let keeper = keeper(dir.path(), &marla);
        let (eldermere, _) = world(&marla, "Eldermere");
        let all = marla.store.inventory(&eldermere).unwrap();

        // Named once, given in pieces: the first piece brings the world here.
        let Reply::Records { wanted, .. } = keeper.answer(&eldermere, &marla.me.public(), Ask::Swap { have: all.clone(), give: Vec::new() }) else { panic!("the server wants it") };
        let order = marla.store.in_order(&eldermere, &wanted).unwrap();
        for piece in order.chunks(2) {
            let records = marla.store.records(&eldermere, piece).unwrap();
            assert!(matches!(keeper.answer(&eldermere, &marla.me.public(), Ask::Give { records }), Reply::Taken { .. }));
        }
        let mut there = keeper.store.inventory(&eldermere).unwrap();
        let mut here = all;
        there.sort();
        here.sort();
        assert_eq!(there, here);
        assert_eq!(keeper.offers(Some(marla.me.id().as_str())).unwrap().iter().map(|o| o.gm.clone()).collect::<Vec<_>>(), vec!["Marla"]);

        // Being written: reading goes on; another write waits a moment, then
        // hears busy. The world is listed as last known.
        let writing = keeper.claim(&eldermere, true).unwrap().unwrap();
        assert!(matches!(keeper.answer(&eldermere, &marla.me.public(), Ask::Digest { presence: None, rolls: Vec::new() }), Reply::Digest { .. }));
        let started = std::time::Instant::now();
        let record = marla.store.records(&eldermere, &marla.store.inventory(&eldermere).unwrap()[..1]).unwrap();
        assert_eq!(keeper.answer(&eldermere, &marla.me.public(), Ask::Give { records: record }), Reply::Busy);
        assert!(started.elapsed() >= WRITE_WAIT);
        let Reply::Worlds { worlds, .. } = keeper.answer("", &marla.me.public(), Ask::Worlds) else { panic!("the worlds") };
        assert_eq!(worlds.iter().map(|o| o.gm.clone()).collect::<Vec<_>>(), vec!["Marla"]);
        assert!(!state(&keeper, &marla).worlds.is_empty());
        drop(writing);
        assert!(matches!(keeper.answer(&eldermere, &marla.me.public(), Ask::Digest { presence: None, rolls: Vec::new() }), Reply::Digest { .. }));
    }

    #[test]
    fn a_world_here_is_backed_up_by_the_server_as_its_gm_asks() {
        let dir = tempfile::tempdir().unwrap();
        let marla = someone(dir.path(), "Marla");
        let tim = someone(dir.path(), "Tim");
        let keeper = keeper(dir.path(), &marla);
        let (eldermere, _) = world(&marla, "Eldermere");
        swap(&keeper, &marla, &eldermere);
        assert!(keeper.store.holds(&eldermere));
        let backups = |who: &Someone, plan: Option<BackupPlan>| keeper.answer(&eldermere, &who.me.public(), Ask::Backups { plan });

        // Daily, seven kept — until its GM says otherwise. Nobody else asks.
        let Reply::Backups { backups: first } = backups(&marla, None) else { panic!("its GM asks") };
        assert_eq!((first.plan.every.as_str(), first.plan.keep, first.count), ("daily", 7, 0));
        assert_eq!(backups(&tim, None), Reply::NotOffered);
        assert_eq!(backups(&tim, Some(BackupPlan { every: "off".into(), keep: 1 })), Reply::NotOffered);

        // Due: backed up once, then not again before a day is over.
        keeper.due();
        keeper.due();
        let Reply::Backups { backups: after } = backups(&marla, None) else { panic!() };
        assert_eq!(after.count, 1);
        assert!(after.last > 0);
        let there = backup::list(&keeper.backups_dir(&eldermere));
        assert!(there[0].1.join("world").is_dir(), "the world's folder, as it was");

        // Being written: not now, the next time round.
        let writing = keeper.claim(&eldermere, true).unwrap().unwrap();
        keeper.due();
        drop(writing);

        // Weekly, three kept — on disk, after a restart too. Nonsense is refused.
        assert!(matches!(backups(&marla, Some(BackupPlan { every: "weekly".into(), keep: 3 })), Reply::Backups { .. }));
        assert_eq!(backups(&marla, Some(BackupPlan { every: "hourly".into(), keep: 3 })), Reply::NotOffered);
        let again = Keeper::open(keeper.store.clone(), dir.path(), marla.me.id().to_string()).unwrap();
        assert_eq!(again.plan(&eldermere).unwrap(), BackupPlan { every: "weekly".into(), keep: 3 });
    }

    #[test]
    fn the_admin_lets_gms_in_and_closes_and_nobody_brings_what_is_not_theirs() {
        let dir = tempfile::tempdir().unwrap();
        let (marla, olaf, eve) = (someone(dir.path(), "Marla"), someone(dir.path(), "Olaf"), someone(dir.path(), "Eve"));
        let keeper = keeper(dir.path(), &marla);
        let (eldermere, _) = world(&marla, "Eldermere");
        // Named to everyone who asks the server — Thule is not.
        marla.store.set_listed(&eldermere, true).unwrap();
        let (thule, _) = world(&olaf, "Thule");

        // The admin brings theirs; another GM waits for the admin.
        assert!(matches!(swap(&keeper, &marla, &eldermere), Reply::Records { .. }));
        assert!(keeper.store.holds(&eldermere));
        assert_eq!(swap(&keeper, &olaf, &thule), Reply::Requested);
        assert!(!keeper.store.holds(&thule));
        assert_eq!(state(&keeper, &marla).asking.iter().map(|p| p.name.clone()).collect::<Vec<_>>(), vec!["Olaf"]);
        manage(&keeper, &marla, Some(Order::Allow { id: olaf.me.id().to_string() }));
        assert!(matches!(swap(&keeper, &olaf, &thule), Reply::Records { .. }));
        assert!(keeper.store.holds(&thule));

        // Who may bring worlds is told; whoever may not asks.
        let Reply::Worlds { may_bring, .. } = keeper.answer("", &eve.me.public(), Ask::Worlds) else { panic!() };
        assert!(!may_bring);
        assert_eq!(keeper.answer("", &eve.me.public(), Ask::HostWorlds), Reply::Requested);
        let Reply::Worlds { asked: true, .. } = keeper.answer("", &eve.me.public(), Ask::Worlds) else { panic!("Eve asked") };
        let Reply::Worlds { may_bring: true, .. } = keeper.answer("", &olaf.me.public(), Ask::Worlds) else { panic!("Olaf may") };
        manage(&keeper, &marla, Some(Order::Refuse { id: eve.me.id().to_string() }));

        // Its GM takes a world away; nobody else does.
        let (gone, _) = world(&olaf, "Gone");
        swap(&keeper, &olaf, &gone);
        assert_eq!(keeper.answer("", &eve.me.public(), Ask::Manage { order: Some(Order::Delete { world: gone.clone() }) }), Reply::NotOffered);
        assert!(keeper.store.holds(&gone));
        assert!(matches!(keeper.answer("", &olaf.me.public(), Ask::Manage { order: Some(Order::Delete { world: gone.clone() }) }), Reply::Managed { .. }));
        assert!(!keeper.store.holds(&gone));
        // The admin takes their own away too, without closing it first.
        let (mine, _) = world(&marla, "Mine");
        swap(&keeper, &marla, &mine);
        assert!(keeper.store.holds(&mine));
        assert!(matches!(keeper.answer("", &marla.me.public(), Ask::Manage { order: Some(Order::Delete { world: mine.clone() }) }), Reply::Managed { .. }));
        assert!(!keeper.store.holds(&mine));

        // A world of someone else's is not taken in.
        let (khom, _) = world(&marla, "Khom");
        let records = marla.store.records(&khom, &marla.store.inventory(&khom).unwrap()).unwrap();
        let have = marla.store.inventory(&khom).unwrap();
        assert_eq!(keeper.answer(&khom, &olaf.me.public(), Ask::Swap { have, give: records }), Reply::NotOffered);
        assert!(!keeper.store.holds(&khom));

        // Only the admin manages; they see each world's name, GM, form and size.
        assert_eq!(manage(&keeper, &olaf, None), Reply::NotOffered);
        let seen = state(&keeper, &marla);
        let thule_there = seen.worlds.iter().find(|w| w.world == thule).unwrap();
        assert_eq!((thule_there.name.as_str(), thule_there.gm.as_str(), thule_there.encrypted), ("Thule", "Olaf", true));
        assert!(thule_there.bytes > 0);
        assert_eq!(seen.gms.iter().map(|p| p.name.clone()).collect::<Vec<_>>(), vec!["Marla", "Olaf"]);

        // Closed, nobody gets it — its GM neither; deleted only once closed.
        assert!(matches!(manage(&keeper, &marla, Some(Order::Delete { world: thule.clone() })), Reply::NotOffered));
        manage(&keeper, &marla, Some(Order::Close { world: thule.clone() }));
        assert_eq!(swap(&keeper, &olaf, &thule), Reply::NotOffered);
        let Reply::Worlds { worlds, .. } = keeper.answer("", &eve.me.public(), Ask::Worlds) else { panic!() };
        assert_eq!(worlds.iter().map(|w| w.name.clone()).collect::<Vec<_>>(), vec!["Eldermere"]);
        manage(&keeper, &marla, Some(Order::Delete { world: thule.clone() }));
        assert!(!keeper.store.holds(&thule));
        assert_eq!(swap(&keeper, &olaf, &thule), Reply::NotOffered, "and it does not come back");
        assert!(state(&keeper, &marla).worlds.iter().any(|w| w.name == "Thule" && w.closed));

        // Turned away: whatever they ask. The admin cannot be.
        manage(&keeper, &marla, Some(Order::Block { id: eve.me.id().to_string() }));
        assert_eq!(keeper.answer("", &eve.me.public(), Ask::Worlds), Reply::NotOffered);
        assert_eq!(keeper.answer(&eldermere, &eve.me.public(), Ask::Join), Reply::NotOffered);
        assert!(matches!(manage(&keeper, &marla, Some(Order::Block { id: marla.me.id().to_string() })), Reply::NotOffered));
        manage(&keeper, &marla, Some(Order::Unblock { id: eve.me.id().to_string() }));
        assert_eq!(keeper.answer(&eldermere, &eve.me.public(), Ask::Join), Reply::Requested);

        // Started anew, it knows all of it still.
        drop(keeper);
        let keeper = Keeper::open(Store::new(dir.path().join("server")), dir.path(), marla.me.id().to_string()).unwrap();
        let again = state(&keeper, &marla);
        assert_eq!(again.gms.len(), 2);
        assert!(again.worlds.iter().any(|w| w.name == "Thule" && w.closed));
        assert!(again.users.iter().any(|u| u.name == "Eve" && !u.blocked));
    }

    #[test]
    fn an_encrypted_world_goes_to_its_members_and_without_copies_only_while_online() {
        let dir = tempfile::tempdir().unwrap();
        let (marla, bob, eve) = (someone(dir.path(), "Marla"), someone(dir.path(), "Bob"), someone(dir.path(), "Eve"));
        let keeper = keeper(dir.path(), &marla);
        let (world, _) = world(&marla, "Eldermere");
        marla.store.control(&world, aw_store::journal::Item::Copies { players: false }, "").unwrap();
        swap(&keeper, &marla, &world);

        // Not on the list: to a stranger it is not there at all.
        assert_eq!(keeper.answer(&world, &bob.me.public(), Ask::Join), Reply::NotOffered);
        let Reply::Worlds { worlds, .. } = keeper.answer("", &bob.me.public(), Ask::Worlds) else { panic!() };
        assert!(worlds.is_empty(), "not named to a stranger");
        // On the list: asked through the server, decided by the GM.
        marla.store.set_listed(&world, true).unwrap();
        swap(&keeper, &marla, &world);
        assert_eq!(keeper.answer(&world, &bob.me.public(), Ask::Join), Reply::Requested);
        assert_eq!(swap(&keeper, &bob, &world), Reply::NotMember);
        let Reply::Digest { requests, .. } = keeper.answer(&world, &marla.me.public(), Ask::Digest { presence: None, rolls: Vec::new() }) else { panic!() };
        assert_eq!(requests, vec![bob.me.public()]);
        admit(&marla.store, &world, bob.me.public(), Vec::new()).unwrap();
        swap(&keeper, &marla, &world);

        // Bob gets it — held only while online — and opens his part alone.
        assert!(matches!(swap(&keeper, &bob, &world), Reply::Records { .. }));
        assert!(bob.store.holds(&world) && bob.store.is_fleeting(&world));
        let seen = titles(&bob, &world);
        assert!(seen.contains(&"Rumours".to_string()) && !seen.contains(&"Truth".to_string()), "{seen:?}");
        assert_eq!(swap(&keeper, &eve, &world), Reply::NotMember, "on the list, a stranger may ask");

        // With copies, he keeps it; the next start does not take it.
        marla.store.control(&world, aw_store::journal::Item::Copies { players: true }, "").unwrap();
        swap(&keeper, &marla, &world);
        swap(&keeper, &bob, &world);
        swap(&keeper, &bob, &world);
        assert!(bob.store.holds(&world) && !bob.store.is_fleeting(&world));
        marla.store.control(&world, aw_store::journal::Item::Copies { players: false }, "").unwrap();
        swap(&keeper, &marla, &world);
        swap(&keeper, &bob, &world);
        swap(&keeper, &bob, &world);
        assert!(bob.store.is_fleeting(&world));
        bob.store.forget_fleeting();
        assert!(!bob.store.holds(&world), "gone with the next start");
        // Still a member: fetched again once online.
        assert_eq!(bob.store.fleeting_worlds(), vec![world.clone()]);
        swap(&keeper, &bob, &world);
        assert!(bob.store.is_fleeting(&world) && titles(&bob, &world).contains(&"Rumours".to_string()));
    }

    #[test]
    fn an_unencrypted_world_is_read_and_written_by_the_server_in_the_members_name() {
        let dir = tempfile::tempdir().unwrap();
        let (marla, bob) = (someone(dir.path(), "Marla"), someone(dir.path(), "Bob"));
        let keeper = keeper(dir.path(), &marla);
        let (world, article) = world(&marla, "Eldermere");
        marla.store.set_encrypted(&world, false).unwrap();
        admit(&marla.store, &world, bob.me.public(), Vec::new()).unwrap();
        // The grant names Bob's attributes as they are — and no keys.
        let open = marla.store.with_journal(&world, |j| Ok(j.grant(bob.me.id().as_str()).map(|(_, s)| s.is_plain()))).unwrap();
        assert_eq!(open, Some(true));
        marla.store.add_host(&world, keeper.me.id.as_str()).unwrap();
        swap(&keeper, &marla, &world);

        // Bob works online: the server shows him his part, not the GM's.
        assert_eq!(swap(&keeper, &bob, &world), Reply::OnlineOnly);
        assert!(!bob.store.holds(&world));
        let call = |command: &str, args: serde_json::Value| -> Result<serde_json::Value, String> {
            match keeper.answer(&world, &bob.me.public(), Ask::Call { command: command.into(), args: args.to_string() }) {
                Reply::Called { value: Some(v), .. } => Ok(serde_json::from_str(&v).unwrap()),
                Reply::Called { error: Some(e), .. } => Err(e),
                other => panic!("{other:?}"),
            }
        };
        let opened = call("document_open", serde_json::json!({ "article": article })).unwrap();
        let code = opened["code"].as_str().unwrap().to_string();
        assert!(code.contains("Content of Rumours") && !code.contains("Content of Truth"), "{code}");

        // He writes into his part — and not into the one he only sees.
        let save = |text: String| call("document_save", serde_json::json!({ "article": article, "text": text, "base": opened["base"], "session": "s1" }));
        save(code.replace("Content of Rumours", "Bob heard something.")).unwrap();
        assert!(save(code.replace("<name=Tavern>", "<name=Bob's tavern>")).is_err());
        call("session_end", serde_json::json!({ "session": "s1" })).unwrap();

        // The GM gets it, written in Bob's name.
        swap(&keeper, &marla, &world);
        let text = document::code_of(&marla.store, &marla.me, &world, &article).unwrap().code;
        assert!(text.contains("Bob heard something.") && text.contains("<name=Tavern>"), "{text}");
        let versions = document::article_versions_in(&marla.store, world.clone(), article.clone()).unwrap();
        assert!(versions.iter().any(|v| v.author == "Bob"), "{:?}", versions.iter().map(|v| &v.author).collect::<Vec<_>>());
    }
}
