//! The Tauri commands.
//!
//! The frontend only ever sees plaintext, never keys. Rust decrypts on load
//! and encrypts on save; key material never leaves this process.
//!
//! In particular there is no visibility check here that the frontend could
//! bypass: `view` simply tries to open every layer and returns what opened.

// The server (`--no-default-features`) uses less of it than the app.
#![cfg_attr(not(feature = "gui"), allow(dead_code, unused_imports))]

mod applog;
mod document;
#[cfg(feature = "gui")]
mod window_state;
mod store;
mod profile;
mod network;
mod media;
mod transfer;
mod host;
mod keys;
mod backup;
mod online;
#[cfg(feature = "gui")]
mod browser;
mod readable;
pub mod server;
mod keeper;

use aw_crypto::{Identity, IdentityPublic};
use aw_model::{
    editor, markdown, names, template_text, types, ArticleId, Attribute, LayerId, Policy, Level, Template,
    WorldCalendar, WorldDate,
};
use aw_policy::{HeldKeys, Keyring};
use aw_store::journal::{Access, Plain};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use store::{
    fields_map, LayerContent, Member, NewPart, Role, Store, StoredArticle, StoredLayer, WorldMeta,
};
#[cfg(feature = "gui")]
use tauri::Manager;

/// An error the frontend gets to see — as readable text.
#[derive(Debug)]
struct AppError(String);

impl<E: std::fmt::Display> From<E> for AppError {
    fn from(e: E) -> Self {
        AppError(e.to_string())
    }
}

impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

type Answer<T> = Result<T, AppError>;

// --- What crosses the bridge --------------------------------------------

#[derive(Serialize)]
pub struct IdentityInfo {
    pub name: String,
    pub id: String,
    pub fingerprint: String,
    pub file: String,
    /// The public key — visible in the settings.
    pub public_key: String,
    /// Avatar as a `data:` URL, if one is set.
    pub avatar: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct MemberInfo {
    pub id: String,
    pub name: String,
    pub fingerprint: String,
    pub role: String,
    pub attributes: Vec<String>,
    /// Their writing counts without waiting for the game master.
    pub trusted: bool,
}

#[derive(Serialize, Deserialize)]
pub struct WorldInfo {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub public_key: String,
    pub token_file: String,
    pub folder: String,
    /// `gm` or `player` — decides what the interface offers.
    pub role: String,
    pub is_gm: bool,
    pub attributes: Vec<String>,
    pub members: Vec<MemberInfo>,
    pub article_count: usize,
    /// Id of the home page, empty if there is none yet.
    pub home: String,
    /// Requests to join, waiting for the GM.
    pub requests: usize,
    /// Ids let in ahead of time, not seen on the network yet.
    pub invited: Vec<String>,
    /// New ids that take a player's place once let in: new id → old id.
    pub takeovers: std::collections::BTreeMap<String, String>,
    /// Members' colours by the terms that name them — `player:<name>`, `doc:…`.
    pub colours: std::collections::BTreeMap<String, String>,
    /// The member the GM hands the world over to, once they show up.
    pub handover: Option<String>,
    /// Whether players' writing waits for the GM before the others see it.
    pub moderated: bool,
    /// Versions waiting: for the GM, what players wrote; for a player, their own.
    pub waiting: usize,
    /// Changes that reach no one: for the GM all, for a player their own.
    pub conflicts: usize,
    /// How many versions of each document stay.
    pub version_limit: u32,
    /// Encrypted: views and rights count. Without, whoever has the world
    /// reads and edits all of it.
    pub encrypted: bool,
    /// Where this device keeps the readable copy — empty: nowhere.
    pub readable: String,
    /// The copy lies in the world's folder (GM, unencrypted): always on.
    pub readable_in_world: bool,
    /// The GM's: whether players of this encrypted world get a copy, or
    /// work online with the GM's device as host.
    pub copies: bool,
    /// Held here only while online — it goes when the app starts again.
    pub fleeting: bool,
    /// The server it lives on, by code — empty: this device.
    pub server: String,
    /// Hosting identity survives address changes and a removed server entry.
    pub server_id: String,
    /// The GM's: offered to players over the network.
    pub offered: bool,
    /// The dice the world rolls with, by sides — empty for the usual set.
    pub dice: Vec<u32>,
    /// A server names it to everyone who asks — else only to its GM and members.
    pub listed: bool,
    /// How the world counts its days.
    pub calendar: WorldCalendar,
    /// Who supports the world, by id: the players the GM chose (`chosen`)
    /// and the admins of its server.
    pub supporters: Vec<String>,
    /// The GM's choice among them.
    pub chosen: Vec<String>,
    /// The admins of its server, as the GM's device knows them, by name.
    pub admins: Vec<String>,
    /// This device's member supports it: lets players in, and may take the
    /// GM's place — with the keys sealed for them (`can_take_over`).
    pub supporter: bool,
    pub can_take_over: bool,
    /// The roles there are to give — the GM's and a supporter's.
    pub roles: Vec<String>,
}

#[derive(Serialize, Clone)]
pub struct LayerView {
    pub id: String,
    pub title: String,
    /// Key of the section in the template.
    pub section: String,
    /// Whether this viewer may change the section.
    pub may_edit: bool,
    pub text: String,
    pub fields: serde_json::Value,
    pub policy: String,
    /// Who may change it — to know whom to ask.
    pub edit: String,
    pub level: String,
    pub epoch: u32,
}

#[derive(Serialize, Clone)]
pub struct ArticleView {
    pub id: String,
    pub article_type: String,
    pub slug: String,
    pub name: String,
    /// Visibility of the article as a whole.
    pub visibility: String,
    /// Who may change the article as a whole.
    pub edit: String,
    pub layers: Vec<LayerView>,
    pub layer_total: usize,
    /// The order of its own sections, by key — see `document::own_order`.
    pub order: Vec<String>,
    /// Asked of the GM's device and not made yet: only here, on the device
    /// of the player who asked (`pending_views`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub pending: bool,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct NewLayer {
    pub title: String,
    pub text: String,
    pub policy: String,
    pub level: String,
    /// Key of the section in the template.
    #[serde(default)]
    pub section: String,
    /// Who may change this section.
    #[serde(default)]
    pub edit: String,
    #[serde(default)]
    pub fields: serde_json::Value,
    /// Index of the enclosing section in the same list, if there is one.
    /// Must be smaller than its own index.
    #[serde(default)]
    pub parent: Option<usize>,
}

/// Combines the visibility of every section with that of its parents.
///
/// A section inside a section is only visible when the outer one is visible
/// too. Without this combination a player could open the inner box while the
/// outer one stays shut for them — the nesting would then be mere looks and
/// no boundary.
///
/// The computation lives here and not in the frontend: it decides who can
/// read something.
fn effective_policies(inputs: &[NewLayer]) -> Result<Vec<Policy>, AppError> {
    let mut effective: Vec<Policy> = Vec::with_capacity(inputs.len());

    for (index, input) in inputs.iter().enumerate() {
        let own: Policy = input.policy.parse().map_err(AppError::from)?;
        let policy = match input.parent {
            None => own,
            Some(parent) if parent < index => own.and(&effective[parent]),
            Some(parent) => {
                return Err(AppError(format!(
                    "Part {index} points at {parent} — the outer part has to come first."
                )))
            }
        };
        effective.push(policy);
    }
    Ok(effective)
}

// --- Helpers ------------------------------------------------------------

#[cfg(feature = "gui")]
fn store(app: &tauri::AppHandle) -> Result<Store, AppError> {
    if let Some(store) = online::command_store() {
        return Ok(store);
    }
    profile::store(app)
}

fn level_from(text: &str) -> Level {
    match text {
        "open" => Level::Open,
        "sealed" => Level::Sealed,
        _ => Level::GmOnly,
    }
}

fn level_text(level: &Level) -> &'static str {
    match level {
        Level::Open => aw_model::template::LEVEL_OPEN,
        Level::Sealed => aw_model::template::LEVEL_SEALED,
        Level::GmOnly => aw_model::template::LEVEL_GM,
    }
}

fn role_text(role: Role) -> &'static str {
    match role {
        Role::Gm => "gm",
        Role::Player => "player",
    }
}

/// Whether the attributes at hand satisfy the edit rule.
///
/// Separate from seeing: whoever may read a section may not therefore change
/// it. This tells the interface what to offer; what holds is the write key —
/// a change without it is refused by every device it reaches.
fn may_edit(rule: &str, held: &HeldKeys) -> bool {
    match rule.parse::<Policy>() {
        Ok(policy) => policy.satisfied_by(&held.attributes()),
        // Without a rule the game master decides.
        Err(_) => held.attributes().contains(&Attribute::Gm),
    }
}

/// Opens whatever opens with these keys. Nothing more happens — there is no
/// second check that could be forgotten.
fn open_layer(layer: &StoredLayer, held: &HeldKeys) -> Option<LayerView> {
    let content = layer.content(held)?;
    Some(LayerView {
        id: layer.id.to_string(),
        title: content.title,
        section: content.section,
        // Unencrypted, there are no rights: everyone edits.
        may_edit: (layer.plain && host::acting().is_none()) || may_edit(&content.edit, held),
        text: content.text,
        fields: content.fields,
        // The header names no one; who may see it stands in the layer itself.
        policy: content.own.parse::<Policy>().map(|p| p.to_string()).unwrap_or_else(|_| content.own.clone()),
        edit: content.edit.parse::<Policy>().map(|p| p.to_string()).unwrap_or_else(|_| content.edit.clone()),
        level: level_text(&layer.level).to_string(),
        epoch: layer.header.epoch,
    })
}

/// What each document showed the last time a page asked — by world and by
/// who looks (a preview is someone else), with what it was opened from. A
/// document whose layers are the same is not opened again: with ten
/// thousand of them, one edit should not cost them all.
type ViewCache = std::collections::HashMap<(String, String), std::collections::HashMap<ArticleId, (u64, Option<ArticleView>)>>;
static VIEWS: std::sync::OnceLock<std::sync::Mutex<ViewCache>> = std::sync::OnceLock::new();

pub(crate) fn forget_world_views(world: &str) {
    if let Some(all) = VIEWS.get() {
        if let Ok(mut all) = all.lock() { all.retain(|(id, _), _| id != world); }
    }
}

fn fingerprint(a: &StoredArticle) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    a.article_type.hash(&mut h);
    a.slug.hash(&mut h);
    for l in &a.layers {
        l.id.to_string().hash(&mut h);
        l.sealed.hash(&mut h);
        l.is_base.hash(&mut h);
    }
    h.finish()
}

fn cached_view(world: &str, who: &str, articles: &[StoredArticle], held: &HeldKeys) -> Vec<ArticleView> {
    let Ok(mut all) = VIEWS.get_or_init(Default::default).lock() else {
        return build_view(articles, held);
    };
    let cache = all.entry((world.to_string(), who.to_string())).or_default();
    let mut fresh = std::collections::HashMap::with_capacity(articles.len());
    let mut out = Vec::with_capacity(articles.len());
    for a in articles {
        let print = fingerprint(a);
        let view = match cache.remove(&a.id) {
            Some((p, view)) if p == print => view,
            _ => build_view(std::slice::from_ref(a), held).into_iter().next(),
        };
        if let Some(v) = &view {
            out.push(v.clone());
        }
        fresh.insert(a.id, (print, view));
    }
    *cache = fresh;
    out
}

fn build_view(articles: &[StoredArticle], held: &HeldKeys) -> Vec<ArticleView> {
    let mut out = Vec::new();
    for a in articles {
        // The base layer decides about the article as a whole. If it does not
        // open, the article does not appear — not even when single sections
        // would be readable. A section without an article name would make no
        // sense anyway, and mere existence is already information.
        let Some((name, visibility, edit, order)) = a
            .layers
            .iter()
            .find(|l| l.is_base)
            .and_then(|layer| open_layer(layer, held))
            .map(|opened| (opened.title, opened.policy, opened.edit, document::own_order(&opened.fields)))
        else {
            continue;
        };

        let visible: Vec<LayerView> = a
            .layers
            .iter()
            .filter(|l| !l.is_base)
            .filter_map(|l| open_layer(l, held))
            .collect();

        out.push(ArticleView {
            id: a.id.to_string(),
            article_type: a.article_type.clone(),
            slug: a.slug.clone(),
            name,
            visibility,
            edit,
            layers: visible,
            layer_total: a.layers.iter().filter(|l| !l.is_base).count(),
            order,
            pending: false,
        });
    }
    out
}

fn info_from(store: &Store, identity: &Identity) -> IdentityInfo {
    let public = identity.public();
    IdentityInfo {
        name: identity.name().to_string(),
        id: public.id.to_string(),
        fingerprint: public.fingerprint(),
        file: store.identity_path().display().to_string(),
        public_key: public.verify_key.clone(),
        avatar: store.load_avatar().unwrap_or(None),
    }
}

/// A world as the list of worlds shows it. A big world takes seconds to read:
/// until it is, the list shows it as it was last seen and reads it meanwhile.
fn summary(store: &Store, id: &str, me: &str) -> Answer<Option<serde_json::Value>> {
    use std::collections::{HashMap, HashSet};
    use std::sync::{Mutex, OnceLock};
    static WRITTEN: OnceLock<Mutex<HashMap<String, Vec<u8>>>> = OnceLock::new();
    static READING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let id = id.to_string();
    let file = store.private_world_dir(&id).join("summary.json");
    if !store.journal_is_open(&id) {
        if let Some(known) = std::fs::read(&file).ok().and_then(|raw| serde_json::from_slice::<serde_json::Value>(&raw).ok()) {
            if READING.get_or_init(Default::default).lock().map_err(AppError::from)?.insert(id.clone()) {
                let (store, world) = (store.clone(), id.clone());
                std::thread::spawn(move || {
                    let _ = store.article_count(&world);
                    if let Ok(mut reading) = READING.get_or_init(Default::default).lock() {
                        reading.remove(&world);
                    }
                });
            }
            return Ok(Some(known));
        }
    }
    let Ok(meta) = store.load_world_meta(&id) else { return Ok(None) };
    let info = serde_json::to_value(world_info(store, &meta, me)?)?;
    let bytes = serde_json::to_vec(&info)?;
    let mut written = WRITTEN.get_or_init(Default::default).lock().map_err(AppError::from)?;
    if written.get(&id) != Some(&bytes) {
        aw_store::durable::replace(&file, &bytes)?;
        written.insert(id, bytes);
    }
    Ok(Some(info))
}

