//! aw-server — the door keeper of a group's worlds: on a NAS, a small
//! server, in a container. Always on, so players work when the GM's device
//! is off.
//!
//! It holds no world's keys. It knows who is who — everyone proves their
//! identity when they connect — and lets each one in where the world itself
//! says they belong: its GM, and the members the GM granted. What it passes
//! on of an encrypted world only the members' own keys open; their apps keep
//! it while they are online where the GM keeps players from copies. An
//! unencrypted world it reads, as anyone holding it could, and lets each
//! member see and change what their view and rights say (see `host`) —
//! writing in their name, as itself, which the GM vouches for
//! (`Item::Hosts`).
//!
//! Its admin, a GM named when it starts (`--admin`), manages it from the
//! app: which GMs may bring worlds, which worlds are closed, who is turned
//! away. Of other GMs' worlds the admin sees name, size and form — no more
//! than the server itself can read.
//!
//! ```text
//! <data>/profile/      the server's own identity and endpoint key
//! <data>/worlds/       the worlds, as in the app
//! <data>/server.json   GMs, closed worlds, who came, who is turned away
//! <data>/host-code.txt what GMs and players enter
//! ```

use super::*;
use crate::store::JoinRequest;
use aw_sync::world::{Ask, HeldWorld, Host, Offer, Order, Person, Reach, Reply, Server, ServerState, WorldNet};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const USAGE: &str = "aw-server — keeps the worlds of AnotherWorldbuilder for groups

  --admin <id>        the GM who manages it: their id, id:…  (AW_ADMIN)
  --data <dir>        where everything lies                  (AW_DATA, /data)
  --port <port>       port to answer on: UDP, and TCP for its code
                                                             (AW_PORT, 7447)
  --name <name>       how it is called                       (AW_NAME, aw-server)
  --address <h:p>     how it is reached, for its code; again for more
                                                             (AW_ADDRESS, comma-separated)
  --lan               be found on the local network          (AW_LAN=1)
  how connections go, for finding faults                     (AW_LOG=1)
";

/// Someone seen this long ago is noted again on disk.
const NOTE_AGAIN: u64 = 10 * 60;

struct Options {
    admin: String,
    data: PathBuf,
    port: u16,
    name: String,
    addresses: Vec<String>,
    lan: bool,
}

fn options() -> Result<Options, String> {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
    let mut o = Options {
        admin: env("AW_ADMIN").unwrap_or_default().trim().to_string(),
        data: env("AW_DATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/data")),
        port: env("AW_PORT").map(|p| p.trim().parse().map_err(|_| "AW_PORT is no port")).transpose()?.unwrap_or(7447),
        name: env("AW_NAME").unwrap_or_else(|| "aw-server".into()),
        addresses: env("AW_ADDRESS").map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default(),
        lan: env("AW_LAN").is_some_and(|v| matches!(v.as_str(), "1" | "true" | "yes")),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--admin" => o.admin = value()?.trim().to_string(),
            "--data" => o.data = PathBuf::from(value()?),
            "--port" => o.port = value()?.parse().map_err(|_| "--port needs a port")?,
            "--name" => o.name = value()?,
            "--address" => o.addresses.push(value()?),
            "--lan" => o.lan = true,
            "-h" | "--help" => return Err(USAGE.into()),
            _ => return Err(format!("unknown: {arg}\n\n{USAGE}")),
        }
    }
    // The 16 characters alone are just as clear.
    if !o.admin.starts_with("id:") {
        o.admin = format!("id:{}", o.admin);
    }
    o.admin = o.admin.to_ascii_lowercase();
    if !is_id(&o.admin) {
        return Err(format!("Name the admin by their id — id: and 16 characters, under Profile in the app.\n\n{USAGE}"));
    }
    Ok(o)
}

fn is_id(id: &str) -> bool {
    id.strip_prefix("id:").is_some_and(|body| body.len() == 16 && body.chars().all(|c| c.is_ascii_hexdigit()))
}

pub fn main() {
    let o = match options() {
        Ok(o) => o,
        Err(text) => {
            eprintln!("{text}");
            std::process::exit(2);
        }
    };
    // AW_LOG=1: how connections go; anything else is a filter of its own.
    if let Some(log) = std::env::var("AW_LOG").ok().filter(|v| !v.trim().is_empty() && v != "0") {
        let filter = if log == "1" { "warn,iroh=debug,noq=debug,noq_proto=debug,aw_sync=debug".to_string() } else { log };
        let _ = tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::new(filter)).with_ansi(false).try_init();
    }
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("aw-server: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = runtime.block_on(run(o)) {
        eprintln!("aw-server: {}", e.0);
        std::process::exit(1);
    }
}

