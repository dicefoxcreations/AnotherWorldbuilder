//! The network: this app's own server, offered to the members of its worlds,
//! and the servers elsewhere it reaches.
//!
//! Being rebuilt on world format 5 (see the draft of 26 October 2026,
//! section 10): until then the network is off, and these commands tell so.

use super::*;
use std::collections::HashMap;

/// What the network settings page shows.
#[derive(Serialize)]
pub struct NetworkView {
    pub enabled: bool,
    pub internet: bool,
    pub folders: bool,
    pub port: u16,
    /// What members enter to reach this app's server — empty while nothing is reachable.
    pub code: String,
    pub reachable: Vec<String>,
    pub via: Vec<String>,
    pub stun: Vec<String>,
    /// Whether the network actually runs.
    pub running: bool,
    pub servers: Vec<String>,
    pub server_list: Vec<serde_json::Value>,
    pub worlds: Vec<serde_json::Value>,
    pub uploads: HashMap<String, serde_json::Value>,
}

fn view(store: &Store) -> Answer<NetworkView> {
    let settings = store.load_network()?;
    Ok(NetworkView {
        enabled: false,
        internet: settings.internet,
        folders: false,
        port: settings.port,
        code: String::new(),
        reachable: Vec::new(),
        via: Vec::new(),
        stun: settings.stun,
        running: false,
        servers: settings.servers,
        server_list: Vec::new(),
        worlds: Vec::new(),
        uploads: HashMap::new(),
    })
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn network_settings(app: tauri::AppHandle) -> Answer<NetworkView> {
    view(&store(&app)?)
}

/// What one round of keeping up brought.
#[derive(Serialize, Default)]
pub struct SyncOutcome {
    pub joined: Vec<serde_json::Value>,
    pub updated: Vec<String>,
}

#[cfg(feature = "gui")]
/// Keeps up with the servers — nothing to keep up with while the network is off.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn network_sync() -> Answer<SyncOutcome> {
    Ok(SyncOutcome::default())
}

#[cfg(feature = "gui")]
/// The worlds found on the network and on the servers — none while it is off.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn network_worlds() -> Answer<Vec<serde_json::Value>> {
    Ok(Vec::new())
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
/// Opens a world to work in: one this app holds at once; one on a server
/// once the network is back.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn world_connect(app: tauri::AppHandle, world: String) -> Answer<WorldInfo> {
    let store = store(&app)?;
    if store.holds(&world) {
        return world_now(&store, &world);
    }
    Err(no_servers_yet())
}

#[cfg(feature = "gui")]
/// Leaves a world worked in — nothing to let go of for one this app holds.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn world_disconnect(world: String) -> Answer<()> {
    let _ = world;
    Ok(())
}
