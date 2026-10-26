//! The network: this app's own server, offered to the members of its worlds,
//! and the servers elsewhere it reaches.
//!
//! A world lies in one place: here, or on a server — an `aw-server`, or
//! another GM's app. One on a server this device holds nothing of: each
//! command for it is asked of the server (`world_call`), which does it in
//! this member's name. Whoever works in a world here, from afar, asks this
//! app's own server (`keeper::here`) the same way.

use super::*;
use aw_sync::world::{Ask, Host, Offer, Reach, Reply, Server, WorldNet};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use store::{NetworkSettings, RemoteWorld};

static NET: OnceLock<Mutex<Option<Arc<WorldNet>>>> = OnceLock::new();

pub(crate) fn running() -> Option<Arc<WorldNet>> {
    NET.get_or_init(|| Mutex::new(None)).lock().ok()?.clone()
}

/// Said of a world whose server is not reached — the interface pauses it.
pub(crate) const UNREACHED: &str = "[online] Its server is not reached.";

fn off() -> AppError {
    AppError(format!("{UNREACHED} The network is switched off."))
}

/// The host side: the app's own keeper for whichever profile is in use now.
struct DesktopHost;

impl Host for DesktopHost {
    /// What is told on the local network: the worlds on the list by name;
    /// the others offered here without one — their members find the GM's
    /// device by the id alone.
    fn offers(&self) -> Vec<Offer> {
        let Some(store) = profile::store_now() else { return Vec::new() };
        let Ok(settings) = store.load_network() else { return Vec::new() };
        let Ok(keeper) = keeper::here(&store) else { return Vec::new() };
        let listed: Vec<Offer> = keeper.offers(None).unwrap_or_default();
        let unnamed = |world: &String| Offer { world: world.clone(), name: String::new(), icon: String::new(), gm: String::new() };
        // Those gone to a server too: their members ask here, and are sent there.
        settings
            .shared
            .iter()
            .filter(|w| store.holds(w) && store.is_gm(w))
            .map(|w| listed.iter().find(|o| &o.world == w).cloned().unwrap_or_else(|| unnamed(w)))
            .chain(keeper.gone().iter().map(unnamed))
            .collect()
    }
    fn serves(&self, world: &str) -> bool {
        profile::store_now().and_then(|store| keeper::here(&store).ok()).is_some_and(|k| k.serves(world))
    }
    fn answer(&self, world: &str, who: &IdentityPublic, ask: Ask) -> Reply {
        match profile::store_now().map(|store| keeper::here(&store)) {
            Some(Ok(keeper)) => keeper.answer(world, who, ask),
            _ => Reply::NotOffered,
        }
    }
}

/// This device's endpoint key — made once and kept in the profile, so its
/// id, and with it its code, stays the same.
pub(crate) fn endpoint_secret(store: &Store) -> Answer<[u8; 32]> {
    let path = store.profile_dir().join("endpoint.key");
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(Ok(bytes)) = hex::decode(text.trim()).map(<[u8; 32]>::try_from) {
            return Ok(bytes);
        }
    }
    let bytes = aw_crypto::random_bytes::<32>().map_err(|e| AppError(e.to_string()))?;
    aw_store::durable::write(&path, hex::encode(bytes).as_bytes())?;
    Ok(bytes)
}

/// Telling this device's code on TCP — stopped when the network starts anew.
static TELLING: OnceLock<Mutex<Option<tokio::task::JoinHandle<()>>>> = OnceLock::new();

/// Whether anything needs the network: offering worlds, or working in one
/// on a server.
fn any_network(settings: &NetworkSettings) -> bool {
    settings.enabled || settings.internet || !settings.servers.is_empty() || !settings.remote.is_empty()
}

/// Starts the network as the settings say — or stops it.
pub(crate) async fn switch(store: &Store, settings: &NetworkSettings) -> Answer<()> {
    let slot = NET.get_or_init(|| Mutex::new(None));
    // Dropping it takes the announcements back and closes the endpoint.
    slot.lock().map_err(AppError::from)?.take();
    let telling = TELLING.get_or_init(Default::default);
    if let Some(told) = telling.lock().map_err(AppError::from)?.take() {
        told.abort();
    }
    if !any_network(settings) {
        return Ok(());
    }
    let reach = Reach {
        lan: settings.enabled,
        internet: settings.internet,
        stun: settings.stun.clone(),
        // The same endpoint every start: a code stays valid.
        secret: Some(endpoint_secret(store)?),
        port: Some(settings.port),
        port_required: false,
        only: None,
    };
    let net = WorldNet::start(Arc::new(DesktopHost), reach).await.map_err(|e| AppError(format!("The network could not start: {e}")))?;
    // Its address is enough: the code is told on TCP at the port, as a
    // server does.
    if settings.enabled || settings.internet {
        let port = settings.port;
        let mut addrs: Vec<String> = net.reachable().iter().map(|a| a.to_string()).collect();
        addrs.extend(net.home_address().map(|a| a.to_string()));
        if !addrs.is_empty() {
            let code = Server { endpoint: net.id(), addrs }.code();
            *telling.lock().map_err(AppError::from)? = Some(tokio::spawn(async move {
                let _ = aw_sync::world::tell_code(port, code).await;
            }));
        }
    }
    *slot.lock().map_err(AppError::from)? = Some(Arc::new(net));
    Ok(())
}