fn world_info(store: &Store, meta: &WorldMeta, me: &str) -> Result<WorldInfo, AppError> {
    let id = meta.public.world_id.to_string();
    // The game master is whoever is listed as such *and* holds the world
    // token. Without the token the entry is useless — the keys are missing.
    let is_gm = store.is_gm(&id) && meta.role_of(me) == Role::Gm;
    let (supporters, sealed) = store.with_journal(&id, |j| Ok((j.supporters(), j.keys_for(me).is_some())))?;
    let supporter = supporters.iter().any(|s| s == me);
    // A supporter sees the members as the GM does — from the GM's keys sealed for them.
    let roster = if is_gm || (supporter && sealed) { store.roster(&id).ok() } else { None };
    let members = match &roster {
        Some(r) if !is_gm => &r.members,
        _ => &meta.members,
    };
    let attributes = if is_gm {
        store
            .load_keyring(&id)?
            .attributes()
            .map(|a| a.to_string())
            .collect()
    } else {
        meta.member(me)
            .map(|m| m.attributes.clone())
            .unwrap_or_default()
    };

    Ok(WorldInfo {
        name: meta.public.name.clone(),
        icon: meta.icon.clone(),
        public_key: meta.public.owner_key.clone(),
        token_file: store.private_world_dir(&id).join("owner.key").display().to_string(),
        folder: store.world_dir(&id).display().to_string(),
        role: role_text(if is_gm { Role::Gm } else { Role::Player }).to_string(),
        is_gm,
        attributes,
        members: members
            .iter()
            .map(|m| {
                Ok(MemberInfo {
                    id: m.identity.id.to_string(),
                    name: m.identity.name.clone(),
                    fingerprint: m.identity.fingerprint(),
                    role: role_text(m.role).to_string(),
                    attributes: m.attributes.clone(),
                    trusted: store.trusted(&id, m.identity.id.as_str())?,
                })
            })
            .collect::<Result<_, AppError>>()?,
        article_count: store.article_count(&id)?,
        home: meta.home.clone(),
        requests: if is_gm || supporter { store.load_requests(&id)?.len() } else { 0 },
        invited: if is_gm { meta.invited.clone() } else { Vec::new() },
        takeovers: if is_gm { store.takeovers(&id)? } else { Default::default() },
        colours: store.colours(&id)?,
        handover: if is_gm { store.handing_to(&id)? } else { None },
        moderated: store.moderated(&id)?,
        version_limit: store.version_limit(&id)?,
        encrypted: store.is_encrypted(&id)?,
        readable: readable::place(store, &id)?.map(|p| p.display().to_string()).unwrap_or_default(),
        readable_in_world: readable::in_world(store, &id)?,
        copies: store.copies(&id)?,
        fleeting: store.is_fleeting(&id),
        server: store.load_network()?.home_of(&id).cloned().unwrap_or_default(),
        server_id: store.load_network()?.homes.get(&id).cloned().unwrap_or_default(),
        offered: is_gm && store.load_network()?.shared.contains(&id),
        dice: store.dice(&id)?,
        listed: store.on_the_list(&id)?,
        waiting: store.waiting(&id)?.len(),
        conflicts: store.conflicts(&id)?.len(),
        calendar: meta.calendar.clone(),
        chosen: if is_gm { meta.supporters.clone() } else { Vec::new() },
        admins: if is_gm { store.server_admins(&id)?.into_iter().map(|a| a.name).collect() } else { Vec::new() },
        can_take_over: supporter && sealed && !is_gm,
        roles: roster.map(|r| r.ring.attributes().map(|a| a.to_string()).collect()).unwrap_or_default(),
        supporter,
        supporters,
        id,
    })
}

// --- Identity -----------------------------------------------------------

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
fn identity_status(app: tauri::AppHandle) -> Answer<Option<IdentityInfo>> {
    let store = store(&app)?;
    if !store.has_identity() {
        return Ok(None);
    }
    let identity = store.load_identity()?;
    Ok(Some(info_from(&store, &identity)))
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
fn identity_create(app: tauri::AppHandle, name: String) -> Answer<IdentityInfo> {
    let store = store(&app)?;
    let identity = Identity::create(name)?;
    store.save_identity(&identity)?;
    Ok(info_from(&store, &identity))
}

#[cfg(feature = "gui")]
/// Renaming does not change the id — access hangs on the key.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn identity_rename(app: tauri::AppHandle, name: String) -> Answer<IdentityInfo> {
    let store = store(&app)?;
    let mut identity = store.load_identity()?;
    identity.rename(name);
    store.save_identity(&identity)?;
    Ok(info_from(&store, &identity))
}

#[cfg(feature = "gui")]
/// Writes the public part of one's own identity to a file.
///
/// That is the player card: you send it to the game master so they can
/// admit you. No private keys are in it.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn identity_export(app: tauri::AppHandle, path: String) -> Answer<String> {
    let store = store(&app)?;
    let identity = store.load_identity()?;
    let path = with_extension(&path, "awid");
    std::fs::write(&path, serde_json::to_vec_pretty(&identity.public()).map_err(AppError::from)?)
        .map_err(AppError::from)?;
    Ok(path.display().to_string())
}

fn with_extension(path: &str, extension: &str) -> PathBuf {
    let p = Path::new(path);
    if p.extension().is_some() {
        p.to_path_buf()
    } else {
        p.with_extension(extension)
    }
}

#[cfg(feature = "gui")]
/// Sets the avatar from an image file.
///
/// The image is copied and returned as a `data:` URL — so it lives in our own
/// storage and not as a reference to someone else's file that may have moved
/// by tomorrow.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn avatar_set(app: tauri::AppHandle, path: String) -> Answer<String> {
    const MAX: u64 = 4 * 1024 * 1024;
    let store = store(&app)?;
    let meta = std::fs::metadata(&path).map_err(AppError::from)?;
    if meta.len() > MAX {
        return Err(AppError(format!(
            "The image is {:.1} MB — 4 MB at most, please.",
            meta.len() as f64 / 1_048_576.0
        )));
    }
    let media_type = match Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => return Err(AppError("Only PNG, JPEG, GIF or WebP.".into())),
    };
    let bytes = std::fs::read(&path).map_err(AppError::from)?;
    Ok(store.save_avatar(&bytes, media_type)?)
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
fn avatar_remove(app: tauri::AppHandle) -> Answer<()> {
    Ok(store(&app)?.remove_avatar()?)
}

// --- Worlds -------------------------------------------------------------

#[cfg(feature = "gui")]
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn worlds_list(app: tauri::AppHandle) -> Answer<Vec<serde_json::Value>> {
    let store = store(&app)?.for_sync();
    let me = store.load_identity()?.id().to_string();
    let mut out = Vec::new();
    for id in store.held_worlds()? {
        if unfinished_copy(&store, &id) {
            continue;
        }
        let Some(mut info) = summary(&store, &id, &me)? else { continue };
        if online::is_server_world(&store, &id) {
            info["online"] = true.into();
        }
        out.push(info);
    }
    out.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    // Worked on online: as their host last showed them — and where it is.
    let settings = store.load_network()?;
    out.extend(network::online_worlds(&store).into_iter().filter(|w| !store.holds(&w.world)).map(|mut w| {
        // The server as it is listed now — its addresses may have changed since.
        let endpoint = w.server.as_deref().and_then(|c| c.split('@').next()).unwrap_or_default();
        let code = settings.servers.iter().find(|c| !endpoint.is_empty() && c.split('@').next() == Some(endpoint));
        w.info["server"] = code.cloned().unwrap_or_default().into();
        w.info["server_id"] = endpoint.to_string().into();
        w.info
    }));
    Ok(out)
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Destination {
    Local { path: String },
    /// On a server, by its code: it lives there; this device keeps the GM's copy.
    Server { server: String },
}

#[cfg(feature = "gui")]
/// Creates a world at the chosen location — a folder here, or one a cloud drive mounts.
#[cfg_attr(feature = "gui", tauri::command(async))]
async fn world_create(app: tauri::AppHandle, name: String, destination: Destination) -> Answer<WorldInfo> {
    let store = store(&app)?.for_sync();
    tauri::async_runtime::spawn_blocking(move || match destination {
        Destination::Local { path } => {
            let store = store.destination(Path::new(&path))?;
            let identity = store.load_identity()?;
            let meta = new_world(&store, &identity, name)?;
            world_info(&store, &meta, identity.id().as_str())
        }
        Destination::Server { server } => {
            let identity = store.load_identity()?;
            let meta = new_world(&store, &identity, name)?;
            network::live_on(&store, meta.public.world_id.as_str(), Some(server))?;
            world_info(&store, &meta, identity.id().as_str())
        }
    }).await.map_err(AppError::from)?
}

#[cfg(feature = "gui")]
/// Copies a world — into a folder here (none: the usual one), or onto a
/// server: a world of its own, with the types, documents and images, without
/// its players and versions. Tells how far it is (`copy-progress`). Broken
/// off, the half copy goes again — at once, or at the next start.
#[cfg_attr(feature = "gui", tauri::command(async))]
async fn world_copy(app: tauri::AppHandle, world: String, destination: Destination) -> Answer<WorldInfo> {
    use tauri::Emitter;
    let store = store(&app)?;
    let home = profile::store(&app)?.for_sync();
    tauri::async_runtime::spawn_blocking(move || {
        if !store.is_gm(&world) {
            return Err(AppError("Only the GM copies a world.".into()));
        }
        let identity = store.load_identity()?;
        let (target, server) = match destination {
            Destination::Local { path } if path.is_empty() => (home.clone(), None),
            Destination::Local { path } => (home.destination(Path::new(&path))?, None),
            Destination::Server { server } => (home.clone(), Some(server)),
        };
        let meta = new_world(&target, &identity, copy_name(&store, &store.load_world_meta(&world)?.public.name)?)?;
        let id = meta.public.world_id.to_string();
        let _copying = Copying::start(&id);
        if let Some(server) = server {
            network::live_on(&target, &id, Some(server))?;
        }
        // Everything, through a package — kept among the copy's own files meanwhile.
        let types: Vec<String> = store.load_templates(&world)?.into_iter().map(|t| t.id).collect();
        let choice = transfer::Choice {
            name: meta.public.name.clone(),
            types: types.clone(),
            documents_of: types,
            documents: store.load_articles(&world)?.iter().map(|a| a.id.to_string()).collect(),
        };
        let path = target.private_world_dir(&id).join("copying.awpack");
        let packing = |what: &str, done: usize, total: usize| {
            let _ = app.emit("copy-progress", (what, done, total));
        };
        let copied = transfer::export(&store, &identity, &world, &choice, &path, &packing).and_then(|_| {
            let (folder, package) = transfer::read(&path)?;
            let tell = |done: usize, total: usize| {
                let _ = app.emit("copy-progress", ("writing", done, total));
            };
            transfer::import(&target, &identity, &id, &folder, &package, &tell)
        });
        if copied.is_err() {
            let _ = target.delete_world(&id);
        }
        let _ = std::fs::remove_dir_all(&path);
        copied?;
        world_info(&target, &target.load_world_meta(&id)?, identity.id().as_str())
    })
    .await
    .map_err(AppError::from)?
}

#[cfg(feature = "gui")]
/// Moves a GM's world: into another folder here — off its server, if it was
/// on one — or onto a server, which keeps it for the players; the GM's copy
/// stays in its folder then. Tells how far a folder is (`copy-progress`).
#[cfg_attr(feature = "gui", tauri::command(async))]
async fn world_move(app: tauri::AppHandle, world: String, destination: Destination) -> Answer<WorldInfo> {
    use tauri::Emitter;
    let store = store(&app)?.for_sync();
    if !store.is_gm(&world) {
        return Err(AppError("Only the GM moves a world.".into()));
    }
    match destination {
        Destination::Server { server } => network::rehome(&store, &world, Some(server)).await?,
        Destination::Local { path } => {
            network::rehome(&store, &world, None).await?;
            if path.is_empty() {
                return world_info(&store, &store.load_world_meta(&world)?, store.load_identity()?.id().as_str());
            }
            let (moving, world) = (store.clone(), world.clone());
            tauri::async_runtime::spawn_blocking(move || {
                let tell = |done: usize, total: usize| {
                    let _ = app.emit("copy-progress", ("moving", done, total));
                };
                moving.move_world(&world, Path::new(&path), &tell)
            })
            .await
            .map_err(AppError::from)??;
        }
    }
    let identity = store.load_identity()?;
    world_info(&store, &store.load_world_meta(&world)?, identity.id().as_str())
}

/// Copies being made in this run, by the new world's id.
fn copying() -> &'static std::sync::Mutex<std::collections::HashSet<String>> {
    static COPYING: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> = std::sync::OnceLock::new();
    COPYING.get_or_init(Default::default)
}

struct Copying(String);

impl Copying {
    fn start(id: &str) -> Self {
        if let Ok(mut all) = copying().lock() {
            all.insert(id.to_string());
        }
        Copying(id.to_string())
    }
}

impl Drop for Copying {
    fn drop(&mut self) {
        if let Ok(mut all) = copying().lock() {
            all.remove(&self.0);
        }
    }
}

/// A copy left half made — the app closed while copying: it goes.
fn unfinished_copy(store: &Store, world: &str) -> bool {
    let left = store.private_world_dir(world).join("copying.awpack").exists() && !copying().lock().is_ok_and(|c| c.contains(world));
    left && store.delete_world(world).is_ok()
}

/// "Name (copy)" — or with a number, where that is taken.
fn copy_name(store: &Store, name: &str) -> Answer<String> {
    let taken: Vec<String> = store.list_worlds()?.iter().map(|m| m.public.name.trim().to_lowercase()).collect();
    let mut n = 1;
    loop {
        let name = if n == 1 { format!("{} (copy)", name.trim()) } else { format!("{} (copy {n})", name.trim()) };
        if !taken.contains(&name.to_lowercase()) {
            return Ok(name);
        }
        n += 1;
    }
}

/// Creates a world with its token, keyring and its own page.
fn new_world(store: &Store, identity: &Identity, name: String) -> Answer<WorldMeta> {
    let name = name.trim().to_string();
    if name.is_empty() { return Err(AppError("Enter a world name.".into())); }
    // No two worlds of one name — in the menu they could not be told apart.
    if store.list_worlds()?.iter().any(|m| m.public.name.trim().eq_ignore_ascii_case(name.trim())) {
        return Err(AppError(format!("A world called “{}” already exists.", name.trim())));
    }
    let token = aw_crypto::WorldToken::create(name)?;
    let id = token.world_id().to_string();

    let mut ring = Keyring::new();
    for a in ["gm", "players", "all"] {
        ring.ensure(&a.parse::<Attribute>().map_err(AppError::from)?)?;
    }

    // The creator is listed as game master — with every attribute a world
    // has from the start.
    let gm = Member {
        identity: identity.public(),
        role: Role::Gm,
        attributes: vec!["gm".into(), "players".into(), "all".into()],
    };

    let meta = WorldMeta {
        public: token.public(),
        calendar: WorldCalendar::default(),
        icon: store::WORLD_ICON.to_string(),
        owner: identity.id().to_string(),
        home: String::new(),
        members: vec![gm],
        invited: Vec::new(),
        supporters: Vec::new(),
    };

    store.save_token(&token)?;
    store.save_keyring(&id, &ring)?;
    store.save_world_meta(&meta)?;

    // The world is a document too — its page is the first thing one sees
    // inside it. It comes into being with the world, not on request.
    let mut meta = meta;
    meta.home = create_page(store, &id, "home", &meta.public.name.clone(), String::new())?;
    store.save_world_meta(&meta)?;
    Ok(meta)
}

