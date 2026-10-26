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
#[cfg(feature = "gui")]
mod window_state;
mod store;
mod profile;
mod media;
mod live;

use aw_crypto::{Identity, IdentityPublic};
use aw_model::{editor, markdown, names, template_text, types, Attribute, Policy, Template, WorldCalendar, WorldDate};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use store::{Access, Document, Layer, Member, Reader, Role, Store, WorldMeta};
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
        level: layer.access.level.clone(),
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
    let base = seen.base()?;
    Some(ArticleView {
        id: seen.id.clone(),
        article_type: seen.doc_type.clone(),
        slug: seen.slug.clone(),
        name: base.title.clone(),
        visibility: shown_rule(&base.access.view),
        edit: shown_rule(&base.access.edit),
        order: document::own_order(&fields_value(&base.fields)),
        layers: seen.layers.iter().filter(|l| !l.base).map(|l| layer_view(l, reader)).collect(),
        layer_total: doc.layers.iter().filter(|l| !l.base).count(),
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
    let waiting = document::waiting_in(store, &id)?.len();
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
            })
            .collect(),
        article_count: db.document_ids()?.len(),
        home: meta.home.clone(),
        requests: if manages { db.requests()?.len() } else { 0 },
        invited: if manages { meta.invited.clone() } else { Vec::new() },
        takeovers: if is_gm { store.setting(&id, "takeovers", Default::default())? } else { Default::default() },
        colours: store.colours(&id)?,
        moderated: store.moderated(&id)?,
        waiting,
        version_limit: store.version_limit(&id)?,
        online: false,
        server: String::new(),
        server_id: String::new(),
        offered: network.shared.contains(&id),
        dice: store.dice(&id)?,
        listed: store.on_the_list(&id)?,
        calendar: meta.calendar.clone(),
        supporters: meta.members.iter().filter(|m| m.role == Role::Supporter).map(|m| m.id.clone()).collect(),
        supporter,
        roles: if manages { roles_of(store, &id)? } else { Vec::new() },
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
/// Renaming does not change the id — access hangs on the key. The worlds
/// this device holds show the new name.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn identity_rename(app: tauri::AppHandle, name: String) -> Answer<IdentityInfo> {
    let store = store(&app)?;
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
fn worlds_list(app: tauri::AppHandle) -> Answer<Vec<WorldInfo>> {
    let store = store(&app)?;
    let mut out = Vec::new();
    for id in store.held_worlds()? {
        let Ok(meta) = store.load_world_meta(&id) else { continue };
        // A world this device holds for others, without being a member of it.
        if meta.member(&store.actor()?).is_none() {
            continue;
        }
        out.push(world_info(&store, &meta)?);
    }
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

/// Servers are reached again once the network is built on the new world format.
fn no_servers_yet() -> AppError {
    AppError("Worlds on a server are not available in this build yet.".into())
}

#[cfg(feature = "gui")]
/// Creates a world at the chosen location — a folder here.
#[cfg_attr(feature = "gui", tauri::command(async))]
async fn world_create(app: tauri::AppHandle, name: String, destination: Destination) -> Answer<WorldInfo> {
    let store = store(&app)?;
    tauri::async_runtime::spawn_blocking(move || match destination {
        Destination::Local { path } => {
            let store = if path.is_empty() { store } else { store.destination(Path::new(&path))? };
            let identity = store.load_identity()?;
            let meta = new_world(&store, &identity, name)?;
            world_info(&store, &meta)
        }
        Destination::Server { .. } => Err(no_servers_yet()),
    })
    .await
    .map_err(AppError::from)?
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
            Destination::Server { .. } => return Err(no_servers_yet()),
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
    must_be_gm(&store, &world, "moves a world")?;
    match destination {
        Destination::Server { .. } => return Err(no_servers_yet()),
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

// --- Members ------------------------------------------------------------

#[cfg(feature = "gui")]
/// Admits a player to the world — from their player card.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn member_add(app: tauri::AppHandle, world: String, path: String, attributes: Vec<String>) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let raw = std::fs::read(&path).map_err(AppError::from)?;
    let card: IdentityPublic = serde_json::from_slice(&raw).map_err(AppError::from)?;
    admit(&store, &world, card, attributes)?;
    world_now(&store, &world)
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

#[cfg(feature = "gui")]
/// One's own colour in a world — on one's name and the documents assigned to one.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn member_colour_set(app: tauri::AppHandle, world: String, colour: String) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let db = store.db(&world)?;
    let me = store.actor()?;
    let mut member = db.member(&me)?.ok_or_else(|| AppError("You are not a member of this world.".into()))?;
    member.colour = colour.chars().take(32).collect();
    db.put_member(&member)?;
    world_now(&store, &world)
}

#[cfg(feature = "gui")]
/// A player's new id takes the place of their old one — for someone whose
/// device was set up anew without their keys. What was assigned to the old
/// id goes to the new one, and the old one leaves. A new id not let in yet
/// takes over as soon as it is.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn member_replace(app: tauri::AppHandle, world: String, old: String, new: String) -> Answer<WorldInfo> {
    let store = store(&app)?;
    replace_member(&store, &world, &old, &new)?;
    world_now(&store, &world)
}

pub(crate) fn replace_member(store: &Store, world: &str, old: &str, new: &str) -> Answer<()> {
    must_be_gm(store, world, "gives a player a new id")?;
    let new = new.trim();
    let valid = new.strip_prefix("id:").is_some_and(|body| body.len() == 16 && body.chars().all(|c| c.is_ascii_hexdigit()));
    if !valid {
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
        let card: IdentityPublic = serde_json::from_value(serde_json::json!({ "id": request.id, "name": request.name, "verify_key": request.verify_key, "seal_key": request.seal_key }))?;
        return admit(store, world, card, Vec::new());
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
    let store = store(&app)?;
    remove_member(&store, &world, &id)?;
    world_now(&store, &world)
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
    let store = store(&app)?;
    must_be_gm(&store, &world, "decides about review")?;
    store.set_setting(&world, "moderated", &on)?;
    world_now(&store, &world)
}

#[cfg(feature = "gui")]
/// Sets how many versions of each document stay — the GM's.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_versions_set(app: tauri::AppHandle, world: String, max: u32) -> Answer<WorldInfo> {
    let store = store(&app)?;
    must_be_gm(&store, &world, "decides how many versions stay")?;
    if max == 0 {
        return Err(AppError("At least one version stays.".into()));
    }
    store.set_setting(&world, "version_limit", &max)?;
    let db = store.db(&world)?;
    for doc in db.document_ids()? {
        db.trim_versions(&doc, max as usize)?;
    }
    world_now(&store, &world)
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
    must_be_gm(&store, &world, "decides who sees the world")?;
    store.set_setting(&world, "listed", &listed)?;
    world_now(&store, &world)
}

#[cfg(feature = "gui")]
/// How far moving the world is — asked while it runs.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_switch_progress(app: tauri::AppHandle, world: String) -> Answer<Option<store::SwitchProgress>> {
    let _ = store(&app)?;
    Ok(store::switch_progress(&world))
}