#[cfg(feature = "gui")]
/// Called on start, and once a place for the data is chosen: only if
/// switched on.
pub fn start(app: &tauri::AppHandle) -> Answer<()> {
    let Ok(store) = store(app) else { return Ok(()) };
    let settings = store.load_network()?;
    if !any_network(&settings) {
        return Ok(());
    }
    tauri::async_runtime::spawn(async move {
        if let Err(e) = switch(&store, &settings).await {
            crate::applog::line("network", &e.0);
        }
    });
    Ok(())
}

/// Tells the local network anew what is offered here.
pub(crate) fn republish() {
    if let Some(net) = running() {
        let _ = net.publish();
    }
}

/// The network as it runs — started first where it is off but needed now.
async fn net(store: &Store) -> Answer<Arc<WorldNet>> {
    if let Some(net) = running() {
        return Ok(net);
    }
    let settings = store.load_network()?;
    if !any_network(&settings) {
        return Err(off());
    }
    switch(store, &settings).await?;
    running().ok_or_else(off)
}

/// Asks a server, by its code — about a world, or none (`""`). A device
/// found on the local network is asked where it is now: an app's port may
/// change when it starts anew.
async fn ask(store: &Store, code: &str, world: &str, what: Ask) -> Answer<Reply> {
    let mut server = Server::parse(code).map_err(|e| AppError(e.to_string()))?;
    let net = net(store).await?;
    if let Some((_, here)) = net.found_hosts().into_iter().find(|(_, s)| s.endpoint == server.endpoint) {
        let old: Vec<String> = server.addrs.into_iter().filter(|a| !here.addrs.contains(a)).collect();
        server.addrs = here.addrs.into_iter().chain(old).collect();
    }
    let me = store.load_identity()?;
    let reply = net.ask_server(&server, world, &me, what).await.map_err(|e| AppError(format!("{UNREACHED} {e}")))?;
    match reply {
        Reply::Failed { error } => Err(AppError(error)),
        Reply::Busy => Err(AppError("The world is being moved right now. Try again in a moment.".into())),
        reply => Ok(reply),
    }
}

// --- Worlds on a server ------------------------------------------------------

/// The worlds being worked in on their servers now, with how far their
/// writes were when last asked.
static OPEN: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
/// What their servers last said of them, as the interface gets it.
static TOLD: OnceLock<Mutex<HashMap<String, WorldInfo>>> = OnceLock::new();
/// Those whose server was not reached last time.
static UNREACHED_NOW: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn open_now() -> std::sync::MutexGuard<'static, HashMap<String, u64>> {
    OPEN.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

fn told() -> std::sync::MutexGuard<'static, HashMap<String, WorldInfo>> {
    TOLD.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

/// A world on a server this device works in — none: one here, or unknown.
pub(crate) fn remote_of(store: &Store, world: &str) -> Option<RemoteWorld> {
    store.load_network().ok()?.remote.get(world).filter(|r| !r.asked).cloned()
}

/// A world as its server told it, made this device's: on that server.
fn as_here(mut info: WorldInfo, code: &str) -> WorldInfo {
    info.online = true;
    info.server = code.to_string();
    info.server_id = code.split('@').next().unwrap_or_default().to_string();
    info.folder = String::new();
    info.offered = false;
    info
}

/// A world on a server not opened yet: what this device knows of it.
fn sketch(world: &str, remote: &RemoteWorld) -> WorldInfo {
    WorldInfo {
        id: world.to_string(),
        name: remote.name.clone(),
        icon: remote.icon.clone(),
        folder: String::new(),
        role: "player".into(),
        is_gm: false,
        attributes: Vec::new(),
        members: Vec::new(),
        article_count: 0,
        home: String::new(),
        requests: 0,
        invited: Vec::new(),
        takeovers: Default::default(),
        colours: Default::default(),
        moderated: false,
        waiting: 0,
        version_limit: 0,
        online: true,
        server: remote.server.clone(),
        server_id: remote.server.split('@').next().unwrap_or_default().to_string(),
        offered: false,
        dice: Vec::new(),
        listed: false,
        calendar: Default::default(),
        supporters: Vec::new(),
        supporter: false,
        roles: Vec::new(),
    }
}

/// The worlds on servers, for the list of worlds: as told last, or as known.
pub(crate) fn remote_worlds(store: &Store) -> Answer<Vec<WorldInfo>> {
    let told = told();
    Ok(store
        .load_network()?
        .remote
        .iter()
        .filter(|(_, r)| !r.asked)
        .map(|(world, remote)| told.get(world).map(|i| as_here(clone_info(i), &remote.server)).unwrap_or_else(|| sketch(world, remote)))
        .collect())
}

fn clone_info(info: &WorldInfo) -> WorldInfo {
    serde_json::from_value(serde_json::to_value(info).unwrap_or_default()).unwrap_or_else(|_| sketch(&info.id, &RemoteWorld::default()))
}

/// Notes a world on a server — or forgets it (`None`).
fn note_remote(store: &Store, world: &str, remote: Option<RemoteWorld>) -> Answer<()> {
    let mut settings = store.load_network()?;
    match remote {
        Some(r) => settings.remote.insert(world.to_string(), r),
        None => {
            told().remove(world);
            settings.remote.remove(world)
        }
    };
    store.save_network(&settings)?;
    Ok(())
}