async fn run(o: Options) -> Answer<()> {
    let (profile, worlds) = (o.data.join("profile"), o.data.join("worlds"));
    std::fs::create_dir_all(&profile)?;
    std::fs::create_dir_all(&worlds)?;
    let store = profile::use_folders(&profile, &worlds, &o.data.join("locks"))?;
    // Its own identity: what it writes in members' names, it signs as itself.
    if !store.has_identity() {
        store.save_identity(&Identity::create(&o.name).map_err(|e| AppError(e.to_string()))?)?;
    }
    let keeper = Arc::new(Keeper::open(store.clone(), &o.data, o.admin.clone())?);
    let reach = Reach {
        lan: o.lan,
        internet: true,
        stun: store::NetworkSettings::default().stun,
        // The same endpoint every start: its code stays valid.
        secret: Some(network::endpoint_secret(&store)?),
        port: Some(o.port),
        port_required: true,
        only: home_ip(&o.addresses),
    };
    let net = WorldNet::start(keeper.clone(), reach).await.map_err(|e| AppError(format!("The network could not start: {e}")))?;

    let mut addresses = o.addresses.clone();
    if addresses.is_empty() {
        addresses.extend(net.reachable().iter().map(|a| a.to_string()));
        addresses.extend(net.home_address().map(|a| a.to_string()).filter(|a| !addresses.contains(a)));
    }
    let code = Server { endpoint: net.id(), addrs: addresses }.code();
    aw_store::durable::write(&o.data.join("host-code.txt"), format!("{code}\n").as_bytes())?;
    println!("aw-server on UDP port {} — admin {}", o.port, o.admin);
    println!("code: {code}");
    // Its address alone is enough then: the app asks for the code on TCP, same port.
    let told = code.clone();
    let port = o.port;
    tokio::spawn(async move {
        if let Err(e) = aw_sync::world::tell_code(port, told).await {
            eprintln!("TCP port {port}: {e} — the full code is needed then");
        }
    });
    stopped().await;
    drop(net);
    Ok(())
}

/// What the server keeps of whom: who may bring worlds, which are closed,
/// who came and who is turned away.
#[derive(Default, Serialize, Deserialize)]
struct Book {
    /// Besides the admin.
    gms: Vec<String>,
    /// GMs who would bring a world.
    asking: Vec<String>,
    /// By world: its name, for when it is gone.
    closed: BTreeMap<String, String>,
    blocked: Vec<String>,
    /// By id: name, and when last seen.
    users: BTreeMap<String, (String, u64)>,
}

struct Keeper {
    store: Store,
    admin: String,
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
fn writes(ask: &Ask) -> bool {
    match ask {
        Ask::Swap { give, .. } => !give.is_empty(),
        Ask::Give { records } => !records.is_empty(),
        Ask::Parts { give, .. } => !give.is_empty(),
        _ => false,
    }
}

impl Keeper {
    fn open(store: Store, data: &Path, admin: String) -> Answer<Self> {
        let path = data.join("server.json");
        let book = match std::fs::read(&path) {
            Ok(raw) => serde_json::from_slice(&raw)?,
            Err(_) => Book::default(),
        };
        let me = store.load_identity()?.public();
        Ok(Keeper { store, admin, me, book: Mutex::new(book), path, writing: Mutex::default(), known: Mutex::default() })
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
        Ok(id == self.admin || self.book()?.gms.iter().any(|g| g == id))
    }

