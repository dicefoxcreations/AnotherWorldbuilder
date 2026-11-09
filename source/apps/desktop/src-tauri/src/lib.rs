//! The Tauri commands.
//!
//! Every world is held by one: this app's own server, or a server
//! elsewhere. Whoever holds it decides what each member sees — the `Reader`
//! of `aw_store::db` — and nothing leaves it that the member may not see.
//! There is no check here the frontend could bypass: `view` gives out what
//! the reader sees, and nothing else.

// The server (`--no-default-features`) uses less of it than the app.
#![cfg_attr(not(feature = "gui"), allow(dead_code, unused_imports))]

mod applog;
mod document;
#[cfg(all(feature = "gui", not(any(target_os = "android", target_os = "ios"))))]
mod window_state;
mod store;
mod profile;
mod media;
mod live;
mod backup;
mod transfer;
mod network;
mod keys;
mod remote;
mod views;
mod mailbox;
#[cfg(feature = "gui")]
mod browser;
mod keeper;
pub mod server;
mod dedicated;
#[cfg(target_os = "android")]
mod android;
#[cfg(all(feature = "gui", not(any(target_os = "android", target_os = "ios"))))]
mod tray;

/// AW Server on this computer, instead of the app — see `tray`.
#[cfg(all(feature = "gui", not(any(target_os = "android", target_os = "ios"))))]
pub fn run_server(hidden: bool) {
    tray::run(hidden)
}

use aw_crypto::{Identity, IdentityPublic};
use aw_model::{editor, markdown, names, template_text, types, Attribute, Policy, Template, WorldCalendar, WorldDate};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use store::{Access, Document, Layer, Reader, Role, Store, WorldMeta};
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
    /// `gm`, `supporter` or `player`.
    pub role: String,
    pub attributes: Vec<String>,
    /// Their writing counts without waiting for the game master.
    pub trusted: bool,
    /// Their mailbox, in a world without a server.
    #[serde(default)]
    pub slot: String,
}

#[derive(Serialize, Deserialize)]
pub struct WorldInfo {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub folder: String,
    /// `gm`, `supporter` or `player` — decides what the interface offers.
    pub role: String,
    pub is_gm: bool,
    pub attributes: Vec<String>,
    pub members: Vec<MemberInfo>,
    pub article_count: usize,
    /// Id of the home page, empty if there is none yet.
    pub home: String,
    /// Requests to join, waiting.
    pub requests: usize,
    /// Ids let in ahead of time, not seen yet.
    pub invited: Vec<String>,
    /// New ids that take a player's place once let in: new id → old id.
    pub takeovers: std::collections::BTreeMap<String, String>,
    /// Members' colours by the terms that name them — `player:<name>`, `doc:…`.
    pub colours: std::collections::BTreeMap<String, String>,
    /// Whether players' writing waits for the GM before the others see it.
    pub moderated: bool,
    /// Versions waiting: for the GM, what players wrote; for a player, their own.
    pub waiting: usize,
    /// How many versions of each document stay.
    pub version_limit: u32,
    /// Worked on online: a server elsewhere holds it.
    pub online: bool,
    /// The server it lives on, by code — empty: this device.
    pub server: String,
    /// Hosting identity even when its address has been removed.
    pub server_id: String,
    /// Offered by this app's server to its members over the network.
    pub offered: bool,
    /// The dice the world rolls with, by sides — empty for the usual set.
    pub dice: Vec<u32>,
    /// A server names it to everyone who asks — else only to its GM and members.
    pub listed: bool,
    /// How the world counts its days.
    pub calendar: WorldCalendar,
    /// The world's supporters, by id.
    pub supporters: Vec<String>,
    /// The one this app acts as supports the world: lets players in, gives
    /// and takes their roles, may take the GM's place.
    pub supporter: bool,
    /// Without a server: the folder of its mailboxes — the GM's.
    #[serde(default)]
    pub mailboxes: String,
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
    /// Known only by its cover: a name to follow links to, nothing more.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub cover: bool,
    /// The type's sections with nothing in them yet that the reader may
    /// start — not told the game master, who may start any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub startable: Vec<String>,
    /// Out of the index: its texts left out, to be read when needed.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub partial: bool,
    /// How its type's sections look in it where it differs — see `document::Look`.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub looks: std::collections::BTreeMap<String, document::Look>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct NewLayer {
    pub title: String,
    pub text: String,
    pub policy: String,
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
    profile::store(app)
}

fn role_text(role: Role) -> &'static str {
    match role {
        Role::Gm => "gm",
        Role::Supporter => "supporter",
        Role::Player => "player",
    }
}

/// A rule as the interface shows it: parsed and written again, or as it stands.
fn shown_rule(rule: &str) -> String {
    rule.parse::<Policy>().map(|p| p.to_string()).unwrap_or_else(|_| rule.to_string())
}

/// A layer as the interface gets it — for one who may see it.
fn layer_view(layer: &Layer, reader: &Reader) -> LayerView {
    LayerView {
        id: layer.id.clone(),
        title: layer.title.clone(),
        section: layer.section.clone(),
        may_edit: reader.may_edit(&layer.access),
        text: layer.text.clone(),
        fields: fields_value(&layer.fields),
        policy: shown_rule(&layer.access.view),
        edit: shown_rule(&layer.access.edit),
        epoch: 0,
    }
}

/// Fields as the views know them: none at all is `null`.
pub(crate) fn fields_value(fields: &serde_json::Map<String, serde_json::Value>) -> serde_json::Value {
    if fields.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::Object(fields.clone())
    }
}

/// Fields as a layer holds them.
pub(crate) fn fields_map(value: &serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    value.as_object().cloned().unwrap_or_default()
}

/// A document as `reader` sees it — `None` where it is not there for them.
pub(crate) fn article_view(doc: &Document, reader: &Reader) -> Option<ArticleView> {
    let seen = doc.seen_by(reader)?;
    if seen.is_cover() {
        return Some(ArticleView {
            id: seen.id.clone(),
            article_type: String::new(),
            slug: String::new(),
            name: seen.name().to_string(),
            visibility: String::new(),
            edit: "gm".into(),
            layers: Vec::new(),
            layer_total: 0,
            order: Vec::new(),
            cover: true,
            startable: Vec::new(),
            partial: false,
            looks: Default::default(),
        });
    }
    let base = seen.base()?;
    Some(ArticleView {
        id: seen.id.clone(),
        article_type: seen.doc_type.clone(),
        slug: seen.slug.clone(),
        name: base.title.clone(),
        visibility: shown_rule(&base.access.view),
        edit: shown_rule(&base.access.edit),
        order: document::own_order(&fields_value(&base.fields)),
        // The cover is no part of the page.
        layers: seen.layers.iter().filter(|l| !l.base && l.section != store::COVER).map(|l| layer_view(l, reader)).collect(),
        layer_total: doc.layers.iter().filter(|l| !l.base && l.section != store::COVER).count(),
        cover: false,
        startable: Vec::new(),
        partial: false,
        looks: document::looks_of(&fields_value(&base.fields)),
    })
}