/// Does a command for a world on a server: asked of the server, done
/// there in this member's name. What names a file here travels as its
/// content.
pub(crate) async fn call(store: &Store, world: &str, command: &str, args: serde_json::Value) -> Answer<serde_json::Value> {
    use base64::Engine;
    let remote = remote_of(store, world).ok_or_else(|| AppError("This world is not on a server.".into()))?;
    let (command, args) = match command {
        "image_upload" => {
            let path: String = serde_json::from_value(args.get("path").cloned().unwrap_or_default())?;
            if std::fs::metadata(&path)?.len() > media::MAX_UPLOAD {
                return Err(AppError("This file is too large.".into()));
            }
            let data = base64::engine::general_purpose::STANDARD.encode(std::fs::read(&path)?);
            ("image_put", serde_json::json!({ "name": args.get("name"), "folder": args.get("folder"), "data": data }))
        }
        "member_add" => {
            let path: String = serde_json::from_value(args.get("path").cloned().unwrap_or_default())?;
            let card: IdentityPublic = serde_json::from_slice(&std::fs::read(&path)?)?;
            ("member_admit", serde_json::json!({ "card": card, "attributes": args.get("attributes") }))
        }
        _ => (command, args),
    };
    let what = Ask::Call { command: command.to_string(), args: args.to_string() };
    let mut reply = ask(store, &remote.server, world, what.clone()).await;
    // Gone to another server: followed there.
    let mut remote = remote;
    if let Ok(Reply::Moved { server }) = &reply {
        remote = RemoteWorld { server: server.clone(), ..remote };
        note_remote(store, world, Some(remote.clone()))?;
        reply = ask(store, &remote.server, world, what).await;
    }
    let reply = match reply {
        Err(e) if e.0.starts_with("[online]") => {
            unreached(world, true);
            return Err(e);
        }
        reply => reply?,
    };
    unreached(world, false);
    let value: serde_json::Value = match reply {
        Reply::Called { value: Some(value), .. } => serde_json::from_str(&value)?,
        Reply::Called { error, .. } => return Err(AppError(error.unwrap_or_else(|| "The server did not say why.".into()))),
        Reply::NotMember => return Err(AppError("[online] You are not a member of this world (any more).".into())),
        _ => return Err(AppError("[online] The server does not offer this world.".into())),
    };
    // A world as the interface gets it: on its server.
    if let Ok(info) = serde_json::from_value::<WorldInfo>(value.clone()) {
        if info.id == world {
            told().insert(world.to_string(), clone_info(&info));
            return Ok(serde_json::to_value(as_here(info, &remote.server))?);
        }
    }
    Ok(value)
}

/// Notes whether a world's server answers — and tells the interface when
/// that changes.
fn unreached(world: &str, now: bool) {
    let mut all = UNREACHED_NOW.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    let changed = if now { all.insert(world.to_string()) } else { all.remove(world) };
    if changed && !now {
        crate::tell_windows("online-available", serde_json::json!({ "world": world }));
    }
}

#[cfg(feature = "gui")]
/// A command for a world on a server — see `call`.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub async fn world_call(app: tauri::AppHandle, world: String, command: String, args: serde_json::Value) -> Answer<serde_json::Value> {
    let store = store(&app)?;
    let label = command.clone();
    let result = call(&store, &world, &command, args).await;
    if let Err(e) = &result {
        crate::applog::line("command", &format!("{world}: {label} failed: {}", e.0));
    }
    result
}

#[cfg(feature = "gui")]
/// Opens a world to work in: one here at once; one on a server once the
/// server lets this member in.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub async fn world_connect(app: tauri::AppHandle, world: String) -> Answer<WorldInfo> {
    connect(&store(&app)?, &world).await
}

/// `world_connect` for whom the store is given.
pub(crate) async fn connect(store: &Store, world: &str) -> Answer<WorldInfo> {
    let (store, world) = (store, world.to_string());
    if store.holds(&world) {
        return world_now(store, &world);
    }
    let value = call(store, &world, "world_sync", serde_json::json!({ "since": 0 })).await?;
    let info: WorldInfo = serde_json::from_value(value.get("world").cloned().unwrap_or_default())?;
    open_now().insert(world.clone(), value.get("writes").and_then(|w| w.as_u64()).unwrap_or(0));
    told().insert(world.clone(), clone_info(&info));
    let server = remote_of(store, &world).map(|r| r.server).unwrap_or_default();
    // Its name and icon as they are now, for the list.
    if let Some(mut remote) = remote_of(store, &world) {
        if remote.name != info.name || remote.icon != info.icon {
            remote.name = info.name.clone();
            remote.icon = info.icon.clone();
            note_remote(store, &world, Some(remote))?;
        }
    }
    Ok(as_here(info, &server))
}

#[cfg(feature = "gui")]
/// Leaves a world worked in: its server is no longer asked what changed.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn world_disconnect(world: String) -> Answer<()> {
    open_now().remove(&world);
    Ok(())
}

// --- Keeping up --------------------------------------------------------------

/// What one round of keeping up brought.
#[derive(Serialize, Default)]
pub struct SyncOutcome {
    /// Worlds asked to join that let this member in now.
    pub joined: Vec<WorldInfo>,
    /// Worlds others changed — on their server, or here from afar.
    pub updated: Vec<String>,
}

/// How far each world here was when its GM last looked — for what members
/// changed from afar.
static SEEN_HERE: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();