#[cfg(feature = "gui")]
/// Sets the dice the world rolls with — the GM's. Each between 2 and 1000
/// sides, each once, in the order given.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_dice_set(app: tauri::AppHandle, world: String, sides: Vec<u32>) -> Answer<WorldInfo> {
    let store = store(&app)?;
    must_be_gm(&store, &world, "chooses the world's dice")?;
    let mut kept: Vec<u32> = Vec::new();
    for n in sides.into_iter().filter(|n| (2..=1000).contains(n)) {
        if !kept.contains(&n) {
            kept.push(n);
        }
    }
    store.set_setting(&world, "dice", &kept)?;
    world_now(&store, &world)
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
    store.reader(&world)?;
    Ok(store.navigation(&world)?)
}

#[cfg(feature = "gui")]
/// Sets the side bar's entries — the GM's.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn navigation_set(app: tauri::AppHandle, world: String, entries: Vec<aw_model::navigation::NavEntry>) -> Answer<()> {
    let store = store(&app)?;
    must_be_gm(&store, &world, "sets the side bar")?;
    store.set_navigation(&world, &entries)?;
    Ok(())
}

#[cfg(feature = "gui")]
/// Trusts a member — their writing counts without review — or no longer.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn member_trust_set(app: tauri::AppHandle, world: String, member: String, trusted: bool) -> Answer<WorldInfo> {
    let store = store(&app)?;
    must_be_gm(&store, &world, "decides about review")?;
    let db = store.db(&world)?;
    let mut m = db.member(&member)?.ok_or_else(|| AppError("There is no such member.".into()))?;
    m.trusted = trusted;
    db.put_member(&m)?;
    world_now(&store, &world)
}

