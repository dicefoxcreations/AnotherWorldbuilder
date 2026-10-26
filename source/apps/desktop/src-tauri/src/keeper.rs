//! The keeper of a group's worlds: who comes in, and what each one may do
//! there — the same rules wherever it runs. On a server (see `server`) it
//! holds the worlds GMs brought; in the app (`here`) it answers for the
//! GM's own worlds, those offered on the network.
//!
//! A member works in a world here by asking for commands of the app, done
//! in their name (`remote::call`): what they get to see and change, the
//! world itself says.

use super::*;
use store::WorldDb;
use aw_sync::world::{Ask, BackupPlan, HeldWorld, Host, Offer, Order, Person, Reply, ServerBackups, ServerState};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Someone seen this long ago is noted again on disk, in seconds.
const NOTE_AGAIN: u64 = 10 * 60;
/// How much of a file one piece carries.
const PIECE: usize = 4 * 1024 * 1024;
/// How large a world brought here may be.
const MAX_WORLD: u64 = 64 * 1024 * 1024 * 1024;

/// What the server keeps of whom: who may bring worlds, which are closed,
/// who came and who is turned away.
#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Book {
    /// Admins besides the first, who is named when the server starts.
    #[serde(default)]
    admins: Vec<String>,
    /// Who may bring worlds besides the admins.
    #[serde(default)]
    worldbuilders: Vec<String>,
    /// Who would bring a world.
    #[serde(default)]
    asking: Vec<String>,
    /// By world: its name, for when it is gone.
    #[serde(default)]
    closed: BTreeMap<String, String>,
    #[serde(default)]
    blocked: Vec<String>,
    /// By id: name, and when last seen in seconds.
    #[serde(default)]
    users: BTreeMap<String, (String, u64)>,
    /// By world: how it is backed up, where its GM said otherwise than daily, 7 kept.
    #[serde(default)]
    backups: BTreeMap<String, BackupPlan>,
    /// By world: the server it went to from here, by its code — for its
    /// members to follow.
    #[serde(default)]
    moved: BTreeMap<String, String>,
}

/// Unless its GM says otherwise, a world here is backed up daily and the last
/// seven stay.
fn default_plan() -> BackupPlan {
    BackupPlan { every: "daily".into(), keep: 7 }
}

pub(crate) struct Keeper {
    store: Store,
    /// In the app: it answers for the GM's own worlds offered on the
    /// network, and nobody brings any.
    in_app: bool,
    admin: String,
    /// Its folder: `server.json`, `backups/`, and what is on its way.
    data: PathBuf,
    me: IdentityPublic,
    book: Mutex<Book>,
}

fn secs() -> u64 {
    store::now() / 1000
}

impl Keeper {
    pub(crate) fn open(store: Store, data: &Path, admin: String) -> Answer<Self> {
        let book = match std::fs::read(data.join("server.json")) {
            Ok(raw) => serde_json::from_slice(&raw)?,
            Err(_) => Book::default(),
        };
        let me = store.load_identity()?.public();
        // What was on its way when it stopped is asked for again.
        let _ = std::fs::remove_dir_all(data.join("outgoing"));
        Ok(Keeper { store, in_app: false, admin, data: data.to_path_buf(), me, book: Mutex::new(book) })
    }