#[cfg(feature = "gui")]
/// One round of keeping up: what others changed in the worlds open now,
/// whether a world asked to join lets this member in, what servers say.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub async fn network_sync(app: tauri::AppHandle) -> Answer<SyncOutcome> {
    let store = store(&app)?;
    let mut out = SyncOutcome::default();
    let me = store.load_identity()?.id().to_string();
    // Worlds here that members change from afar.
    let shared = store.load_network()?.shared;
    for world in shared.iter().filter(|w| store.holds(w)) {
        let Ok(db) = store.db(world) else { continue };
        let since = SEEN_HERE.get_or_init(Default::default).lock().map_err(AppError::from)?.get(world).copied().unwrap_or(db.writes());
        let (writes, changed) = keeper::others_wrote(&db, world, since, &me, &me);
        SEEN_HERE.get_or_init(Default::default).lock().map_err(AppError::from)?.insert(world.clone(), writes);
        if changed {
            out.updated.push(world.clone());
        }
    }
    if running().is_none() && !any_network(&store.load_network()?) {
        return Ok(out);
    }
    // Worlds on servers, open now.
    let open: Vec<(String, u64)> = open_now().iter().map(|(w, s)| (w.clone(), *s)).collect();
    for (world, since) in open {
        let Ok(value) = call(&store, &world, "world_sync", serde_json::json!({ "since": since })).await else {
            crate::tell_windows("online-unavailable", serde_json::json!({ "world": world, "error": UNREACHED }));
            continue;
        };
        if let Some(writes) = value.get("writes").and_then(|w| w.as_u64()) {
            open_now().insert(world.clone(), writes);
        }
        if let Ok(info) = serde_json::from_value::<WorldInfo>(value.get("world").cloned().unwrap_or_default()) {
            told().insert(world.clone(), info);
        }
        if value.get("changed").and_then(|c| c.as_bool()) == Some(true) {
            out.updated.push(world);
        }
    }
    // Worlds asked to join: let in by now?
    let asked: Vec<(String, RemoteWorld)> = store.load_network()?.remote.into_iter().filter(|(_, r)| r.asked).collect();
    for (world, mut remote) in asked {
        if let Ok(Reply::Joined) = ask(&store, &remote.server, &world, Ask::Join).await {
            remote.asked = false;
            note_remote(&store, &world, Some(remote.clone()))?;
            out.joined.push(sketch(&world, &remote));
        }
    }
    look_at_servers(&store).await;
    Ok(out)
}

/// What a server said of this device last time.
#[derive(Clone, Default)]
struct ServerStatus {
    name: String,
    admin: bool,
    may_bring: bool,
    asked: bool,
    reached: bool,
    worlds: Vec<Offer>,
    at: Option<std::time::Instant>,
}

static SERVER_STATUS: OnceLock<Mutex<HashMap<String, ServerStatus>>> = OnceLock::new();

fn server_status() -> std::sync::MutexGuard<'static, HashMap<String, ServerStatus>> {
    SERVER_STATUS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

fn status(code: &str) -> ServerStatus {
    server_status().get(code).cloned().unwrap_or_default()
}

/// How long what a server said holds before it is asked again.
const STATUS_FOR: std::time::Duration = std::time::Duration::from_secs(30);

/// Asks each server what it holds for this device — now and then.
async fn look_at_servers(store: &Store) {
    let Ok(settings) = store.load_network() else { return };
    for code in &settings.servers {
        if status(code).at.is_some_and(|at| at.elapsed() < STATUS_FOR) {
            continue;
        }
        look_at(store, code).await;
    }
}

async fn look_at(store: &Store, code: &str) -> ServerStatus {
    let status = match ask(store, code, "", Ask::Worlds).await {
        Ok(Reply::Worlds { worlds, host, admin, may_bring, asked }) => {
            ServerStatus { name: host.map(|h| h.name).unwrap_or_default(), admin, may_bring, asked, reached: true, worlds, at: Some(std::time::Instant::now()) }
        }
        _ => ServerStatus { reached: false, at: Some(std::time::Instant::now()), ..status(code) },
    };
    server_status().insert(code.to_string(), status.clone());
    status
}

// --- Settings ----------------------------------------------------------------

/// What the network settings page shows.
#[derive(Serialize)]
pub struct NetworkView {
    pub enabled: bool,
    pub internet: bool,
    pub port: u16,
    /// What members enter to reach this app's server — empty while nothing is reachable.
    pub code: String,
    pub reachable: Vec<String>,
    pub via: Vec<String>,
    pub stun: Vec<String>,
    /// Whether the network actually runs.
    pub running: bool,
    pub servers: Vec<String>,
    pub server_list: Vec<ServerEntry>,
    /// The worlds this app could offer: those it is GM of.
    pub worlds: Vec<SharedWorld>,
}

/// A server as it answered last.
#[derive(Serialize)]
pub struct ServerEntry {
    pub code: String,
    /// What it calls itself — empty while it has not answered yet.
    pub name: String,
    pub admin: bool,
    pub may_bring: bool,
    pub asked: bool,
    pub reached: bool,
}

#[derive(Serialize)]
pub struct SharedWorld {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub shared: bool,
    /// The server it lives on, by code — empty: this device.
    pub server: String,
}