#[cfg(feature = "gui")]
/// Makes these players the world's supporters — the GM's. Supporters not
/// named any more are players again.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_supporters_set(app: tauri::AppHandle, world: String, ids: Vec<String>) -> Answer<WorldInfo> {
    let store = store(&app)?;
    must_be_gm(&store, &world, "names supporters")?;
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
    world_now(&store, &world)
}

#[cfg(feature = "gui")]
/// Takes the GM's place, as a supporter. Everyone sees it: the world names
/// the new GM, and the old one is a player of it.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn world_take_over(app: tauri::AppHandle, world: String) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let me = store.actor()?;
    let db = store.db(&world)?;
    if !db.member(&me)?.is_some_and(|m| m.role == Role::Supporter) {
        return Err(AppError("Only a supporter of the world can take the GM's place.".into()));
    }
    hand_world(&store, &world, &me)?;
    world_now(&store, &world)
}

#[cfg(feature = "gui")]
/// Hands the world over to another member: they are its GM from now on, and
/// the one who was is a player of it.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn gm_handover(app: tauri::AppHandle, world: String, member: Option<String>) -> Answer<WorldInfo> {
    let store = store(&app)?;
    must_be_gm(&store, &world, "can hand the world over")?;
    let Some(member) = member else { return world_now(&store, &world) };
    if member == store.actor()? || store.db(&world)?.member(&member)?.is_none() {
        return Err(AppError("The world goes only to another member.".into()));
    }
    hand_world(&store, &world, &member)?;
    world_now(&store, &world)
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
fn world_delete(app: tauri::AppHandle, world: String, confirmation: String) -> Answer<()> {
    let store = store(&app)?;
    must_be_gm(&store, &world, "deletes a world")?;
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
    let store = store(&app)?;
    must_be_gm(&store, &world, "sets the world's icon")?;
    let mut meta = store.load_world_meta(&world)?;
    // A few characters are enough for any emoji including variation selectors.
    meta.icon = icon.chars().take(4).collect();
    store.save_world_meta(&meta)?;
    world_info(&store, &meta)
}

#[cfg(feature = "gui")]
/// Sets how the world counts its days — the game master's. Dates already
/// written stay the same days; they are only written in the new calendar.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn calendar_set(app: tauri::AppHandle, world: String, calendar: WorldCalendar) -> Answer<WorldInfo> {
    let store = store(&app)?;
    must_be_gm(&store, &world, "sets the calendar")?;
    calendar.validate().map_err(AppError::from)?;
    let mut meta = store.load_world_meta(&world)?;
    meta.calendar = calendar;
    store.save_world_meta(&meta)?;
    world_info(&store, &meta)
}

// --- Roles --------------------------------------------------------------

#[cfg(feature = "gui")]
/// A role to give — `class:rogue`, `knowledge:crypt`. The GM's or a supporter's.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn attribute_create(app: tauri::AppHandle, world: String, attribute: String) -> Answer<Vec<String>> {
    let store = store(&app)?;
    if !manages(&store, &world)? {
        return Err(AppError("Only the GM or a supporter makes roles.".into()));
    }
    let parsed: Attribute = attribute.parse().map_err(AppError::from)?;
    let mut made: Vec<String> = store.setting(&world, "roles", Vec::new())?;
    if !made.contains(&parsed.to_string()) {
        made.push(parsed.to_string());
        store.set_setting(&world, "roles", &made)?;
    }
    roles_of(&store, &world)
}