    fn book(&self) -> Answer<std::sync::MutexGuard<'_, Book>> {
        self.book.lock().map_err(AppError::from)
    }

    fn save(&self, book: &Book) -> Answer<()> {
        aw_store::durable::write(&self.data.join("server.json"), &serde_json::to_vec_pretty(book)?)?;
        Ok(())
    }

    /// Notes who came — on disk when new, or not seen for a while.
    fn note(&self, who: &IdentityPublic) -> Answer<()> {
        let mut book = self.book()?;
        let now = secs();
        let id = who.id.to_string();
        let again = book.users.get(&id).is_none_or(|(name, last)| *name != who.name || now.saturating_sub(*last) > NOTE_AGAIN);
        book.users.insert(id, (who.name.clone(), now));
        if again {
            self.save(&book)?;
        }
        Ok(())
    }

    /// Whether someone manages the server.
    fn is_admin(&self, id: &str) -> Answer<bool> {
        Ok(id == self.admin || self.book()?.admins.iter().any(|a| a == id))
    }

    fn may_bring(&self, id: &str) -> Answer<bool> {
        Ok(!self.in_app && (self.is_admin(id)? || self.book()?.worldbuilders.iter().any(|g| g == id)))
    }

    /// Whether a world is here for others: on a server every world it
    /// holds; in the app the GM's own, offered on the network.
    fn keeps(&self, world: &str) -> bool {
        if !self.store.holds(world) {
            return false;
        }
        !self.in_app || (self.store.is_gm(world) && self.store.load_network().is_ok_and(|s| s.shared.iter().any(|w| w == world)))
    }

    fn closed(&self, world: &str) -> Answer<bool> {
        Ok(self.book()?.closed.contains_key(world))
    }

    fn gm_of(&self, world: &str) -> Answer<Option<store::Member>> {
        Ok(self.store.db(world)?.members()?.into_iter().find(|m| m.role == Role::Gm))
    }

    fn is_member(&self, world: &str, id: &str) -> Answer<bool> {
        Ok(self.store.db(world)?.member(id)?.is_some())
    }

    /// The worlds named to whoever asks — `id`: also those they are GM or a
    /// member of or let into ahead of time, all of them for the admin; none:
    /// only those on the list.
    pub(crate) fn offers(&self, id: Option<&str>) -> Answer<Vec<Offer>> {
        let closed: Vec<String> = self.book()?.closed.keys().cloned().collect();
        let admin = id.map(|id| self.is_admin(id)).transpose()?.unwrap_or(false);
        let mut out = Vec::new();
        for world in self.store.held_worlds()? {
            if closed.contains(&world) || !self.keeps(&world) {
                continue;
            }
            let theirs = match id {
                Some(id) => admin || self.is_member(&world, id)? || self.store.db(&world)?.invited()?.iter().any(|i| i == id),
                None => false,
            };
            if !theirs && !self.store.on_the_list(&world)? {
                continue;
            }
            let meta = self.store.load_world_meta(&world)?;
            let gm = meta.gm().map(|g| g.name.clone()).unwrap_or_default();
            out.push(Offer { world, name: meta.name, icon: meta.icon, gm });
        }
        Ok(out)
    }

    fn reply(&self, world: &str, who: &IdentityPublic, ask: Ask) -> Answer<Reply> {
        let id = who.id.to_string();
        self.note(who)?;
        if self.book()?.blocked.contains(&id) {
            return Ok(Reply::NotOffered);
        }
        if !world.is_empty() && store::moving(world) {
            return Ok(Reply::Busy);
        }
        match ask {
            Ask::Worlds => Ok(Reply::Worlds {
                worlds: self.offers(Some(&id))?,
                host: Some(self.me.clone()),
                admin: self.is_admin(&id)? && !self.in_app,
                may_bring: self.may_bring(&id)?,
                asked: self.book()?.asking.contains(&id),
            }),
            // Its GM takes their own world away — to another server, say.
            Ask::Manage { order: Some(Order::Delete { world }) } if !self.in_app && self.store.holds(&world) && self.gm_of(&world)?.is_some_and(|g| g.id == id) => {
                self.store.delete_world(&world)?;
                println!("{world}: taken away by {}", who.name);
                Ok(Reply::Managed { state: ServerState::default() })
            }
            Ask::Manage { order } if !self.in_app && self.is_admin(&id)? => {
                if let Some(order) = order {
                    self.carry_out(order)?;
                }
                Ok(Reply::Managed { state: self.state()? })
            }
            Ask::Manage { .. } => Ok(Reply::NotOffered),
            // Its GM, or the admin: how the server backs the world up.
            Ask::Backups { plan } => {
                let gm = self.keeps(world) && self.gm_of(world)?.is_some_and(|g| g.id == id);
                if self.in_app || !(gm || (self.is_admin(&id)? && self.store.holds(world))) {
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
                Ok(Reply::Backups { backups: self.backups_of(world)? })
            }
            // A world brought here: not by just anyone.
            Ask::MayIBring | Ask::Put { .. } | Ask::Bring { .. } if !self.may_bring(&id)? => {
                if self.in_app {
                    return Ok(Reply::NotOffered);
                }
                let mut book = self.book()?;
                if !book.asking.contains(&id) {
                    book.asking.push(id);
                    self.save(&book)?;
                }
                Ok(Reply::Requested)
            }
            Ask::MayIBring => Ok(Reply::Joined),
            Ask::Put { file, at, data } => self.put(world, &id, &file, at, &data),
            Ask::Bring { name } => self.bring(world, who, &name),
            _ if !self.keeps(world) || self.closed(world)? => Ok(match self.gone_to(world) {
                Some(server) => Reply::Moved { server },
                None => Reply::NotOffered,
            }),
            Ask::Join => Ok(if network::join(&self.store, world, who)? { Reply::Joined } else { Reply::Requested }),
            _ if !self.is_member(world, &id)? => Ok(Reply::NotMember),
            Ask::Call { command, args } => {
                let args: serde_json::Value = serde_json::from_str(&args)?;
                let db = self.store.db(world)?;
                if command == "world_sync" {
                    let since = args.get("since").and_then(|s| s.as_u64()).unwrap_or(0);
                    let (writes, changed) = others_wrote(&db, world, since, &id, self.host());
                    let info = store::acting_for(&id, || world_now(&self.store, world))?;
                    let value = serde_json::json!({ "writes": writes, "changed": changed, "world": info });
                    return Ok(Reply::Called { value: Some(value.to_string()), error: None });
                }
                let before = db.writes();
                let done = store::acting_for(&id, || remote::call(&self.store, world, &command, &args));
                note_writes(world, before, db.writes(), &id);
                Ok(match done {
                    Ok(value) => Reply::Called { value: Some(serde_json::to_string(&value)?), error: None },
                    Err(e) => Reply::Called { value: None, error: Some(e.0) },
                })
            }
            // Its GM takes it along — to their own device, or another server.
            Ask::Files | Ask::Get { .. } if !self.gm_of(world)?.is_some_and(|g| g.id == id) => Ok(Reply::NotOffered),
            Ask::Files => self.files(world),
            Ask::Get { file, at } => self.get(world, &file, at),
        }
    }

    /// Notes where a world went from here — or that it is back (`None`).
    pub(crate) fn moved(&self, world: &str, server: Option<&str>) -> Answer<()> {
        let mut book = self.book()?;
        match server {
            Some(code) => book.moved.insert(world.to_string(), code.to_string()),
            None => book.moved.remove(world),
        };
        self.save(&book)
    }

    /// Where a world went from here, by the server's code.
    pub(crate) fn gone_to(&self, world: &str) -> Option<String> {
        self.book().ok()?.moved.get(world).cloned().filter(|_| !self.store.holds(world))
    }

    /// The worlds gone from here to a server.
    pub(crate) fn gone(&self) -> Vec<String> {
        self.book().map(|b| b.moved.keys().filter(|w| !self.store.holds(w)).cloned().collect()).unwrap_or_default()
    }

    /// Whose writes are not noted — the app's own user; a server writes
    /// nothing of itself.
    fn host(&self) -> &str {
        if self.in_app { &self.admin } else { "" }
    }

    // --- Bringing and fetching worlds -------------------------------------

    /// Where a world brought by `id` arrives, until it is whole.
    fn incoming(&self, world: &str, id: &str) -> PathBuf {
        self.data.join("incoming").join(format!("{}-{}", store::dir_name(world), store::dir_name(id)))
    }

    /// Where a world is copied to be taken away, a piece at a time.
    fn outgoing(&self, world: &str) -> PathBuf {
        self.data.join("outgoing").join(store::dir_name(world))
    }

    /// A piece of a world brought here.
    fn put(&self, world: &str, id: &str, file: &str, at: u64, data: &str) -> Answer<Reply> {
        use base64::Engine;
        let path = self.incoming(world, id).join(safe_file(file)?);
        let bytes = base64::engine::general_purpose::STANDARD.decode(data.as_bytes()).map_err(|_| AppError("A piece did not arrive whole.".into()))?;
        let had = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if at == 0 {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
        } else if at != had {
            return Err(AppError(format!("Expected the piece at {had}.")));
        }
        if at + bytes.len() as u64 > MAX_WORLD || size(&self.incoming(world, id)) > MAX_WORLD {
            return Err(AppError("This world is too large for the server.".into()));
        }
        use std::io::Write;
        let mut out = std::fs::OpenOptions::new().create(true).write(true).truncate(at == 0).append(at != 0).open(&path)?;
        out.write_all(&bytes)?;
        Ok(Reply::Taken)
    }

    /// The world brought here is whole: one whose GM brought it, not here yet.
    fn bring(&self, world: &str, who: &IdentityPublic, name: &str) -> Answer<Reply> {
        let from = self.incoming(world, who.id.as_str());
        let db = WorldDb::open(&from).map_err(|_| AppError("The world did not arrive whole.".into()))?;
        if db.get("id")?.as_deref() != Some(world) {
            return Err(AppError("This is another world.".into()));
        }
        if !db.members()?.iter().any(|m| m.role == Role::Gm && m.id == who.id.as_str()) {
            return Err(AppError("Only its GM brings a world.".into()));
        }
        drop(db);
        if self.store.holds(world) {
            return Err(AppError("This world is here already.".into()));
        }
        if self.book()?.closed.contains_key(world) {
            return Err(AppError("This world was closed here.".into()));
        }
        let into = self.store.worlds_dir().join(store::dir_name(world));
        std::fs::create_dir_all(self.store.worlds_dir())?;
        std::fs::rename(&from, &into)?;
        println!("{world}: “{name}” brought by {}", who.name);
        Ok(Reply::Joined)
    }

    /// The files of a world, copied as they are now — to be taken away.
    fn files(&self, world: &str) -> Answer<Reply> {
        let out = self.outgoing(world);
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out)?;
        let db = self.store.db(world)?;
        db.copy_to(&out.join(aw_store::db::FILE))?;
        let mut files = vec![(aw_store::db::FILE.to_string(), std::fs::metadata(out.join(aw_store::db::FILE))?.len())];
        for entry in std::fs::read_dir(db.dir().join(aw_store::db::MEDIA)).into_iter().flatten().flatten() {
            if entry.file_type().is_ok_and(|t| t.is_file()) {
                files.push((format!("{}/{}", aw_store::db::MEDIA, entry.file_name().to_string_lossy()), entry.metadata()?.len()));
            }
        }
        Ok(Reply::Files { files })
    }

    /// A piece of one of a world's files — its database as copied for `Files`.
    fn get(&self, world: &str, file: &str, at: u64) -> Answer<Reply> {
        use base64::Engine;
        use std::io::{Read, Seek};
        let file = safe_file(file)?;
        let path = if file == aw_store::db::FILE { self.outgoing(world).join(file) } else { self.store.world_dir(world).join(file) };
        let mut f = std::fs::File::open(&path)?;
        f.seek(std::io::SeekFrom::Start(at))?;
        let mut buf = Vec::with_capacity(PIECE);
        f.take(PIECE as u64).read_to_end(&mut buf)?;
        Ok(Reply::Piece { data: base64::engine::general_purpose::STANDARD.encode(&buf) })
    }

    // --- Backups -----------------------------------------------------------

    fn plan(&self, world: &str) -> Answer<BackupPlan> {
        Ok(self.book()?.backups.get(world).cloned().unwrap_or_else(default_plan))
    }

    fn backups_dir(&self, world: &str) -> PathBuf {
        self.data.join("backups").join(store::dir_name(world))
    }

    fn backups_of(&self, world: &str) -> Answer<ServerBackups> {
        let there = backup::list(&self.backups_dir(world));
        Ok(ServerBackups { plan: self.plan(world)?, count: there.len() as u32, last: there.first().map(|(a, _)| a.at).unwrap_or(0) })
    }

    /// Backs up the worlds whose time has come.
    pub(crate) fn due(&self) {
        let Ok(worlds) = self.store.held_worlds() else { return };
        for world in worlds {
            let Ok(plan) = self.plan(&world) else { continue };
            let Some(period) = backup::period(&plan.every) else { continue };
            let into = self.backups_dir(&world);
            let last = backup::list(&into).first().map(|(a, _)| a.at).unwrap_or(0);
            if store::now().saturating_sub(last) < period || store::moving(&world) {
                continue;
            }
            let Ok(db) = self.store.db(&world) else { continue };
            let name = db.get("name").ok().flatten().unwrap_or_default();
            match backup::copy_world(&db, &into, &world, &name, "", Some(plan.keep)) {
                Ok(at) => println!("{world}: backed up to {}", at.display()),
                Err(e) => eprintln!("{world}: backup failed: {}", e.0),
            }
        }
    }

    // --- Managing ------------------------------------------------------------

    /// Does what the GM of a world would — for an admin's order.
    fn as_gm<T>(&self, world: &str, f: impl FnOnce() -> Answer<T>) -> Answer<T> {
        let gm = self.gm_of(world)?.ok_or_else(|| AppError("This world has no GM.".into()))?;
        store::acting_for(&gm.id, f)
    }

    fn carry_out(&self, order: Order) -> Answer<()> {
        let mut book = self.book()?;
        match order {
            Order::Allow { id } => {
                if !is_player_id(&id) {
                    return Err(AppError("That is no id.".into()));
                }
                book.asking.retain(|a| a != &id);
                if id != self.admin && !book.worldbuilders.contains(&id) {
                    book.worldbuilders.push(id);
                }
            }
            Order::Refuse { id } => {
                book.asking.retain(|a| a != &id);
                book.worldbuilders.retain(|g| g != &id);
            }
            Order::Admin { id, on: true } => {
                if !is_player_id(&id) {
                    return Err(AppError("That is no id.".into()));
                }
                if id != self.admin && !book.admins.contains(&id) {
                    book.admins.push(id.clone());
                }
                book.blocked.retain(|b| b != &id);
            }
            Order::Admin { id, .. } if id == self.admin => return Err(AppError("The first admin stays — named when the server starts.".into())),
            Order::Admin { id, on: false } => book.admins.retain(|a| a != &id),
            Order::Approve { world, id } => {
                let request = self.store.db(&world)?.requests()?.into_iter().find(|r| r.id == id).ok_or_else(|| AppError("There is no such request.".into()))?;
                self.as_gm(&world, || admit(&self.store, &world, card_of(&request)?, Vec::new()))?;
            }
            Order::Remove { world, id } => {
                if self.store.db(&world)?.member(&id)?.is_some_and(|m| m.role != Role::Player) {
                    return Err(AppError("Only players are taken out here — the GM takes out supporters.".into()));
                }
                self.as_gm(&world, || remove_member(&self.store, &world, &id))?;
            }
            Order::Close { world } => {
                let name = self.store.load_world_meta(&world).map(|m| m.name).unwrap_or_default();
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
            Order::Block { id } | Order::Kick { id } if id == self.admin || book.admins.contains(&id) => {
                return Err(AppError("An admin cannot be turned away.".into()));
            }
            Order::Block { id } => {
                if !book.blocked.contains(&id) {
                    book.blocked.push(id);
                }
            }
            Order::Kick { id } => {
                if !book.blocked.contains(&id) {
                    book.blocked.push(id.clone());
                }
                book.worldbuilders.retain(|w| w != &id);
                book.asking.retain(|a| a != &id);
                // Out of every world they play in; a GM or supporter only
                // their GM takes out — turned away, they get none of it.
                for world in self.store.held_worlds()? {
                    if self.store.db(&world)?.member(&id)?.is_some_and(|m| m.role == Role::Player) {
                        self.as_gm(&world, || remove_member(&self.store, &world, &id))?;
                    }
                }
            }
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
        for world in self.store.held_worlds()? {
            let db = self.store.db(&world)?;
            let members = db.members()?;
            let gm = members.iter().find(|m| m.role == Role::Gm);
            worlds.push(HeldWorld {
                name: db.get("name")?.unwrap_or_default(),
                gm: gm.map(|g| g.name.clone()).unwrap_or_default(),
                gm_id: gm.map(|g| g.id.clone()).unwrap_or_default(),
                bytes: size(db.dir()),
                closed: book.closed.contains_key(&world),
                requests: db.requests()?.into_iter().map(|r| Person { name: r.name.clone(), ..person(&r.id) }).collect(),
                supporters: members.iter().filter(|m| m.role == Role::Supporter).map(|m| m.id.clone()).collect(),
                members: members.iter().filter(|m| m.role != Role::Gm).map(|m| Person { name: m.name.clone(), ..person(&m.id) }).collect(),
                world,
            });
        }
        // Closed and gone: by the name it had.
        for (world, name) in book.closed.iter().filter(|(w, _)| !self.store.holds(w)) {
            worlds.push(HeldWorld { world: world.clone(), name: name.clone(), gm: String::new(), gm_id: String::new(), bytes: 0, closed: true, requests: Vec::new(), members: Vec::new(), supporters: Vec::new() });
        }
        worlds.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        let mut users: Vec<Person> = book.users.keys().map(|id| person(id)).collect();
        users.sort_by(|a, b| b.last.cmp(&a.last));
        Ok(ServerState {
            worlds,
            admins: std::iter::once(&self.admin).chain(book.admins.iter()).map(|id| person(id)).collect(),
            worldbuilders: book.worldbuilders.iter().map(|id| person(id)).collect(),
            asking: book.asking.iter().map(|id| person(id)).collect(),
            users,
        })
    }
}

/// Who wrote last, by world: the writes from and to, and by whom — for
/// telling each member what the others changed.
static WRITTEN: std::sync::OnceLock<Mutex<BTreeMap<String, std::collections::VecDeque<(u64, u64, String)>>>> = std::sync::OnceLock::new();
/// How many writes are noted per world.
const NOTED: usize = 500;

fn note_writes(world: &str, before: u64, after: u64, who: &str) {
    if after <= before {
        return;
    }
    let Ok(mut all) = WRITTEN.get_or_init(Default::default).lock() else { return };
    let noted = all.entry(world.to_string()).or_default();
    noted.push_back((before, after, who.to_string()));
    while noted.len() > NOTED {
        noted.pop_front();
    }
}

/// How far a world's writes are, and whether someone other than `who`
/// wrote since `since`. Writes noted for nobody are `host`'s own.
pub(crate) fn others_wrote(db: &WorldDb, world: &str, since: u64, who: &str, host: &str) -> (u64, bool) {
    let now = db.writes();
    // Opened anew since — a server started again, say.
    if now < since {
        return (now, true);
    }
    if now == since {
        return (now, false);
    }
    let Ok(all) = WRITTEN.get_or_init(Default::default).lock() else { return (now, true) };
    let noted = all.get(world).map(|n| n.iter().filter(|(_, to, _)| *to > since).collect::<Vec<_>>()).unwrap_or_default();
    if noted.iter().any(|(.., by)| by != who) {
        return (now, true);
    }
    let covered: u64 = noted.iter().map(|(from, to, _)| (*to).min(now).saturating_sub((*from).max(since))).sum();
    (now, covered < now - since && who != host)
}

/// A file of a world as named on the network: `world.sqlite`, or a picture
/// in `media/` — nothing else, nowhere else.
fn safe_file(file: &str) -> Answer<&str> {
    let plain = |name: &str| !name.is_empty() && !name.starts_with('.') && name.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    let ok = file == aw_store::db::FILE || file.strip_prefix("media/").is_some_and(plain);
    if ok { Ok(file) } else { Err(AppError("No such file in a world.".into())) }
}

/// The app's own keeper, by profile: the GM's device answering others.
static HERE: std::sync::OnceLock<Mutex<BTreeMap<PathBuf, Arc<Keeper>>>> = std::sync::OnceLock::new();

/// The keeper of the GM's worlds on this device. Its notes (who came, who
/// is turned away) lie in the profile's `host/`.
pub(crate) fn here(store: &Store) -> Answer<Arc<Keeper>> {
    let mut all = HERE.get_or_init(Default::default).lock().map_err(AppError::from)?;
    if let Some(keeper) = all.get(store.profile_dir()) {
        return Ok(keeper.clone());
    }
    let admin = store.load_identity()?.id().to_string();
    let mut keeper = Keeper::open(store.clone(), &store.profile_dir().join("host"), admin)?;
    keeper.in_app = true;
    let keeper = Arc::new(keeper);
    all.insert(store.profile_dir().to_path_buf(), keeper.clone());
    Ok(keeper)
}

impl Host for Keeper {
    fn offers(&self) -> Vec<Offer> {
        Keeper::offers(self, None).unwrap_or_default()
    }
    /// On a server every world: one not here yet may be brought.
    fn serves(&self, world: &str) -> bool {
        !self.in_app || world.is_empty() || self.keeps(world) || self.gone_to(world).is_some()
    }
    fn answer(&self, world: &str, who: &IdentityPublic, ask: Ask) -> Reply {
        let what = format!("{ask:?}").chars().take(40).collect::<String>();
        self.reply(world, who, ask).unwrap_or_else(|e| {
            let line = format!("{world}: {} asked {what}…: {}", who.name, e.0);
            crate::applog::line("answer", &line);
            eprintln!("{line}");
            Reply::Failed { error: e.0 }
        })
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
    use serde_json::json;

    /// Someone with a profile of their own.
    struct Someone {
        store: Store,
        me: Identity,
    }

    impl Someone {
        fn card(&self) -> IdentityPublic {
            self.me.public()
        }
    }

    struct Place(PathBuf);

    impl Drop for Place {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    fn place(tag: &str) -> Place {
        let dir = std::env::temp_dir().join(format!("aw-keeper-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        Place(dir)
    }

    fn someone(at: &Place, name: &str) -> Someone {
        let store = Store::new(at.0.join(name));
        let me = Identity::create(name).unwrap();
        store.save_identity(&me).unwrap();
        Someone { store, me }
    }

    /// A server whose first admin is `admin`.
    fn server(at: &Place, admin: &Someone) -> Keeper {
        let store = Store::new(at.0.join("server"));
        store.save_identity(&Identity::create("Server").unwrap()).unwrap();
        Keeper::open(store, &at.0.join("server-data"), admin.me.id().to_string()).unwrap()
    }

    fn call(keeper: &Keeper, world: &str, who: &Someone, command: &str, args: serde_json::Value) -> Result<serde_json::Value, String> {
        match keeper.answer(world, &who.card(), Ask::Call { command: command.into(), args: args.to_string() }) {
            Reply::Called { value: Some(v), .. } => Ok(serde_json::from_str(&v).unwrap()),
            Reply::Called { error, .. } => Err(error.unwrap_or_default()),
            other => Err(format!("{other:?}")),
        }
    }

    /// Brings a GM's world to the server, as the app does.
    fn bring_world(keeper: &Keeper, gm: &Someone, world: &str) -> Reply {
        use base64::Engine;
        let out = gm.store.private_world_dir(world).join("bringing");
        std::fs::create_dir_all(&out).unwrap();
        gm.store.db(world).unwrap().copy_to(&out.join(aw_store::db::FILE)).unwrap();
        let mut files = vec![(aw_store::db::FILE.to_string(), out.join(aw_store::db::FILE))];
        for e in std::fs::read_dir(gm.store.world_dir(world).join("media")).into_iter().flatten().flatten() {
            files.push((format!("media/{}", e.file_name().to_string_lossy()), e.path()));
        }
        for (file, path) in files {
            let data = base64::engine::general_purpose::STANDARD.encode(std::fs::read(path).unwrap());
            match keeper.answer(world, &gm.card(), Ask::Put { file, at: 0, data }) {
                Reply::Taken => {}
                other => return other,
            }
        }
        keeper.answer(world, &gm.card(), Ask::Bring { name: "Eldermere".into() })
    }

    fn new_world_of(gm: &Someone) -> String {
        new_world(&gm.store, &gm.me, "Eldermere".into()).unwrap().id
    }

    fn layer(text: &str, policy: &str) -> serde_json::Value {
        json!({ "title": "", "text": text, "policy": policy, "level": "", "section": "", "edit": "", "fields": null })
    }

    #[test]
    fn a_player_works_in_a_world_on_a_server_as_far_as_it_lets_them() {
        let at = place("member");
        let (marla, olaf, eve) = (someone(&at, "Marla"), someone(&at, "Olaf"), someone(&at, "Eve"));
        let keeper = server(&at, &marla);
        let world = new_world_of(&marla);
        assert_eq!(bring_world(&keeper, &marla, &world), Reply::Joined);
        // Its GM works in it there, in their own name.
        let info = call(&keeper, &world, &marla, "world_connect", json!({})).unwrap();
        assert_eq!(info["is_gm"], true);
        let tower = call(&keeper, &world, &marla, "article_create", json!({
            "articleType": "", "slug": "tower", "name": "The Tower", "visibility": "all",
            "layers": [layer("Everyone sees the tower.", "all"), layer("A dragon sleeps below.", "gm")],
        })).unwrap();
        assert!(tower.as_str().is_some_and(|t| t.starts_with("art:")));

        // A stranger asks to join; the GM lets them in.
        assert_eq!(call(&keeper, &world, &olaf, "view", json!({})).unwrap_err(), "NotMember");
        assert_eq!(keeper.answer(&world, &olaf.card(), Ask::Join), Reply::Requested);
        let asking = call(&keeper, &world, &marla, "join_requests", json!({})).unwrap();
        assert_eq!(asking[0]["name"], "Olaf");
        call(&keeper, &world, &marla, "join_answer", json!({ "id": olaf.me.id().to_string(), "accept": true })).unwrap();
        assert_eq!(keeper.answer(&world, &olaf.card(), Ask::Join), Reply::Joined);

        // The player sees what is there for players — not what is the GM's.
        let seen = call(&keeper, &world, &olaf, "view", json!({})).unwrap().to_string();
        assert!(seen.contains("Everyone sees the tower."), "{seen}");
        assert!(!seen.contains("A dragon sleeps below."), "{seen}");
        let info = call(&keeper, &world, &olaf, "world_connect", json!({})).unwrap();
        assert_eq!(info["role"], "player");
        assert!(call(&keeper, &world, &olaf, "world_icon_set", json!({ "icon": "🐉" })).is_err(), "only the GM");
        // Someone else gets nothing.
        assert_eq!(call(&keeper, &world, &eve, "view", json!({})).unwrap_err(), "NotMember");
        assert!(!matches!(keeper.answer(&world, &eve.card(), Ask::Files), Reply::Files { .. }));

        // What the player writes, the GM is told of; their own writing not.
        let synced = call(&keeper, &world, &marla, "world_sync", json!({ "since": 0 })).unwrap();
        let since = synced["writes"].as_u64().unwrap();
        call(&keeper, &world, &marla, "world_icon_set", json!({ "icon": "🏰" })).unwrap();
        let mine = call(&keeper, &world, &marla, "world_sync", json!({ "since": since })).unwrap();
        assert_eq!(mine["changed"], false);
        let since = mine["writes"].as_u64().unwrap();
        call(&keeper, &world, &olaf, "member_colour_set", json!({ "colour": "#336699" })).unwrap();
        assert_eq!(call(&keeper, &world, &marla, "world_sync", json!({ "since": since })).unwrap()["changed"], true);
    }

    #[test]
    fn its_gm_takes_a_world_back_whole() {
        let at = place("fetch");
        let marla = someone(&at, "Marla");
        let keeper = server(&at, &marla);
        let world = new_world_of(&marla);
        assert_eq!(bring_world(&keeper, &marla, &world), Reply::Joined);
        assert_eq!(bring_world(&keeper, &marla, &world), Reply::Failed { error: "This world is here already.".into() });
        let Reply::Files { files } = keeper.answer(&world, &marla.card(), Ask::Files) else { panic!("its GM gets the files") };
        let (name, size) = &files[0];
        assert_eq!(name, aw_store::db::FILE);
        let Reply::Piece { data } = keeper.answer(&world, &marla.card(), Ask::Get { file: name.clone(), at: 0 }) else { panic!("a piece") };
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD.decode(data).unwrap();
        assert_eq!(bytes.len() as u64, *size);
        let back = at.0.join("back");
        std::fs::create_dir_all(&back).unwrap();
        std::fs::write(back.join(aw_store::db::FILE), bytes).unwrap();
        assert_eq!(WorldDb::open(&back).unwrap().get("id").unwrap().as_deref(), Some(world.as_str()));
        // Nothing but a world's own files.
        assert!(matches!(keeper.answer(&world, &marla.card(), Ask::Get { file: "../server.json".into(), at: 0 }), Reply::Failed { .. }));
        // Its GM takes it away from there.
        assert!(matches!(keeper.answer("", &marla.card(), Ask::Manage { order: Some(Order::Delete { world: world.clone() }) }), Reply::Managed { .. }));
        assert!(!keeper.store.holds(&world));
    }

    #[test]
    fn only_whom_the_admin_lets_brings_worlds() {
        let at = place("bring");
        let (marla, olaf) = (someone(&at, "Marla"), someone(&at, "Olaf"));
        let keeper = server(&at, &marla);
        let world = new_world_of(&olaf);
        assert_eq!(bring_world(&keeper, &olaf, &world), Reply::Requested);
        let Reply::Managed { state } = keeper.answer("", &marla.card(), Ask::Manage { order: None }) else { panic!("the admin manages") };
        assert_eq!(state.asking[0].name, "Olaf");
        assert_eq!(keeper.answer("", &olaf.card(), Ask::Manage { order: None }), Reply::NotOffered);
        keeper.answer("", &marla.card(), Ask::Manage { order: Some(Order::Allow { id: olaf.me.id().to_string() }) });
        assert_eq!(bring_world(&keeper, &olaf, &world), Reply::Joined);
        // Closed, nobody gets it — its GM neither; deleted only once closed.
        assert!(matches!(keeper.answer("", &marla.card(), Ask::Manage { order: Some(Order::Delete { world: world.clone() }) }), Reply::Failed { .. }));
        keeper.answer("", &marla.card(), Ask::Manage { order: Some(Order::Close { world: world.clone() }) });
        assert_eq!(keeper.answer(&world, &olaf.card(), Ask::Join), Reply::NotOffered);
        assert!(matches!(keeper.answer("", &marla.card(), Ask::Manage { order: Some(Order::Delete { world: world.clone() }) }), Reply::Managed { .. }));
        assert!(!keeper.store.holds(&world));
    }

    #[test]
    fn a_world_file_is_named_plainly() {
        assert!(safe_file("world.sqlite").is_ok());
        assert!(safe_file("media/abc_12.preview.webp").is_ok());
        for bad in ["../x", "media/../world.sqlite", "media/.hidden", "media/a/b", "other.db", "media/"] {
            assert!(safe_file(bad).is_err(), "{bad}");
        }
    }
}