fn view(store: &Store) -> Answer<NetworkView> {
    let settings = store.load_network()?;
    let worlds = store
        .list_worlds()?
        .into_iter()
        .filter(|m| store.is_gm(&m.id))
        .map(|m| SharedWorld { shared: settings.shared.contains(&m.id), server: String::new(), id: m.id, name: m.name, icon: m.icon })
        .collect();
    let net = running();
    let reachable: Vec<String> = net.as_ref().map(|n| n.reachable().iter().map(|a| a.to_string()).collect()).unwrap_or_default();
    let mut via: Vec<String> = net.as_ref().map(|n| n.reach_by().iter().map(|(k, _)| k.to_string()).collect()).unwrap_or_default();
    via.dedup();
    // Its outside addresses, and the one at home for members in the same network.
    let code = net
        .as_ref()
        .filter(|_| settings.enabled || settings.internet)
        .map(|n| {
            let mut addrs = reachable.clone();
            addrs.extend(n.home_address().map(|a| a.to_string()).filter(|a| !addrs.contains(a)));
            if addrs.is_empty() { String::new() } else { Server { endpoint: n.id(), addrs }.code() }
        })
        .unwrap_or_default();
    Ok(NetworkView {
        enabled: settings.enabled,
        internet: settings.internet,
        port: settings.port,
        code,
        reachable,
        via,
        stun: settings.stun,
        running: net.is_some(),
        server_list: settings
            .servers
            .iter()
            .map(|code| {
                let s = status(code);
                ServerEntry { code: code.clone(), name: s.name, admin: s.admin, may_bring: s.may_bring, asked: s.asked, reached: s.reached }
            })
            .collect(),
        servers: settings.servers,
        worlds,
    })
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn network_settings(app: tauri::AppHandle) -> Answer<NetworkView> {
    view(&store(&app)?)
}

/// Changes the settings and starts the network anew.
async fn set(store: &Store, change: impl FnOnce(&mut NetworkSettings)) -> Answer<NetworkView> {
    let mut settings = store.load_network()?;
    change(&mut settings);
    store.save_network(&settings)?;
    switch(store, &settings).await?;
    view(store)
}

#[cfg(feature = "gui")]
/// The local network: offering worlds there, and finding them.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub async fn network_enable(app: tauri::AppHandle, enabled: bool) -> Answer<NetworkView> {
    set(&store(&app)?, |s| s.enabled = enabled).await
}

#[cfg(feature = "gui")]
/// Reachable over the internet — and whom to ask how this device looks from outside.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub async fn network_internet(app: tauri::AppHandle, enabled: bool, stun: Vec<String>) -> Answer<NetworkView> {
    set(&store(&app)?, |s| {
        s.internet = enabled;
        s.stun = stun.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    })
    .await
}

#[cfg(feature = "gui")]
/// The port to answer on — for a router's forwarding. None: the usual one.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub async fn network_port(app: tauri::AppHandle, port: Option<u16>) -> Answer<NetworkView> {
    set(&store(&app)?, |s| s.port = port.filter(|p| *p > 0).unwrap_or(aw_sync::world::DEFAULT_PORT)).await
}

#[cfg(feature = "gui")]
/// The servers to keep worlds with, by their code or address.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub async fn network_servers(app: tauri::AppHandle, servers: Vec<String>) -> Answer<NetworkView> {
    let store = store(&app)?;
    let mut codes = Vec::new();
    // A code, or an address alone — the server tells its code there.
    for entry in servers.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let server = Server::look_up(entry).await.map_err(|e| AppError(e.to_string()))?;
        if !codes.contains(&server.code()) {
            codes.push(server.code());
        }
    }
    let view = set(&store, |s| s.servers = codes.clone()).await?;
    for code in &codes {
        look_at(&store, code).await;
    }
    Ok(NetworkView { server_list: self::view(&store)?.server_list, ..view })
}

#[cfg(feature = "gui")]
/// Offers a world to its members over the network — the GM's.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn network_share(app: tauri::AppHandle, world: String, shared: bool) -> Answer<NetworkView> {
    let store = store(&app)?;
    must_be_gm(&store, &world, "offers a world")?;
    let mut settings = store.load_network()?;
    settings.shared.retain(|w| w != &world);
    if shared {
        settings.shared.push(world);
    }
    store.save_network(&settings)?;
    republish();
    view(&store)
}

// --- Finding and joining -----------------------------------------------------

/// A world found on the network.
#[derive(Serialize)]
pub struct NetWorld {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub gm: String,
    /// `player` when this app already holds it or works in it.
    pub local: Option<String>,
    /// The server holding it, by code.
    pub server: String,
}