/// What the one a command runs for reads in a world — or, as its game
/// master, what a player holding `preview_as` would.
pub(crate) fn reader_in(store: &Store, world: &str, preview_as: Option<Vec<String>>) -> Answer<Reader> {
    let reader = store.reader(world)?;
    Ok(match preview_as {
        Some(names) if reader.is_gm() => Reader::holding(&names),
        _ => reader,
    })
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

/// A short form of an id for display next to a name — as a player id's.
fn fingerprint_of(id: &str) -> String {
    let body = id.split_once(':').map(|(_, b)| b).unwrap_or(id);
    let short = &body[..body.len().min(8)];
    if short.len() == 8 { format!("{}-{}", &short[..4], &short[4..]) } else { short.to_string() }
}

/// A world as the interface gets it, for the one a command runs for.
fn world_info(store: &Store, meta: &WorldMeta) -> Result<WorldInfo, AppError> {
    let id = meta.id.clone();
    let me = store.actor()?;
    let mine = meta.member(&me);
    let role = mine.map(|m| m.role).unwrap_or_default();
    let is_gm = role == Role::Gm;
    let supporter = role == Role::Supporter;
    let manages = is_gm || supporter;
    let db = store.db(&id)?;
    let network = store.load_network()?;
    let waiting = db.waiting_count((!is_gm).then_some(me.as_str()))?;
    Ok(WorldInfo {
        name: meta.name.clone(),
        icon: meta.icon.clone(),
        folder: store.world_dir(&id).display().to_string(),
        role: role_text(role).to_string(),
        is_gm,
        attributes: mine.map(|m| m.attributes.clone()).unwrap_or_default(),
        // A player knows of the members only themselves.
        members: meta
            .members
            .iter()
            .filter(|m| manages || m.id == me)
            .map(|m| MemberInfo {
                id: m.id.clone(),
                name: m.name.clone(),
                fingerprint: fingerprint_of(&m.id),
                role: role_text(m.role).to_string(),
                attributes: m.attributes.clone(),
                trusted: m.trusted,
                slot: m.slot.clone(),
            })
            .collect(),
        article_count: db.document_count()?,
        home: meta.home.clone(),
        requests: if manages { db.requests()?.len() } else { 0 },
        invited: if manages { meta.invited.clone() } else { Vec::new() },
        takeovers: if is_gm { store.setting(&id, "takeovers", Default::default())? } else { Default::default() },
        colours: store.colours(&id)?,
        moderated: store.moderated(&id)?,
        waiting,
        version_limit: store.version_limit(&id)?,
        // Taken part in through a mailbox: each change goes there too.
        online: network.mailboxes.contains_key(&id),
        server: String::new(),
        server_id: String::new(),
        offered: network.shared.contains(&id),
        dice: store.dice(&id)?,
        listed: store.on_the_list(&id)?,
        calendar: meta.calendar.clone(),
        supporters: meta.members.iter().filter(|m| m.role == Role::Supporter).map(|m| m.id.clone()).collect(),
        supporter,
        roles: if manages { roles_of(store, &id)? } else { Vec::new() },
        mailboxes: if is_gm { network.posts.get(&id).cloned().unwrap_or_default() } else { String::new() },
        id,
    })
}

/// The roles there are to give: those made, and those members hold.
fn roles_of(store: &Store, world: &str) -> Answer<Vec<String>> {
    let mut all: Vec<String> = store.setting(world, "roles", Vec::<String>::new())?;
    for m in store.db(world)?.members()? {
        for a in m.attributes {
            if !all.contains(&a) {
                all.push(a);
            }
        }
    }
    for base in ["gm", "players", "all"] {
        if !all.iter().any(|a| a == base) {
            all.push(base.to_string());
        }
    }
    all.sort();
    Ok(all)
}

/// The world, after a command changed it, as the one it ran for sees it now.
fn world_now(store: &Store, world: &str) -> Answer<WorldInfo> {
    world_info(store, &store.load_world_meta(world)?)
}

/// Whether the one a command runs for is the world's game master — or says why not.
fn must_be_gm(store: &Store, world: &str, what: &str) -> Answer<()> {
    if store.is_gm(world) {
        Ok(())
    } else {
        Err(AppError(format!("Only the GM {what}.")))
    }
}

/// Whether the one a command runs for lets players in and gives roles: the
/// GM or a supporter.
fn manages(store: &Store, world: &str) -> Answer<bool> {
    let me = store.actor()?;
    Ok(store.db(world)?.member(&me)?.is_some_and(|m| matches!(m.role, Role::Gm | Role::Supporter)))
}

// --- Identity -----------------------------------------------------------

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
fn identity_status(app: tauri::AppHandle) -> Answer<Option<IdentityInfo>> {
    identity_status_in(&store(&app)?)
}

/// `identity_status` for whom the store is given.
pub(crate) fn identity_status_in(store: &Store) -> Answer<Option<IdentityInfo>> {
    if !store.has_identity() {
        return Ok(None);
    }
    let identity = store.load_identity()?;
    Ok(Some(info_from(store, &identity)))
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
fn identity_create(app: tauri::AppHandle, name: String) -> Answer<IdentityInfo> {
    identity_create_in(&store(&app)?, name)
}

/// `identity_create` for whom the store is given.
pub(crate) fn identity_create_in(store: &Store, name: String) -> Answer<IdentityInfo> {
    let identity = Identity::create(name)?;
    store.save_identity(&identity)?;
    Ok(info_from(store, &identity))
}

#[cfg(feature = "gui")]
/// Renaming does not change the id — access hangs on the key. The worlds
/// this device holds show the new name.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn identity_rename(app: tauri::AppHandle, name: String) -> Answer<IdentityInfo> {
    identity_rename_in(&store(&app)?, name)
}

/// `identity_rename` for whom the store is given.
pub(crate) fn identity_rename_in(store: &Store, name: String) -> Answer<IdentityInfo> {
    let mut identity = store.load_identity()?;
    identity.rename(name);
    store.save_identity(&identity)?;
    for world in store.held_worlds()? {
        let db = store.db(&world)?;
        if let Some(mut me) = db.member(identity.id().as_str())? {
            me.name = identity.name().to_string();
            db.put_member(&me)?;
        }
    }
    Ok(info_from(store, &identity))
}

#[cfg(feature = "gui")]
/// Writes the public part of one's own identity to a file.
///
/// That is the player card: you send it to the game master so they can
/// admit you. No private keys are in it.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn identity_export(app: tauri::AppHandle, path: String) -> Answer<String> {
    identity_export_in(&store(&app)?, path)
}

/// `identity_export` for whom the store is given.
pub(crate) fn identity_export_in(store: &Store, path: String) -> Answer<String> {
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
fn worlds_list(app: tauri::AppHandle) -> Answer<Vec<WorldInfo>> {
    worlds_list_in(&store(&app)?)
}

/// `worlds_list` for whom the store is given.
pub(crate) fn worlds_list_in(store: &Store) -> Answer<Vec<WorldInfo>> {
    let mut out = Vec::new();
    for id in store.held_worlds()? {
        let Ok(meta) = store.load_world_meta(&id) else { continue };
        // A world this device holds for others, without being a member of it.
        if meta.member(&store.actor()?).is_none() {
            continue;
        }
        out.push(world_info(store, &meta)?);
    }
    out.extend(network::remote_worlds(store)?);
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Destination {
    Local { path: String },
    /// On a server, by its code: it lives there alone.
    Server { server: String },
}

#[cfg(feature = "gui")]
static APP: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

/// Tells the app's windows of something that happened — nothing without them.
pub(crate) fn tell_windows(event: &str, payload: serde_json::Value) {
    #[cfg(feature = "gui")]
    if let Some(app) = APP.get() {
        use tauri::Emitter;
        let _ = app.emit(event, payload);
    }
    #[cfg(not(feature = "gui"))]
    let _ = (event, payload);
}

#[cfg(feature = "gui")]
/// Creates a world at the chosen location — a folder here.
#[cfg_attr(feature = "gui", tauri::command(async))]
async fn world_create(app: tauri::AppHandle, name: String, destination: Destination) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let (made, server) = {
        let store = store.clone();
        tauri::async_runtime::spawn_blocking(move || -> Answer<(WorldMeta, Option<String>)> {
            let (store, server) = match destination {
                Destination::Local { path } if path.is_empty() => (store, None),
                Destination::Local { path } => (store.destination(Path::new(&path))?, None),
                Destination::Server { server } => (store, Some(server)),
            };
            let identity = store.load_identity()?;
            Ok((new_world(&store, &identity, name)?, server))
        })
        .await
        .map_err(AppError::from)??
    };
    // On a server: made here, then brought there — it lies there alone.
    if let Some(server) = server {
        network::move_world(&store, &made.id, Some(server), None, &|_, _| {}).await?;
        return network::connect(&store, &made.id).await;
    }
    world_info(&store, &made)
}

#[cfg(feature = "gui")]
/// Copies a world into a folder here (none: the usual one): a world of its
/// own, with the types, documents and pictures, without its players and
/// versions. Tells how far it is (`copy-progress`).
#[cfg_attr(feature = "gui", tauri::command(async))]
async fn world_copy(app: tauri::AppHandle, world: String, destination: Destination) -> Answer<WorldInfo> {
    use tauri::Emitter;
    let store = store(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        must_be_gm(&store, &world, "copies a world")?;
        let target = match destination {
            Destination::Local { path } if path.is_empty() => store.clone(),
            Destination::Local { path } => store.destination(Path::new(&path))?,
            Destination::Server { .. } => return Err(AppError("Copy the world here, then move the copy onto the server.".into())),
        };
        let name = copy_name(&store, &store.load_world_meta(&world)?.name)?;
        let tell = |done: usize, total: usize| {
            let _ = app.emit("copy-progress", ("writing", done, total));
        };
        let meta = copy_world(&store, &target, &world, &name, &tell)?;
        world_info(&target, &meta)
    })
    .await
    .map_err(AppError::from)?
}

/// A copy of a world as a world of its own: a new id, its name, the game
/// master alone — no players, no versions, nobody asking or let in ahead.
fn copy_world(from: &Store, to: &Store, world: &str, name: &str, tell: &dyn Fn(usize, usize)) -> Answer<WorldMeta> {
    let id = store::new_world_id()?;
    let source = from.world_dir(world);
    let target = to.world_dir(&id);
    // Written to the disk first: what is copied is the world as it stands.
    from.db(world)?.checkpoint()?;
    store::copy_dir(&source, &target, tell)?;
    let made = (|| -> Answer<WorldMeta> {
        let db = store::WorldDb::open(&target)?;
        db.set("id", &id)?;
        db.set("name", name.trim())?;
        db.set_json("takeovers", &serde_json::json!({}))?;
        db.set_invited(&[])?;
        for request in db.requests()? {
            db.remove_request(&request.id)?;
        }
        for member in db.members()? {
            if member.role != Role::Gm {
                db.remove_member(&member.id)?;
            }
        }
        for doc in db.document_ids()? {
            db.delete_versions(&doc)?;
        }
        drop(db);
        let mut meta = to.load_world_meta(&id)?;
        // The world's page names the world.
        if !meta.home.is_empty() {
            document::rename_document(to, &id, &meta.home, name.trim())?;
        }
        meta = to.load_world_meta(&id)?;
        Ok(meta)
    })();
    if made.is_err() {
        let _ = to.delete_world(&id);
    }
    made
}