/// Creates a page: a document without a type, starting with one block —
/// built like every other document.
fn create_page(store: &Store, world: &str, slug: &str, name: &str, text: String) -> Answer<String> {
    create_document(
        store,
        world,
        String::new(),
        slug.into(),
        name.into(),
        "all".into(),
        vec![NewLayer {
            title: String::new(),
            text,
            policy: "all".into(),
            level: "open".into(),
            section: aw_model::template_text::new_id(),
            edit: "gm".into(),
            fields: serde_json::Value::Null,
            parent: None,
        }],
    )
}

/// A type's page — it comes into being with the type.
fn create_type_page(store: &Store, world: &str, template: &Template) -> Answer<String> {
    // Stored as text is stored: the type by its id.
    let text = format!("<list type={} columns=2>", template.id);
    create_page(store, world, &format!("{}-page", template.id), &template.name, text)
}

/// A world as a file — for handing it over without a network: the records
/// the member may have, exactly as they were signed.
#[derive(Serialize, Deserialize)]
struct WorldFile {
    world: String,
    member: String,
    records: Vec<String>,
}

#[cfg(feature = "gui")]
/// Takes in a world the game master handed over — or what is new in it.
///
/// Every record is checked as if it came over the network: a forged world
/// leaves nothing behind.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_open(app: tauri::AppHandle, path: String, location: String) -> Answer<WorldInfo> {
    let store = store(&app)?.destination(Path::new(&location))?;
    let identity = store.load_identity()?;
    let raw = std::fs::read(&path).map_err(AppError::from)?;
    // A world handed over: from now on this device is its game master.
    if let Ok(package) = serde_json::from_slice::<store::HandoverPackage>(&raw) {
        if online::requires_online(&store, &package.world) {
            return Err(AppError("This is an online world. A handover file cannot replace its server session.".into()));
        }
        let world = store.handover_take(&package, &identity)?;
        network::offer_taken_over(&store, &world)?;
        return world_info(&store, &store.load_world_meta(&world)?, identity.id().as_str());
    }
    let file: WorldFile = serde_json::from_slice(&raw).map_err(AppError::from)?;
    if file.member != identity.id().as_str() {
        return Err(AppError("This world file was made out to someone else.".into()));
    }
    if online::requires_online(&store, &file.world) {
        return Err(AppError("This is already an online world. Make an explicit local copy to open it separately.".into()));
    }
    store.receive(&file.world, &file.records)?;
    let meta = store.load_world_meta(&file.world)?;
    world_info(&store, &meta, identity.id().as_str())
}

#[cfg(feature = "gui")]
/// Admits a player to the world — from their player card.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn member_add(
    app: tauri::AppHandle,
    world: String,
    path: String,
    attributes: Vec<String>,
) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    let raw = std::fs::read(&path).map_err(AppError::from)?;
    let card: IdentityPublic = serde_json::from_slice(&raw).map_err(AppError::from)?;
    let meta = admit(&store, &world, card, attributes)?;
    world_info(&store, &meta, me.id().as_str())
}

/// Takes a player in — however they came: a file, an id let in ahead of
/// time, a request on the network. They get `players` and `all`, plus what
/// is asked for. The game master's, or a supporter's.
pub(crate) fn admit(
    store: &Store,
    world: &str,
    card: IdentityPublic,
    attributes: Vec<String>,
) -> Answer<WorldMeta> {
    if !card.is_genuine() {
        return Err(AppError("This player id does not match its keys.".into()));
    }
    let gm = store.is_gm(world);
    if !gm && !store.supports(world)? {
        return Err(AppError("Only the GM or a supporter lets players in.".into()));
    }
    if !gm && attributes.iter().any(|a| a == "gm") {
        return Err(AppError("Only the GM makes someone GM.".into()));
    }
    let before = store.roster(world)?;
    let mut after = before.clone();
    let id = card.id.to_string();
    // Taking someone's place: what was theirs is theirs now, and the old id leaves.
    let replaced = if gm { store.takeovers(world)?.get(&id).and_then(|old| before.member(old)).filter(|m| m.role == Role::Player).cloned() } else { None };
    let mut attributes = attributes;
    if let Some(old) = &replaced {
        attributes.extend(old.attributes.iter().cloned());
        after.remove(old.identity.id.as_str())?;
    }
    after.admit(card, &attributes)?;
    store.keep_roster(world, &before, &after)?;
    if gm && store.takeovers(world)?.contains_key(&id) {
        store.set_takeover(world, &id, None)?;
    }
    Ok(store.load_world_meta(world)?)
}

#[cfg(feature = "gui")]
/// One's own colour in a world — on one's name and the documents assigned to one.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn member_colour_set(app: tauri::AppHandle, world: String, colour: String) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    store.set_badge(&world, &colour)?;
    world_info(&store, &store.load_world_meta(&world)?, me.id().as_str())
}

#[cfg(feature = "gui")]
/// A player's new id takes the place of their old one — for someone whose
/// device was set up anew without their keys. What was assigned to the old
/// id goes to the new one, and the old one leaves: its keys turn over. A
/// new id not let in yet takes over as soon as it is.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn member_replace(app: tauri::AppHandle, world: String, old: String, new: String) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    let meta = replace_member(&store, &world, &old, &new)?;
    world_info(&store, &meta, me.id().as_str())
}

pub(crate) fn replace_member(store: &Store, world: &str, old: &str, new: &str) -> Answer<WorldMeta> {
    if !store.is_gm(world) {
        return Err(AppError("Only the GM gives a player a new id.".into()));
    }
    let new = new.trim();
    let valid = new.strip_prefix("id:").is_some_and(|body| body.len() == 16 && body.chars().all(|c| c.is_ascii_hexdigit()));
    if !valid {
        return Err(AppError("That is no player id. It is the whole id — id: and 16 characters.".into()));
    }
    let mut meta = store.load_world_meta(world)?;
    if new == old || !meta.member(old).is_some_and(|m| m.role == Role::Player) {
        return Err(AppError("There is no such player.".into()));
    }
    store.set_takeover(world, new, Some(old))?;
    // Let in already, or asking: at once.
    if let Some(member) = meta.member(new).cloned() {
        return admit(store, world, member.identity, member.attributes);
    }
    if let Some(request) = store.load_requests(world)?.into_iter().find(|r| r.identity.id.as_str() == new) {
        return admit(store, world, request.identity, Vec::new());
    }
    // Otherwise once it shows up, as anyone let in ahead of time.
    if !meta.invited.iter().any(|i| i == new) {
        meta.invited.push(new.to_string());
        store.save_world_meta(&meta)?;
    }
    Ok(meta)
}

#[cfg(feature = "gui")]
/// Ties up the world file for a member.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_share(
    app: tauri::AppHandle,
    world: String,
    member: String,
    path: String,
) -> Answer<String> {
    let store = store(&app)?;
    if store.load_world_meta(&world)?.member(&member).is_none() {
        return Err(AppError("Only a member gets the world.".into()));
    }
    let file = WorldFile { records: store.all_records(&world)?, world, member };
    let path = with_extension(&path, "awworld");
    std::fs::write(&path, serde_json::to_vec(&file).map_err(AppError::from)?).map_err(AppError::from)?;
    Ok(path.display().to_string())
}

#[cfg(feature = "gui")]
/// Takes a member out of the world — or an id let in ahead of time.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn member_remove(app: tauri::AppHandle, world: String, id: String) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    let meta = store.remove_member(&world, &id)?;
    world_info(&store, &meta, me.id().as_str())
}

#[cfg(feature = "gui")]
/// Switches review on or off: whether what players write waits for the GM.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_moderation_set(app: tauri::AppHandle, world: String, on: bool) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    if !store.is_gm(&world) {
        return Err(AppError("Only the GM decides about review.".into()));
    }
    store.set_moderation(&world, on)?;
    world_info(&store, &store.load_world_meta(&world)?, me.id().as_str())
}

#[cfg(feature = "gui")]
/// Sets how many versions of each document stay — the GM's.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_versions_set(app: tauri::AppHandle, world: String, max: u32) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    if !store.is_gm(&world) {
        return Err(AppError("Only the GM decides how many versions stay.".into()));
    }
    store.set_version_limit(&world, max)?;
    world_info(&store, &store.load_world_meta(&world)?, me.id().as_str())
}

#[cfg(feature = "gui")]
/// Starts the app anew — after an update. An AppImage's old mount stays as
/// long as anything holds what it was started with: the new one is started
/// with none of the old one's open files.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn app_restart(app: tauri::AppHandle) {
    #[cfg(target_os = "linux")]
    if let Some(image) = std::env::var_os("APPIMAGE") {
        use std::os::unix::process::CommandExt;
        let mut start = std::process::Command::new(image);
        start.current_dir("/");
        // SAFETY: between fork and exec only a system call that cannot allocate.
        unsafe {
            start.pre_exec(|| {
                libc::close_range(3, libc::c_uint::MAX, 0);
                Ok(())
            });
        }
        if start.spawn().is_ok() {
            app.exit(0);
            return;
        }
    }
    app.restart();
}

#[cfg(feature = "gui")]
/// Whether the world's server names it to everyone who asks — the GM's.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_listed_set(app: tauri::AppHandle, world: String, listed: bool) -> Answer<WorldInfo> {
    let store = store(&app)?;
    if !store.is_gm(&world) {
        return Err(AppError("Only the GM decides who sees the world.".into()));
    }
    store.set_listed(&world, listed)?;
    world_info(&store, &store.load_world_meta(&world)?, store.load_identity()?.id().as_str())
}

#[cfg(feature = "gui")]
/// Lets go of what newer records replaced — the GM's. How many went.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_tidy(app: tauri::AppHandle, world: String) -> Answer<usize> {
    let store = store(&app)?;
    if !store.is_gm(&world) {
        return Err(AppError("Only the GM tidies the world.".into()));
    }
    Ok(store.tidy(&world)?)
}

#[cfg(feature = "gui")]
/// How far switching the world's encryption is — asked while it runs.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_switch_progress(app: tauri::AppHandle, world: String) -> Answer<Option<store::SwitchProgress>> {
    let _ = store(&app)?;
    Ok(store::switch_progress(&world))
}

#[cfg(feature = "gui")]
/// Keeps a readable copy of the world beside the profile, or not.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_readable_set(app: tauri::AppHandle, world: String, on: bool) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    if on && store.is_fleeting(&world) {
        return Err(AppError("This world is held only while online.".into()));
    }
    readable::set_own(&store, &world, on)?;
    world_info(&store, &store.load_world_meta(&world)?, me.id().as_str())
}

#[cfg(feature = "gui")]
/// Encrypts the world or keeps it as it is — the GM's. Every part begins
/// anew in the other form, which takes its while in a big world.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_encryption_set(app: tauri::AppHandle, world: String, on: bool) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    store.set_encrypted(&world, on)?;
    world_info(&store, &store.load_world_meta(&world)?, me.id().as_str())
}

#[cfg(feature = "gui")]
/// Sets the dice the world rolls with — the GM's. Each between 2 and 1000
/// sides, each once, in the order given.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_dice_set(app: tauri::AppHandle, world: String, sides: Vec<u32>) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    if !store.is_gm(&world) {
        return Err(AppError("Only the GM chooses the world's dice.".into()));
    }
    let mut kept: Vec<u32> = Vec::new();
    for n in sides.into_iter().filter(|n| (2..=1000).contains(n)) {
        if !kept.contains(&n) {
            kept.push(n);
        }
    }
    store.set_dice(&world, kept)?;
    world_info(&store, &store.load_world_meta(&world)?, me.id().as_str())
}

#[cfg(feature = "gui")]
/// The side bar's entries as far as this device may see them — `None` while
/// the game master has set none.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn navigation_load(app: tauri::AppHandle, world: String) -> Answer<Option<Vec<aw_model::navigation::NavEntry>>> {
    navigation_load_in(&store(&app)?, world)
}

/// `navigation_load` for whom the store is given — the host, acting for a player.
pub(crate) fn navigation_load_in(store: &Store, world: String) -> Answer<Option<Vec<aw_model::navigation::NavEntry>>> {
    Ok(store.clone().navigation(&world)?)
}

#[cfg(feature = "gui")]
/// Sets the side bar's entries — the GM's.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn navigation_set(app: tauri::AppHandle, world: String, entries: Vec<aw_model::navigation::NavEntry>) -> Answer<()> {
    store(&app)?.set_navigation(&world, &entries)?;
    Ok(())
}

#[cfg(feature = "gui")]
/// Trusts a member — their writing counts without review — or no longer.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn member_trust_set(app: tauri::AppHandle, world: String, member: String, trusted: bool) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    if !store.is_gm(&world) {
        return Err(AppError("Only the GM decides about review.".into()));
    }
    let meta = store.load_world_meta(&world)?;
    if meta.member(&member).is_none() {
        return Err(AppError("There is no such member.".into()));
    }
    store.set_trust(&world, &member, trusted)?;
    world_info(&store, &meta, me.id().as_str())
}

#[cfg(feature = "gui")]
/// Makes these players the world's supporters — the GM's.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_supporters_set(app: tauri::AppHandle, world: String, ids: Vec<String>) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    let meta = store.set_supporters(&world, ids)?;
    world_info(&store, &meta, me.id().as_str())
}

#[cfg(feature = "gui")]
/// Takes the GM's place, as a supporter — with the keys the GM sealed for
/// this device's member. Everyone sees it.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_take_over(app: tauri::AppHandle, world: String) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    if !store.supports(&world)? {
        return Err(AppError("Only a supporter of the world can take the GM's place.".into()));
    }
    store.take_over(&world, &me)?;
    world_info(&store, &store.load_world_meta(&world)?, me.id().as_str())
}

#[cfg(feature = "gui")]
/// Hands the world over to a member — their device takes it over with the
/// keys sealed for them, as soon as it reads the world — or, without
/// `member`, takes that back.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn gm_handover(app: tauri::AppHandle, world: String, member: Option<String>) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    let meta = store.load_world_meta(&world)?;
    if !store.is_gm(&world) {
        return Err(AppError("Only the GM can hand the world over.".into()));
    }
    if let Some(member) = &member {
        if member == me.id().as_str() || meta.member(member).is_none() {
            return Err(AppError("The world goes only to another member.".into()));
        }
    }
    store.hand_over(&world, member.as_deref())?;
    world_info(&store, &store.load_world_meta(&world)?, me.id().as_str())
}