#[cfg(feature = "gui")]
/// Takes a role away from everyone, and out of the roles to give.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn attribute_revoke(app: tauri::AppHandle, world: String, attribute: String) -> Answer<u32> {
    let store = store(&app)?;
    must_be_gm(&store, &world, "takes roles away")?;
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
    store.reader(&world)?;
    Ok(store.load_templates(&world)?)
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
    let known = document::known_from(view_with(store, world, &store.reader(world)?)?, with_this);
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
    let store = store(&app)?;
    must_be_gm(&store, &world, "makes documents with access of their own")?;
    let layers = names_stored(&store, &world, &article_type, layers)?;
    create_document(&store, &world, article_type, slug, name, visibility, layers)
}

/// Names as written in new texts, ids as stored — the same as when a
/// document is saved.
fn names_stored(store: &Store, world: &str, article_type: &str, mut layers: Vec<NewLayer>) -> Answer<Vec<NewLayer>> {
    let known = known_to_me(store, world)?;
    for layer in &mut layers {
        layer.text = names::stored(&layer.text, &known, article_type);
        if let Some(map) = layer.fields.as_object_mut() {
            for value in map.values_mut() {
                if let serde_json::Value::String(s) = value {
                    *s = names::stored(s, &known, article_type);
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
    let name: String = name.trim().chars().take(200).collect();
    let slug = if slug.trim().is_empty() { name.to_lowercase() } else { slug };
    create_document(store, world, own.id.clone(), slug, name, auth.view.clone(), layers)
}

/// What the one a command runs for knows by name in this world.
fn known_to_me(store: &Store, world: &str) -> Answer<document::Known> {
    let reader = store.reader(world)?;
    let views = view_with(store, world, &reader)?;
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

/// `texts_written` for whom the store is given.
pub(crate) fn texts_written_in(store: &Store, world: String, article_type: String, texts: Vec<String>) -> Answer<Vec<String>> {
    let known = known_to_me(store, &world)?;
    Ok(texts.iter().map(|t| names::written(t, &known, &article_type)).collect())
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
    for layer in &mut layers {
        layer.policy = own_view(&layer.policy, &id, &article_type);
        layer.edit = own_view(&layer.edit, &id, &article_type);
    }
    // Nested sections inherit their parents' visibility.
    let effective = effective_policies(&layers)?;
    let view: Policy = visibility.parse().map_err(AppError::from)?;
    let by = store.actor()?;
    let at = store::now();
    let new_id = || aw_model::LayerId::new().to_string();
    // The base layer carries the article's name and visibility.
    let mut all = vec![Layer {
        id: new_id(),
        base: true,
        section: String::new(),
        title: name,
        access: Access { view: visibility, edit: "gm".into(), level: if view.to_string() == "gm" { "gm-only".into() } else { "open".into() } },
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
            access: Access { view: view.to_string(), edit: if input.edit.is_empty() { "gm".into() } else { input.edit }, level: input.level },
            text: input.text,
            fields: fields_map(&input.fields),
            images,
        });
    }
    let doc = Document { id: id.clone(), doc_type: article_type, slug, layers: all, created: at, created_by: by.clone(), changed: at, changed_by: by, proposed: false };
    store.db(world)?.put_document(&doc)?;
    Ok(id)
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
/// Deletes an article — for good. The game master's: a document consists of
/// sections that different people may see, and whoever sees one of them
/// should not throw the others away with it.
///
/// `confirmation` has to be the article's name. Deleted is deleted — there
/// is no trash bin.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn article_delete(app: tauri::AppHandle, world: String, article: String, confirmation: String) -> Answer<()> {
    let store = store(&app)?;
    must_be_gm(&store, &world, "deletes documents")?;
    // A page belongs to its owner and goes only with it: the world's with
    // the world, a type's with the type.
    let owned = store.load_world_meta(&world)?.home == article || store.load_templates(&world)?.iter().any(|t| t.page == article);
    if owned {
        return Err(AppError("This page belongs to the world or to a type — it goes only with it.".into()));
    }
    let db = store.db(&world)?;
    let doc = db.document(&article)?.ok_or_else(|| AppError("There is no such article.".into()))?;
    let name = doc.name().to_string();
    if confirmation.trim() != name.trim() {
        return Err(AppError(format!("To delete, the name has to match exactly — expected “{name}”.")));
    }
    document::forget_relations(&store, &world, &article)?;
    db.delete_document(&article)?;
    Ok(())
}

#[cfg(feature = "gui")]
/// The view of a world.
///
/// As a player always as oneself. As game master without `preview_as` the
/// full view, with it exactly what a player with those roles sees — `all`
/// and `players` besides.
// Off the main thread: a big world takes its time.
#[cfg_attr(feature = "gui", tauri::command(async))]
fn view(app: tauri::AppHandle, world: String, preview_as: Option<Vec<String>>) -> Answer<Vec<ArticleView>> {
    view_in(&store(&app)?, world, preview_as)
}

/// `view` for whom the store is given.
pub(crate) fn view_in(store: &Store, world: String, preview_as: Option<Vec<String>>) -> Answer<Vec<ArticleView>> {
    let reader = reader_in(store, &world, preview_as)?;
    view_with(store, &world, &reader)
}

/// Every document `reader` sees, as the interface gets it.
pub(crate) fn view_with(store: &Store, world: &str, reader: &Reader) -> Answer<Vec<ArticleView>> {
    let db = store.db(world)?;
    // What one wrote that waits for the game master stands there for oneself.
    let own_waiting = !reader.is_gm() && store.moderated(world)?;
    let mut out = Vec::new();
    for doc in db.documents()? {
        let doc = if own_waiting { document::with_own_waiting(store, world, doc)? } else { doc };
        if let Some(view) = article_view(&doc, reader) {
            out.push(view);
        }
    }
    Ok(out)
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
            if let Ok(dir) = profile::config_dir(app.handle()) {
                applog::set_fallback(dir);
            }
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
            document::article_versions,
            document::version_compare,
            document::version_revert,
            document::version_document,
            applog::app_log,
            applog::app_log_open,
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
            world_switch_progress,
            navigation_load,
            navigation_set,
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

    fn reader(attributes: &[&str]) -> Reader {
        Reader::holding(&attributes.iter().map(|a| a.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn editing_is_separate_from_seeing() {
        // Whoever may read may not therefore write.
        let player = reader(&["players"]);
        assert!(!player.allows("gm"));
        assert!(player.allows("players"));
    }

    #[test]
    fn edit_can_hold_for_individuals() {
        let bob = reader(&["player:bob"]);
        let anna = reader(&["player:anna"]);
        assert!(bob.allows("player:bob"));
        assert!(!anna.allows("player:bob"));
    }

    #[test]
    fn without_a_rule_the_gm_decides() {
        // Content without a rule — then only the game master may touch it.
        assert!(Reader::gm().allows(""));
        assert!(!reader(&["players"]).allows(""));
    }

    #[test]
    fn edit_understands_and_terms() {
        assert!(reader(&["class:rogue"]).allows("players AND class:rogue"));
        assert!(!reader(&[]).allows("players AND class:rogue"));
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
        assert_eq!(store.load_world_meta(meta.id.as_str()).unwrap().home, meta.home);

        let id = meta.id.clone();
        let view = view_in(&store, id.clone(), None).unwrap();
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
        let id = meta.id.clone();
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
        let all = save_template(&store, &id, kind("Characters")).unwrap();
        save_template(&store, &id, kind("Places")).unwrap();
        let characters = all[0].clone();
        let tim = create_document(&store, &id, characters.id.clone(), "tim".into(), "Tim".into(), "all".into(), vec![]).unwrap();
        let home = document::code_of(&store, &id, &meta.home).unwrap();
        let with_link = home.code.replace("<block>\n", "<block>\nMeet @@Characters:Tim@@.\n");
        document::save_now(&store, &id, &meta.home, &with_link).unwrap();
        let stored = view_in(&store, id.clone(), None).unwrap();
        let home_text = &stored.iter().find(|a| a.id == meta.home).unwrap().layers[0].text;
        assert_eq!(home_text, &format!("Meet @@{tim}@@."), "a link is stored by id");

        // Renamed as a type: the page follows, and text shows the new name —
        // nothing had to be rewritten for it.
        save_template(&store, &id, Template { name: "Heroes".into(), ..characters.clone() }).unwrap();
        let page = document::code_of(&store, &id, &characters.page).unwrap();
        assert_eq!(page.document.name, "Heroes");
        assert!(page.code.contains("<list type=Heroes columns=2>"), "{}", page.code);
        let home = document::code_of(&store, &id, &meta.home).unwrap();
        assert!(home.code.contains("@@Heroes:Tim@@"), "{}", home.code);

        // A renamed document too.
        let tim_code = document::code_of(&store, &id, &tim).unwrap().code;
        document::save_now(&store, &id, &tim, &tim_code.replace("<name=Tim>", "<name=Timothy>")).unwrap();
        let home = document::code_of(&store, &id, &meta.home).unwrap();
        assert!(home.code.contains("@@Heroes:Timothy@@"), "{}", home.code);

        // Renamed as a page: the type follows.
        let renamed = page.code.replace("<name=Heroes>", "<name=Villains>");
        document::save_now(&store, &id, &characters.page, &renamed).unwrap();
        let types = store.load_templates(&id).unwrap();
        assert!(types.iter().any(|t| t.id == characters.id && t.name == "Villains"));

        // A name is given once.
        let clash = renamed.replace("<name=Villains>", "<name=places>");
        let err = document::save_now(&store, &id, &characters.page, &clash).unwrap_err();
        assert!(err.0.contains("already exists"), "{}", err.0);

        // The world's page names the world.
        let world_page = document::code_of(&store, &id, &meta.home).unwrap();
        let code = world_page.code.replace("<name=Eldermere>", "<name=\"The Mist Lands\">");
        document::save_now(&store, &id, &meta.home, &code).unwrap();
        assert_eq!(store.load_world_meta(&id).unwrap().name, "The Mist Lands");
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
        let id = meta.id.clone();

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
        let saved = save_template(&store, &id, character).unwrap();
        let page = saved[0].page.clone();
        assert!(!page.is_empty(), "the type came with its page");

        let view = view_in(&store, id.clone(), None).unwrap();
        let text = &view.iter().find(|a| a.id == page).unwrap().layers[0].text;
        assert_eq!(text, &format!("<list type={} columns=2>", saved[0].id), "stored by id");
        let code = document::code_of(&store, &id, &page).unwrap().code;
        assert!(code.contains("<list type=Characters columns=2>"), "written by name: {code}");

        // Saving again keeps the page — the editor's copy does not know it.
        let again = save_template(&store, &id, Template { page: String::new(), ..saved[0].clone() }).unwrap();
        assert_eq!(again[0].page, page);
        assert_eq!(store.db(&id).unwrap().document_ids().unwrap().len(), 2, "home and the one page");

        delete_template(&store, &id, &saved[0].id).unwrap();
        assert!(!store.db(&id).unwrap().document_ids().unwrap().contains(&page), "the page went with it");

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
        let id = meta.id.clone();
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
        let all = save_template(&store, &id, abilities).unwrap();
        let group = all.iter().find(|t| t.name == "Abilities").unwrap().clone();
        assert!(group.page.is_empty(), "a group has no page");

        let stats = aw_model::template::TemplateField::new("stats", "Stats", aw_model::FieldKind::Group { group: group.id.clone() });
        let all = save_template(&store, &id, Template { properties: vec![stats], ..blank("Characters") }).unwrap();
        let characters = all.iter().find(|t| t.name == "Characters").unwrap().clone();

        let layer: NewLayer = serde_json::from_value(serde_json::json!({
            "title": "", "text": "", "policy": "all", "level": "open", "section": "properties", "edit": "gm", "fields": { "str": 15 }
        }))
        .unwrap();
        let tim = create_document(&store, &id, characters.id.clone(), String::new(), "Tim".into(), "all".into(), vec![layer]).unwrap();
        let code = document::code_of(&store, &id, &tim).unwrap().code;
        assert!(code.starts_with("---\nStats:\n  Strength: 15\n---"), "{code}");

        // The group grows — and the character with it.
        let grown = Template { properties: vec![number("str", "Strength"), number("dex", "Dexterity")], ..group.clone() };
        save_template(&store, &id, grown).unwrap();
        let code = document::code_of(&store, &id, &tim).unwrap().code;
        assert!(code.contains("  Dexterity:\n"), "{code}");
        document::save_now(&store, &id, &tim, &code.replace("  Dexterity:", "  Dexterity: 12")).unwrap();
        let code = document::code_of(&store, &id, &tim).unwrap().code;
        assert!(code.contains("  Strength: 15\n  Dexterity: 12\n"), "{code}");

        // A layout of its own: stored by keys and ids, so renaming changes
        // nothing stored — and a document shows it with `<view of=…>`.
        let mut layout = aw_model::template::TemplateBlock::fresh("layout", "");
        layout.parts[0].text = "**Strength** %%Strength%%".into();
        let all = save_template(&store, &id, Template { blocks: vec![layout], ..store.load_templates(&id).unwrap().into_iter().find(|t| t.id == group.id).unwrap() }).unwrap();
        let stored = all.iter().find(|t| t.id == group.id).unwrap();
        assert_eq!(stored.blocks[0].parts[0].text, "**Strength** %%str%%");
        let code = document::code_of(&store, &id, &tim).unwrap().code;
        let with_view = code.replace("<body>\n", "<body>\n<block title=Sheet>\n<view of=Abilities>\n</block>\n");
        document::save_now(&store, &id, &tim, &with_view).unwrap();
        let view = view_in(&store, id.clone(), None).unwrap();
        let texts: Vec<&str> = view.iter().find(|a| a.id == tim).unwrap().layers.iter().map(|l| l.text.as_str()).collect();
        assert!(texts.iter().any(|t| t.contains(&format!("<view of={}>", group.id))), "{texts:?}");
        let code = document::code_of(&store, &id, &tim).unwrap().code;
        assert!(code.contains("<view of=Abilities>"), "{code}");

        // A number of its own per pick: the levels in each class.
        let all = save_template(&store, &id, blank("Classes")).unwrap();
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
        save_template(&store, &id, hero).unwrap();
        let code = document::code_of(&store, &id, &tim).unwrap().code;
        let code = code.replace("Classes:", "Classes: @@Classes:Fighter@@ 3, @@Classes:Rogue@@");
        document::save_now(&store, &id, &tim, &code).unwrap();
        let back = document::code_of(&store, &id, &tim).unwrap().code;
        assert!(back.contains("Classes: @@Classes:Fighter@@ 3, @@Classes:Rogue@@\n"), "{back}");
        let view = view_in(&store, id.clone(), None).unwrap();
        let stored = view.iter().find(|a| a.id == tim).unwrap().layers.iter().find_map(|l| l.fields.get("classes").cloned()).unwrap();
        assert_eq!(stored[0], serde_json::json!({ "with": fighter, "count": 3 }));
        let bad = back.replace("Fighter@@ 3", "Fighter@@ three");
        assert!(document::save_now(&store, &id, &tim, &bad).unwrap_err().0.contains("no number"));

        // Every class, always: only numbers other than 0 are kept.
        let mut hero = store.load_templates(&id).unwrap().into_iter().find(|t| t.id == characters.id).unwrap();
        for f in &mut hero.properties {
            if let aw_model::FieldKind::Reference { every, .. } = &mut f.kind {
                if f.key == "classes" {
                    *every = true;
                }
            }
        }
        save_template(&store, &id, hero).unwrap();
        let code = back.replace("Classes: @@Classes:Fighter@@ 3, @@Classes:Rogue@@", "Classes: @@Classes:Fighter@@ 3, @@Classes:Rogue@@ 0");
        document::save_now(&store, &id, &tim, &code).unwrap();
        let back = document::code_of(&store, &id, &tim).unwrap().code;
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