#[cfg(feature = "gui")]
/// Moves a GM's world into another folder here. Tells how far it is
/// (`copy-progress`).
#[cfg_attr(feature = "gui", tauri::command(async))]
async fn world_move(app: tauri::AppHandle, world: String, destination: Destination) -> Answer<WorldInfo> {
    use tauri::Emitter;
    let store = store(&app)?;
    let tell = |done: u64, total: u64| {
        let _ = app.emit("copy-progress", ("transfer", done, total));
    };
    if network::remote_of(&store, &world).is_some() {
        let (server, folder) = match destination {
            Destination::Server { server } => (Some(server), None),
            Destination::Local { path } => (None, (!path.is_empty()).then(|| PathBuf::from(path))),
        };
        network::move_world(&store, &world, server, folder, &tell).await?;
        return network::connect(&store, &world).await;
    }
    must_be_gm(&store, &world, "moves a world")?;
    match destination {
        Destination::Server { server } => {
            network::move_world(&store, &world, Some(server), None, &tell).await?;
            return network::connect(&store, &world).await;
        }
        Destination::Local { path } => {
            if !path.is_empty() {
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
    }
    world_now(&store, &world)
}

/// "Name (copy)" — or with a number, where that is taken.
fn copy_name(store: &Store, name: &str) -> Answer<String> {
    let taken: Vec<String> = store.list_worlds()?.iter().map(|m| m.name.trim().to_lowercase()).collect();
    let mut n = 1;
    loop {
        let name = if n == 1 { format!("{} (copy)", name.trim()) } else { format!("{} (copy {n})", name.trim()) };
        if !taken.contains(&name.to_lowercase()) {
            return Ok(name);
        }
        n += 1;
    }
}

/// Creates a world with its own page.
fn new_world(store: &Store, identity: &Identity, name: String) -> Answer<WorldMeta> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(AppError("Enter a world name.".into()));
    }
    // No two worlds of one name — in the menu they could not be told apart.
    if store.list_worlds()?.iter().any(|m| m.name.trim().eq_ignore_ascii_case(&name)) {
        return Err(AppError(format!("A world called “{name}” already exists.")));
    }
    let mut meta = store.create_world(&name, identity)?;
    // The world is a document too — its page is the first thing one sees
    // inside it. It comes into being with the world, not on request.
    meta.home = store::acting_for(identity.id().as_str(), || create_page(store, &meta.id, "home", &name, String::new()))?;
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

// --- Members ------------------------------------------------------------

#[cfg(feature = "gui")]
/// Admits a player to the world — from their player card.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn member_add(app: tauri::AppHandle, world: String, path: String, attributes: Vec<String>) -> Answer<WorldInfo> {
    member_add_in(&store(&app)?, world, path, attributes)
}

/// `member_add` for whom the store is given.
pub(crate) fn member_add_in(store: &Store, world: String, path: String, attributes: Vec<String>) -> Answer<WorldInfo> {
    let raw = std::fs::read(&path).map_err(AppError::from)?;
    let card: IdentityPublic = serde_json::from_slice(&raw).map_err(AppError::from)?;
    admit(store, &world, card, attributes)?;
    world_now(store, &world)
}

/// Takes a player in — however they came: a card, an id let in ahead of
/// time, a request. They get what is asked for besides `players` and `all`,
/// which every member holds. The game master's, or a supporter's.
pub(crate) fn admit(store: &Store, world: &str, card: IdentityPublic, attributes: Vec<String>) -> Answer<()> {
    if !card.is_genuine() {
        return Err(AppError("This player id does not match its keys.".into()));
    }
    if !manages(store, world)? {
        return Err(AppError("Only the GM or a supporter lets players in.".into()));
    }
    let gm = store.is_gm(world);
    if !gm && attributes.iter().any(|a| a.trim() == "gm") {
        return Err(AppError("Only the GM makes someone GM.".into()));
    }
    let db = store.db(world)?;
    let id = card.id.to_string();
    let mut member = db.member(&id)?.unwrap_or_else(|| store::member_of(&card, Role::Player));
    member.name = card.name.clone();
    member.verify_key = card.verify_key.clone();
    for a in attributes.into_iter().filter(|a| !["players", "all", "gm"].contains(&a.trim())) {
        if !member.attributes.contains(&a) {
            member.attributes.push(a);
        }
    }
    // Taking someone's place: what was theirs is theirs now, and the old id leaves.
    let mut takeovers: std::collections::BTreeMap<String, String> = store.setting(world, "takeovers", Default::default())?;
    if let Some(old) = takeovers.remove(&id) {
        if let Some(gone) = db.member(&old)?.filter(|m| m.role == Role::Player) {
            for a in gone.attributes {
                if !member.attributes.contains(&a) {
                    member.attributes.push(a);
                }
            }
            member.colour = gone.colour;
            member.trusted = gone.trusted;
            db.remove_member(&old)?;
        }
        store.set_setting(world, "takeovers", &takeovers)?;
    }
    db.put_member(&member)?;
    db.remove_request(&id)?;
    let invited: Vec<String> = db.invited()?.into_iter().filter(|i| *i != id).collect();
    db.set_invited(&invited)?;
    Ok(())
}

/// The card of someone who asked to join.
pub(crate) fn card_of(request: &aw_store::db::Request) -> Answer<IdentityPublic> {
    Ok(serde_json::from_value(serde_json::json!({ "id": request.id, "name": request.name, "verify_key": request.verify_key, "seal_key": request.seal_key }))?)
}

/// Whether an id is a whole player id: `id:` and 16 hex characters.
pub(crate) fn is_player_id(id: &str) -> bool {
    id.strip_prefix("id:").is_some_and(|body| body.len() == 16 && body.chars().all(|c| c.is_ascii_hexdigit()))
}

#[cfg(feature = "gui")]
/// One's own colour in a world — on one's name and the documents assigned to one.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn member_colour_set(app: tauri::AppHandle, world: String, colour: String) -> Answer<WorldInfo> {
    member_colour_set_in(&store(&app)?, world, colour)
}

/// `member_colour_set` for whom the store is given.
pub(crate) fn member_colour_set_in(store: &Store, world: String, colour: String) -> Answer<WorldInfo> {
    let db = store.db(&world)?;
    let me = store.actor()?;
    let mut member = db.member(&me)?.ok_or_else(|| AppError("You are not a member of this world.".into()))?;
    member.colour = colour.chars().take(32).collect();
    db.put_member(&member)?;
    world_now(store, &world)
}

#[cfg(feature = "gui")]
/// A player's new id takes the place of their old one — for someone whose
/// device was set up anew without their keys. What was assigned to the old
/// id goes to the new one, and the old one leaves. A new id not let in yet
/// takes over as soon as it is.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn member_replace(app: tauri::AppHandle, world: String, old: String, new: String) -> Answer<WorldInfo> {
    member_replace_in(&store(&app)?, world, old, new)
}

/// `member_replace` for whom the store is given.
pub(crate) fn member_replace_in(store: &Store, world: String, old: String, new: String) -> Answer<WorldInfo> {
    replace_member(store, &world, &old, &new)?;
    world_now(store, &world)
}

pub(crate) fn replace_member(store: &Store, world: &str, old: &str, new: &str) -> Answer<()> {
    must_be_gm(store, world, "gives a player a new id")?;
    let new = new.trim();
    if !is_player_id(new) {
        return Err(AppError("That is no player id. It is the whole id — id: and 16 characters.".into()));
    }
    let db = store.db(world)?;
    if new == old || !db.member(old)?.is_some_and(|m| m.role == Role::Player) {
        return Err(AppError("There is no such player.".into()));
    }
    let mut takeovers: std::collections::BTreeMap<String, String> = store.setting(world, "takeovers", Default::default())?;
    takeovers.insert(new.to_string(), old.to_string());
    store.set_setting(world, "takeovers", &takeovers)?;
    // Asking already: at once.
    if let Some(request) = db.requests()?.into_iter().find(|r| r.id == new) {
        return admit(store, world, card_of(&request)?, Vec::new());
    }
    // Otherwise once it shows up, as anyone let in ahead of time.
    let mut invited = db.invited()?;
    if !invited.iter().any(|i| i == new) {
        invited.push(new.to_string());
        db.set_invited(&invited)?;
    }
    Ok(())
}

#[cfg(feature = "gui")]
/// Takes a member out of the world — or an id let in ahead of time.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn member_remove(app: tauri::AppHandle, world: String, id: String) -> Answer<WorldInfo> {
    member_remove_in(&store(&app)?, world, id)
}

/// `member_remove` for whom the store is given.
pub(crate) fn member_remove_in(store: &Store, world: String, id: String) -> Answer<WorldInfo> {
    remove_member(store, &world, &id)?;
    world_now(store, &world)
}

/// Takes a member out — the GM's, or a supporter's for players. Nobody but
/// the GM takes a supporter out, and the GM stays.
pub(crate) fn remove_member(store: &Store, world: &str, id: &str) -> Answer<()> {
    if !manages(store, world)? {
        return Err(AppError("Only the GM or a supporter takes players out.".into()));
    }
    let db = store.db(world)?;
    let invited: Vec<String> = db.invited()?;
    if invited.iter().any(|i| i == id) {
        db.set_invited(&invited.into_iter().filter(|i| i != id).collect::<Vec<_>>())?;
        return Ok(());
    }
    match db.member(id)? {
        Some(m) if m.role == Role::Gm => Err(AppError("The GM stays. Hand the world over first.".into())),
        Some(m) if m.role == Role::Supporter && !store.is_gm(world) => Err(AppError("Only the GM takes a supporter out.".into())),
        Some(_) => Ok(db.remove_member(id)?),
        None => Err(AppError("There is no such member.".into())),
    }
}