#[cfg(feature = "gui")]
/// Hands the world over as a file, sealed for the member alone. This device
/// steps down at once — keep the file safe until it has arrived.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn gm_handover_file(app: tauri::AppHandle, world: String, member: String, path: String) -> Answer<String> {
    let store = store(&app)?;
    let meta = store.load_world_meta(&world)?;
    let to = meta
        .member(&member)
        .filter(|m| m.role != Role::Gm)
        .ok_or_else(|| AppError("The world goes only to another member.".into()))?;
    let package = store.handover_pack(&world, &to.identity)?;
    let path = with_extension(&path, "awworld");
    std::fs::write(&path, serde_json::to_vec(&package).map_err(AppError::from)?).map_err(AppError::from)?;
    store.step_down(&world)?;
    network::republish();
    Ok(path.display().to_string())
}

#[cfg(feature = "gui")]
/// Deletes a world for good.
///
/// `confirmation` has to be the world's name. The check deliberately lives
/// here too and not only in the interface: what deletes the world token,
/// nobody brings back.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_delete(app: tauri::AppHandle, world: String, confirmation: String) -> Answer<()> {
    let store = store(&app)?;
    // Worked on online: only this device's note of it goes; the world stays with its host.
    if !store.holds(&world) && network::online_worlds(&store).iter().any(|w| w.world == world) {
        return network::forget_online(&store, &world);
    }
    let meta = store.load_world_meta(&world)?;
    if confirmation.trim() != meta.public.name.trim() {
        return Err(AppError(format!(
            "To delete, the name has to match exactly: {:?}",
            meta.public.name
        )));
    }
    store.delete_world(&world)?;
    Ok(())
}

#[cfg(feature = "gui")]
/// Sets the world's emoji.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_icon_set(app: tauri::AppHandle, world: String, icon: String) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    let mut meta = store.load_world_meta(&world)?;
    // A few characters are enough for any emoji including variation selectors.
    meta.icon = icon.chars().take(4).collect();
    store.save_world_meta(&meta)?;
    world_info(&store, &meta, me.id().as_str())
}

#[cfg(feature = "gui")]
/// Sets how the world counts its days — the game master's. Dates already
/// written stay the same days; they are only written in the new calendar.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn calendar_set(app: tauri::AppHandle, world: String, calendar: WorldCalendar) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    if !store.is_gm(&world) {
        return Err(AppError("Only the GM sets the calendar.".into()));
    }
    calendar.validate().map_err(AppError::from)?;
    let mut meta = store.load_world_meta(&world)?;
    meta.calendar = calendar;
    store.save_world_meta(&meta)?;
    world_info(&store, &meta, me.id().as_str())
}

#[cfg(feature = "gui")]
/// Checks whether the world token is still there and matches the public key.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_token_check(app: tauri::AppHandle, world: String) -> Answer<bool> {
    let store = store(&app)?;
    let meta = store.load_world_meta(&world)?;
    let token = store.load_token(&world)?;
    Ok(token.public() == meta.public)
}

// --- Attributes ---------------------------------------------------------

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
fn attribute_create(app: tauri::AppHandle, world: String, attribute: String) -> Answer<Vec<String>> {
    let store = store(&app)?;
    let mut ring = store.load_keyring(&world)?;
    let parsed: Attribute = attribute.parse().map_err(AppError::from)?;
    ring.ensure(&parsed)?;
    store.save_keyring(&world, &ring)?;
    Ok(ring.attributes().map(|a| a.to_string()).collect())
}

#[cfg(feature = "gui")]
/// What a player is assigned to — documents (`doc:…`: their character, a
/// class) and attributes — and so what they may see. The game master's, or
/// a supporter's for players. An attribute taken away is rotated: what is
/// written from then on is closed to them; what they had, they keep.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn member_attributes_set(app: tauri::AppHandle, world: String, member: String, attributes: Vec<String>) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    member_attributes_set_in(&store, world, member, attributes, me.id().as_str())
}

/// `member_attributes_set` for whom the store is given — the host, acting
/// for a supporter: `me`.
pub(crate) fn member_attributes_set_in(store: &Store, world: String, member: String, attributes: Vec<String>, me: &str) -> Answer<WorldInfo> {
    let gm = store.is_gm(&world);
    if !gm && !store.supports(&world)? {
        return Err(AppError("Only the GM or a supporter assigns players.".into()));
    }
    let before = store.roster(&world)?;
    // The GM too may note whom they play: they see everything anyway, but
    // handing the world over, what they play stays theirs as a player.
    if !gm && (before.member(&member).is_some_and(|m| m.role == Role::Gm) || attributes.iter().any(|a| a.trim() == "gm")) {
        return Err(AppError("Only the GM assigns the GM.".into()));
    }
    let mut after = before.clone();
    after.assign(&member, &attributes)?;
    store.keep_roster(&world, &before, &after)?;
    world_info(store, &store.load_world_meta(&world)?, me)
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
fn attribute_revoke(app: tauri::AppHandle, world: String, attribute: String) -> Answer<u32> {
    let store = store(&app)?;
    let mut ring = store.load_keyring(&world)?;
    let parsed: Attribute = attribute.parse().map_err(AppError::from)?;
    let epoch = ring.rotate(&parsed)?;
    store.save_keyring(&world, &ring)?;
    store.reseal(&world)?;
    Ok(epoch)
}

// --- Templates ----------------------------------------------------------

#[cfg(feature = "gui")]
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn templates_load(app: tauri::AppHandle, world: String) -> Answer<Vec<Template>> {
    templates_load_in(&store(&app)?, world)
}

/// `templates_load` for whom the store is given — the host, acting for a player.
pub(crate) fn templates_load_in(store: &Store, world: String) -> Answer<Vec<Template>> {
    Ok(store.clone().load_templates(&world)?)
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
fn template_save(
    app: tauri::AppHandle,
    world: String,
    template: Template,
) -> Answer<Vec<Template>> {
    let store = store(&app)?;
    let identity = store.load_identity()?;
    save_template(&store, &identity, &world, template)
}

/// The default of a property that picks documents names them — by name as
/// written, by id as stored: `Status: @@Life@@ = alive`.
fn default_documents(template: &mut Template, known: &document::Known, to_ids: bool) {
    use aw_model::names::Catalog;
    for field in &mut template.properties {
        let aw_model::FieldKind::Reference { article_type, .. } = &field.kind else { continue };
        if field.default.is_empty() {
            continue;
        }
        let of = known.type_name(article_type).unwrap_or_default();
        field.default = field
            .default
            .split(',')
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(|n| {
                if to_ids {
                    known.document_id(&of, n)
                } else {
                    known.document_name(n).map(|(_, name)| name)
                }
                .unwrap_or_else(|| n.to_string())
            })
            .collect::<Vec<_>>()
            .join(", ");
    }
}

/// Saves a type — a new one together with its page. A renamed one takes its
/// page and every text that names it along: the name is one.
fn save_template(store: &Store, identity: &Identity, world: &str, mut template: Template) -> Answer<Vec<Template>> {
    // Normalise before checking: a new type gets its id here.
    template.normalize();
    template.validate().map_err(AppError::from)?;
    let mut all = store.load_templates(world)?;
    // The texts a new document starts with are stored like any text: ids —
    // its own properties as this very template now has them.
    let mut with_this: Vec<Template> = all.iter().filter(|t| t.id != template.id).cloned().collect();
    with_this.push(template.clone());
    let known = document::known(&store.load_articles(world)?, &store.view_keys(world, identity)?, with_this);
    for part in template.blocks.iter_mut().flat_map(|b| b.parts.iter_mut()) {
        part.text = names::stored(&part.text, &known, &template.id);
    }
    // A create button beside a title names types: by id, a rename breaks nothing.
    for block in &mut template.blocks {
        block.new = document::types_as(&known, &block.new, true);
    }
    default_documents(&mut template, &known, true);

    // Supertype and inherited parent field can only be checked among the
    // others.
    let others: Vec<Template> = all.iter().filter(|t| t.id != template.id).cloned().collect();
    types::validate_hierarchy(&others, &template).map_err(AppError::from)?;

    // Two types of the same name are indistinguishable in every list. The
    // interface already catches this; it is repeated here because the command
    // is reachable without it too — when importing foreign worlds, say.
    let same_name = all.iter().any(|t| {
        t.id != template.id && t.name.trim().eq_ignore_ascii_case(template.name.trim())
    });
    if same_name {
        return Err(AppError(format!(
            "A type called “{}” already exists in this world.",
            template.name.trim()
        )));
    }

    let mut renamed: Option<(String, String, String)> = None;
    match all.iter_mut().find(|t| t.id == template.id) {
        // The page belongs to the type, not to the editor's copy of it.
        Some(existing) => {
            if existing.name != template.name {
                renamed = Some((existing.name.clone(), template.name.clone(), existing.page.clone()));
            }
            *existing = Template { page: existing.page.clone(), ..template }
        }
        // A new type comes with its page. A group has none: it has no
        // documents to list, and nothing to say of its own.
        None => {
            if !template.group {
                template.page = create_type_page(store, world, &template)?;
            }
            all.push(template);
        }
    }
    store.save_templates(world, &all)?;
    // The page carries the type's name. Nothing else needs to follow: text
    // stores the type's id.
    if let Some((_, new, page)) = renamed {
        if !page.is_empty() {
            document::rename_document(store, identity, world, &page, &new)?;
        }
    }
    Ok(store.load_templates(world)?)
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
fn template_delete(app: tauri::AppHandle, world: String, slug: String) -> Answer<Vec<Template>> {
    delete_template(&store(&app)?, &world, &slug)
}

/// Deletes a type — and its page with it.
fn delete_template(store: &Store, world: &str, slug: &str) -> Answer<Vec<Template>> {
    let mut all = store.load_templates(world)?;
    // A type with subtypes stays: they inherit from it, and without it they
    // would silently lose fields and blocks.
    let below: Vec<&str> = all
        .iter()
        .filter(|t| t.supertype == slug)
        .map(|t| t.name.as_str())
        .collect();
    if !below.is_empty() {
        return Err(AppError(format!(
            "Other types build on this one: {}. Give them another supertype first.",
            below.join(", ")
        )));
    }
    // A group in use stays too: the types would lose its properties.
    let users: Vec<&str> = all
        .iter()
        .filter(|t| t.id != slug && t.properties.iter().any(|f| matches!(&f.kind, aw_model::FieldKind::Group { group } if group == slug)))
        .map(|t| t.name.as_str())
        .collect();
    if !users.is_empty() {
        return Err(AppError(format!("These take the group in: {}. Take it out of them first.", users.join(", "))));
    }
    // Its page goes with it — it was the type's, and a page listing a type
    // that no longer exists would only show an error.
    if let Some(page) = all.iter().find(|t| t.id == slug).and_then(|t| t.page.parse::<ArticleId>().ok()) {
        store.remove(world, page, None, "")?;
    }
    all.retain(|t| t.id != slug);
    store.save_templates(world, &all)?;
    Ok(all)
}

// --- Articles -----------------------------------------------------------

#[cfg(feature = "gui")]
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn article_create(
    app: tauri::AppHandle,
    world: String,
    article_type: String,
    slug: String,
    name: String,
    visibility: String,
    mut layers: Vec<NewLayer>,
) -> Answer<String> {
    let store = store(&app)?;
    // Names as written, ids as stored — the same as when a document is saved.
    let known = known_to_me(&store, &world)?;
    for layer in &mut layers {
        layer.text = names::stored(&layer.text, &known, &article_type);
        if let Some(map) = layer.fields.as_object_mut() {
            for value in map.values_mut() {
                if let serde_json::Value::String(s) = value {
                    *s = names::stored(s, &known, &article_type);
                }
            }
        }
    }
    create_document(&store, &world, article_type, slug, name, visibility, layers)
}

/// A new document a player asks for — what `article_propose` seals for the
/// GM's device, which makes it (`answer_proposals`).
#[derive(Deserialize, Serialize)]
struct Asked {
    /// The id the document will have — chosen on the player's device, so it
    /// is theirs to open and link to before the GM's device made it.
    #[serde(default)]
    article: String,
    #[serde(default)]
    article_type: String,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    layers: Vec<NewLayer>,
    /// Or who sees parts of `article`, as one who may edit them asks it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    shown: Vec<Shown>,
}

/// Who sees a part, as an editor asks it — see `propose_shown`.
#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Shown {
    pub(crate) layer: String,
    pub(crate) view: String,
}

/// Who sees parts of a document, asked by one who may edit them and is not
/// the GM: the GM's device holds the keys and sets it when it has it
/// (`answer_proposals`), if they still may edit them.
pub(crate) fn propose_shown(store: &Store, me: &Identity, world: &str, article: &str, shown: Vec<Shown>) -> Answer<()> {
    let asked = Asked { article: article.to_string(), article_type: String::new(), slug: String::new(), name: String::new(), layers: Vec::new(), shown };
    store.propose(world, me, &serde_json::to_vec(&asked)?)?;
    Ok(())
}

/// A view asked for by an editor, set — where they still may edit the part
/// and who edits it stays as it is. Whether it was set.
fn set_shown(store: &Store, world: &str, member: &store::Member, article: &str, shown: &Shown) -> Answer<bool> {
    let held = store.view_keys(world, &store.load_identity()?)?;
    let all = store.load_articles(world)?;
    let Some(target) = all.iter().find(|a| a.id.to_string() == article) else { return Ok(false) };
    let Some(entry) = target.layers.iter().find(|l| l.id.to_string() == shown.layer) else { return Ok(false) };
    let content = layer_content(entry, &held)?;
    let mut theirs: std::collections::BTreeSet<Attribute> = member.granted_attributes().iter().filter_map(|a| a.parse().ok()).collect();
    theirs.extend(["players", "all"].iter().filter_map(|a| a.parse().ok()));
    if !content.edit.parse::<Policy>().is_ok_and(|p| p.satisfied_by(&theirs)) {
        return Ok(false);
    }
    let view = own_view(&shown.view, article, &target.article_type);
    let wanted: Policy = view.parse().map_err(AppError::from)?;
    store.ensure_attributes(world, &wanted)?;
    store.rekey(world, entry.id, &wanted, Access { section: content.section, own: view, edit: content.edit }, entry.level, "")?;
    Ok(true)
}

#[cfg(feature = "gui")]
/// A new document, by a player: sent to the GM's device, which makes it
/// when it has it — if the type lets this player make its documents
/// (`create=`). Names are stored as ids here already, as when one saves.
///
/// `article`: the document asked for again, changed — a fresh one without.
/// Returns its id.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn article_propose(
    app: tauri::AppHandle,
    world: String,
    article: Option<String>,
    article_type: String,
    slug: String,
    name: String,
    layers: Vec<NewLayer>,
) -> Answer<String> {
    let store = store(&app)?;
    let me = store.load_identity()?;
    propose_document(&store, &me, &world, article, article_type, slug, name, layers)
}

/// What `article_propose` does — without Tauri, for the tests too.
#[allow(clippy::too_many_arguments)]
pub(crate) fn propose_document(
    store: &Store,
    me: &Identity,
    world: &str,
    article: Option<String>,
    article_type: String,
    slug: String,
    name: String,
    mut layers: Vec<NewLayer>,
) -> Answer<String> {
    let article = match article {
        Some(id) => serde_json::from_value::<ArticleId>(serde_json::Value::String(id)).map_err(|_| AppError("Not a document's id.".into()))?,
        None => ArticleId::new(),
    };
    // Names as written, ids as stored — documents waiting on the GM among them.
    let known = known_to_me(store, world)?;
    for layer in &mut layers {
        layer.text = names::stored(&layer.text, &known, &article_type);
        if let Some(map) = layer.fields.as_object_mut() {
            for value in map.values_mut() {
                if let serde_json::Value::String(s) = value {
                    *s = names::stored(s, &known, &article_type);
                }
            }
        }
    }
    let asked = Asked { article: article.to_string(), article_type, slug, name: name.trim().chars().take(200).collect(), layers, shown: Vec::new() };
    store.propose(world, me, &serde_json::to_vec(&asked)?)?;
    Ok(article.to_string())
}

/// The documents this player asked for and the GM's device has not made yet
/// — on their own device, to open and to link to: the newest proposal of
/// each, as a document of its type, marked `pending`.
fn pending_views(store: &Store, world: &str, me: &Identity) -> Answer<Vec<ArticleView>> {
    if store.is_gm(world) {
        return Ok(Vec::new());
    }
    let stored: std::collections::HashSet<String> = store.load_articles(world)?.iter().map(|a| a.id.to_string()).collect();
    let templates = store.load_templates(world)?;
    let mut newest: std::collections::BTreeMap<String, (u64, Asked)> = Default::default();
    for (_, author, at, content) in store.proposals(world, me)? {
        if author.id != *me.id() {
            continue;
        }
        let Some(asked) = content.and_then(|c| serde_json::from_slice::<Asked>(&c).ok()) else { continue };
        if asked.article.is_empty() || stored.contains(&asked.article) {
            continue;
        }
        if newest.get(&asked.article).is_none_or(|(t, _)| at >= *t) {
            newest.insert(asked.article.clone(), (at, asked));
        }
    }
    Ok(newest
        .into_values()
        .map(|(_, asked)| {
            let visibility = types::find(&templates, &asked.article_type).and_then(|t| t.auth.as_ref().map(|a| a.view.clone())).unwrap_or_else(|| "all".into());
            let edit = types::find(&templates, &asked.article_type).and_then(|t| t.auth.as_ref().map(|a| a.edit.clone())).unwrap_or_else(|| "gm".into());
            let layers: Vec<LayerView> = asked
                .layers
                .into_iter()
                .map(|l| LayerView {
                    id: format!("pending:{}:{}", asked.article, l.section),
                    title: l.title,
                    section: l.section,
                    may_edit: true,
                    text: l.text,
                    fields: l.fields,
                    policy: l.policy,
                    edit: l.edit,
                    level: l.level,
                    epoch: 0,
                })
                .collect();
            ArticleView {
                id: asked.article,
                article_type: asked.article_type,
                slug: asked.slug,
                name: asked.name,
                edit,
                visibility,
                layer_total: layers.len(),
                layers,
                order: Vec::new(),
                pending: true,
            }
        })
        .collect())
}

/// Makes the documents players asked for — on the GM's device, whenever it
/// has their proposals. Each with its type's access, not the one asked:
/// the base, the properties and every part as the type sets them out; a
/// part the type does not know is left out. A proposal not allowed — no
/// member, no such type, the type not theirs to make — is answered all the
/// same: it goes, without a document. Returns how many were made.
pub(crate) fn answer_proposals(store: &Store, world: &str) -> Answer<usize> {
    if !store.is_gm(world) {
        return Ok(0);
    }
    // What supporters did becomes the GM's own word.
    store.adopt_support(world)?;
    let me = store.load_identity()?;
    let asked = store.proposals(world, &me)?;
    if asked.is_empty() {
        return Ok(0);
    }
    let meta = store.load_world_meta(world)?;
    let templates = store.load_templates(world)?;
    let stored: std::collections::HashSet<String> = store.load_articles(world)?.iter().map(|a| a.id.to_string()).collect();
    let mut answered = Vec::new();
    let mut made = 0;
    // Of a document asked for again, changed, the newest counts.
    let mut newest: std::collections::BTreeMap<String, (u64, IdentityPublic, Asked)> = Default::default();
    for (id, author, at, content) in asked {
        answered.push(id);
        let Some(asked) = content.and_then(|c| serde_json::from_slice::<Asked>(&c).ok()) else { continue };
        if !asked.shown.is_empty() {
            if let Some(member) = meta.member(author.id.as_str()) {
                for shown in &asked.shown {
                    made += usize::from(set_shown(store, world, member, &asked.article, shown)?);
                }
            }
            continue;
        }
        let key = if asked.article.is_empty() { format!("new:{}", answered.len()) } else { asked.article.clone() };
        if newest.get(&key).is_none_or(|(t, ..)| at >= *t) {
            newest.insert(key, (at, author, asked));
        }
    }
    for (_, author, asked) in newest.into_values() {
        // Made already — asked again meanwhile: what came first stands.
        if stored.contains(&asked.article) {
            continue;
        }
        let Some(member) = meta.member(author.id.as_str()) else { continue };
        let Some(own) = templates.iter().find(|t| t.id == asked.article_type && !t.group) else { continue };
        // Every player holds `players` and `all`, besides what they are assigned.
        let mut held: std::collections::BTreeSet<Attribute> = member.granted_attributes().iter().filter_map(|a| a.parse().ok()).collect();
        held.extend(["players", "all"].iter().filter_map(|a| a.parse().ok()));
        let allowed = member.role == Role::Gm || own.create.parse::<Policy>().is_ok_and(|p| !own.create.trim().is_empty() && p.satisfied_by(&held));
        if !allowed {
            continue;
        }
        let template = types::effective(&templates, own);
        let auth = template.auth.clone().unwrap_or_default();
        let layers: Vec<NewLayer> = asked
            .layers
            .into_iter()
            .filter_map(|mut layer| {
                if layer.section == document::INFOBOX_SECTION {
                    (layer.policy, layer.edit, layer.level) = (auth.view.clone(), auth.edit.clone(), "open".into());
                } else if let Some(key) = layer.section.strip_prefix(&format!("{}:", document::INFOBOX_SECTION)) {
                    let access = template.properties.iter().find(|f| f.key == key)?.access.clone()?;
                    (layer.policy, layer.edit, layer.level) = (access.view, access.edit, access.level);
                } else {
                    let (block, part) = template.blocks.iter().find_map(|b| b.parts.iter().find(|p| p.key == layer.section).map(|p| (b, p)))?;
                    (layer.policy, layer.edit, layer.level, layer.title) = (part.view.clone(), part.edit.clone(), part.level.clone(), block.title.clone());
                    layer.fields = serde_json::Value::Object(Default::default());
                }
                layer.parent = None;
                Some(layer)
            })
            .collect();
        let slug = if asked.slug.trim().is_empty() { asked.name.to_lowercase() } else { asked.slug };
        let article = serde_json::from_value::<ArticleId>(serde_json::Value::String(asked.article)).unwrap_or_else(|_| ArticleId::new());
        if create_document_as(store, world, article, own.id.clone(), slug, asked.name, auth.view.clone(), layers).is_ok() {
            made += 1;
        }
    }
    store.forget_proposals(world, answered)?;
    Ok(made)
}

/// What the one using the app knows by name in this world.
/// With the documents this player asked for and waits on: they may be linked to.
fn known_to_me(store: &Store, world: &str) -> Answer<document::Known> {
    let identity = store.load_identity()?;
    let held = store.view_keys(world, &identity)?;
    let mut views = build_view(&store.load_articles(world)?, &held);
    views.extend(pending_views(store, world, &identity)?);
    Ok(document::known_from(views, store.load_templates(world)?))
}

#[cfg(feature = "gui")]
/// Texts as they are written — ids turned back into the current names. For
/// a new document, whose texts start as its type's.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn texts_written(app: tauri::AppHandle, world: String, article_type: String, texts: Vec<String>) -> Answer<Vec<String>> {
    texts_written_in(&store(&app)?, world, article_type, texts)
}