#[cfg(feature = "gui")]
/// The worlds found on the local network and on the servers.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub async fn network_worlds(app: tauri::AppHandle) -> Answer<Vec<NetWorld>> {
    let store = store(&app)?;
    let settings = store.load_network()?;
    let mut found: Vec<(Offer, String)> = running().map(|n| n.found_hosts().into_iter().map(|(o, s)| (o, s.code())).collect()).unwrap_or_default();
    for code in &settings.servers {
        let s = if status(code).at.is_some_and(|at| at.elapsed() < STATUS_FOR) { status(code) } else { look_at(&store, code).await };
        found.extend(s.worlds.into_iter().map(|o| (o, code.clone())));
    }
    let mut out: Vec<NetWorld> = Vec::new();
    for (offer, server) in found {
        if offer.name.is_empty() || out.iter().any(|w| w.id == offer.world) {
            continue;
        }
        let local = (store.holds(&offer.world) || settings.remote.get(&offer.world).is_some_and(|r| !r.asked)).then(|| "player".to_string());
        out.push(NetWorld { id: offer.world, name: offer.name, icon: offer.icon, gm: offer.gm, local, server });
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

/// What asking to join came to.
#[derive(Serialize)]
pub struct JoinOutcome {
    /// `joined`, `requested`, `not-member`, `not-offered`.
    pub kind: String,
    pub world: Option<WorldInfo>,
}

#[cfg(feature = "gui")]
/// Asks to join a world found on the network — a member already, or let in
/// ahead of time, in at once.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub async fn network_join(app: tauri::AppHandle, world: String, join: bool) -> Answer<JoinOutcome> {
    let store = store(&app)?;
    let _ = join;
    let found = network_worlds(app.clone()).await?.into_iter().find(|w| w.id == world).ok_or_else(|| AppError("This world is not found on the network any more.".into()))?;
    let remote = RemoteWorld { name: found.name.clone(), icon: found.icon.clone(), server: found.server.clone(), asked: false };
    let kind = match ask(&store, &found.server, &world, Ask::Join).await? {
        Reply::Joined => {
            note_remote(&store, &world, Some(remote.clone()))?;
            "joined"
        }
        Reply::Requested => {
            note_remote(&store, &world, Some(RemoteWorld { asked: true, ..remote.clone() }))?;
            "requested"
        }
        Reply::NotMember => "not-member",
        _ => "not-offered",
    };
    let world = (kind == "joined").then(|| sketch(&world, &remote));
    Ok(JoinOutcome { kind: kind.into(), world })
}

/// Someone who asked to join a world, as the GM and supporters see them.
#[derive(Serialize)]
pub struct RequestInfo {
    pub id: String,
    pub name: String,
    pub fingerprint: String,
    pub at: u64,
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn join_requests(app: tauri::AppHandle, world: String) -> Answer<Vec<RequestInfo>> {
    join_requests_in(&store(&app)?, world)
}

/// `join_requests` for whom the store is given.
pub(crate) fn join_requests_in(store: &Store, world: String) -> Answer<Vec<RequestInfo>> {
    if !manages(store, &world)? {
        return Ok(Vec::new());
    }
    Ok(store
        .db(&world)?
        .requests()?
        .into_iter()
        .map(|r| RequestInfo { fingerprint: fingerprint_of(&r.id), id: r.id, name: r.name, at: r.at })
        .collect())
}

#[cfg(feature = "gui")]
/// Lets someone in who asked — or turns them down. The GM's, or a supporter's.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn join_answer(app: tauri::AppHandle, world: String, id: String, accept: bool) -> Answer<WorldInfo> {
    join_answer_in(&store(&app)?, world, id, accept)
}

/// `join_answer` for whom the store is given.
pub(crate) fn join_answer_in(store: &Store, world: String, id: String, accept: bool) -> Answer<WorldInfo> {
    if !manages(store, &world)? {
        return Err(AppError("Only the GM or a supporter lets someone in.".into()));
    }
    let db = store.db(&world)?;
    let request = db.requests()?.into_iter().find(|r| r.id == id).ok_or_else(|| AppError("There is no such request.".into()))?;
    if accept {
        admit(store, &world, card_of(&request)?, Vec::new())?;
    } else {
        db.remove_request(&id)?;
    }
    world_now(store, &world)
}

#[cfg(feature = "gui")]
/// Lets a player in ahead of time, by their id alone — they are a member
/// once they first ask, with keys that make that id.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn member_invite(app: tauri::AppHandle, world: String, id: String) -> Answer<WorldInfo> {
    member_invite_in(&store(&app)?, world, id)
}

/// `member_invite` for whom the store is given.
pub(crate) fn member_invite_in(store: &Store, world: String, id: String) -> Answer<WorldInfo> {
    must_be_gm(store, &world, "lets players in ahead of time")?;
    let id = id.trim().to_string();
    if !is_player_id(&id) {
        return Err(AppError("That is no player id. It is the whole id — id: and 16 characters.".into()));
    }
    let db = store.db(&world)?;
    if let Some(request) = db.requests()?.into_iter().find(|r| r.id == id) {
        admit(store, &world, card_of(&request)?, Vec::new())?;
    } else if db.member(&id)?.is_none() {
        let mut invited = db.invited()?;
        if !invited.contains(&id) {
            invited.push(id);
            db.set_invited(&invited)?;
        }
    }
    world_now(store, &world)
}

/// Someone asks to join a world held here: a member already, or let in
/// ahead of time — then in at once; otherwise their request waits.
pub(crate) fn join(store: &Store, world: &str, who: &IdentityPublic) -> Answer<bool> {
    let db = store.db(world)?;
    let id = who.id.to_string();
    if let Some(mut member) = db.member(&id)? {
        if member.name != who.name {
            member.name = who.name.clone();
            db.put_member(&member)?;
        }
        return Ok(true);
    }
    let takeovers: std::collections::BTreeMap<String, String> = store.setting(world, "takeovers", Default::default())?;
    if db.invited()?.contains(&id) || takeovers.contains_key(&id) {
        // As the GM lets them in: they named them beforehand.
        let gm = db.members()?.into_iter().find(|m| m.role == Role::Gm).map(|m| m.id).unwrap_or_default();
        store::acting_for(&gm, || admit(store, world, who.clone(), Vec::new()))?;
        return Ok(true);
    }
    if db.requests()?.len() >= MAX_REQUESTS && !db.requests()?.iter().any(|r| r.id == id) {
        return Err(AppError("Too many are asking to join this world right now.".into()));
    }
    db.put_request(&aw_store::db::Request { id, name: who.name.clone(), verify_key: who.verify_key.clone(), seal_key: who.seal_key.clone(), at: store::now() })?;
    Ok(false)
}