#[cfg(feature = "gui")]
/// Switches review on or off: whether what players write waits for the GM.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_moderation_set(app: tauri::AppHandle, world: String, on: bool) -> Answer<WorldInfo> {
    world_moderation_set_in(&store(&app)?, world, on)
}

/// `world_moderation_set` for whom the store is given.
pub(crate) fn world_moderation_set_in(store: &Store, world: String, on: bool) -> Answer<WorldInfo> {
    must_be_gm(store, &world, "decides about review")?;
    store.set_setting(&world, "moderated", &on)?;
    // What waits for review stands in its author's view, or no longer.
    store.db(&world)?.changed_all();
    world_now(store, &world)
}

#[cfg(feature = "gui")]
/// Sets how many versions of each document stay — the GM's.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_versions_set(app: tauri::AppHandle, world: String, max: u32) -> Answer<WorldInfo> {
    world_versions_set_in(&store(&app)?, world, max)
}

/// `world_versions_set` for whom the store is given.
pub(crate) fn world_versions_set_in(store: &Store, world: String, max: u32) -> Answer<WorldInfo> {
    must_be_gm(store, &world, "decides how many versions stay")?;
    if max == 0 {
        return Err(AppError("At least one version stays.".into()));
    }
    store.set_setting(&world, "version_limit", &max)?;
    let db = store.db(&world)?;
    for doc in db.document_ids()? {
        db.trim_versions(&doc, max as usize)?;
    }
    world_now(store, &world)
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
    world_listed_set_in(&store(&app)?, world, listed)
}

/// `world_listed_set` for whom the store is given.
pub(crate) fn world_listed_set_in(store: &Store, world: String, listed: bool) -> Answer<WorldInfo> {
    must_be_gm(store, &world, "decides who sees the world")?;
    store.set_setting(&world, "listed", &listed)?;
    // Named on the local network now, or no more.
    network::republish();
    world_now(store, &world)
}

#[cfg(feature = "gui")]
/// Sets the dice the world rolls with — the GM's. Each between 2 and 1000
/// sides, each once, in the order given.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_dice_set(app: tauri::AppHandle, world: String, sides: Vec<u32>) -> Answer<WorldInfo> {
    world_dice_set_in(&store(&app)?, world, sides)
}

/// `world_dice_set` for whom the store is given.
pub(crate) fn world_dice_set_in(store: &Store, world: String, sides: Vec<u32>) -> Answer<WorldInfo> {
    must_be_gm(store, &world, "chooses the world's dice")?;
    let mut kept: Vec<u32> = Vec::new();
    for n in sides.into_iter().filter(|n| (2..=1000).contains(n)) {
        if !kept.contains(&n) {
            kept.push(n);
        }
    }
    store.set_setting(&world, "dice", &kept)?;
    world_now(store, &world)
}

#[cfg(feature = "gui")]
/// The side bar's entries — `None` while the game master has set none.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn navigation_load(app: tauri::AppHandle, world: String) -> Answer<Option<Vec<aw_model::navigation::NavEntry>>> {
    navigation_load_in(&store(&app)?, world)
}

/// `navigation_load` for whom the store is given.
pub(crate) fn navigation_load_in(store: &Store, world: String) -> Answer<Option<Vec<aw_model::navigation::NavEntry>>> {
    let reader = store.reader(&world)?;
    Ok(store.navigation_for(&world, &reader)?)
}

#[cfg(feature = "gui")]
/// Sets the side bar's entries — the GM's.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn navigation_set(app: tauri::AppHandle, world: String, entries: Vec<aw_model::navigation::NavEntry>) -> Answer<()> {
    navigation_set_in(&store(&app)?, world, entries)
}

/// `navigation_set` for whom the store is given.
pub(crate) fn navigation_set_in(store: &Store, world: String, entries: Vec<aw_model::navigation::NavEntry>) -> Answer<()> {
    must_be_gm(store, &world, "sets the side bar")?;
    store.set_navigation(&world, &entries)?;
    Ok(())
}

#[cfg(feature = "gui")]
/// Trusts a member — their writing counts without review — or no longer.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn member_trust_set(app: tauri::AppHandle, world: String, member: String, trusted: bool) -> Answer<WorldInfo> {
    member_trust_set_in(&store(&app)?, world, member, trusted)
}

/// `member_trust_set` for whom the store is given.
pub(crate) fn member_trust_set_in(store: &Store, world: String, member: String, trusted: bool) -> Answer<WorldInfo> {
    must_be_gm(store, &world, "decides about review")?;
    let db = store.db(&world)?;
    let mut m = db.member(&member)?.ok_or_else(|| AppError("There is no such member.".into()))?;
    m.trusted = trusted;
    db.put_member(&m)?;
    world_now(store, &world)
}

#[cfg(feature = "gui")]
/// Makes these players the world's supporters — the GM's. Supporters not
/// named any more are players again.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_supporters_set(app: tauri::AppHandle, world: String, ids: Vec<String>) -> Answer<WorldInfo> {
    world_supporters_set_in(&store(&app)?, world, ids)
}

/// `world_supporters_set` for whom the store is given.
pub(crate) fn world_supporters_set_in(store: &Store, world: String, ids: Vec<String>) -> Answer<WorldInfo> {
    must_be_gm(store, &world, "names supporters")?;
    let db = store.db(&world)?;
    for mut m in db.members()? {
        let role = match (m.role, ids.contains(&m.id)) {
            (Role::Gm, _) => Role::Gm,
            (_, true) => Role::Supporter,
            (_, false) => Role::Player,
        };
        if role != m.role {
            m.role = role;
            db.put_member(&m)?;
        }
    }
    world_now(store, &world)
}

#[cfg(feature = "gui")]
/// Takes the GM's place, as a supporter. Everyone sees it: the world names
/// the new GM, and the old one is a player of it.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_take_over(app: tauri::AppHandle, world: String) -> Answer<WorldInfo> {
    world_take_over_in(&store(&app)?, world)
}

/// `world_take_over` for whom the store is given.
pub(crate) fn world_take_over_in(store: &Store, world: String) -> Answer<WorldInfo> {
    let me = store.actor()?;
    let db = store.db(&world)?;
    if !db.member(&me)?.is_some_and(|m| m.role == Role::Supporter) {
        return Err(AppError("Only a supporter of the world can take the GM's place.".into()));
    }
    hand_world(store, &world, &me)?;
    world_now(store, &world)
}

#[cfg(feature = "gui")]
/// Hands the world over to another member: they are its GM from now on, and
/// the one who was is a player of it.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn gm_handover(app: tauri::AppHandle, world: String, member: Option<String>) -> Answer<WorldInfo> {
    gm_handover_in(&store(&app)?, world, member)
}

/// `gm_handover` for whom the store is given.
pub(crate) fn gm_handover_in(store: &Store, world: String, member: Option<String>) -> Answer<WorldInfo> {
    must_be_gm(store, &world, "can hand the world over")?;
    let Some(member) = member else { return world_now(store, &world) };
    if member == store.actor()? || store.db(&world)?.member(&member)?.is_none() {
        return Err(AppError("The world goes only to another member.".into()));
    }
    hand_world(store, &world, &member)?;
    world_now(store, &world)
}

/// `to` is the world's GM now; whoever was is a player.
fn hand_world(store: &Store, world: &str, to: &str) -> Answer<()> {
    let db = store.db(world)?;
    for mut m in db.members()? {
        if m.role == Role::Gm && m.id != to {
            m.role = Role::Player;
            db.put_member(&m)?;
        }
    }
    let mut new = db.member(to)?.ok_or_else(|| AppError("There is no such member.".into()))?;
    new.role = Role::Gm;
    db.put_member(&new)?;
    Ok(())
}

#[cfg(feature = "gui")]
/// Deletes a world for good.
///
/// `confirmation` has to be the world's name. The check deliberately lives
/// here too and not only in the interface: what is deleted, nobody brings back.
#[cfg_attr(feature = "gui", tauri::command(async))]
async fn world_delete(app: tauri::AppHandle, world: String, confirmation: String) -> Answer<()> {
    let store = store(&app)?;
    // On a server: its GM deletes it there; a player leaves it.
    if let Some(info) = network::remote_worlds(&store)?.into_iter().find(|w| w.id == world) {
        if confirmation.trim() != info.name.trim() {
            return Err(AppError(format!("To delete, the name has to match exactly: {:?}", info.name)));
        }
        return network::leave(&store, &world, info.is_gm).await;
    }
    if mailbox::is_mailbox(&store, &world) {
        return mailbox::leave(&store, &world);
    }
    world_delete_in(&store, world, confirmation)
}

/// `world_delete` for whom the store is given.
pub(crate) fn world_delete_in(store: &Store, world: String, confirmation: String) -> Answer<()> {
    must_be_gm(store, &world, "deletes a world")?;
    let meta = store.load_world_meta(&world)?;
    if confirmation.trim() != meta.name.trim() {
        return Err(AppError(format!("To delete, the name has to match exactly: {:?}", meta.name)));
    }
    store.delete_world(&world)?;
    Ok(())
}