/// `texts_written` for whom the store is given — the host, acting for a player.
pub(crate) fn texts_written_in(store: &Store, world: String, article_type: String, texts: Vec<String>) -> Answer<Vec<String>> {
    let known = known_to_me(&store.clone(), &world)?;
    Ok(texts.iter().map(|t| names::written(t, &known, &article_type)).collect())
}

/// Encrypts and stores a new document — for the command and for the world's
/// own page, which comes into being together with the world.
fn create_document(
    store: &Store,
    world: &str,
    article_type: String,
    slug: String,
    name: String,
    visibility: String,
    layers: Vec<NewLayer>,
) -> Answer<String> {
    create_document_as(store, world, ArticleId::new(), article_type, slug, name, visibility, layers)
}

/// The same, under an id chosen beforehand — a player's, whose own device
/// knew the document under it before the GM's made it.
#[allow(clippy::too_many_arguments)]
fn create_document_as(
    store: &Store,
    world: &str,
    article: ArticleId,
    article_type: String,
    slug: String,
    name: String,
    visibility: String,
    mut layers: Vec<NewLayer>,
) -> Answer<String> {
    // The id first: a part its type gives `view=self` is the document's own.
    let id = article.to_string();
    let visibility = own_view(&visibility, &id, &article_type);
    for layer in &mut layers {
        layer.policy = own_view(&layer.policy, &id, &article_type);
        layer.edit = own_view(&layer.edit, &id, &article_type);
    }
    // Nested sections inherit their parents' visibility.
    let effective = effective_policies(&layers)?;
    let view: Policy = visibility.parse().map_err(AppError::from)?;
    store.ensure_attributes(world, &view)?;
    for policy in &effective {
        store.ensure_attributes(world, policy)?;
    }
    // The base layer carries the article's name and visibility — and what
    // the article is, for its readers only.
    let mut parts = vec![NewPart {
        is_base: true,
        level: if view.to_string() == "gm" { Level::GmOnly } else { Level::Open },
        view,
        access: Access { section: String::new(), own: visibility, edit: "gm".into() },
        plain: Plain { title: name, ..Plain::default() },
    }];
    for (input, view) in layers.into_iter().zip(effective) {
        parts.push(NewPart {
            is_base: false,
            level: level_from(&input.level),
            view,
            access: Access { section: input.section, own: input.policy, edit: input.edit },
            plain: Plain { title: input.title, text: input.text, fields: fields_map(&input.fields), ..Plain::default() },
        });
    }
    Ok(store.create_article_as(world, article, article_type, slug, parts, "")?.to_string())
}

/// `self` in a view: whoever is assigned to this document — its own
/// attribute, `doc:<id>` — or to its whole type, `type:<id>`. A type says it
/// for every document of it: the sheet its player sees, and the GM. A
/// document without a type — the world's page — has only its own.
pub(crate) fn own_view(view: &str, article: &str, article_type: &str) -> String {
    let doc = format!("doc:{}", article.trim_start_matches("art:"));
    let kind = (!article_type.is_empty()).then(|| format!("type:{}", attribute_part(article_type)));
    let is_self = |w: &str| w.eq_ignore_ascii_case("self");
    // An OR of ANDs: a group naming `self` becomes two — one for the
    // document, one for its type.
    let mut groups = Vec::new();
    for group in view.split(" OR ").flat_map(|g| g.split(" or ")) {
        let words: Vec<&str> = group.split_whitespace().collect();
        if words.iter().any(|w| is_self(w)) {
            for own in std::iter::once(&doc).chain(kind.as_ref()) {
                groups.push(words.iter().map(|w| if is_self(w) { own.as_str() } else { w }).collect::<Vec<_>>().join(" "));
            }
        } else {
            groups.push(words.join(" "));
        }
    }
    groups.join(" OR ")
}

/// A type's id as part of an attribute — which allows only letters, digits,
/// `-`, `_` and `.`.
fn attribute_part(text: &str) -> String {
    text.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '-' }).collect()
}

/// Opens a layer and returns its plaintext.
fn layer_content(entry: &StoredLayer, held: &HeldKeys) -> Result<LayerContent, AppError> {
    entry.content(held).ok_or_else(|| AppError("This part cannot be opened here.".into()))
}

#[cfg(feature = "gui")]
/// Sets who may see an article or one of its sections: a new generation of
/// the layer, with fresh keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn visibility_set(
    app: tauri::AppHandle,
    world: String,
    article: String,
    layer: Option<String>,
    policy: String,
) -> Answer<()> {
    let store = store(&app)?;
    if !store.is_gm(&world) {
        return Err(AppError("Only the GM can hand out views.".into()));
    }
    let identity = store.load_identity()?;
    let held = store.view_keys(&world, &identity)?;
    let all = store.load_articles(&world)?;
    let target = all
        .iter()
        .find(|a| a.id.to_string() == article)
        .ok_or_else(|| AppError("There is no such article.".into()))?;
    let policy = own_view(&policy, &article, &target.article_type);
    let wanted: Policy = policy.parse().map_err(AppError::from)?;
    store.ensure_attributes(&world, &wanted)?;
    let entry = match &layer {
        Some(id) => target.layers.iter().find(|l| l.id.to_string() == *id),
        None => target.layers.iter().find(|l| l.is_base),
    }
    .ok_or_else(|| AppError("There is no such part.".into()))?;
    // The section key and who may edit live in the encrypted part.
    let content = layer_content(entry, &held)?;
    store.rekey(
        &world,
        entry.id,
        &wanted,
        Access { section: content.section, own: policy, edit: content.edit },
        entry.level,
        "",
    )?;
    Ok(())
}