/// Requests to join one world that may wait at once.
const MAX_REQUESTS: usize = 50;

// --- Servers -----------------------------------------------------------------

#[cfg(feature = "gui")]
/// Asks a server's admin to let this device bring worlds there.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub async fn server_host_worlds(app: tauri::AppHandle, server: String) -> Answer<NetworkView> {
    let store = store(&app)?;
    ask(&store, &server, "", Ask::MayIBring).await?;
    look_at(&store, &server).await;
    view(&store)
}

#[cfg(feature = "gui")]
/// What a server holds and who uses it — for its admin; with an order,
/// carried out first.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub async fn server_manage(app: tauri::AppHandle, server: String, order: Option<aw_sync::world::Order>) -> Answer<aw_sync::world::ServerState> {
    match ask(&store(&app)?, &server, "", Ask::Manage { order }).await? {
        Reply::Managed { state } => Ok(state),
        _ => Err(AppError("Only the server's admin manages it.".into())),
    }
}

#[cfg(feature = "gui")]
/// How the server a world lives on backs it up — with a plan, set first. Its GM asks.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub async fn server_backups(app: tauri::AppHandle, world: String, plan: Option<aw_sync::world::BackupPlan>) -> Answer<aw_sync::world::ServerBackups> {
    let store = store(&app)?;
    let remote = remote_of(&store, &world).ok_or_else(|| AppError("This world lives on no server.".into()))?;
    match ask(&store, &remote.server, &world, Ask::Backups { plan }).await? {
        Reply::Backups { backups } => Ok(backups),
        _ => Err(AppError("The server does not back this world up.".into())),
    }
}

// --- Bringing worlds to servers and back ---------------------------------------

/// How much of a file one piece brings.
const PIECE: usize = 4 * 1024 * 1024;

/// Brings a world lying in `dir` to a server — its GM's — piece by piece.
/// `tell`: how far, in bytes.
async fn bring(store: &Store, code: &str, world: &str, name: &str, dir: &Path, tell: &(dyn Fn(u64, u64) + Sync)) -> Answer<()> {
    use base64::Engine;
    use std::io::Read;
    let mut files = vec![(aw_store::db::FILE.to_string(), dir.join(aw_store::db::FILE))];
    for entry in std::fs::read_dir(dir.join(aw_store::db::MEDIA)).into_iter().flatten().flatten() {
        if entry.file_type().is_ok_and(|t| t.is_file()) {
            files.push((format!("{}/{}", aw_store::db::MEDIA, entry.file_name().to_string_lossy()), entry.path()));
        }
    }
    let total: u64 = files.iter().map(|(_, p)| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)).sum();
    let mut done = 0;
    for (file, path) in files {
        let mut f = std::fs::File::open(&path)?;
        let mut at = 0u64;
        loop {
            let mut buf = Vec::with_capacity(PIECE);
            (&mut f).take(PIECE as u64).read_to_end(&mut buf)?;
            // An empty file comes as one empty piece.
            if buf.is_empty() && at > 0 {
                break;
            }
            let n = buf.len() as u64;
            let data = base64::engine::general_purpose::STANDARD.encode(&buf);
            match ask(store, code, world, Ask::Put { file: file.clone(), at, data }).await? {
                Reply::Taken => {}
                Reply::Requested => return Err(AppError("The server's admin has not let you bring worlds there yet.".into())),
                _ => return Err(AppError("The server did not take the world.".into())),
            }
            at += n;
            done += n;
            tell(done, total);
            if n < PIECE as u64 {
                break;
            }
        }
    }
    match ask(store, code, world, Ask::Bring { name: name.to_string() }).await? {
        Reply::Joined => Ok(()),
        _ => Err(AppError("The server did not take the world.".into())),
    }
}

/// Fetches a world from its server into `dir` — its GM's.
async fn fetch(store: &Store, code: &str, world: &str, dir: &Path, tell: &(dyn Fn(u64, u64) + Sync)) -> Answer<()> {
    use base64::Engine;
    use std::io::Write;
    let Reply::Files { files } = ask(store, code, world, Ask::Files).await? else {
        return Err(AppError("Only its GM takes a world from its server.".into()));
    };
    let total: u64 = files.iter().map(|(_, n)| n).sum();
    let mut done = 0;
    std::fs::create_dir_all(dir.join(aw_store::db::MEDIA))?;
    for (file, size) in files {
        if file != aw_store::db::FILE && !file.strip_prefix("media/").is_some_and(|n| !n.is_empty() && !n.contains(['/', '\\']) && !n.starts_with('.')) {
            continue;
        }
        let mut out = std::fs::File::create(dir.join(&file))?;
        let mut at = 0;
        while at < size {
            let Reply::Piece { data } = ask(store, code, world, Ask::Get { file: file.clone(), at }).await? else {
                return Err(AppError("The server did not give the world.".into()));
            };
            let bytes = base64::engine::general_purpose::STANDARD.decode(data.as_bytes()).map_err(|_| AppError("A piece did not arrive whole.".into()))?;
            if bytes.is_empty() {
                return Err(AppError("The world did not arrive whole.".into()));
            }
            out.write_all(&bytes)?;
            at += bytes.len() as u64;
            done += bytes.len() as u64;
            tell(done, total);
        }
        out.sync_all()?;
    }
    Ok(())
}