#[cfg(feature = "gui")]
/// Sets the world's emoji.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_icon_set(app: tauri::AppHandle, world: String, icon: String) -> Answer<WorldInfo> {
    world_icon_set_in(&store(&app)?, world, icon)
}

/// `world_icon_set` for whom the store is given.
pub(crate) fn world_icon_set_in(store: &Store, world: String, icon: String) -> Answer<WorldInfo> {
    must_be_gm(store, &world, "sets the world's icon")?;
    let mut meta = store.load_world_meta(&world)?;
    // A few characters are enough for any emoji including variation selectors.
    meta.icon = icon.chars().take(4).collect();
    store.save_world_meta(&meta)?;
    world_info(store, &meta)
}

#[cfg(feature = "gui")]
/// Sets how the world counts its days — the game master's. Dates already
/// written stay the same days; they are only written in the new calendar.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn calendar_set(app: tauri::AppHandle, world: String, calendar: WorldCalendar) -> Answer<WorldInfo> {
    calendar_set_in(&store(&app)?, world, calendar)
}

/// `calendar_set` for whom the store is given.
pub(crate) fn calendar_set_in(store: &Store, world: String, calendar: WorldCalendar) -> Answer<WorldInfo> {
    must_be_gm(store, &world, "sets the calendar")?;
    calendar.validate().map_err(AppError::from)?;
    let mut meta = store.load_world_meta(&world)?;
    meta.calendar = calendar;
    store.save_world_meta(&meta)?;
    world_info(store, &meta)
}

// --- Roles --------------------------------------------------------------

#[cfg(feature = "gui")]
/// A role to give — `class:rogue`, `knowledge:crypt`. The GM's or a supporter's.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn attribute_create(app: tauri::AppHandle, world: String, attribute: String) -> Answer<Vec<String>> {
    attribute_create_in(&store(&app)?, world, attribute)
}

/// `attribute_create` for whom the store is given.
pub(crate) fn attribute_create_in(store: &Store, world: String, attribute: String) -> Answer<Vec<String>> {
    if !manages(store, &world)? {
        return Err(AppError("Only the GM or a supporter makes roles.".into()));
    }
    let parsed: Attribute = attribute.parse().map_err(AppError::from)?;
    let mut made: Vec<String> = store.setting(&world, "roles", Vec::new())?;
    if !made.contains(&parsed.to_string()) {
        made.push(parsed.to_string());
        store.set_setting(&world, "roles", &made)?;
    }
    roles_of(store, &world)
}

#[cfg(feature = "gui")]
/// Takes a role away from everyone, and out of the roles to give.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn attribute_revoke(app: tauri::AppHandle, world: String, attribute: String) -> Answer<u32> {
    attribute_revoke_in(&store(&app)?, world, attribute)
}

/// `attribute_revoke` for whom the store is given.
pub(crate) fn attribute_revoke_in(store: &Store, world: String, attribute: String) -> Answer<u32> {
    must_be_gm(store, &world, "takes roles away")?;
    let made: Vec<String> = store.setting(&world, "roles", Vec::<String>::new())?;
    store.set_setting(&world, "roles", &made.into_iter().filter(|a| *a != attribute).collect::<Vec<_>>())?;
    let db = store.db(&world)?;
    for mut m in db.members()? {
        if m.attributes.contains(&attribute) {
            m.attributes.retain(|a| *a != attribute);
            db.put_member(&m)?;
        }
    }
    Ok(0)
}

#[cfg(feature = "gui")]
/// What a player is assigned to — documents (`doc:…`: their character, a
/// class) and roles — and so what they may see. The game master's, or a
/// supporter's for players.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn member_attributes_set(app: tauri::AppHandle, world: String, member: String, attributes: Vec<String>) -> Answer<WorldInfo> {
    let store = store(&app)?;
    member_attributes_set_in(&store, &world, &member, attributes)?;
    world_now(&store, &world)
}

pub(crate) fn member_attributes_set_in(store: &Store, world: &str, member: &str, attributes: Vec<String>) -> Answer<()> {
    if !manages(store, world)? {
        return Err(AppError("Only the GM or a supporter assigns players.".into()));
    }
    let gm = store.is_gm(world);
    let db = store.db(world)?;
    let mut m = db.member(member)?.ok_or_else(|| AppError("There is no such member.".into()))?;
    // The GM too may note whom they play: they see everything anyway, but
    // handing the world over, what they play stays theirs as a player.
    if !gm && (m.role != Role::Player || attributes.iter().any(|a| a.trim() == "gm")) {
        return Err(AppError("Only the GM assigns the GM and supporters.".into()));
    }
    for a in &attributes {
        a.trim().parse::<Attribute>().map_err(AppError::from)?;
    }
    m.attributes = attributes.into_iter().map(|a| a.trim().to_string()).filter(|a| !["players", "all", "gm"].contains(&a.as_str())).collect();
    m.attributes.dedup();
    db.put_member(&m)?;
    Ok(())
}
// --- Templates ----------------------------------------------------------

#[cfg(feature = "gui")]
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn templates_load(app: tauri::AppHandle, world: String) -> Answer<Vec<Template>> {
    templates_load_in(&store(&app)?, world)
}