/// A type's access, given to documents of it that are there already: who
/// sees and who edits each of `articles` — and each section of them that
/// had the same access as its document; a section with access of its own
/// keeps it. The game master's. How many documents changed.
pub(crate) fn type_access_apply_in(store: &Store, world: &str, article_type: &str, articles: &[String], view: Option<String>, edit: Option<String>) -> Answer<usize> {
    if !store.is_gm(world) {
        return Err(AppError("Only the GM sets who sees and edits documents.".into()));
    }
    let identity = store.load_identity()?;
    let held = store.view_keys(world, &identity)?;
    let all = store.load_articles(world)?;
    let same = |a: &str, b: &str| a == b || a.parse::<Policy>().ok().is_some_and(|a| b.parse::<Policy>().ok() == Some(a));
    let mut changed = 0;
    for article in all.iter().filter(|a| a.article_type == article_type && articles.contains(&a.id.to_string())) {
        let id = article.id.to_string();
        let base = article.layers.iter().find(|l| l.is_base).ok_or_else(|| AppError("A document without its base.".into()))?;
        let base = layer_content(base, &held)?;
        let view = view.as_deref().map(|v| own_view(v, &id, &article.article_type));
        let edit = edit.as_deref().map(|e| own_view(e, &id, &article.article_type));
        let mut touched = false;
        for layer in &article.layers {
            let content = layer_content(layer, &held)?;
            // A section with access of its own — a secret, say — keeps it.
            if !(same(&content.own, &base.own) && same(&content.edit, &base.edit)) {
                continue;
            }
            let own = view.clone().unwrap_or_else(|| content.own.clone());
            let rule = edit.clone().unwrap_or_else(|| content.edit.clone());
            if same(&own, &content.own) && same(&rule, &content.edit) {
                continue;
            }
            let wanted: Policy = own.parse().map_err(AppError::from)?;
            store.ensure_attributes(world, &wanted)?;
            store.ensure_attributes(world, &rule.parse().map_err(AppError::from)?)?;
            store.rekey(world, layer.id, &wanted, Access { section: content.section, own, edit: rule }, layer.level, "")?;
            touched = true;
        }
        changed += usize::from(touched);
    }
    Ok(changed)
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
fn type_access_apply(app: tauri::AppHandle, world: String, article_type: String, articles: Vec<String>, view: Option<String>, edit: Option<String>) -> Answer<usize> {
    type_access_apply_in(&store(&app)?, &world, &article_type, &articles, view, edit)
}

#[cfg(feature = "gui")]
/// Deletes an article — for good.
///
/// Only with the full keyring, so only the game master: an article consists
/// of sections that different people may see. Whoever can open only one of
/// them should not be able to throw the others away with it.
///
/// `confirmation` has to be the article's name. Deleted is deleted — there is
/// no trash bin, and without the keys a copy would be worth nothing anyway.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn article_delete(
    app: tauri::AppHandle,
    world: String,
    article: String,
    confirmation: String,
) -> Answer<()> {
    let store = store(&app)?;
    let identity = store.load_identity()?;
    let held = store.view_keys(&world, &identity)?;
    store
        .load_keyring(&world)
        .map_err(|_| AppError("Only the GM can delete articles.".into()))?;

    // A page belongs to its owner and goes only with it: the world's with
    // the world, a type's with the type.
    let owned = store.load_world_meta(&world)?.home == article
        || store.load_templates(&world)?.iter().any(|t| t.page == article);
    if owned {
        return Err(AppError("This page belongs to the world or to a type — it goes only with it.".into()));
    }

    let all = store.load_articles(&world)?;
    let target = all
        .iter()
        .find(|a| a.id.to_string() == article)
        .ok_or_else(|| AppError("There is no such article.".into()))?;

    // The name lives in the base layer, so in the encrypted part.
    let base = target
        .layers
        .iter()
        .find(|l| l.is_base)
        .ok_or_else(|| AppError("This article has no base layer.".into()))?;
    let name = layer_content(base, &held)?.title;
    if confirmation.trim() != name.trim() {
        return Err(AppError(format!(
            "To delete, the name has to match exactly — expected “{name}”."
        )));
    }

    let id = target.id;
    document::forget_relations(&store, &identity, &world, &article)?;
    store.remove(&world, id, None, "")?;
    Ok(())
}

#[cfg(feature = "gui")]
/// Reads a template from its text form.
///
/// The logic lives in `aw_model::editor` — it belongs to the model and not to
/// Tauri. Only so can the test bench call it too, without a second version of
/// the same rules.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn template_read(
    app: tauri::AppHandle,
    world: String,
    text: String,
    base: Template,
) -> Answer<editor::ReadResult> {
    // What the supertypes and groups bring along may be bound as well.
    let templates = store(&app)?.load_templates(&world)?;
    Ok(editor::read(&text, base, &directory(&app, &world)?, &templates))
}

#[cfg(feature = "gui")]
/// Which types exist in this world — id and name.
///
/// The text carries the name of a type, what is stored is its id. Without the
/// directory neither could be translated into the other, nor could it be said
/// that a name does not occur at all.
fn directory(app: &tauri::AppHandle, world: &str) -> Result<template_text::Directory, AppError> {
    Ok(template_text::Directory::new(
        store(app)?
            .load_templates(world)?
            .into_iter()
            .map(|t| (t.id, t.name)),
    ))
}

#[cfg(feature = "gui")]
/// Writes a template into its text form.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn template_write(
    app: tauri::AppHandle,
    world: String,
    mut template: Template,
) -> Answer<String> {
    let store = store(&app)?;
    let known = known_to_me(&store, &world)?;
    for part in template.blocks.iter_mut().flat_map(|b| b.parts.iter_mut()) {
        part.text = names::written(&part.text, &known, &template.id);
    }
    for block in &mut template.blocks {
        block.new = document::types_as(&known, &block.new, false);
    }
    default_documents(&mut template, &known, false);
    // The infobox shows properties by key; the text names them.
    let mut all: Vec<Template> = store.load_templates(&world)?.into_iter().filter(|t| t.id != template.id).collect();
    all.push(template.clone());
    let whole = types::effective(&all, &template);
    for shown in &mut template.infobox.show {
        if let Some(p) = whole.property(&shown.property) {
            shown.property = p.label.clone();
        }
    }
    // The order keeps blocks by key; the text names them.
    for key in &mut template.order {
        if let Some(b) = whole.blocks.iter().find(|b| b.key == *key && !b.title.is_empty()) {
            *key = b.title.clone();
        }
    }
    // The sort too: properties by key, written by label.
    for key in &mut template.sort {
        if let Some(p) = whole.properties.iter().find(|p| p.key == *key) {
            *key = p.label.clone();
        }
    }
    Ok(template_text::to_text(&template, &directory(&app, &world)?))
}

#[cfg(feature = "gui")]
/// Opens the legend in a window of its own.
///
/// No dialog in the main window: looking something up while typing only
/// works if one can be pushed to the edge of the screen while the other keeps
/// the focus. If the window already exists, it comes to the front.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn legend_open(app: tauri::AppHandle) -> Answer<()> {
    if let Some(existing) = app.get_webview_window("legend") {
        existing.show().map_err(AppError::from)?;
        existing.set_focus().map_err(AppError::from)?;
        return Ok(());
    }

    tauri::WebviewWindowBuilder::new(
        &app,
        "legend",
        tauri::WebviewUrl::App("index.html?window=legend".into()),
    )
    .title("Legend — writing templates")
    .inner_size(520.0, 760.0)
    .min_inner_size(360.0, 320.0)
    .build()
    .map_err(AppError::from)?;
    Ok(())
}

#[cfg(feature = "gui")]
/// Opens a place of a world in a window of its own — a document beside
/// another, to read one while writing the other. Every such window is the
/// whole app, opened at that place; `part` scrolls to a part by its title.
/// The windows share this process, and with it the profile.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn window_open(app: tauri::AppHandle, world: String, place: String, title: String, part: Option<String>) -> Answer<()> {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut url = format!("index.html?world={}&place={}", query_part(&world), query_part(&place));
    if let Some(part) = part.filter(|p| !p.is_empty()) {
        url.push_str(&format!("&part={}", query_part(&part)));
    }
    tauri::WebviewWindowBuilder::new(&app, format!("apart-{n}"), tauri::WebviewUrl::App(url.into()))
        .title(if title.trim().is_empty() { "AnotherWorldbuilder".to_string() } else { title })
        .inner_size(860.0, 820.0)
        .min_inner_size(380.0, 360.0)
        .build()
        .map_err(AppError::from)?;
    Ok(())
}

/// A value as it may stand in a URL's query: letters, digits and `-_.~`
/// as they are, everything else as `%XX` of its UTF-8 bytes.
fn query_part(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(feature = "gui")]
/// Shows the main window as a phone — or at another size, a tablet's — or
/// as it was: for layout checks from the tests (`invoke`), no button in the app.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn phone_view(window: tauri::WebviewWindow, on: bool, size: Option<(f64, f64)>) -> Answer<()> {
    window_state::phone(&window, on, size).map_err(AppError::from)
}

/// Splits Markdown into the tree the frontend builds elements from.
///
/// Deliberately here and not in the frontend: so there is only one
/// implementation, it can be tested in Rust, and no HTML string comes into
/// being anywhere that someone might insert by accident.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn markdown_read(text: String) -> Answer<Vec<markdown::Node>> {
    Ok(markdown::split(&text))
}

#[cfg(feature = "gui")]
/// The view of a world.
///
/// As a player always with one's own keys. As game master without
/// `preview_as` the full view, with it exactly the keys a player with those
/// attributes holds — `all` and `players` besides — not a rebuilt simulation
/// but the same decryption attempt.
// Off the main thread: a big world takes its time.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn view(app: tauri::AppHandle, world: String, preview_as: Option<Vec<String>>) -> Answer<Vec<ArticleView>> {
    view_in(&store(&app)?, world, preview_as)
}

/// `view` for whom the store is given — the host, acting for a player.
pub(crate) fn view_in(store: &Store, world: String, preview_as: Option<Vec<String>>) -> Answer<Vec<ArticleView>> {
    let store = store.clone();
    let identity = store.load_identity()?;
    let articles = store.load_articles(&world)?;

    let previewing = preview_as.is_some();
    let preview_key = preview_as.as_ref().map(|names| format!("preview:{}", names.join(",")));
    let held = match (store.is_gm(&world), preview_as) {
        // Only the game master can switch the view — they hold the keys.
        // Every player holds `all` and `players` — a preview as one does too.
        (true, Some(names)) => {
            let attributes: Vec<Attribute> = ["all", "players"]
                .iter()
                .map(|n| n.to_string())
                .chain(names)
                .filter_map(|n| n.parse().ok())
                .collect();
            store.load_keyring(&world)?.restrict_to(&attributes)
        }
        _ => store.view_keys(&world, &identity)?,
    };
    // Acting for a member: their view, cached apart.
    let who = preview_key.or_else(|| host::acting().map(|a| format!("member:{}", a.member.id))).unwrap_or_default();
    let mut scope = blake3::Hasher::new();
    for (at, key) in held.iter() {
        scope.update(at.to_string().as_bytes());
        scope.update(key);
    }
    let who = format!("{}:{}:{}:{who}", store.profile_dir().display(), identity.id(), scope.finalize().to_hex());
    let mut views = cached_view(&world, &who, &articles, &held);
    // What this player asked for and waits on — theirs alone, until made.
    if !previewing {
        views.extend(pending_views(&store, &world, &identity)?);
    }
    Ok(views)
}

#[cfg(feature = "gui")]
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn today_in_world(app: tauri::AppHandle, world: String, day: i64) -> Answer<String> {
    today_in_world_in(&store(&app)?, world, day)
}

/// `today_in_world` for whom the store is given — the host, acting for a player.
pub(crate) fn today_in_world_in(store: &Store, world: String, day: i64) -> Answer<String> {
    let store = store.clone();
    let meta = store.load_world_meta(&world)?;
    Ok(meta.calendar.format(WorldDate(day)))
}

#[cfg(test)]
mod window_tests {
    #[test]
    fn a_query_part_keeps_only_what_a_url_may_hold() {
        assert_eq!(super::query_part("article:4f-x"), "article%3A4f-x");
        assert_eq!(super::query_part("Ä b"), "%C3%84%20b");
    }
}