    /// The worlds held and open: name, icon, GM.
    fn offers(&self) -> Answer<Vec<Offer>> {
        let closed: Vec<String> = self.book()?.closed.keys().cloned().collect();
        let mut out = Vec::new();
        for meta in self.store.list_worlds()? {
            let world = meta.public.world_id.to_string();
            if closed.contains(&world) {
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
        match ask {
            Ask::Worlds => {
                let asked = self.book()?.asking.contains(&id);
                return Ok(Reply::Worlds {
                    worlds: self.offers()?,
                    host: Some(self.me.clone()),
                    admin: id == self.admin,
                    may_bring: self.may_bring(&id)?,
                    asked,
                });
            }
            // Its GM takes their own world away — to another server, say; the admin too,
            // without closing it first.
            Ask::Manage { order: Some(Order::Delete { world }) } if self.store.holds(&world) && self.store.gm_of(&world)?.as_deref() == Some(id.as_str()) => {
                self.store.delete_world(&world)?;
                println!("{world}: taken away by {}", who.name);
                return Ok(Reply::Managed { state: ServerState::default() });
            }
            Ask::Manage { order } if id == self.admin => {
                if let Some(order) = order {
                    self.carry_out(order)?;
                }
                return Ok(Reply::Managed { state: self.state()? });
            }
            Ask::Manage { .. } => return Ok(Reply::NotOffered),
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
        // One write at a time, and nobody waits behind it.
        let Some(_writing) = self.claim(world, writes(&ask))? else {
            return Ok(Reply::Busy);
        };
        if !self.store.holds(world) {
            return self.bring(world, who, ask);
        }
        let store = &self.store;
        // Its GM: the whole of it, and who asked to join.
        if store.gm_of(world)?.as_deref() == Some(id.as_str()) {
            let mut reply = network::answer_member(store, world, &id, ask)?;
            if let Reply::Digest { requests, .. } = &mut reply {
                *requests = self.requests(world)?;
            }
            return Ok(reply);
        }
        // A member by the grant the GM wrote; anyone else may ask.
        if !store.has_grant(world, &id)? {
            return Ok(match ask {
                Ask::Join => {
                    let mut requests = store.load_requests(world)?;
                    if who.is_genuine() && !requests.iter().any(|r| r.identity.id == who.id) {
                        requests.push(JoinRequest { identity: who.clone(), at: network::now() });
                        store.save_requests(world, &requests)?;
                    }
                    Reply::Requested
                }
                _ => Reply::NotMember,
            });
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

impl Host for Keeper {
    fn offers(&self) -> Vec<Offer> {
        Keeper::offers(self).unwrap_or_default()
    }
    /// Every world: one not here yet may be brought.
    fn serves(&self, _world: &str) -> bool {
        true
    }
    fn answer(&self, world: &str, who: &IdentityPublic, ask: Ask) -> Reply {
        self.reply(world, who, ask).unwrap_or(Reply::NotOffered)
    }
}

/// The one address it answers at: the first of its own addresses it was
/// told that belongs to this machine, otherwise its home network's.
fn home_ip(addresses: &[String]) -> Option<std::net::Ipv4Addr> {
    let own = |ip: &std::net::Ipv4Addr| std::net::UdpSocket::bind((*ip, 0)).is_ok();
    addresses
        .iter()
        .filter_map(|a| a.parse::<std::net::SocketAddr>().ok())
        .find_map(|a| match a.ip() {
            std::net::IpAddr::V4(ip) if !ip.is_loopback() && own(&ip) => Some(ip),
            _ => None,
        })
        .or_else(aw_sync::world::own_ipv4)
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

/// Ctrl+C — or, in a container, the stop it is sent.
async fn stopped() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        if let Ok(mut term) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = term.recv() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use aw_model::Level;
    use aw_store::journal::{Access, Plain};
    use store::NewPart;

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
        assert_eq!(keeper.offers().unwrap().iter().map(|o| o.gm.clone()).collect::<Vec<_>>(), vec!["Marla"]);

        // Being written: asked about meanwhile, the server says so at once,
        // and lists it as it last knew it.
        let writing = keeper.claim(&eldermere, true).unwrap().unwrap();
        assert_eq!(keeper.answer(&eldermere, &marla.me.public(), Ask::Digest { presence: None, rolls: Vec::new() }), Reply::Busy);
        let Reply::Worlds { worlds, .. } = keeper.answer("", &marla.me.public(), Ask::Worlds) else { panic!("the worlds") };
        assert_eq!(worlds.iter().map(|o| o.gm.clone()).collect::<Vec<_>>(), vec!["Marla"]);
        assert!(!state(&keeper, &marla).worlds.is_empty());
        drop(writing);
        assert!(matches!(keeper.answer(&eldermere, &marla.me.public(), Ask::Digest { presence: None, rolls: Vec::new() }), Reply::Digest { .. }));
    }

    #[test]
    fn the_admin_lets_gms_in_and_closes_and_nobody_brings_what_is_not_theirs() {
        let dir = tempfile::tempdir().unwrap();
        let (marla, olaf, eve) = (someone(dir.path(), "Marla"), someone(dir.path(), "Olaf"), someone(dir.path(), "Eve"));
        let keeper = keeper(dir.path(), &marla);
        let (eldermere, _) = world(&marla, "Eldermere");
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

        // Asked through the server, decided by the GM.
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
        assert_eq!(swap(&keeper, &eve, &world), Reply::NotMember);

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