/// `templates_load` for whom the store is given.
pub(crate) fn templates_load_in(store: &Store, world: String) -> Answer<Vec<Template>> {
    let reader = store.reader(&world)?;
    Ok(store.templates_for(&world, &reader)?)
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
fn template_save(
    app: tauri::AppHandle,
    world: String,
    template: Template,
) -> Answer<Vec<Template>> {
    save_template(&store(&app)?, &world, template)
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
fn save_template(store: &Store, world: &str, mut template: Template) -> Answer<Vec<Template>> {
    must_be_gm(store, world, "changes types")?;
    // Normalise before checking: a new type gets its id here.
    template.normalize();
    template.validate().map_err(AppError::from)?;
    let mut all = store.load_templates(world)?;
    // The texts a new document starts with are stored like any text: ids —
    // its own properties as this very template now has them.
    let mut with_this: Vec<Template> = all.iter().filter(|t| t.id != template.id).cloned().collect();
    with_this.push(template.clone());
    let known = document::known_shared(store, world, &store.reader(world)?, with_this)?;
    let known = &*known;
    for part in template.blocks.iter_mut().flat_map(|b| b.parts.iter_mut()) {
        part.text = names::stored(&part.text, known, &template.id);
    }
    // A create button beside a title names types: by id, a rename breaks nothing.
    for block in &mut template.blocks {
        block.new = document::types_as(known, &block.new, true);
    }
    default_documents(&mut template, known, true);

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
            document::rename_document(store, world, &page, &new)?;
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
    must_be_gm(store, world, "deletes types")?;
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
    if let Some(page) = all.iter().find(|t| t.id == slug).map(|t| t.page.clone()).filter(|p| !p.is_empty()) {
        store.db(world)?.delete_document(&page)?;
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
    layers: Vec<NewLayer>,
) -> Answer<String> {
    article_create_in(&store(&app)?, world, article_type, slug, name, visibility, layers)
}

/// `article_create` for whom the store is given.
pub(crate) fn article_create_in(store: &Store, world: String, article_type: String, slug: String, name: String, visibility: String, layers: Vec<NewLayer>) -> Answer<String> {
    // A player makes what their type lets them — with the access the type
    // gives, not one of their own (`propose_document`).
    if !store.reader(&world)?.is_gm() {
        return propose_document(store, &world, article_type, slug, name, layers);
    }
    let layers = names_stored(store, &world, &article_type, layers)?;
    create_document(store, &world, article_type, slug, name, visibility, layers)
}

/// Names as written in new texts, ids as stored — the same as when a
/// document is saved.
fn names_stored(store: &Store, world: &str, article_type: &str, mut layers: Vec<NewLayer>) -> Answer<Vec<NewLayer>> {
    let known = known_to_me(store, world)?;
    let known = &*known;
    for layer in &mut layers {
        layer.text = names::stored(&layer.text, known, article_type);
        if let Some(map) = layer.fields.as_object_mut() {
            for value in map.values_mut() {
                if let serde_json::Value::String(s) = value {
                    *s = names::stored(s, known, article_type);
                }
            }
        }
    }
    Ok(layers)
}

#[cfg(feature = "gui")]
/// A new document, by a player — made at once where the type lets this
/// player make its documents (`create=`), with the type's access: the base,
/// the properties and every part as the type sets them out; a part the type
/// does not know is left out. Returns its id.
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
    let _ = article;
    propose_document(&store, &world, article_type, slug, name, layers)
}

/// What `article_propose` does — without Tauri, for the tests too.
pub(crate) fn propose_document(store: &Store, world: &str, article_type: String, slug: String, name: String, layers: Vec<NewLayer>) -> Answer<String> {
    let templates = store.load_templates(world)?;
    let own = templates
        .iter()
        .find(|t| t.id == article_type && !t.group)
        .ok_or_else(|| AppError("There is no such type to make a document of.".into()))?;
    let reader = store.reader(world)?;
    let allowed = reader.is_gm() || (!own.create.trim().is_empty() && reader.allows(&own.create));
    if !allowed {
        return Err(AppError("This type's documents are not yours to make.".into()));
    }
    let template = types::effective(&templates, own);
    let auth = template.auth.clone().unwrap_or_default();
    let layers: Vec<NewLayer> = names_stored(store, world, &article_type, layers)?
        .into_iter()
        .filter_map(|mut layer| {
            if layer.section == document::INFOBOX_SECTION {
                (layer.policy, layer.edit) = (auth.view.clone(), auth.edit.clone());
            } else if let Some(key) = layer.section.strip_prefix(&format!("{}:", document::INFOBOX_SECTION)) {
                let access = template.properties.iter().find(|f| f.key == key)?.access.clone()?;
                (layer.policy, layer.edit) = (access.view, access.edit);
            } else {
                let (block, part) = template.blocks.iter().find_map(|b| b.parts.iter().find(|p| p.key == layer.section).map(|p| (b, p)))?;
                (layer.policy, layer.edit, layer.title) = (part.view.clone(), part.edit.clone(), block.title.clone());
                layer.fields = serde_json::Value::Object(Default::default());
            }
            layer.parent = None;
            Some(layer)
        })
        .collect();
    let name: String = name.trim().chars().take(200).collect();
    let slug = if slug.trim().is_empty() { name.to_lowercase() } else { slug };
    create_document(store, world, own.id.clone(), slug, name, auth.view.clone(), layers)
}

/// What the one a command runs for knows by name in this world.
/// The names the one a command runs for knows — from the documents' bases,
/// kept while the world stays as it is (`document::known_shared`).
fn known_to_me(store: &Store, world: &str) -> Answer<std::sync::Arc<document::Known>> {
    document::known_shared(store, world, &store.reader(world)?, store.load_templates(world)?)
}

#[cfg(feature = "gui")]
/// Texts as they are written — ids turned back into the current names. For
/// a new document, whose texts start as its type's.
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn texts_written(app: tauri::AppHandle, world: String, article_type: String, texts: Vec<String>) -> Answer<Vec<String>> {
    texts_written_in(&store(&app)?, world, article_type, texts)
}

/// `texts_written` for whom the store is given.
pub(crate) fn texts_written_in(store: &Store, world: String, article_type: String, texts: Vec<String>) -> Answer<Vec<String>> {
    let known = known_to_me(store, &world)?;
    let known = &*known;
    Ok(texts.iter().map(|t| names::written(t, known, &article_type)).collect())
}

/// Stores a new document — for the commands and for the world's own page,
/// which comes into being together with the world.
fn create_document(store: &Store, world: &str, article_type: String, slug: String, name: String, visibility: String, layers: Vec<NewLayer>) -> Answer<String> {
    create_document_as(store, world, new_document_id(), article_type, slug, name, visibility, layers)
}

/// A fresh document id: `art:<ulid>`.
pub(crate) fn new_document_id() -> String {
    aw_model::ArticleId::new().to_string()
}

/// The same, under an id chosen beforehand.
#[allow(clippy::too_many_arguments)]
fn create_document_as(
    store: &Store,
    world: &str,
    id: String,
    article_type: String,
    slug: String,
    name: String,
    visibility: String,
    mut layers: Vec<NewLayer>,
) -> Answer<String> {
    // The id first: a part its type gives `view=self` is the document's own.
    let visibility = own_view(&visibility, &id, &article_type);
    // Who edits it: as its type says — the GM where it says nothing.
    let templates = store.load_templates(world)?;
    let whole = aw_model::types::find(&templates, &article_type).map(|t| aw_model::types::effective(&templates, t));
    // The documents it belongs to, as its parent field names them — for `parent`.
    let parents: Vec<String> = whole
        .as_ref()
        .and_then(|t| t.properties.iter().find(|f| f.kind.is_parent()))
        .and_then(|f| layers.iter().find_map(|l| l.fields.get(f.key.as_str())))
        .map(|v| document::picks(v).into_iter().map(|(id, _)| id).filter(|id| !id.is_empty()).collect())
        .unwrap_or_default();
    let edit = aw_model::types::find(&templates, &article_type)
        .and_then(|t| t.auth.as_ref())
        .map(|a| own_view(&parent_view(&a.edit, &parents), &id, &article_type))
        .unwrap_or_else(|| "gm".into());
    // A section as its type sets it out goes as the document — see
    // `types::set_out`; one given access of its own keeps it.
    if let Some(whole) = whole {
        let document = aw_model::template::Access { view: visibility.clone(), edit: edit.clone() };
        for layer in &mut layers {
            let typed = whole.blocks.iter().flat_map(|b| b.parts.iter().map(move |p| (b, p))).find(|(_, p)| p.key == layer.section);
            if let Some((block, part)) = typed {
                if part.view == layer.policy && part.edit == layer.edit {
                    let a = document::typed_access(&templates, &id, &article_type, &block.key, part, &document);
                    layer.policy = a.view;
                    layer.edit = a.edit;
                }
            }
        }
    }
    for layer in &mut layers {
        layer.policy = own_view(&layer.policy, &id, &article_type);
        layer.edit = own_view(&layer.edit, &id, &article_type);
    }
    // Nested sections inherit their parents' visibility.
    let effective = effective_policies(&layers)?;
    // A rule that cannot be read is no rule to keep.
    visibility.parse::<Policy>().map_err(AppError::from)?;
    let by = store.actor()?;
    let at = store::now();
    let new_id = || aw_model::LayerId::new().to_string();
    // The base layer carries the article's name and visibility.
    let mut all = vec![Layer {
        id: new_id(),
        base: true,
        section: String::new(),
        title: name,
        access: Access { view: visibility, edit },
        text: String::new(),
        fields: Default::default(),
        images: Vec::new(),
    }];
    for (input, view) in layers.into_iter().zip(effective) {
        let images = store::named_images(&input.text).into_iter().collect();
        all.push(Layer {
            id: new_id(),
            base: false,
            section: input.section,
            title: input.title,
            access: Access { view: view.to_string(), edit: if input.edit.is_empty() { "gm".into() } else { input.edit } },
            text: input.text,
            fields: fields_map(&input.fields),
            images,
        });
    }
    let doc = Document { id: id.clone(), doc_type: article_type, slug, layers: all, created: at, created_by: by.clone(), changed: at, changed_by: by, proposed: false };
    store.db(world)?.put_document(&doc)?;
    Ok(id)
}

/// `parent` in an access: whoever is assigned to a document this one belongs
/// to — the characters a backstory belongs to, as their `doc:…`, one group
/// each. Without one, what names it falls away; nothing left, the GM.
pub(crate) fn parent_view(policy: &str, parents: &[String]) -> String {
    let is_parent = |w: &str| w.eq_ignore_ascii_case("parent");
    let mut groups: Vec<String> = Vec::new();
    for group in policy.split(" OR ").flat_map(|g| g.split(" or ")) {
        let words: Vec<&str> = group.split_whitespace().collect();
        if !words.iter().any(|w| is_parent(w)) {
            groups.push(words.join(" "));
            continue;
        }
        for parent in parents {
            let doc = format!("doc:{}", parent.trim_start_matches("art:"));
            groups.push(words.iter().map(|w| if is_parent(w) { doc.as_str() } else { w }).collect::<Vec<_>>().join(" "));
        }
    }
    groups.retain(|g| !g.is_empty());
    if groups.is_empty() { "gm".into() } else { groups.join(" OR ") }
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

#[cfg(feature = "gui")]
/// Sets who may see an article or one of its sections. The GM's — or, for a
/// part, whoever may edit it.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn visibility_set(app: tauri::AppHandle, world: String, article: String, layer: Option<String>, policy: String) -> Answer<()> {
    visibility_set_in(&store(&app)?, &world, &article, layer.as_deref(), &policy)
}

pub(crate) fn visibility_set_in(store: &Store, world: &str, article: &str, layer: Option<&str>, policy: &str) -> Answer<()> {
    let reader = store.reader(world)?;
    let db = store.db(world)?;
    let mut doc = db.document(article)?.ok_or_else(|| AppError("There is no such article.".into()))?;
    let policy = own_view(policy, article, &doc.doc_type);
    policy.parse::<Policy>().map_err(AppError::from)?;
    let target = doc
        .layers
        .iter_mut()
        .find(|l| match layer {
            Some(id) => l.id == id,
            None => l.base,
        })
        .ok_or_else(|| AppError("There is no such part.".into()))?;
    // Who may edit a part may say who sees it; the document as a whole is the GM's.
    let allowed = reader.is_gm() || (!target.base && reader.may_edit(&target.access));
    if !allowed {
        return Err(AppError("Only the GM or who may edit a part sets who sees it.".into()));
    }
    target.access.view = policy;
    doc.changed = store::now();
    doc.changed_by = store.actor()?;
    db.put_document(&doc)?;
    Ok(())
}

/// A type's access, given to documents of it that are there already: who
/// sees and who edits each of `articles` — and each section of them that
/// had the same access as its document; a section with access of its own
/// keeps it. The game master's. How many documents changed.
pub(crate) fn type_access_apply_in(store: &Store, world: &str, article_type: &str, articles: &[String], view: Option<String>, edit: Option<String>) -> Answer<usize> {
    must_be_gm(store, world, "sets who sees and edits documents")?;
    let db = store.db(world)?;
    let same = |a: &str, b: &str| a == b || a.parse::<Policy>().ok().is_some_and(|a| b.parse::<Policy>().ok() == Some(a));
    let mut changed = 0;
    for id in articles {
        let Some(mut doc) = db.document(id)? else { continue };
        if doc.doc_type != article_type {
            continue;
        }
        let base = doc.base().cloned().ok_or_else(|| AppError("A document without its base.".into()))?;
        let view = view.as_deref().map(|v| own_view(v, id, &doc.doc_type));
        let edit = edit.as_deref().map(|e| own_view(e, id, &doc.doc_type));
        let mut touched = false;
        for layer in &mut doc.layers {
            // A section with access of its own — a secret, say — keeps it.
            if !(same(&layer.access.view, &base.access.view) && same(&layer.access.edit, &base.access.edit)) {
                continue;
            }
            let own = view.clone().unwrap_or_else(|| layer.access.view.clone());
            let rule = edit.clone().unwrap_or_else(|| layer.access.edit.clone());
            if same(&own, &layer.access.view) && same(&rule, &layer.access.edit) {
                continue;
            }
            own.parse::<Policy>().map_err(AppError::from)?;
            rule.parse::<Policy>().map_err(AppError::from)?;
            layer.access.view = own;
            layer.access.edit = rule;
            touched = true;
        }
        if touched {
            doc.changed = store::now();
            doc.changed_by = store.actor()?;
            db.put_document(&doc)?;
            changed += 1;
        }
    }
    Ok(changed)
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
fn type_access_apply(app: tauri::AppHandle, world: String, article_type: String, articles: Vec<String>, view: Option<String>, edit: Option<String>) -> Answer<usize> {
    type_access_apply_in(&store(&app)?, &world, &article_type, &articles, view, edit)
}

#[cfg(feature = "gui")]
/// Deletes a document into the trash — whoever may edit it. The GM brings
/// it back or deletes it for good. `confirmation` has to be its name.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn article_delete(app: tauri::AppHandle, world: String, article: String, confirmation: String) -> Answer<()> {
    article_delete_in(&store(&app)?, world, article, confirmation)
}

/// `article_delete` for whom the store is given.
pub(crate) fn article_delete_in(store: &Store, world: String, article: String, confirmation: String) -> Answer<()> {
    // A page belongs to its owner and goes only with it: the world's with
    // the world, a type's with the type.
    let owned = store.load_world_meta(&world)?.home == article || store.load_templates(&world)?.iter().any(|t| t.page == article);
    if owned {
        return Err(AppError("This page belongs to the world or to a type — it goes only with it.".into()));
    }
    let db = store.db(&world)?;
    let mut doc = db.document(&article)?.filter(|d| d.trashed().is_none()).ok_or_else(|| AppError("There is no such article.".into()))?;
    let name = doc.name().to_string();
    if confirmation.trim() != name.trim() {
        return Err(AppError(format!("To delete, the name has to match exactly — expected “{name}”.")));
    }
    // Whoever may edit it may delete it: into the trash, whole, for the GM
    // to bring back or delete for good.
    let reader = store.reader(&world)?;
    if !reader.is_gm() && !doc.base().is_some_and(|b| reader.allows(&b.access.edit)) {
        return Err(AppError("You may not delete this document.".into()));
    }
    let by = store.actor()?;
    let trashed = store::Trashed { at: store::now(), by: by.clone() };
    let base = doc.layers.iter_mut().find(|l| l.base).ok_or_else(|| AppError("The document has no base.".into()))?;
    base.fields.insert(store::TRASHED.into(), serde_json::to_value(&trashed)?);
    doc.changed = trashed.at;
    doc.changed_by = by;
    db.put_document(&doc)?;
    Ok(())
}

#[cfg(feature = "gui")]
/// Makes a document one of another type — see `article_type_set_in`.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn article_type_set(app: tauri::AppHandle, world: String, article: String, article_type: String, parents: Vec<String>) -> Answer<()> {
    article_type_set_in(&store(&app)?, world, article, article_type, parents)
}

/// Makes a document one of another type — an NPC one of a character's
/// backstory. What it says stays; the properties both types have keep
/// their values. `parents`: the documents it then belongs to, where the
/// type has a parent field — `parent` in the type's edit rule names their
/// players, and the parts that went as the document go with it.
///
/// The GM's — and whoever may edit it and may make documents of that type.
pub(crate) fn article_type_set_in(store: &Store, world: String, article: String, article_type: String, parents: Vec<String>) -> Answer<()> {
    let owned = store.load_world_meta(&world)?.home == article || store.load_templates(&world)?.iter().any(|t| t.page == article);
    if owned {
        return Err(AppError("This page belongs to the world or to a type — it keeps its kind.".into()));
    }
    for id in &parents {
        id.parse::<aw_model::ArticleId>().map_err(AppError::from)?;
    }
    let templates = store.load_templates(&world)?;
    let target = aw_model::types::find(&templates, &article_type).filter(|t| !t.group).ok_or_else(|| AppError("There is no such type.".into()))?;
    let db = store.db(&world)?;
    let mut doc = db.document(&article)?.filter(|d| d.trashed().is_none()).ok_or_else(|| AppError("There is no such article.".into()))?;
    let reader = store.reader(&world)?;
    let may_make = !target.create.trim().is_empty() && reader.allows(&target.create);
    if !reader.is_gm() && !(may_make && doc.base().is_some_and(|b| reader.may_edit(&b.access))) {
        return Err(AppError("You may not make this document one of that type.".into()));
    }
    let whole = aw_model::types::effective(&templates, target);
    let parent = whole.properties.iter().find(|f| f.kind.is_parent());
    let multiple = matches!(parent.map(|f| &f.kind), Some(aw_model::template::FieldKind::Reference { multiple: true, .. }));
    if parents.len() > 1 && !multiple {
        return Err(AppError("This type belongs to one document only.".into()));
    }
    doc.doc_type = article_type.clone();
    if let (Some(field), false) = (parent, parents.is_empty()) {
        let value = if multiple { serde_json::json!(parents) } else { serde_json::json!(parents[0]) };
        // Into the part that holds the properties — a new one as the document goes.
        let base = doc.base().map(|b| b.access.clone()).unwrap_or(store::Access { view: "gm".into(), edit: "gm".into() });
        match doc.layers.iter_mut().find(|l| l.section == document::INFOBOX_SECTION) {
            Some(layer) => {
                layer.fields.insert(field.key.clone(), value);
            }
            None => {
                let mut fields = serde_json::Map::new();
                fields.insert(field.key.clone(), value);
                doc.layers.push(Layer {
                    id: aw_model::LayerId::new().to_string(),
                    base: false,
                    section: document::INFOBOX_SECTION.into(),
                    title: String::new(),
                    access: base,
                    text: String::new(),
                    fields,
                    images: Vec::new(),
                });
            }
        }
        // Its edit rule as the type gives it to one of these parents.
        if let Some(rule) = target.auth.as_ref().filter(|a| a.edit.split_whitespace().any(|w| w.eq_ignore_ascii_case("parent"))) {
            let edit = own_view(&parent_view(&rule.edit, &parents), &doc.id, &article_type);
            edit.parse::<Policy>().map_err(AppError::from)?;
            let was = doc.base().map(|b| b.access.clone());
            if let Some(was) = was {
                for layer in doc.layers.iter_mut().filter(|l| l.base || l.access == was) {
                    layer.access.edit = edit.clone();
                }
            }
        }
    }
    doc.changed = store::now();
    doc.changed_by = store.actor()?;
    db.put_document(&doc)?;
    Ok(())
}

/// A document in the trash, as the GM sees the list.
#[derive(Serialize)]
pub struct TrashedView {
    pub id: String,
    pub name: String,
    pub article_type: String,
    pub at: u64,
    /// Who put it there — by name, where they are a member.
    pub by: String,
}

/// What lies in the trash, newest first — the GM's.
pub(crate) fn trash_list_in(store: &Store, world: &str) -> Answer<Vec<TrashedView>> {
    must_be_gm(store, world, "looks into the trash")?;
    let db = store.db(world)?;
    let members = db.members()?;
    let mut out: Vec<TrashedView> = db
        .trashed_documents()?
        .into_iter()
        .filter_map(|d| {
            let t = d.trashed()?;
            let by = members.iter().find(|m| m.id == t.by).map(|m| m.name.clone()).unwrap_or(t.by);
            Some(TrashedView { id: d.id.clone(), name: d.name().to_string(), article_type: d.doc_type.clone(), at: t.at, by })
        })
        .collect();
    out.sort_by(|a, b| b.at.cmp(&a.at));
    Ok(out)
}

/// Brings a document back out of the trash — the GM's.
pub(crate) fn trash_restore_in(store: &Store, world: &str, article: &str) -> Answer<()> {
    must_be_gm(store, world, "brings documents back")?;
    let db = store.db(world)?;
    let mut doc = db.document(article)?.filter(|d| d.trashed().is_some()).ok_or_else(|| AppError("It is not in the trash.".into()))?;
    if let Some(base) = doc.layers.iter_mut().find(|l| l.base) {
        base.fields.remove(store::TRASHED);
    }
    doc.changed = store::now();
    doc.changed_by = store.actor()?;
    db.put_document(&doc)?;
    Ok(())
}

/// Deletes a document out of the trash for good, with its versions — and
/// the documents it named forget it. The GM's.
pub(crate) fn trash_purge_in(store: &Store, world: &str, article: &str) -> Answer<()> {
    must_be_gm(store, world, "deletes for good")?;
    let db = store.db(world)?;
    db.document(article)?.filter(|d| d.trashed().is_some()).ok_or_else(|| AppError("It is not in the trash.".into()))?;
    document::forget_relations(store, world, article)?;
    db.delete_document(article)?;
    Ok(())
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
fn trash_list(app: tauri::AppHandle, world: String) -> Answer<Vec<TrashedView>> {
    trash_list_in(&store(&app)?, &world)
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
fn trash_restore(app: tauri::AppHandle, world: String, article: String) -> Answer<()> {
    trash_restore_in(&store(&app)?, &world, &article)
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
fn trash_purge(app: tauri::AppHandle, world: String, article: String) -> Answer<()> {
    trash_purge_in(&store(&app)?, &world, &article)
}

#[cfg(feature = "gui")]
/// A part of a world's index — see `views`. As a player always as oneself;
/// as game master with `preview_as` exactly what a player with those roles
/// sees.
// Off the main thread: a big world takes its time.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn view_index(app: tauri::AppHandle, world: String, preview_as: Option<Vec<String>>, after: Option<String>) -> Answer<tauri::ipc::Response> {
    Ok(json_response(views::index_in(&store(&app)?, &world, preview_as, after.as_deref().unwrap_or(""))?))
}

#[cfg(feature = "gui")]
/// Whole documents by id — see `views`.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn view_texts(app: tauri::AppHandle, world: String, preview_as: Option<Vec<String>>, ids: Vec<String>) -> Answer<tauri::ipc::Response> {
    Ok(json_response(views::texts_in(&store(&app)?, &world, preview_as, ids)?))
}

#[cfg(feature = "gui")]
/// What changed after a write — see `views`.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn view_since(app: tauri::AppHandle, world: String, preview_as: Option<Vec<String>>, epoch: u64, since: u64) -> Answer<tauri::ipc::Response> {
    Ok(json_response(views::since_in(&store(&app)?, &world, preview_as, epoch, since)?))
}

#[cfg(feature = "gui")]
/// JSON text handed on as it is — not read and written again.
fn json_response(text: String) -> tauri::ipc::Response {
    tauri::ipc::Response::new(tauri::ipc::InvokeResponseBody::Json(text))
}

/// The whole view for whom the store is given.
#[cfg(test)]
pub(crate) fn view_in(store: &Store, world: String, preview_as: Option<Vec<String>>) -> Answer<Vec<ArticleView>> {
    let reader = reader_in(store, &world, preview_as)?;
    view_with(store, &world, &reader)
}

/// Which of these documents the world holds already — the GM's question, so
/// what is there need not be read again elsewhere (the browser's import).
pub(crate) fn documents_known_in(store: &Store, world: &str, ids: Vec<String>) -> Answer<Vec<String>> {
    if !store.reader(world)?.is_gm() {
        return Err(AppError("Only the GM asks this.".into()));
    }
    let held: std::collections::HashSet<String> = store.db(world)?.document_ids()?.into_iter().collect();
    Ok(ids.into_iter().filter(|id| held.contains(id)).collect())
}

/// Every document `reader` sees, whole.
#[cfg(test)]
pub(crate) fn view_with(store: &Store, world: &str, reader: &Reader) -> Answer<Vec<ArticleView>> {
    views::whole(store, world, reader)
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
    template_read_in(&store(&app)?, world, text, base)
}

/// `template_read` for whom the store is given.
pub(crate) fn template_read_in(store: &Store, world: String, text: String, base: Template) -> Answer<editor::ReadResult> {
    // What the supertypes and groups bring along may be bound as well.
    let templates = store.load_templates(&world)?;
    Ok(editor::read(&text, base, &directory(store, &world)?, &templates))
}

/// Which types exist in this world — id and name.
///
/// The text carries the name of a type, what is stored is its id. Without the
/// directory neither could be translated into the other, nor could it be said
/// that a name does not occur at all.
fn directory(store: &Store, world: &str) -> Result<template_text::Directory, AppError> {
    Ok(template_text::Directory::new(store.load_templates(world)?.into_iter().map(|t| (t.id, t.name))))
}

#[cfg(feature = "gui")]
/// Writes a template into its text form.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn template_write(
    app: tauri::AppHandle,
    world: String,
    template: Template,
) -> Answer<String> {
    template_write_in(&store(&app)?, world, template)
}