#[cfg(feature = "gui")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    let builder = if profile::profile_root().is_none() { builder
        // Remembers the window's position and state across restarts.
        //
        // The *size* is excluded and lives in `window_state.rs`: the plugin
        // stores the reported size and sets it again on the next start — under
        // Wayland the window then grows on every start by the invisible shadow
        // margin of the decoration.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                // Only the main window comes back where it was; the others are for the moment.
                .with_filter(|label| label == "main")
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::all()
                        - tauri_plugin_window_state::StateFlags::SIZE,
                )
                .build(),
        )
    } else { builder };
    builder.plugin(tauri_plugin_dialog::init())
        // Updates from the releases, signed with the publisher's key.
        .plugin(tauri_plugin_updater::Builder::new().build())
        // Runs after the plugin has restored things: first the remembered size
        // stands, then it is clamped into the usable area.
        .setup(|app| {
            online::tell_progress_to(app.handle().clone());
            if let Ok(dir) = profile::config_dir(app.handle()) {
                applog::set_fallback(dir);
            }
            // The game master's worlds in shared folders: the next state
            // once nobody wrote for a while.
            std::thread::spawn(|| loop {
                std::thread::sleep(std::time::Duration::from_secs(20));
                if let Some(store) = profile::store_now() {
                    store.consolidate_quiet();
                }
            });
            network::start(app.handle()).map_err(|e| std::io::Error::other(e.0))?;
            browser::start(app.handle());
            if let Some(w) = app.get_webview_window("main") {
                window_state::on_start(app.handle().clone(), w);
            }
            Ok(())
        })
        // Only the main window's size is remembered — not the legend's or a
        // document's opened apart.
        .on_window_event(|w, event| match event {
            tauri::WindowEvent::CloseRequested { .. } if w.label() == "main" => window_state::remember(w.app_handle(), w, true),
            // While dragging too, throttled: otherwise the size would be lost
            // when the application is not shut down cleanly.
            tauri::WindowEvent::Resized(_) if w.label() == "main" => window_state::remember(w.app_handle(), w, false),
            _ => {}
        })
        .invoke_handler({
            let commands: fn(tauri::ipc::Invoke) -> bool = tauri::generate_handler![
            online::world_call,
            online::world_connect,
            online::world_disconnect,
            document::article_versions,
            document::version_compare,
            document::version_revert,
            document::version_document,
            document::version_carry,
            document::world_conflicts,
            applog::app_log,
            applog::app_log_open,
            document::conflict_dismiss,
            document::conflict_rescue,
            network::network_settings,
            network::network_enable,
            network::network_internet,
            network::network_share,
            network::network_servers,
            network::server_manage,
            network::server_backups,
            network::server_host_worlds,
            network::remote_call,
            network::network_copies_set,
            network::network_folders,
            network::network_port,
            network::network_worlds,
            network::network_join,
            network::network_sync,
            network::presence,
            network::world_presence,
            network::dice_roll,
            network::dice_rolls,
            network::dice_log,
            network::dice_log_add,
            network::dice_log_clear,
            network::dice_favorites_set,
            network::join_requests,
            network::join_answer,
            network::member_invite,
            profile::storage_status,
            profile::storage_locations,
            profile::storage_choose,
            profile::storage_peek,
            profile::storage_move,
            profile::storage_open,
            identity_status,
            identity_create,
            identity_rename,
            identity_export,
            avatar_set,
            avatar_remove,
            worlds_list,
            world_create,
            world_copy,
            world_move,
            network::world_check,
            world_open,
            world_share,
            world_token_check,
            world_icon_set,
            world_delete,
            member_add,
            member_remove,
            member_replace,
            member_colour_set,
            backup::backup_view,
            backup::backup_plan_set,
            backup::backup_now,
            backup::backup_open,
            backup::backup_restore,
            gm_handover,
            world_supporters_set,
            world_take_over,
            gm_handover_file,
            world_moderation_set,
            member_trust_set,
            document::version_verdict,
            document::waiting_versions,
            document::version_marks,
            attribute_create,
            attribute_revoke,
            member_attributes_set,
            templates_load,
            template_save,
            template_delete,
            article_create,
            view,
            visibility_set,
            type_access_apply,
            article_delete,
            template_read,
            template_write,
            texts_written,
            markdown_read,
            legend_open,
            phone_view,
            window_open,
            today_in_world,
            calendar_set,
            document::document_open,
            document::document_read,
            document::document_write,
            document::document_save,
            document::session_end,
            document::image_field_set,
            document::references_set,
            media::image_upload,
            media::image_rename,
            media::image_defaults_set,
            media::image_move,
            media::gallery_folders,
            media::gallery_folders_set,
            media::image_delete,
            media::gallery,
            media::image_data,
            document::version_delete,
            world_versions_set,
            world_tidy,
            world_listed_set,
            app_restart,
            world_dice_set,
            world_encryption_set,
            world_readable_set,
            world_switch_progress,
            keys::keys_backup,
            keys::keys_restore,
            navigation_load,
            transfer::package_export,
            browser::browser_settings,
            browser::browser_enable,
            browser::browser_pair,
            browser::browser_forget,
            transfer::package_read,
            transfer::package_import,
            navigation_set,
            article_propose,
            document::relations_set,
            ];
            move |invoke| {
                if online::gate_invoke(&invoke) { true } else { commands(invoke) }
            }
        })
        .build(tauri::generate_context!())
        .expect("the Tauri application could not start")
        .run(|_, event| {
            // What was held only while online goes with the app.
            if let tauri::RunEvent::Exit = event {
                if let Some(store) = profile::store_now() {
                    store.forget_fleeting();
                    online::clear_sessions(&store);
                }
            }
        });
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_is_whoever_is_assigned_to_the_document_or_its_type() {
        assert_eq!(own_view("self OR gm", "art:01ABC", "pc"), "doc:01ABC OR type:pc OR gm");
        assert_eq!(own_view("self OR gm", "art:01ABC", ""), "doc:01ABC OR gm", "no type: no type attribute");
        assert_eq!(own_view("self AND class:rogue", "art:01ABC", "t:x"), "doc:01ABC AND class:rogue OR type:t-x AND class:rogue");
        assert_eq!(own_view("players", "art:01ABC", "pc"), "players");
        let parsed: Policy = own_view("self OR gm", "art:01ABC", "pc").parse().unwrap();
        assert_eq!(parsed.groups().len(), 3);
    }

    fn input(policy: &str, parent: Option<usize>) -> NewLayer {
        NewLayer {
            title: policy.into(),
            text: String::new(),
            policy: policy.into(),
            level: "open".into(),
            fields: serde_json::Value::Null,
            section: policy.into(),
            edit: "gm".into(),
            parent,
        }
    }

    #[test]
    fn without_nesting_the_policy_stays_as_it_is() {
        let effective = effective_policies(&[input("players", None), input("gm", None)]).unwrap();
        assert_eq!(effective[0].to_string(), "players");
        assert_eq!(effective[1].to_string(), "gm");
    }

    #[test]
    fn an_inner_part_inherits_the_outer_visibility() {
        let effective = effective_policies(&[
            input("gm", None),
            input("players", Some(0)),
        ])
        .unwrap();

        let held: std::collections::BTreeSet<Attribute> =
            ["players"].iter().map(|s| s.parse().unwrap()).collect();
        assert!(
            !effective[1].satisfied_by(&held),
            "players alone must not open the inner box"
        );

        let both: std::collections::BTreeSet<Attribute> = ["players", "gm"]
            .iter()
            .map(|s| s.parse().unwrap())
            .collect();
        assert!(effective[1].satisfied_by(&both));
    }

    #[test]
    fn inheritance_runs_over_several_levels() {
        let effective = effective_policies(&[
            input("players", None),
            input("class:rogue", Some(0)),
            input("knowledge:crypt", Some(1)),
        ])
        .unwrap();

        let everything: std::collections::BTreeSet<Attribute> =
            ["players", "class:rogue", "knowledge:crypt"]
                .iter()
                .map(|s| s.parse().unwrap())
                .collect();
        assert!(effective[2].satisfied_by(&everything));

        for missing in ["players", "class:rogue", "knowledge:crypt"] {
            let without: std::collections::BTreeSet<Attribute> =
                ["players", "class:rogue", "knowledge:crypt"]
                    .iter()
                    .filter(|a| **a != missing)
                    .map(|s| s.parse().unwrap())
                    .collect();
            assert!(
                !effective[2].satisfied_by(&without),
                "without {missing} the innermost box must stay shut"
            );
        }
    }

    fn keys(attributes: &[&str]) -> HeldKeys {
        let mut ring = Keyring::new();
        let mut held = HeldKeys::new();
        for a in attributes {
            let parsed: Attribute = a.parse().unwrap();
            ring.ensure(&parsed).unwrap();
            let k = ring.current(&parsed).unwrap();
            held.insert(k.at, k.key);
        }
        held
    }

    #[test]
    fn editing_is_separate_from_seeing() {
        // Whoever may read may not therefore write.
        let player = keys(&["players"]);
        assert!(!may_edit("gm", &player));
        assert!(may_edit("players", &player));
    }

    #[test]
    fn edit_can_hold_for_individuals() {
        let bob = keys(&["players", "player:bob"]);
        let anna = keys(&["players", "player:anna"]);
        assert!(may_edit("player:bob", &bob));
        assert!(!may_edit("player:bob", &anna));
    }

    #[test]
    fn without_a_rule_the_gm_decides() {
        // Old content has no rule — then only the game master may touch it.
        assert!(may_edit("", &keys(&["gm"])));
        assert!(!may_edit("", &keys(&["players"])));
    }

    #[test]
    fn edit_understands_and_terms() {
        let both = keys(&["players", "class:rogue"]);
        let only_one = keys(&["players"]);
        assert!(may_edit("players AND class:rogue", &both));
        assert!(!may_edit("players AND class:rogue", &only_one));
    }

    /// A freshly created type: still without an id and without a name.
    fn draft() -> Template {
        Template {
            id: String::new(),
            name: String::new(),
            icon: String::new(),
            supertype: String::new(),
            group: false,
            rules: Vec::new(),
            description: String::new(),
            auth: None,
            name_pattern: aw_model::template::just_input(),
            order: Vec::new(),
            sort: Vec::new(),
            create: String::new(),
            page: String::new(),
            properties: Vec::new(),
            infobox: Default::default(),
            blocks: Vec::new(),
        }
    }

    #[test]
    fn a_draft_without_an_id_is_no_text_error() {
        // Slug and name stand in the header line, not in the text. When
        // creating, both are empty — that must not appear as an error on a
        // line that does not exist.
        let e = editor::read(
            "<article view=all edit=all>\n    <name>\n    <body>\n    </body>\n</article>\n",
            draft(),
            &Default::default(), &[]
        );
        let texts: Vec<&str> = e.errors.iter().map(|f| f.text.as_str()).collect();
        assert!(e.errors.is_empty(), "{texts:?}");
        assert!(e.repair.is_none(), "an intact skeleton needs no repair");
    }

    #[test]
    fn a_deleted_skeleton_comes_back() {
        // Change yes, delete no: if a piece is missing, the text comes back
        // completed — and exactly once, or the editor would write in circles.
        let e = editor::read(
            "<block title=\"What people say\">\n  Text.\n</block>\n",
            draft(),
            &Default::default(), &[]
        );
        let repaired = e.repair.expect("skeleton missing, so repair");
        assert!(repaired.starts_with("<article view=all edit=all>\n"), "{repaired}");
        assert!(repaired.contains("<name>"), "{repaired}");
        assert!(repaired.contains("What people say"), "the content stays: {repaired}");

        let again = editor::read(&repaired, draft(), &Default::default(), &[]);
        assert!(again.repair.is_none(), "repairing once is enough");
    }

    /// Types character by character and runs the same loop as the editor:
    /// read after every keystroke, and put in whatever comes back.
    fn type_in(start: &str, position: usize, input: &str) -> String {
        let mut text = start.to_string();
        let mut at = position;
        for c in input.chars() {
            text.insert(at, c);
            at += c.len_utf8();
            let e = editor::read(&text, draft(), &Default::default(), &[]);
            if let Some(repaired) = e.repair {
                // The editor replaces the text and puts the cursor at the old offset.
                at = at.min(repaired.len());
                text = repaired;
            }
        }
        text
    }

    #[test]
    fn the_body_can_be_written_in() {
        // The reported bug: open a new line between <body> and </body> and
        // insert something there. The half-typed `<` caught the `>` of the
        // next line, so </body> was used up, the skeleton counted as deleted —
        // and the editor rewrote everything.
        let start = "<article view=all edit=all>\n    <name>\n    <body>\n    </body>\n</article>\n";
        let after_body = start.find("<body>\n").unwrap() + "<body>\n".len();

        let text = type_in(start, after_body, "      <block title=\"History\">\n      </block>\n");
        assert!(
            text.contains("<block title=\"History\">"),
            "what was typed must stay:\n{text}"
        );

        let e = editor::read(&text, draft(), &Default::default(), &[]);
        let template = e.template.expect("template");
        assert_eq!(template.blocks.len(), 1, "{text}");
        assert_eq!(template.blocks[0].title, "History");
        assert!(e.repair.is_none(), "and no repair needed any more:\n{text}");
    }

    #[test]
    fn a_note_can_be_typed_too() {
        // The same for `<!--`: without its end it swallowed everything below.
        let start = "<article view=all edit=all>\n    <name>\n    <body>\n    </body>\n</article>\n";
        let after_body = start.find("<body>\n").unwrap() + "<body>\n".len();
        let text = type_in(start, after_body, "      <!-- Idea -->\n");
        assert!(text.contains("</body>"), "{text}");
        assert!(
            editor::read(&text, draft(), &Default::default(), &[])
                .repair
                .is_none(),
            "{text}"
        );
    }

    #[test]
    fn half_written_text_is_not_repaired_away() {
        // Skeleton broken *and* something half-finished in the body: then the
        // repair waits until it is clear where the sentence belongs.
        let e = editor::read(
            "<article view=all edit=all>\n    <name>\n      The innkeeper was cal\n</article>\n",
            draft(),
            &Default::default(), &[]
        );
        assert!(!e.errors.is_empty(), "the loose text is noticed");
        assert!(
            e.repair.is_none(),
            "but it is not tidied away — it belongs to whoever is writing"
        );
    }

    #[test]
    fn a_renamed_block_keeps_its_id() {
        // The id is what the layers of existing articles hang on. If it moved
        // with the title on a rename, their texts would silently vanish — and
        // it does not stand in the text, so reading has to find it again.
        let skeleton = |title: &str| {
            format!("<article view=all edit=all>\n    <name>\n    <body>\n      <block title=\"{title}\">\n        Text.\n      </block>\n    </body>\n</article>\n")
        };

        let before = editor::read(&skeleton("What happened"), draft(), &Default::default(), &[])
            .template
            .expect("template");
        let id = before.blocks[0].key.clone();
        assert!(!id.is_empty());
        assert_ne!(id, "what-happened", "not from the title");

        let after = editor::read(&skeleton("History"), before.clone(), &Default::default(), &[])
            .template
            .expect("template");
        assert_eq!(after.blocks[0].title, "History");
        assert_eq!(after.blocks[0].key, id, "the id stays");
        assert_eq!(after.blocks[0].parts[0].key, id);

        // And it does not show up in the text — not after the rename either.
        let text = template_text::to_text(&after, &Default::default());
        assert!(!text.contains("key="), "{text}");
        assert!(text.contains("<block title=History>"), "{text}");
    }

    #[test]
    fn a_subtype_orders_inherited_blocks_by_title() {
        let skeleton = |body: &str| format!("<article view=all edit=all>\n  <name>\n  <body>\n{body}  </body>\n</article>\n");
        let mut person = editor::read(
            &skeleton("    <block title=Looks>\n    </block>\n    <block title=Life>\n    </block>\n"),
            draft(),
            &Default::default(),
            &[],
        )
        .template
        .expect("person");
        person.id = "person".into();
        let mut hero = draft();
        hero.id = "hero".into();
        hero.supertype = "person".into();
        let read = editor::read(
            &skeleton("    <order of=\"goals, Looks\">\n    <block title=Goals>\n    </block>\n"),
            hero,
            &Default::default(),
            std::slice::from_ref(&person),
        );
        assert!(read.errors.is_empty(), "{:?}", read.errors);
        let hero = read.template.expect("hero");
        let whole = aw_model::types::effective(&[person.clone(), hero.clone()], &hero);
        let titles: Vec<&str> = whole.blocks.iter().map(|b| b.title.as_str()).collect();
        assert_eq!(titles, vec!["Goals", "Looks", "Life"], "one not named comes after");

        let wrong = editor::read(&skeleton("    <order of=Nope>\n"), draft(), &Default::default(), &[]);
        assert!(wrong.errors.iter().any(|e| e.text.contains("“Nope”")), "{:?}", wrong.errors);
        let mut written = hero.clone();
        written.order = vec!["Goals".into(), "Looks".into()];
        assert!(template_text::to_text(&written, &Default::default()).contains("<order of=\"Goals, Looks\">"));
    }

    #[test]
    fn a_type_sorts_its_documents_by_properties_kept_by_key() {
        let text = "---\nStart: date\nPlayed: datetime\n---\n\n<article view=all edit=all>\n  <name>\n  <body>\n    <sort by=\"start, Played, name\">\n  </body>\n</article>\n";
        let read = editor::read(text, draft(), &Default::default(), &[]);
        assert!(read.errors.is_empty(), "{:?}", read.errors);
        let session = read.template.expect("session");
        let keys: Vec<&str> = session.properties.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(session.sort, vec![keys[0], keys[1], "name"]);
        let mut hero = draft();
        hero.id = "hero".into();
        hero.supertype = session.id.clone();
        assert_eq!(aw_model::types::effective(&[session.clone(), hero.clone()], &hero).sort, session.sort, "inherited");

        let wrong = editor::read(&text.replace("start, ", "Nope, "), draft(), &Default::default(), &[]);
        assert!(wrong.errors.iter().any(|e| e.text.contains("“Nope”")), "{:?}", wrong.errors);
        let mut written = session.clone();
        written.sort = vec!["Start".into(), "name".into()];
        assert!(template_text::to_text(&written, &Default::default()).contains("<sort by=\"Start, name\">"));
    }

    #[test]
    fn a_new_block_inherits_no_foreign_id() {
        // Whoever inserts a block at the top pushes the old one down — and it
        // keeps its id. The new one gets its own.
        let one = editor::read(
            "<article view=all edit=all>\n    <name>\n    <body>\n      <block title=\"Old\">\n        A.\n      </block>\n    </body>\n</article>\n",
            draft(),
            &Default::default(), &[]
        )
        .template
        .expect("template");
        let old = one.blocks[0].key.clone();

        let two = editor::read(
            "<article view=all edit=all>\n    <name>\n    <body>\n      <block title=\"New\">\n        N.\n      </block>\n      <block title=\"Old\">\n        A.\n      </block>\n    </body>\n</article>\n",
            one,
            &Default::default(), &[]
        )
        .template
        .expect("template");
        assert_eq!(two.blocks[1].key, old, "the old one stays the same");
        assert_ne!(two.blocks[0].key, old, "the new one is a new one");
    }

    #[test]
    fn a_new_world_comes_with_its_own_page() {
        let dir = std::env::temp_dir().join(format!("aw-new-world-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let store = Store::new(dir.clone());
        let identity = Identity::create("Marla").unwrap();
        store.save_identity(&identity).unwrap();

        let meta = new_world(&store, &identity, "Eldermere".into()).unwrap();
        assert!(!meta.home.is_empty(), "the page exists from the start");
        assert_eq!(store.load_world_meta(meta.public.world_id.as_str()).unwrap().home, meta.home);

        let id = meta.public.world_id.to_string();
        let held = store.view_keys(&id, &identity).unwrap();
        let view = build_view(&store.load_articles(&id).unwrap(), &held);
        assert_eq!(view.len(), 1, "the page is the only document");
        let page = &view[0];
        assert_eq!(page.id, meta.home);
        assert_eq!(page.name, "Eldermere", "it carries the world's name");
        assert_eq!(page.article_type, "", "a page has no type");
        assert_eq!(page.layers.len(), 1);
        assert!(!page.layers[0].section.is_empty(), "one block, like every document");
        assert!(page.layers[0].may_edit, "the game master can write on it");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_type_and_its_page_share_their_name() {
        let dir = std::env::temp_dir().join(format!("aw-names-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let store = Store::new(dir.clone());
        let identity = Identity::create("Marla").unwrap();
        store.save_identity(&identity).unwrap();
        let meta = new_world(&store, &identity, "Eldermere".into()).unwrap();
        let id = meta.public.world_id.to_string();
        let kind = |name: &str| Template {
            id: String::new(),
            name: name.into(),
            icon: String::new(),
            supertype: String::new(),
            group: false,
            rules: Vec::new(),
            description: String::new(),
            properties: Vec::new(),
            infobox: Default::default(),
            auth: None,
            name_pattern: aw_model::template::just_input(),
            order: Vec::new(),
            sort: Vec::new(),
            create: String::new(),
            page: String::new(),
            blocks: Vec::new(),
        };
        let all = save_template(&store, &identity, &id, kind("Characters")).unwrap();
        save_template(&store, &identity, &id, kind("Places")).unwrap();
        let characters = all[0].clone();
        let tim = create_document(&store, &id, characters.id.clone(), "tim".into(), "Tim".into(), "all".into(), vec![]).unwrap();
        let home = document::code_of(&store, &identity, &id, &meta.home).unwrap();
        let with_link = home.code.replace("<block>\n", "<block>\nMeet @@Characters:Tim@@.\n");
        document::save_now(&store, &identity, &id, &meta.home, &with_link).unwrap();
        let held = store.view_keys(&id, &identity).unwrap();
        let stored = build_view(&store.load_articles(&id).unwrap(), &held);
        let home_text = &stored.iter().find(|a| a.id == meta.home).unwrap().layers[0].text;
        assert_eq!(home_text, &format!("Meet @@{tim}@@."), "a link is stored by id");

        // Renamed as a type: the page follows, and text shows the new name —
        // nothing had to be rewritten for it.
        save_template(&store, &identity, &id, Template { name: "Heroes".into(), ..characters.clone() }).unwrap();
        let page = document::code_of(&store, &identity, &id, &characters.page).unwrap();
        assert_eq!(page.document.name, "Heroes");
        assert!(page.code.contains("<list type=Heroes columns=2>"), "{}", page.code);
        let home = document::code_of(&store, &identity, &id, &meta.home).unwrap();
        assert!(home.code.contains("@@Heroes:Tim@@"), "{}", home.code);

        // A renamed document too.
        let tim_code = document::code_of(&store, &identity, &id, &tim).unwrap().code;
        document::save_now(&store, &identity, &id, &tim, &tim_code.replace("<name=Tim>", "<name=Timothy>")).unwrap();
        let home = document::code_of(&store, &identity, &id, &meta.home).unwrap();
        assert!(home.code.contains("@@Heroes:Timothy@@"), "{}", home.code);

        // Renamed as a page: the type follows.
        let renamed = page.code.replace("<name=Heroes>", "<name=Villains>");
        document::save_now(&store, &identity, &id, &characters.page, &renamed).unwrap();
        let types = store.load_templates(&id).unwrap();
        assert!(types.iter().any(|t| t.id == characters.id && t.name == "Villains"));

        // A name is given once.
        let clash = renamed.replace("<name=Villains>", "<name=places>");
        let err = document::save_now(&store, &identity, &id, &characters.page, &clash).unwrap_err();
        assert!(err.0.contains("already exists"), "{}", err.0);

        // The world's page names the world.
        let world_page = document::code_of(&store, &identity, &id, &meta.home).unwrap();
        let code = world_page.code.replace("<name=Eldermere>", "<name=\"The Mist Lands\">");
        document::save_now(&store, &identity, &id, &meta.home, &code).unwrap();
        assert_eq!(store.load_world_meta(&id).unwrap().public.name, "The Mist Lands");
        assert_eq!(store.load_token(&id).unwrap().name(), "The Mist Lands");
        assert!(new_world(&store, &identity, "the mist lands".into()).is_err(), "and no second world takes it");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_type_comes_and_goes_with_its_page() {
        let dir = std::env::temp_dir().join(format!("aw-type-pages-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let store = Store::new(dir.clone());
        let identity = Identity::create("Marla").unwrap();
        store.save_identity(&identity).unwrap();
        let meta = new_world(&store, &identity, "Eldermere".into()).unwrap();
        let id = meta.public.world_id.to_string();

        let character = Template {
            id: String::new(),
            name: "Characters".into(),
            icon: "🧙".into(),
            supertype: String::new(),
            group: false,
            rules: Vec::new(),
            description: String::new(),
            properties: Vec::new(),
            infobox: Default::default(),
            auth: None,
            name_pattern: aw_model::template::just_input(),
            order: Vec::new(),
            sort: Vec::new(),
            create: String::new(),
            page: String::new(),
            blocks: Vec::new(),
        };
        let saved = save_template(&store, &identity, &id, character).unwrap();
        let page = saved[0].page.clone();
        assert!(!page.is_empty(), "the type came with its page");

        let held = store.view_keys(&id, &identity).unwrap();
        let view = build_view(&store.load_articles(&id).unwrap(), &held);
        let text = &view.iter().find(|a| a.id == page).unwrap().layers[0].text;
        assert_eq!(text, &format!("<list type={} columns=2>", saved[0].id), "stored by id");
        let code = document::code_of(&store, &identity, &id, &page).unwrap().code;
        assert!(code.contains("<list type=Characters columns=2>"), "written by name: {code}");

        // Saving again keeps the page — the editor's copy does not know it.
        let again = save_template(&store, &identity, &id, Template { page: String::new(), ..saved[0].clone() }).unwrap();
        assert_eq!(again[0].page, page);
        assert_eq!(store.load_articles(&id).unwrap().len(), 2, "home and the one page");

        delete_template(&store, &id, &saved[0].id).unwrap();
        assert!(store.load_articles(&id).unwrap().iter().all(|a| a.id.to_string() != page), "the page went with it");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_group_lends_its_properties_to_the_types_that_take_it_in() {
        let dir = std::env::temp_dir().join(format!("aw-groups-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let store = Store::new(dir.clone());
        let identity = Identity::create("Marla").unwrap();
        store.save_identity(&identity).unwrap();
        let meta = new_world(&store, &identity, "Eldermere".into()).unwrap();
        let id = meta.public.world_id.to_string();
        let blank = |name: &str| Template {
            id: String::new(),
            name: name.into(),
            icon: String::new(),
            supertype: String::new(),
            group: false,
            rules: Vec::new(),
            description: String::new(),
            properties: Vec::new(),
            infobox: Default::default(),
            auth: None,
            name_pattern: aw_model::template::just_input(),
            order: Vec::new(),
            sort: Vec::new(),
            create: String::new(),
            page: String::new(),
            blocks: Vec::new(),
        };
        let number = |key: &str, label: &str| aw_model::template::TemplateField::new(key, label, aw_model::FieldKind::Number { unit: None });

        let abilities = Template { group: true, properties: vec![number("str", "Strength")], ..blank("Abilities") };
        let all = save_template(&store, &identity, &id, abilities).unwrap();
        let group = all.iter().find(|t| t.name == "Abilities").unwrap().clone();
        assert!(group.page.is_empty(), "a group has no page");

        let stats = aw_model::template::TemplateField::new("stats", "Stats", aw_model::FieldKind::Group { group: group.id.clone() });
        let all = save_template(&store, &identity, &id, Template { properties: vec![stats], ..blank("Characters") }).unwrap();
        let characters = all.iter().find(|t| t.name == "Characters").unwrap().clone();

        let layer: NewLayer = serde_json::from_value(serde_json::json!({
            "title": "", "text": "", "policy": "all", "level": "open", "section": "properties", "edit": "gm", "fields": { "str": 15 }
        }))
        .unwrap();
        let tim = create_document(&store, &id, characters.id.clone(), String::new(), "Tim".into(), "all".into(), vec![layer]).unwrap();
        let code = document::code_of(&store, &identity, &id, &tim).unwrap().code;
        assert!(code.starts_with("---\nStats:\n  Strength: 15\n---"), "{code}");

        // The group grows — and the character with it.
        let grown = Template { properties: vec![number("str", "Strength"), number("dex", "Dexterity")], ..group.clone() };
        save_template(&store, &identity, &id, grown).unwrap();
        let code = document::code_of(&store, &identity, &id, &tim).unwrap().code;
        assert!(code.contains("  Dexterity:\n"), "{code}");
        document::save_now(&store, &identity, &id, &tim, &code.replace("  Dexterity:", "  Dexterity: 12")).unwrap();
        let code = document::code_of(&store, &identity, &id, &tim).unwrap().code;
        assert!(code.contains("  Strength: 15\n  Dexterity: 12\n"), "{code}");

        // A layout of its own: stored by keys and ids, so renaming changes
        // nothing stored — and a document shows it with `<view of=…>`.
        let mut layout = aw_model::template::TemplateBlock::fresh("layout", "");
        layout.parts[0].text = "**Strength** %%Strength%%".into();
        let all = save_template(&store, &identity, &id, Template { blocks: vec![layout], ..store.load_templates(&id).unwrap().into_iter().find(|t| t.id == group.id).unwrap() }).unwrap();
        let stored = all.iter().find(|t| t.id == group.id).unwrap();
        assert_eq!(stored.blocks[0].parts[0].text, "**Strength** %%str%%");
        let code = document::code_of(&store, &identity, &id, &tim).unwrap().code;
        let with_view = code.replace("<body>\n", "<body>\n<block title=Sheet>\n<view of=Abilities>\n</block>\n");
        document::save_now(&store, &identity, &id, &tim, &with_view).unwrap();
        let held = store.view_keys(&id, &identity).unwrap();
        let view = build_view(&store.load_articles(&id).unwrap(), &held);
        let texts: Vec<&str> = view.iter().find(|a| a.id == tim).unwrap().layers.iter().map(|l| l.text.as_str()).collect();
        assert!(texts.iter().any(|t| t.contains(&format!("<view of={}>", group.id))), "{texts:?}");
        let code = document::code_of(&store, &identity, &id, &tim).unwrap().code;
        assert!(code.contains("<view of=Abilities>"), "{code}");

        // A number of its own per pick: the levels in each class.
        let all = save_template(&store, &identity, &id, blank("Classes")).unwrap();
        let classes = all.iter().find(|t| t.name == "Classes").unwrap().clone();
        let class = |name: &str| create_document(&store, &id, classes.id.clone(), String::new(), name.into(), "all".into(), vec![]).unwrap();
        let fighter = class("Fighter");
        class("Rogue");
        let mut hero = store.load_templates(&id).unwrap().into_iter().find(|t| t.id == characters.id).unwrap();
        hero.properties.push(aw_model::template::TemplateField::new(
            "classes",
            "Classes",
            aw_model::FieldKind::Reference { article_type: classes.id.clone(), multiple: true, parent: false, adds: false, count: "Level".into(), every: false, from: String::new() },
        ));
        save_template(&store, &identity, &id, hero).unwrap();
        let code = document::code_of(&store, &identity, &id, &tim).unwrap().code;
        let code = code.replace("Classes:", "Classes: @@Classes:Fighter@@ 3, @@Classes:Rogue@@");
        document::save_now(&store, &identity, &id, &tim, &code).unwrap();
        let back = document::code_of(&store, &identity, &id, &tim).unwrap().code;
        assert!(back.contains("Classes: @@Classes:Fighter@@ 3, @@Classes:Rogue@@\n"), "{back}");
        let view = build_view(&store.load_articles(&id).unwrap(), &store.view_keys(&id, &identity).unwrap());
        let stored = view.iter().find(|a| a.id == tim).unwrap().layers.iter().find_map(|l| l.fields.get("classes").cloned()).unwrap();
        assert_eq!(stored[0], serde_json::json!({ "with": fighter, "count": 3 }));
        let bad = back.replace("Fighter@@ 3", "Fighter@@ three");
        assert!(document::save_now(&store, &identity, &id, &tim, &bad).unwrap_err().0.contains("no number"));

        // Every class, always: only numbers other than 0 are kept.
        let mut hero = store.load_templates(&id).unwrap().into_iter().find(|t| t.id == characters.id).unwrap();
        for f in &mut hero.properties {
            if let aw_model::FieldKind::Reference { every, .. } = &mut f.kind {
                if f.key == "classes" {
                    *every = true;
                }
            }
        }
        save_template(&store, &identity, &id, hero).unwrap();
        let code = back.replace("Classes: @@Classes:Fighter@@ 3, @@Classes:Rogue@@", "Classes: @@Classes:Fighter@@ 3, @@Classes:Rogue@@ 0");
        document::save_now(&store, &identity, &id, &tim, &code).unwrap();
        let back = document::code_of(&store, &identity, &id, &tim).unwrap().code;
        assert!(back.contains("Classes: @@Classes:Fighter@@ 3\n"), "{back}");

        // A group in use stays.
        let err = delete_template(&store, &id, &group.id).unwrap_err();
        assert!(err.0.contains("Characters"), "{}", err.0);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn forward_references_are_rejected() {
        assert!(effective_policies(&[input("players", Some(1)), input("gm", None)]).is_err());
        assert!(effective_policies(&[input("players", Some(0))]).is_err());
    }
}