/// Moves a GM's world: from here onto a server, from its server here, or
/// from one server to another. Afterwards it lies in the one place alone.
pub(crate) async fn move_world(store: &Store, world: &str, to_server: Option<String>, to_folder: Option<PathBuf>, tell: &(dyn Fn(u64, u64) + Sync)) -> Answer<()> {
    let from = remote_of(store, world);
    match (from, to_server) {
        // Here, onto a server.
        (None, Some(code)) => {
            must_be_gm(store, world, "moves a world")?;
            let db = store.db(world)?;
            let name = db.get("name")?.unwrap_or_default();
            let icon = db.get("icon")?.unwrap_or_default();
            // The database as it is now; the pictures as they lie.
            let out = store.private_world_dir(world).join("bringing");
            let _ = std::fs::remove_dir_all(&out);
            std::fs::create_dir_all(&out)?;
            db.copy_to(&out.join(aw_store::db::FILE))?;
            drop(db);
            let media = store.world_dir(world).join(aw_store::db::MEDIA);
            #[cfg(unix)]
            let linked = std::os::unix::fs::symlink(&media, out.join(aw_store::db::MEDIA));
            #[cfg(not(unix))]
            let linked: std::io::Result<()> = Err(std::io::Error::other("no links"));
            if linked.is_err() {
                std::fs::create_dir_all(out.join(aw_store::db::MEDIA))?;
                for entry in std::fs::read_dir(&media).into_iter().flatten().flatten() {
                    std::fs::copy(entry.path(), out.join(aw_store::db::MEDIA).join(entry.file_name()))?;
                }
            }
            let brought = bring(store, &code, world, &name, &out, tell).await;
            let _ = std::fs::remove_dir_all(&out);
            brought?;
            note_remote(store, world, Some(RemoteWorld { name, icon, server: code.clone(), asked: false }))?;
            keeper::here(store)?.moved(world, Some(&code))?;
            let mut settings = store.load_network()?;
            settings.shared.retain(|w| w != world);
            store.save_network(&settings)?;
            store.delete_world(world)?;
            Ok(())
        }
        // From its server, here — or onto another one.
        (Some(remote), to) => {
            if to.as_deref() == Some(remote.server.as_str()) {
                return Ok(());
            }
            let root = to_folder.unwrap_or_else(|| store.worlds_dir());
            let temp = root.join(format!(".{}.coming", store::dir_name(world)));
            let _ = std::fs::remove_dir_all(&temp);
            std::fs::create_dir_all(&temp)?;
            let fetched = fetch(store, &remote.server, world, &temp, tell).await;
            if let Err(e) = fetched {
                let _ = std::fs::remove_dir_all(&temp);
                return Err(e);
            }
            match &to {
                Some(code) => {
                    let brought = bring(store, code, world, &remote.name, &temp, tell).await;
                    let _ = std::fs::remove_dir_all(&temp);
                    brought?;
                }
                None => {
                    let into = root.join(store::dir_name(world));
                    if into.exists() {
                        let _ = std::fs::remove_dir_all(&temp);
                        return Err(AppError("A world of that id lies there already.".into()));
                    }
                    std::fs::rename(&temp, &into)?;
                    // Offered from here again, to the members it has.
                    keeper::here(store)?.moved(world, None)?;
                    let mut settings = store.load_network()?;
                    if !settings.shared.iter().any(|w| w == world) {
                        settings.shared.push(world.to_string());
                        store.save_network(&settings)?;
                    }
                    republish();
                }
            }
            // It lies in its new place alone.
            let left = ask(store, &remote.server, "", Ask::Manage { order: Some(aw_sync::world::Order::Delete { world: world.to_string() }) }).await;
            open_now().remove(world);
            told().remove(world);
            match to {
                Some(code) => note_remote(store, world, Some(RemoteWorld { server: code, ..remote }))?,
                None => note_remote(store, world, None)?,
            }
            if !matches!(left, Ok(Reply::Managed { .. })) {
                crate::applog::line("network", &format!("{world}: its old server did not let it go"));
            }
            Ok(())
        }
        (None, None) => Err(AppError("This world lies here already.".into())),
    }
}

/// Leaves a world on a server — and, its GM, deletes it there for good.
pub(crate) async fn leave(store: &Store, world: &str, delete: bool) -> Answer<()> {
    let remote = remote_of(store, world).or_else(|| store.load_network().ok()?.remote.get(world).cloned()).ok_or_else(|| AppError("This world is not on a server.".into()))?;
    if delete {
        match ask(store, &remote.server, "", Ask::Manage { order: Some(aw_sync::world::Order::Delete { world: world.to_string() }) }).await? {
            Reply::Managed { .. } => {}
            _ => return Err(AppError("Only its GM deletes a world on its server.".into())),
        }
    }
    open_now().remove(world);
    told().remove(world);
    note_remote(store, world, None)
}