/// `template_write` for whom the store is given.
pub(crate) fn template_write_in(store: &Store, world: String, mut template: Template) -> Answer<String> {
    let known = known_to_me(store, &world)?;
    let known = &*known;
    for part in template.blocks.iter_mut().flat_map(|b| b.parts.iter_mut()) {
        part.text = names::written(&part.text, known, &template.id);
    }
    for block in &mut template.blocks {
        block.new = document::types_as(known, &block.new, false);
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
    Ok(template_text::to_text(&template, &directory(store, &world)?))
}

#[cfg(feature = "gui")]
/// Opens AW Server, the dedicated server, beside the app — see `tray`.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn server_launch(app: tauri::AppHandle) -> Answer<()> {
    #[cfg(desktop)]
    return tray::launch(&app);
    #[cfg(mobile)]
    {
        let _ = app;
        Err(AppError("AW Server has its own icon here.".into()))
    }
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
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    { window_state::phone(&window, on, size).map_err(AppError::from) }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    { let _ = (window, on, size); Ok(()) }
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
// Off the main thread: while the store works, the window keeps drawing and taking keys.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn today_in_world(app: tauri::AppHandle, world: String, day: i64) -> Answer<String> {
    today_in_world_in(&store(&app)?, world, day)
}

/// `today_in_world` for whom the store is given.
pub(crate) fn today_in_world_in(store: &Store, world: String, day: i64) -> Answer<String> {
    let meta = store.load_world_meta(&world)?;
    Ok(meta.calendar.format(WorldDate(day)))
}

#[cfg(test)]
#[path = "lib_window_tests.rs"]
mod window_tests;

#[cfg(feature = "gui")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
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
    let builder = builder.plugin(tauri_plugin_dialog::init());
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder
        // Updates from the releases, signed with the publisher's key.
        .plugin(tauri_plugin_updater::Builder::new().build());
    builder
        // Runs after the plugin has restored things: first the remembered size
        // stands, then it is clamped into the usable area.
        .setup(|app| {
            let _ = APP.set(app.handle().clone());
            browser::start(app.handle());
            if let Err(e) = network::start(app.handle()) {
                applog::line("network", &e.0);
            }
            if let Ok(dir) = profile::config_dir(app.handle()) {
                applog::set_fallback(dir);
            }
            #[cfg(target_os = "linux")]
            tray::appimage_entries(app.handle());
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            if let Some(w) = app.get_webview_window("main") {
                window_state::on_start(app.handle().clone(), w);
            }
            Ok(())
        })
        // Only the main window's size is remembered — not the legend's or a
        // document's opened apart.
        .on_window_event(|_w, event| match event {
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            tauri::WindowEvent::CloseRequested { .. } if _w.label() == "main" => window_state::remember(_w.app_handle(), _w, true),
            // While dragging too, throttled: otherwise the size would be lost
            // when the application is not shut down cleanly.
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            tauri::WindowEvent::Resized(_) if _w.label() == "main" => window_state::remember(_w.app_handle(), _w, false),
            _ => {}
        })
        .invoke_handler({
            let commands: fn(tauri::ipc::Invoke) -> bool = tauri::generate_handler![
            document::article_versions,
            document::version_compare,
            document::version_revert,
            document::version_document,
            applog::app_log,
            applog::app_log_open,
            network::network_settings,
            network::network_sync,
            network::network_worlds,
            network::join_requests,
            network::join_answer,
            network::member_invite,
            network::world_connect,
            network::world_disconnect,
            network::world_call,
            mailbox::mailbox_setup,
            mailbox::mailbox_add,
            mailbox::mailbox_reset,
            mailbox::mailbox_open,
            server_launch,
            browser::browser_settings,
            browser::browser_enable,
            network::network_enable,
            network::network_internet,
            network::network_port,
            network::network_servers,
            network::network_share,
            network::network_join,
            network::server_host_worlds,
            network::server_manage,
            network::server_backups,
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
            backup::backup_found,
            backup::backup_receive,
            gm_handover,
            world_supporters_set,
            world_take_over,
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
            view_index,
            view_texts,
            view_since,
            trash_list,
            trash_restore,
            trash_purge,
            visibility_set,
            type_access_apply,
            article_delete,
            article_type_set,
            document::block_look_set,
            document::block_order_set,
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
            document::image_field_set,
            document::references_set,
            live::presence,
            live::world_presence,
            live::dice_roll,
            live::dice_rolls,
            live::dice_log,
            live::dice_log_add,
            live::dice_log_clear,
            live::dice_favorites_set,
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
            world_listed_set,
            app_restart,
            world_dice_set,
            keys::keys_backup,
            keys::keys_restore,
            keys::keys_backup_phone,
            keys::keys_restore_bytes,
            navigation_load,
            navigation_set,
            transfer::package_export,
            transfer::package_read,
            transfer::package_import,
            article_propose,
            document::relations_set,
            ];
            move |invoke| commands(invoke)
        })
        .build(tauri::generate_context!())
        .expect("the Tauri application could not start")
        .run(|_, _| {});
}


#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
