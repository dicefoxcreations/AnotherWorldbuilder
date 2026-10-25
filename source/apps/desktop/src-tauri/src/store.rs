//! Storage on disk.
//!
//! ```text
//! <profile>/identity.json                   identity and private keys
//! <profile>/network.json                    network settings
//! <profile>/worlds/<world-id>/state.json     local members and invitations
//! <profile>/worlds/<world-id>/owner.key      world token (GM)
//! <profile>/worlds/<world-id>/keyring.json    attribute keys per epoch (GM)
//! <profile>/worlds/<world-id>/requests.json   join requests (GM)
//! <profile>/worlds/<world-id>/handover.json   pending handover recipient
//! <profile>/worlds/<world-id>/drafts/        what open editing sessions wrote
//! <worlds>/<world-id>/world.json             public descriptor, layout v3
//! <worlds>/<world-id>/control/               the world's structure, signed
//! <worlds>/<world-id>/articles/<article>/    one file per version of a document
//! <profile>/online/<world-id>/               a world held only while online
//! <profile>/fleeting.json                    which worlds those are, to fetch again
//! ```
//!
//! A world whose players keep no copy, held through a server: it lies in
//! `online/` while the app runs and goes when it starts again. That this
//! device is a member stays — it is fetched again once online.
//!
//! Everything the members share lies in the journal (see
//! `aw_store::journal`): the world's name and calendar, the types, the
//! articles. Nothing is ever overwritten there — a change is a new record.
//! Layers lie there only as ciphertext; ids, policies and levels are the
//! known metadata remainder.

use aw_crypto::{decrypt, encrypt, seal_key, unseal_key, ContentKey, Identity, IdentityPublic, WorldPublic, WorldToken, WrappedKey};
use aw_model::navigation::NavEntry;
use aw_model::{ArticleId, LayerId, Level, Policy, Template, WorldCalendar};
use aw_policy::{HeldKeys, Keyring, LayerHeader};
use aw_store::journal::{self, diff::Range, About, Access, GrantContent, Item, Journal, Plain, Review, Sealed};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

pub const STORE_VERSION: u8 = 1;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("file not readable or writable: {0}")]
    Io(#[from] std::io::Error),

    #[error("file is malformed: {0}")]
    Format(#[from] serde_json::Error),

    #[error(transparent)]
    Crypto(#[from] aw_crypto::CryptoError),

    #[error(transparent)]
    Policy(#[from] aw_policy::PolicyKeyError),

    #[error("world {0} does not exist here")]
    NoSuchWorld(String),

    #[error("unknown storage version {found}, this application knows up to {known}")]
    UnknownVersion { found: u8, known: u8 },

    #[error("{0}")]
    Journal(String),

    #[error("there are no keys for this world")]
    NoKeys,
}

impl From<Box<dyn std::error::Error + Send + Sync>> for StoreError {
    fn from(e: Box<dyn std::error::Error + Send + Sync>) -> Self {
        StoreError::Journal(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// A layer as it stands, encrypted with its data key.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredLayer {
    pub id: LayerId,
    pub header: LayerHeader,
    pub level: Level,
    /// hex-encoded ciphertext of the `LayerContent` — empty where this device
    /// cannot open the layer.
    pub sealed: String,
    /// The base layer carries the article's name and decides whether the
    /// article appears at all. Whoever cannot open it does not see the
    /// article — not even if one of its sections would open.
    pub is_base: bool,
    /// Kept as it is: the world is unencrypted — anyone reads and edits it.
    #[serde(default)]
    pub plain: bool,
}

impl StoredLayer {
    /// The layer's content, if these keys open it — an unencrypted one opens for anyone.
    pub fn content(&self, held: &HeldKeys) -> Option<LayerContent> {
        let key = if self.plain { journal::DataKey::Plain } else { journal::DataKey::Sealed(aw_policy::open_dek(&self.header, held).ok()??) };
        let content: LayerContent = serde_json::from_slice(&key.open(&self.sealed).ok()?).ok()?;
        // Unencrypted, a host acting for a player still keeps to the views.
        (!self.plain || crate::host::may_see(&content.own)).then_some(content)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredArticle {
    pub id: ArticleId,
    pub article_type: String,
    pub slug: String,
    pub layers: Vec<StoredLayer>,
}

/// The plaintext of a layer — only after successful decryption.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayerContent {
    pub title: String,
    pub text: String,
    /// Key of the section in the template.
    ///
    /// With it the display finds the section again unambiguously. The title
    /// would not do: sections may be nameless or share a name.
    pub section: String,
    /// Who may change this section. Stands in the encrypted part — whoever
    /// cannot open the section need not know who may edit it either.
    pub edit: String,
    /// The view as it was set — the policy as written. The header does not
    /// carry it: it goes to everyone.
    pub own: String,
    /// The template fields of this section, key → value.
    #[serde(default)]
    pub fields: serde_json::Value,
    /// The images the section shows, by id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
}

impl LayerContent {
    fn of(access: Access, plain: Plain) -> Self {
        LayerContent {
            title: plain.title,
            text: plain.text,
            section: access.section,
            edit: access.edit,
            own: access.own,
            images: plain.images.keys().cloned().collect(),
            fields: fields_value(plain.fields),
        }
    }
}

/// Fields as the views know them: none at all is `null`.
pub fn fields_value(fields: BTreeMap<String, serde_json::Value>) -> serde_json::Value {
    if fields.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::Object(fields.into_iter().collect())
    }
}

/// Fields as a layer holds them.
pub fn fields_map(value: &serde_json::Value) -> BTreeMap<String, serde_json::Value> {
    value.as_object().map(|m| m.clone().into_iter().collect()).unwrap_or_default()
}

/// Which role someone has in a world.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    /// Holds the world token and with it the full keyring.
    Gm,
    #[default]
    Player,
}

/// A member of the world.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Member {
    pub identity: IdentityPublic,
    pub role: Role,
    /// Attributes in text form, e.g. `class:rogue`.
    pub attributes: Vec<String>,
}

/// Default icon of a world.
pub const WORLD_ICON: &str = "🌍";

/// A world as the app works with it — what the journal says, together with
/// what only this device knows.
#[derive(Clone, Debug)]
pub struct WorldMeta {
    pub public: WorldPublic,
    pub calendar: WorldCalendar,
    /// The world's emoji. Freely chosen, the default is the globe.
    pub icon: String,
    /// Id of whoever created the world.
    pub owner: String,
    /// The world's home page — a document like any other.
    pub home: String,
    /// As game master: every member. As player: oneself.
    pub members: Vec<Member>,
    /// Ids the game master let in ahead of time. Whoever proves one of them
    /// on the network becomes a member without asking.
    pub invited: Vec<String>,
}

/// What the profile keeps in `state.json`: private world state.
#[derive(Serialize, Deserialize)]
struct Local {
    version: u8,
    public: WorldPublic,
    #[serde(default)]
    owner: String,
    #[serde(default)]
    members: Vec<Member>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    invited: Vec<String>,
}

/// Someone who asked to join a world — waiting for the game master.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JoinRequest {
    pub identity: IdentityPublic,
    /// Seconds since 1970, when they asked.
    pub at: u64,
}

/// What a game master hands over along with the world.
#[derive(Serialize, Deserialize)]
struct Handover {
    /// The world token, as it lies on disk.
    token: String,
    keyring: String,
    members: Vec<Member>,
    invited: Vec<String>,
    requests: Vec<JoinRequest>,
    records: Vec<String>,
}

/// A world handed over, sealed for one recipient — over the network, as a
/// file, or through anything else that carries it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HandoverPackage {
    pub world: String,
    /// The id of the one who takes it over.
    pub to: String,
    key: WrappedKey,
    data: String,
}

/// What this device does on the local network. Everything is off until
/// switched on — the network as a whole, and then each world by itself.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkSettings {
    #[serde(default)]
    pub enabled: bool,
    /// The worlds offered on the network, by id.
    #[serde(default)]
    pub shared: Vec<String>,
    /// Reachable from the internet: the router's port, STUN, IPv6 — so
    /// players reach this device as their host.
    #[serde(default)]
    pub internet: bool,
    /// Peer to peer through shared folders: devices of a world in a folder a
    /// cloud app keeps in step meet there and connect directly.
    #[serde(default)]
    pub folders: bool,
    /// The port to answer on — for a router's forwarding. Taken by another
    /// program, the network answers on any free one.
    #[serde(default = "default_port")]
    pub port: u16,
    /// Whom to ask how this device looks from outside — public STUN
    /// servers; they see the address, nothing else. Empty: only the router.
    #[serde(default = "stun_servers")]
    pub stun: Vec<String>,
    /// Servers that keep worlds for the group (`aw-server`), by their code.
    #[serde(default)]
    pub servers: Vec<String>,
    /// Where a world lives, by world: its server, by the server's endpoint
    /// id — which stays when its addresses change. The GM's goes to that
    /// server alone; a member's came from it.
    #[serde(default)]
    pub homes: BTreeMap<String, String>,
    /// Explicitly requested initial uploads. An older GM working copy is
    /// never interpreted as a new upload merely because it has a home.
    #[serde(default)]
    pub uploading: Vec<String>,
}

impl NetworkSettings {
    /// The code of the server a world lives on — none: this device, or a
    /// server no longer in the list.
    pub fn home_of(&self, world: &str) -> Option<&String> {
        let endpoint = self.homes.get(world)?;
        self.servers.iter().find(|code| code.split('@').next() == Some(endpoint.as_str()))
    }
}

impl Default for NetworkSettings {
    fn default() -> Self {
        NetworkSettings {
            enabled: false,
            shared: Vec::new(),
            internet: false,
            folders: false,
            port: default_port(),
            stun: stun_servers(),
            servers: Vec::new(),
            homes: BTreeMap::new(),
            uploading: Vec::new(),
        }
    }
}

fn default_port() -> u16 {
    aw_sync::world::DEFAULT_PORT
}

fn stun_servers() -> Vec<String> {
    // Cloudflare offers its STUN service for free use.
    vec!["stun.cloudflare.com:3478".into()]
}

impl WorldMeta {
    pub fn member(&self, id: &str) -> Option<&Member> {
        self.members.iter().find(|m| m.identity.id.as_str() == id)
    }

    /// The role of an id. Whoever is not listed is a player.
    pub fn role_of(&self, id: &str) -> Role {
        self.member(id).map(|m| m.role).unwrap_or_default()
    }
}

/// A new part of an article: where it goes, who sees and edits it, what it
/// holds.
pub struct NewPart {
    pub is_base: bool,
    pub level: Level,
    pub view: Policy,
    pub access: Access,
    pub plain: Plain,
}

#[derive(Serialize, Deserialize)]
struct AvatarFile {
    media_type: String,
    data: String,
}

impl AvatarFile {
    fn as_data_url(&self) -> String {
        // The frontend gets a data: URL. So the image lies nowhere on the
        // web but travels along with the answer.
        let raw = hex::decode(&self.data).unwrap_or_default();
        format!("data:{};base64,{}", self.media_type, base64_encode(&raw))
    }
}

/// A small Base64 encoder without an extra dependency.
pub fn base64(bytes: &[u8]) -> String {
    base64_encode(bytes)
}

fn base64_encode(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for block in bytes.chunks(3) {
        let b = [block[0], *block.get(1).unwrap_or(&0), *block.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(ABC[(n >> 18) as usize & 63] as char);
        out.push(ABC[(n >> 12) as usize & 63] as char);
        out.push(if block.len() > 1 { ABC[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if block.len() > 2 { ABC[n as usize & 63] as char } else { '=' });
    }
    out
}

/// A filesystem-safe name from an id: `world:c19d…` → `world-c19d…`.
pub fn dir_name(world_id: &str) -> String {
    world_id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' }).collect()
}

/// Milliseconds since 1970 — the journal's clock.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Where the articles as last built are kept between starts — encrypted as
/// they are in the world; only this device's own work saved.
const PROJECTION: &str = "projection.json";
/// How far switching a world's encryption is — for the interface to show.
#[derive(Clone, Default, Serialize)]
pub struct SwitchProgress {
    pub step: u32,
    pub steps: u32,
    pub what: String,
    /// Of `total` in this step; no total: no count to give.
    pub done: usize,
    pub total: usize,
}

const SWITCH_STEPS: u32 = 6;

fn switch_progress_all() -> &'static Mutex<HashMap<String, SwitchProgress>> {
    static ALL: OnceLock<Mutex<HashMap<String, SwitchProgress>>> = OnceLock::new();
    ALL.get_or_init(Default::default)
}

pub(crate) fn progress(world: &str, step: u32, what: &str, done: usize, total: usize) {
    if let Ok(mut all) = switch_progress_all().lock() {
        all.insert(world.to_string(), SwitchProgress { step, steps: SWITCH_STEPS, what: what.into(), done, total });
    }
}

/// Where switching a world stands — `None` while it is not being switched.
pub fn switch_progress(world: &str) -> Option<SwitchProgress> {
    switch_progress_all().lock().ok()?.get(world).cloned()
}

thread_local! {
    /// Set by `quietly`: the journal is not looked at anew on every access.
    static QUIET: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Runs `f` without looking at the disk anew on every journal access — for
/// work over many documents at once, which would otherwise look at every
/// document's folder for each of them. What is on disk is looked at once
/// before.
pub fn quietly<T>(f: impl FnOnce() -> T) -> T {
    let before = QUIET.with(|q| q.replace(true));
    let out = f();
    QUIET.with(|q| q.set(before));
    out
}

/// Lies in a world's private folder while its encryption is being switched.
const SWITCHING: &str = "switching";

#[derive(Serialize, Deserialize)]
struct Projection {
    version: u32,
    articles: HashMap<String, (String, StoredArticle)>,
}

/// A journal kept open between commands, with the articles as last read.
struct Open {
    journal: Journal,
    projection: Option<(u64, Vec<StoredArticle>)>,
}

/// For each attribute no key is held for — in an unencrypted world, where
/// nothing is decrypted — a stand-in, so that what asks for attributes sees them.
pub(crate) fn stand_ins(held: &mut HeldKeys, attributes: impl IntoIterator<Item = aw_model::Attribute>) {
    let have = held.attributes();
    for attribute in attributes.into_iter().filter(|a| !have.contains(a)) {
        held.insert(aw_policy::AttributeAt::new(attribute, 0), [0; 32]);
    }
}

/// The open journals of this process, by profile and world folder. Commands come and go; the
/// world read from its records stays.
/// Worlds on their way to another location, or out — the network and what
/// writes in the background leave them alone meanwhile.
fn moving_worlds() -> &'static Mutex<std::collections::HashSet<String>> {
    static MOVING: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    MOVING.get_or_init(Default::default)
}

/// Whether a world is being moved or deleted right now.
pub fn moving(world_id: &str) -> bool {
    moving_worlds().lock().is_ok_and(|all| all.contains(world_id))
}

/// A world noted as moving while this lives.
struct Away(String);

impl Away {
    fn start(world_id: &str) -> Result<Self> {
        moving_worlds().lock().map_err(poisoned)?.insert(world_id.to_string());
        Ok(Away(world_id.to_string()))
    }
}

impl Drop for Away {
    fn drop(&mut self) {
        if let Ok(mut all) = moving_worlds().lock() {
            all.remove(&self.0);
        }
    }
}

/// A folder gone with all below it — tried again while something still
/// writes there; one already gone is fine.
fn remove_tree(dir: &Path) -> std::io::Result<()> {
    for _ in 0..10 {
        if !dir.exists() {
            return Ok(());
        }
        if std::fs::remove_dir_all(dir).is_ok() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    match std::fs::remove_dir_all(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Copies a folder with all below it, each file on disk before it counts.
pub(crate) fn copy_dir(from: &Path, to: &Path, tell: &dyn Fn(usize, usize)) -> std::io::Result<()> {
    let mut files = Vec::new();
    let mut dirs = vec![from.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() { dirs.push(path) } else { files.push(path) }
        }
    }
    for (done, file) in files.iter().enumerate() {
        let target = to.join(file.strip_prefix(from).map_err(std::io::Error::other)?);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(file, &target)?;
        std::fs::File::open(&target)?.sync_all()?;
        tell(done + 1, files.len());
    }
    Ok(())
}

/// One opening at a time per world — see `Store::journal`.
fn opening() -> &'static Mutex<HashMap<(PathBuf, PathBuf), Arc<Mutex<()>>>> {
    static OPENING: OnceLock<Mutex<HashMap<(PathBuf, PathBuf), Arc<Mutex<()>>>>> = OnceLock::new();
    OPENING.get_or_init(Default::default)
}

/// Whether this process alone writes the folders it keeps worlds in — a
/// server's do not lie in a folder others share.
static ALONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) fn keep_worlds_alone() {
    ALONE.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// How long nobody writes before the game master's device writes the next
/// state of a world in a shared folder.
const STATE_AFTER: std::time::Duration = std::time::Duration::from_secs(60);

fn open_journals() -> &'static Mutex<HashMap<(PathBuf, PathBuf), Arc<Mutex<Open>>>> {
    static OPEN: OnceLock<Mutex<HashMap<(PathBuf, PathBuf), Arc<Mutex<Open>>>>> = OnceLock::new();
    OPEN.get_or_init(Default::default)
}

/// Editing sessions open in this process, by profile, world and session —
/// with when they last wrote.
type Active = HashMap<(PathBuf, String, String), u64>;

fn active_sessions() -> &'static Mutex<Active> {
    static ACTIVE: OnceLock<Mutex<Active>> = OnceLock::new();
    ACTIVE.get_or_init(Default::default)
}

/// A session quiet this long ends by itself (milliseconds).
const IDLE_MS: u64 = 10 * 60 * 1000;

fn poisoned<T>(_: T) -> StoreError {
    StoreError::Journal("a lock was poisoned".into())
}

/// Where the data lies: the profile — identity, avatar, settings — and the
/// worlds. Two folders, which may lie far apart.
#[derive(Clone, Debug)]
pub struct Store {
    profile: PathBuf,
    worlds: PathBuf,
    roots: Vec<PathBuf>,
    /// A disposable online session must never create edits while receiving
    /// or reading a server revision. Edits only happen in an explicit command.
    online_cache: bool,
    online_access: bool,
}

impl Store {
    /// Both in one folder: `profile` and `worlds` below `base`.
    #[cfg(test)]
    pub fn new(base: PathBuf) -> Self {
        Store::at(base.join("profile"), base.join("worlds"))
    }

    #[cfg(test)]
    pub fn at(profile: PathBuf, worlds: PathBuf) -> Self {
        Self::with_roots(profile, vec![worlds])
    }

    pub fn with_roots(profile: PathBuf, roots: Vec<PathBuf>) -> Self {
        let worlds = roots[0].clone();
        Store { profile, worlds, roots, online_cache: false, online_access: false }
    }

    pub(crate) fn online_cache(profile: PathBuf, worlds: PathBuf) -> Self {
        let mut store = Self::with_roots(profile, vec![worlds]);
        store.online_cache = true;
        store.online_access = true;
        store
    }

    /// Internal transport/catalog access. Interactive world commands never
    /// receive this store; they use a server-confirmed disposable session.
    pub(crate) fn for_sync(&self) -> Self {
        let mut store = self.clone();
        store.online_access = true;
        store
    }

    fn require_online_access(&self, world: &str) -> Result<()> {
        if !self.online_access && crate::online::requires_online(self, world) {
            return Err(StoreError::Journal(crate::online::UNAVAILABLE.into()));
        }
        Ok(())
    }

    pub fn local_roots(&self) -> &[PathBuf] { &self.roots }

    /// New worlds go to the selected root; existing worlds retain their location.
    pub fn destination(&self, root: &Path) -> Result<Self> {
        if !self.roots.iter().any(|p| p == root) {
            return Err(StoreError::Journal("Choose a configured world location.".into()));
        }
        if !root.is_dir() { return Err(StoreError::Journal("The world location is not available.".into())); }
        let mut store = self.clone();
        store.worlds = root.to_path_buf();
        Ok(store)
    }

    pub fn profile_dir(&self) -> &Path {
        &self.profile
    }

    pub fn identity_path(&self) -> PathBuf {
        self.profile.join("identity.json")
    }

    pub fn worlds_dir(&self) -> PathBuf {
        self.worlds.clone()
    }

    pub fn world_dir(&self, world_id: &str) -> PathBuf {
        let name = dir_name(world_id);
        let fleeting = self.fleeting_root().join(&name);
        self.roots.clone().into_iter().map(|root| root.join(&name))
            .chain(std::iter::once(fleeting.clone()))
            .find(|dir| self.is_current(dir))
            // Made for a world about to arrive, to be held only while online.
            .or_else(|| fleeting.is_dir().then_some(fleeting))
            .unwrap_or_else(|| self.worlds_dir().join(name))
    }

    /// Where worlds lie that are held only while online.
    fn fleeting_root(&self) -> PathBuf {
        self.profile.join("online")
    }

    /// Whether the world is held here only while online.
    pub fn is_fleeting(&self, world_id: &str) -> bool {
        self.world_dir(world_id).parent() == Some(self.fleeting_root().as_path())
    }

    /// The worlds held only while online, held now or not — fetched again
    /// whenever their server can be reached.
    pub fn fleeting_worlds(&self) -> Vec<String> {
        std::fs::read(self.profile.join("fleeting.json")).ok().and_then(|raw| serde_json::from_slice(&raw).ok()).unwrap_or_default()
    }

    fn note_fleeting(&self, world_id: &str, fleeting: bool) -> Result<()> {
        let mut all = self.fleeting_worlds();
        let before = all.len();
        all.retain(|w| w != world_id);
        if fleeting {
            all.push(world_id.to_string());
        }
        if all.len() != before || fleeting {
            aw_store::durable::write(&self.profile.join("fleeting.json"), &serde_json::to_vec_pretty(&all)?)?;
        }
        Ok(())
    }

    /// Holds a world only while online — or keeps it after all, as the
    /// world says now. One about to arrive goes there; one held already
    /// moves, or goes and comes again.
    pub fn hold_fleeting(&self, world_id: &str, fleeting: bool) -> Result<()> {
        self.note_fleeting(world_id, fleeting)?;
        let to = if fleeting { self.fleeting_root() } else { self.worlds_dir() }.join(dir_name(world_id));
        if !self.holds(world_id) {
            if fleeting {
                std::fs::create_dir_all(&to)?;
            }
            return Ok(());
        }
        if self.is_fleeting(world_id) == fleeting {
            return Ok(());
        }
        let from = self.world_dir(world_id);
        self.close(world_id);
        crate::readable::set_own(self, world_id, false).map_err(|e| StoreError::Journal(e.0))?;
        std::fs::create_dir_all(to.parent().unwrap_or(&to))?;
        // On another drive: gone, and fetched again.
        if std::fs::rename(&from, &to).is_err() {
            std::fs::remove_dir_all(&from)?;
        }
        Ok(())
    }

    /// Lets go of a world held only while online — the server has it no
    /// more for now, or, `for_good`, this device is no member any more.
    pub fn let_go(&self, world_id: &str, for_good: bool) -> Result<()> {
        if for_good {
            self.note_fleeting(world_id, false)?;
        }
        if !self.is_fleeting(world_id) {
            return Ok(());
        }
        let dir = self.world_dir(world_id);
        self.close(world_id);
        crate::readable::set_own(self, world_id, false).map_err(|e| StoreError::Journal(e.0))?;
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    /// Lets a server write in members' names — see `Item::Hosts`. The GM's.
    pub fn add_host(&self, world_id: &str, id: &str) -> Result<()> {
        let mut ids = self.with_journal(world_id, |journal| Ok(journal.hosts()))?;
        if ids.iter().any(|h| h == id) {
            return Ok(());
        }
        ids.push(id.to_string());
        self.control(world_id, Item::Hosts { ids }, "")
    }

    /// Lets go of every world held only while online — when the app starts.
    pub fn forget_fleeting(&self) {
        let root = self.fleeting_root();
        if let Ok(entries) = std::fs::read_dir(&root) {
            for entry in entries.flatten() {
                if let Ok(local) = self.read_descriptor(&entry.path().join("world.json")) {
                    let id = local.public.world_id.to_string();
                    self.close(&id);
                    let _ = crate::readable::set_own(self, &id, false);
                }
            }
        }
        let _ = std::fs::remove_dir_all(root);
    }

    /// Whether a folder holds a world in the current format. Folders of
    /// earlier development formats are passed over — not loaded, not synced.
    fn is_current(&self, dir: &Path) -> bool {
        self.read_descriptor(&dir.join("world.json")).is_ok()
    }

    /// Whether this device holds the world, in the current format.
    pub fn holds(&self, world_id: &str) -> bool {
        self.is_current(&self.world_dir(world_id))
    }

    /// Private state belongs to this profile, independent of the world's location.
    pub fn private_world_dir(&self, world_id: &str) -> PathBuf {
        self.profile.join("worlds").join(dir_name(world_id))
    }

    // --- Identity ------------------------------------------------------

    pub fn has_identity(&self) -> bool {
        self.identity_path().exists()
    }

    pub fn load_identity(&self) -> Result<Identity> {
        Ok(Identity::load(&self.identity_path())?)
    }

    pub fn save_identity(&self, identity: &Identity) -> Result<()> {
        identity.save(&self.identity_path())?;
        Ok(())
    }

    // --- The journal ---------------------------------------------------

    /// The world's journal — opened once, then kept; picks up records that
    /// came meanwhile, and opens with this device's keys.
    fn journal(&self, world_id: &str) -> Result<Arc<Mutex<Open>>> {
        self.require_online_access(world_id)?;
        let local = self.load_local(world_id)?;
        let dir = self.world_dir(world_id);
        let key = (self.profile.clone(), dir.clone());
        let held = open_journals().lock().map_err(poisoned)?.get(&key).cloned();
        let (open, fresh) = match held {
            Some(open) => (open, false),
            None => {
                // Opened by one at a time, world by world: a big world being
                // read keeps no other world waiting.
                let gate = opening().lock().map_err(poisoned)?.entry(key.clone()).or_default().clone();
                let _turn = gate.lock().map_err(poisoned)?;
                let meanwhile = open_journals().lock().map_err(poisoned)?.get(&key).cloned();
                match meanwhile {
                    Some(open) => (open, false),
                    None => {
                        let alone = self.online_cache || ALONE.load(std::sync::atomic::Ordering::Relaxed);
                        let mode = if alone { journal::Mode::Alone } else { journal::Mode::Shared };
                        let journal = Journal::open(&dir, &self.private_world_dir(world_id), &local.public, mode)?;
                        let open = Arc::new(Mutex::new(Open { journal, projection: None }));
                        open_journals().lock().map_err(poisoned)?.insert(key, open.clone());
                        (open, true)
                    }
                }
            }
        };
        // In one go over many documents, what lies on disk is looked at once
        // — not again for each of them.
        if !fresh && QUIET.with(|q| q.get()) {
            return Ok(open);
        }
        {
            let mut guard = open.lock().map_err(poisoned)?;
            guard.journal.refresh()?;
            let keys = self.device_keys(world_id, &guard.journal)?;
            guard.journal.set_keys(keys);
            if self.has_identity() {
                let me = self.load_identity()?;
                guard.journal.set_me(&me);
                if !self.online_cache {
                    self.close_idle(world_id, &mut guard.journal, &me)?;
                }
            }
        }
        if fresh && !self.online_cache {
            // Not finished now, then the next time the world opens.
            let _ = self.resume_switch(world_id);
        }
        Ok(open)
    }

    /// Writes the next state of the game master's worlds in shared folders
    /// that nobody wrote for a quiet minute. A world busy right now waits for
    /// the next time.
    pub fn consolidate_quiet(&self) {
        let open: Vec<Arc<Mutex<Open>>> = match open_journals().lock() {
            Ok(all) => all.iter().filter(|((profile, _), _)| *profile == self.profile).map(|(_, o)| o.clone()).collect(),
            Err(_) => return,
        };
        for open in open {
            // Worked out with the journal, written without it — the world
            // stays open to work in meanwhile — and taken in with it again.
            let (world, next) = {
                let Ok(mut guard) = open.try_lock() else { continue };
                let world = guard.journal.world().world_id.to_string();
                if !self.holds_token(&world) || !guard.journal.consolidation_due(STATE_AFTER) {
                    continue;
                }
                (world, guard.journal.next_state())
            };
            let written = match next {
                Ok(Some(next)) => next.write(),
                Ok(None) => continue,
                Err(e) => Err(e),
            };
            let taken = written.and_then(|w| open.lock().map_err(|_| "the world is poisoned".into()).and_then(|mut g| g.journal.adopt_state(w)));
            if let Err(e) = taken {
                eprintln!("{world}: the next state was not written: {e}");
            }
        }
    }

    /// Ends one's own sessions that rest: no longer open in this process —
    /// left behind by a crash, say — or quiet for a while.
    fn close_idle(&self, world_id: &str, journal: &mut Journal, me: &Identity) -> Result<()> {
        let now = now();
        for (session, last) in journal.open_sessions(me.id().as_str()) {
            let key = (self.profile.clone(), world_id.to_string(), session.clone());
            let active = active_sessions().lock().map_err(poisoned)?.get(&key).copied();
            if active.is_none_or(|at| now.saturating_sub(at.max(last)) > IDLE_MS) {
                journal.close(&session, me)?;
                active_sessions().lock().map_err(poisoned)?.remove(&key);
            }
        }
        Ok(())
    }

    /// Ends an editing session: each document it wrote in gets its version.
    pub fn close_session(&self, world_id: &str, session: &str) -> Result<()> {
        let me = self.load_identity()?;
        active_sessions()
            .lock()
            .map_err(poisoned)?
            .remove(&(self.profile.clone(), world_id.to_string(), session.to_string()));
        self.with_journal(world_id, |journal| {
            journal.close(session, &me)?;
            Ok(())
        })?;
        self.prune(world_id, false)
    }

    /// Works on the world's journal.
    /// Many records of one's own at once, written together at the end — see
    /// `Journal::begin_batch`. What `work` made before it failed is written too.
    pub fn batched<T, E: From<StoreError>>(&self, world_id: &str, work: impl FnOnce() -> std::result::Result<T, E>) -> std::result::Result<T, E> {
        self.with_journal(world_id, |journal| {
            journal.begin_batch();
            Ok(())
        })?;
        let done = work();
        self.with_journal(world_id, |journal| journal.end_batch().map_err(|e| StoreError::Journal(e.to_string())))?;
        done
    }

    pub(crate) fn with_journal<T>(&self, world_id: &str, f: impl FnOnce(&mut Journal) -> Result<T>) -> Result<T> {
        let open = self.journal(world_id)?;
        let mut guard = open.lock().map_err(poisoned)?;
        f(&mut guard.journal)
    }

    /// The keys this device holds for the world: as game master the whole
    /// keyring, as player what was granted to them.
    fn device_keys(&self, world_id: &str, journal: &Journal) -> Result<HeldKeys> {
        // This device's own keys, whomever it acts for.
        if self.holds_token(world_id) {
            return Ok(self.load_keyring(world_id)?.everything());
        }
        let identity = self.load_identity()?;
        let Some(grant) = journal.grant_content(identity.id().as_str(), identity.seal_secret()) else {
            return Ok(HeldKeys::default());
        };
        let mut held = HeldKeys::unseal(identity.seal_secret(), &grant.keys);
        // Unencrypted, the grant names attributes alone: what asks for them sees them.
        if !journal.encrypted() {
            let attributes = [aw_model::Attribute::All, aw_model::Attribute::Players]
                .into_iter()
                .chain(grant.attributes.iter().filter_map(|a| a.parse().ok()));
            stand_ins(&mut held, attributes);
        }
        Ok(held)
    }

    /// Forgets an open journal — the world is gone.
    pub(crate) fn close(&self, world_id: &str) {
        if let Ok(mut all) = open_journals().lock() {
            all.remove(&(self.profile.clone(), self.world_dir(world_id)));
        }
    }

    // --- Worlds --------------------------------------------------------

    pub fn list_worlds(&self) -> Result<Vec<WorldMeta>> {
        let mut out = Vec::new();
        for id in self.held_worlds()? {
            if let Ok(meta) = self.load_world_meta(&id) { out.push(meta); }
        }
        out.sort_by(|a, b| a.public.name.cmp(&b.public.name));
        Ok(out)
    }

    /// The worlds lying here, by id — from their descriptors alone, without
    /// reading a journal.
    pub fn held_worlds(&self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for root in self.roots.clone().into_iter().chain(std::iter::once(self.fleeting_root())) {
            if !root.is_dir() { continue; }
            for entry in std::fs::read_dir(root)? {
                let path = entry?.path().join("world.json");
                let Ok(local) = self.read_descriptor(&path) else { continue };
                let id = local.public.world_id.as_str();
                if path.parent().and_then(Path::file_name).and_then(|p| p.to_str()) != Some(dir_name(id).as_str()) { continue; }
                if !seen.insert(id.to_string()) {
                    return Err(StoreError::Journal(format!("World {id} exists in more than one storage location. Remove the duplicate location from Storage.")));
                }
                out.push(id.to_string());
            }
        }
        Ok(out)
    }

    fn read_local(&self, path: &Path) -> Result<Local> {
        let local: Local = serde_json::from_slice(&std::fs::read(path)?)?;
        if local.version != STORE_VERSION {
            return Err(StoreError::UnknownVersion { found: local.version, known: STORE_VERSION });
        }
        Ok(local)
    }

    fn load_local(&self, world_id: &str) -> Result<Local> {
        let path = self.world_dir(world_id).join("world.json");
        if !path.exists() {
            return Err(StoreError::NoSuchWorld(world_id.to_string()));
        }
        let descriptor = self.read_descriptor(&path)?;
        if descriptor.public.world_id.as_str() != world_id {
            return Err(StoreError::Journal("World descriptor does not match its folder.".into()));
        }
        let private = self.private_world_dir(world_id).join("state.json");
        if private.exists() {
            let local = self.read_local(&private)?;
            if local.public.world_id != descriptor.public.world_id
                || local.public.owner_key != descriptor.public.owner_key {
                return Err(StoreError::Journal("Private world state does not match the world.".into()));
            }
            return Ok(local);
        }
        Ok(Local { version: STORE_VERSION, public: descriptor.public,
            owner: String::new(), members: Vec::new(), invited: Vec::new() })
    }

    fn save_local(&self, local: &Local) -> Result<()> {
        let id = local.public.world_id.as_str();
        aw_store::durable::write(&self.private_world_dir(id).join("state.json"), &serde_json::to_vec_pretty(local)?)?;
        self.save_descriptor(&local.public)
    }

    /// The world as it stands: the journal's part and this device's.
    pub fn load_world_meta(&self, world_id: &str) -> Result<WorldMeta> {
        let local = self.load_local(world_id)?;
        let gm = self.is_gm(world_id);
        let me = if gm { None } else { Some(self.load_identity()?) };
        self.with_journal(world_id, |journal| {
            let mut meta = WorldMeta {
                public: local.public.clone(),
                calendar: WorldCalendar::default(),
                icon: WORLD_ICON.to_string(),
                owner: local.owner.clone(),
                home: String::new(),
                members: local.members.clone(),
                invited: local.invited.clone(),
            };
            if let Some((public, calendar, icon, home, _)) = journal.world_item() {
                meta.public = public.clone();
                meta.calendar = calendar.clone();
                meta.icon = icon.to_string();
                meta.home = home.to_string();
            }
            // A player knows of the members only themselves — from their grant.
            if let Some(me) = me {
                meta.members = journal
                    .grant_content(me.id().as_str(), me.seal_secret())
                    .map(|g| Member { identity: me.public(), role: Role::Player, attributes: g.attributes })
                    .into_iter()
                    .collect();
            }
            Ok(meta)
        })
    }

    /// Rebuild an online GM's private roster from the server's signed grants.
    /// Private metadata from an older device must not re-admit a removed player.
    pub(crate) fn refresh_online_members(&self, world_id: &str) -> Result<()> {
        let me = self.load_identity()?;
        let mut local = self.load_local(world_id)?;
        let (gm, members) = self.with_journal(world_id, |journal| {
            let gm = journal.world_item().map(|(.., gm)| gm.clone());
            let members = journal.granted_members().into_iter().filter_map(|identity| {
                let grant = journal.grant_content(identity.id.as_str(), me.seal_secret())?;
                Some(Member { identity, role: Role::Player, attributes: grant.attributes })
            }).collect::<Vec<_>>();
            Ok((gm, members))
        })?;
        if let Some(gm) = gm {
            if gm.id == *me.id() {
                if !self.holds_token(world_id) {
                    return Err(StoreError::Journal("The GM keys of this online world are missing. Restore the profile's keys from its backup.".into()));
                }
                let attributes = local.members.iter().find(|m| m.identity.id == gm.id).map(|m| m.attributes.clone())
                    .unwrap_or_else(|| vec!["gm".into(), "players".into(), "all".into()]);
                local.members = vec![Member { identity: gm, role: Role::Gm, attributes }];
                local.members.extend(members);
                self.save_local(&local)?;
            } else if self.online_cache && self.holds_token(world_id) {
                // Never destroy the real profile's keys on a role change.
                // This session acts as a player and opens only its own grant.
                std::fs::remove_file(self.private_world_dir(world_id).join("owner.key"))?;
                local.members.clear();
                self.save_local(&local)?;
                self.close(world_id);
            }
        }
        Ok(())
    }

    /// Saves the world. This device's part goes to the profile's `state.json`; as game
    /// master, what changed of the shared part becomes a record.
    pub fn save_world_meta(&self, meta: &WorldMeta) -> Result<()> {
        let id = meta.public.world_id.to_string();
        self.require_online_access(&id)?;
        self.save_local(&Local {
            version: STORE_VERSION,
            public: meta.public.clone(),
            owner: meta.owner.clone(),
            members: if self.is_gm(&id) { meta.members.clone() } else { Vec::new() },
            invited: meta.invited.clone(),
        })?;
        if !self.is_gm(&id) {
            return Ok(());
        }
        let gm = meta
            .members
            .iter()
            .find(|m| m.role == Role::Gm)
            .map(|m| m.identity.clone())
            .ok_or_else(|| StoreError::Journal("a world without its game master".into()))?;
        let same = self.with_journal(&id, |journal| {
            Ok(journal.world_item().is_some_and(|(public, calendar, icon, home, was)| {
                *public == meta.public && *calendar == meta.calendar && icon == meta.icon && home == meta.home && *was == gm
            }))
        })?;
        if !same {
            self.control(
                &id,
                Item::World {
                    public: meta.public.clone(),
                    calendar: meta.calendar.clone(),
                    icon: meta.icon.clone(),
                    home: meta.home.clone(),
                    gm,
                },
                "",
            )?;
        }
        // Those let in ahead of time, where a server can see them: it names
        // the world to them and lets them ask.
        let (listed, expected) = self.with_journal(&id, |journal| Ok((journal.on_the_list(), journal.expected())))?;
        if expected != meta.invited {
            self.control(&id, Item::Listing { listed, expected: meta.invited.clone() }, "")?;
        }
        self.regrant(&id)
    }

    /// Who the world's game master is now, by id — as the journal says.
    /// Whether someone was let in — the game master granted them keys. A
    /// player's device knows the members only by this.
    pub fn has_grant(&self, world_id: &str, id: &str) -> Result<bool> {
        self.with_journal(world_id, |journal| Ok(journal.grant(id).is_some()))
    }

    pub fn gm_of(&self, world_id: &str) -> Result<Option<String>> {
        self.with_journal(world_id, |journal| Ok(journal.world_item().map(|(.., gm)| gm.id.to_string())))
    }

    pub fn load_token(&self, world_id: &str) -> Result<WorldToken> {
        Ok(WorldToken::load(&self.private_world_dir(world_id).join("owner.key"))?)
    }

    pub fn save_token(&self, token: &WorldToken) -> Result<()> {
        aw_store::durable::write(&self.private_world_dir(token.world_id().as_str()).join("owner.key"), &token.to_json()?)?;
        Ok(())
    }

    pub fn load_keyring(&self, world_id: &str) -> Result<Keyring> {
        let path = self.private_world_dir(world_id).join("keyring.json");
        if !path.exists() {
            return Ok(Keyring::new());
        }
        Ok(Keyring::from_json(&std::fs::read_to_string(path)?)?)
    }

    /// Saves the keyring — and hands every member the keys that changed.
    pub fn save_keyring(&self, world_id: &str, ring: &Keyring) -> Result<()> {
        self.require_online_access(world_id)?;
        let dir = self.private_world_dir(world_id);
        std::fs::create_dir_all(&dir)?;
        aw_store::durable::write(&dir.join("keyring.json"), ring.to_json()?.as_bytes())?;
        if self.holds(world_id) {
            self.regrant(world_id)?;
        }
        Ok(())
    }

    /// The articles changed since a version of the journal — `None` where
    /// that is too far back to tell.
    pub fn changed_since(&self, world_id: &str, version: u64) -> Result<Option<std::collections::HashSet<ArticleId>>> {
        self.with_journal(world_id, |journal| Ok(journal.changed_since(version)))
    }

    /// How far the world's journal is — counts up with every change.
    /// The digest of `inventory`, kept until something changes.
    pub fn digest(&self, world_id: &str) -> Result<String> {
        self.require_online_access(world_id)?;
        if !self.holds(world_id) {
            return Ok(journal::Journal::digest(&[]));
        }
        self.with_journal(world_id, |journal| Ok(journal.inventory_digest()))
    }

    /// Where the order of arrivals stands — see `Journal::since`.
    pub fn arrival_mark(&self, world_id: &str) -> Result<Option<(String, u64)>> {
        if !self.holds(world_id) {
            return Ok(None);
        }
        self.with_journal(world_id, |journal| Ok(Some(journal.arrival_mark())))
    }

    /// The records after a mark, each with the mark after it.
    pub fn since(&self, world_id: &str, epoch: &str, at: u64) -> Result<Option<Vec<(u64, String)>>> {
        if !self.holds(world_id) {
            return Ok(None);
        }
        self.with_journal(world_id, |journal| Ok(journal.since(epoch, at)))
    }

    /// Whether a server names the world to everyone — see `Item::Listing`.
    pub fn on_the_list(&self, world_id: &str) -> Result<bool> {
        self.with_journal(world_id, |journal| Ok(journal.on_the_list()))
    }

    /// Named to everyone on its server, or not — the GM's.
    pub fn set_listed(&self, world_id: &str, listed: bool) -> Result<()> {
        let expected = self.load_local(world_id)?.invited;
        self.control(world_id, Item::Listing { listed, expected }, "")
    }

    /// Whether a server lets `id` ask: someone the GM let in ahead of time.
    pub fn expects(&self, world_id: &str, id: &str) -> Result<bool> {
        self.with_journal(world_id, |journal| Ok(journal.expected().iter().any(|e| e == id)))
    }

    /// Whether the world's journal is read and kept in memory already.
    pub fn journal_is_open(&self, world_id: &str) -> bool {
        open_journals().lock().is_ok_and(|all| all.contains_key(&(self.profile.clone(), self.world_dir(world_id))))
    }

    pub fn journal_version(&self, world_id: &str) -> Result<u64> {
        self.with_journal(world_id, |journal| Ok(journal.version()))
    }

    /// Whether players keep a copy of the world — see `Journal::copies`.
    pub fn copies(&self, world_id: &str) -> Result<bool> {
        self.with_journal(world_id, |journal| Ok(journal.copies()))
    }

    /// Whether the world is encrypted.
    pub fn is_encrypted(&self, world_id: &str) -> Result<bool> {
        self.with_journal(world_id, |journal| Ok(journal.encrypted()))
    }

    /// Encrypts the world, or keeps it as it is — the game master's.
    ///
    /// Unencrypted, every part begins anew as it is: whoever has the world
    /// reads and edits all of it; views and rights no longer count. Encrypted
    /// again, every part begins anew with fresh keys, and they count again.
    /// What lay open meanwhile stays readable to whoever kept a copy.
    pub fn set_encrypted(&self, world_id: &str, on: bool) -> Result<()> {
        if !self.is_gm(world_id) {
            return Err(StoreError::Journal("Only the GM decides that.".into()));
        }
        if on == self.is_encrypted(world_id)? {
            return Ok(());
        }
        // Until every part is in its new form, a mark: what breaks off is
        // finished when the world is opened next.
        aw_store::durable::write(&self.private_world_dir(world_id).join(SWITCHING), b"")?;
        self.control(world_id, Item::Encryption { on }, "")?;
        self.finish_switch(world_id)
    }

    /// The rest of switching: types and navigation, then every part not yet
    /// in the form the world is in now.
    fn finish_switch(&self, world_id: &str) -> Result<()> {
        // A copy under way stops first: none may write in the clear meanwhile.
        crate::readable::wait_idle(world_id);
        progress(world_id, 1, "Types and navigation", 0, 0);
        self.reseal(world_id)?;
        let folders = self.gallery_folders(world_id)?;
        if !folders.is_empty() {
            self.seal_gallery_folders(world_id, &folders)?;
        }
        let on = self.is_encrypted(world_id)?;
        if on {
            crate::readable::remove(self, world_id);
        }
        // Over every image and part: what lies on disk is looked at once, not for each.
        let left = quietly(|| -> Result<Vec<String>> {
            let images = self.convert_images(world_id, on)?;
            self.restart_all(world_id, on, &images)?;
            // Only now nothing names the images' old form any more.
            self.left_in_other_form(world_id, on)
        })?;
        if !left.is_empty() {
            progress(world_id, SWITCH_STEPS, "Tidying up", 0, 0);
            self.control(world_id, Item::Deleted { records: left }, "")?;
        }
        // The grants in the world's form too: keys again, or attributes alone.
        self.regrant(world_id)?;
        let _ = std::fs::remove_file(self.private_world_dir(world_id).join(SWITCHING));
        if let Ok(mut all) = switch_progress_all().lock() {
            all.remove(world_id);
        }
        Ok(())
    }

    /// Whether the world is being switched right now — or broke off doing so.
    pub fn switching(&self, world_id: &str) -> bool {
        self.private_world_dir(world_id).join(SWITCHING).exists()
    }

    /// Finishes switching where it broke off — a crash, the app closed meanwhile.
    fn resume_switch(&self, world_id: &str) -> Result<()> {
        if !self.private_world_dir(world_id).join(SWITCHING).exists() || !self.is_gm(world_id) {
            return Ok(());
        }
        self.finish_switch(world_id)
    }

    /// `restart` for every document at once — the keyring loaded and saved
    /// once, not once per part: thousands of documents one by one would hand
    /// the keyring out tens of thousands of times. The new heads are all made
    /// first and the keyring kept before any of them is written, so no record
    /// ever names a key that is not kept. Only parts not already kept as
    /// `encrypted` says.
    fn restart_all(&self, world_id: &str, encrypted: bool, images: &HashMap<String, aw_store::media::ImageRef>) -> Result<()> {
        type Part = (LayerId, Policy, Access, Level, journal::LayerHead, Plain, Option<About>, Vec<String>);
        let parts: Vec<Part> = self.with_journal(world_id, |journal| {
            let mut out = Vec::new();
            let articles = journal.article_list();
            for (n, (article, ..)) in articles.iter().enumerate() {
                let article = *article;
                if n % 100 == 0 {
                    progress(world_id, 3, "Documents", n, articles.len());
                }
                if journal.has_waiting(article) {
                    continue;
                }
                for (layer, generation) in journal.parts_of(article) {
                    let Some(head) = journal.head(layer).cloned() else { continue };
                    if head.plain != encrypted {
                        continue;
                    }
                    let (access, level) = journal.access_at(layer, generation)?;
                    let Ok(view) = access.own.parse::<Policy>() else { continue };
                    let about = if head.is_base { journal.about(head.article) } else { None };
                    let from = journal.state(layer).map(|(_, heads)| heads).unwrap_or_default();
                    let mut plain = journal.plain(layer)?;
                    // Images it shows, in their new form.
                    for (image, found) in plain.images.iter_mut() {
                        if let Some(new) = images.get(image) {
                            *found = new.clone();
                        }
                    }
                    out.push((layer, view, access, level, head, plain, about, from));
                }
            }
            Ok(out)
        })?;
        let mut ring = self.load_keyring(world_id)?;
        if encrypted {
            for (_, view, access, ..) in &parts {
                journal::ensure_keys(&mut ring, view, access)?;
            }
        }
        // Fresh keys and their wrapping for tens of thousands of parts: on every core.
        progress(world_id, 4, "New keys", 0, parts.len());
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        let chunk = parts.len().div_ceil(threads).max(1);
        let ring = &ring;
        let made: Vec<Result<Vec<journal::LayerHead>>> = std::thread::scope(|scope| {
            let workers: Vec<_> = parts
                .chunks(chunk)
                .map(|some| {
                    scope.spawn(move || {
                        some.iter()
                            .map(|(layer, view, access, level, head, plain, about, from)| {
                                let mut next = journal::make_head(
                                    ring, head.article, *layer, head.is_base, *level, head.generation + 1, view, access, plain, about.as_ref(), encrypted,
                                )?;
                                next.from = from.clone();
                                Ok(next)
                            })
                            .collect::<Result<Vec<_>>>()
                    })
                })
                .collect();
            workers.into_iter().map(|w| w.join().unwrap_or_else(|_| Err(StoreError::Journal("A worker failed.".into())))).collect()
        });
        let mut heads = Vec::with_capacity(parts.len());
        for some in made {
            heads.extend(some?);
        }
        progress(world_id, 5, "Writing", 0, 0);
        self.save_keyring(world_id, &ring)?;
        let token = self.load_token(world_id)?;
        self.with_journal(world_id, |journal| {
            journal.control_all(&token, heads.into_iter().map(Item::Layer).collect(), "", now())?;
            Ok(())
        })
    }

    /// Whether we hold the world token for this world — that is, are its GM.
    pub fn is_gm(&self, world_id: &str) -> bool {
        // Acting for a player, the host is that player.
        crate::host::acting().is_none() && self.holds_token(world_id)
    }

    /// Whether this device holds the world token — whomever it acts for.
    pub(crate) fn holds_token(&self, world_id: &str) -> bool {
        self.private_world_dir(world_id).join("owner.key").exists()
    }

    /// A record of the world's structure, signed with the world token.
    pub fn control(&self, world_id: &str, item: Item, session: &str) -> Result<()> {
        let token = self.load_token(world_id)?;
        self.with_journal(world_id, |journal| {
            journal.control(&token, item, session, now())?;
            Ok(())
        })
    }

    /// Makes the attributes a view names, where they are new — the game
    /// master's: `doc:…` comes into being the first time a part asks for it.
    pub fn ensure_attributes(&self, world_id: &str, view: &aw_model::Policy) -> Result<()> {
        if !self.is_gm(world_id) {
            return Ok(());
        }
        let mut ring = self.load_keyring(world_id)?;
        let before = ring.attributes().count();
        for attribute in view.mentioned() {
            ring.ensure(attribute)?;
        }
        if ring.attributes().count() != before {
            self.save_keyring(world_id, &ring)?;
        }
        Ok(())
    }

    /// Writes each member's keys anew where they changed: new attributes,
    /// rotated ones.
    fn regrant(&self, world_id: &str) -> Result<()> {
        if !self.is_gm(world_id) {
            return Ok(());
        }
        let local = self.load_local(world_id)?;
        let ring = self.load_keyring(world_id)?;
        let gm = self.load_identity()?;
        let encrypted = self.is_encrypted(world_id)?;
        for member in local.members.iter().filter(|m| m.role == Role::Player) {
            let attributes: Vec<aw_model::Attribute> =
                member.attributes.iter().filter_map(|a| a.parse().ok()).collect();
            // Unencrypted, a grant names attributes alone, as it is — a
            // server holding the world reads whose views are whose. No keys:
            // they would lie open for good.
            let keys = if encrypted { ring.grant(&member.identity, &attributes)? } else { Vec::new() };
            let wanted: Vec<&String> = keys.iter().map(|k| &k.at).collect();
            // The grant is sealed for the member and for whoever wrote it — so
            // the game master can tell whether it still says the right thing.
            let current = self.with_journal(world_id, |journal| {
                Ok(journal.grant(member.identity.id.as_str()).map(|(identity, sealed)| (identity.clone(), sealed.is_plain())).zip(
                    journal.grant_content(member.identity.id.as_str(), gm.seal_secret()),
                ))
            })?;
            let same = current.as_ref().is_some_and(|((identity, plain), g)| {
                *identity == member.identity
                    && *plain != encrypted
                    && g.attributes == member.attributes
                    && g.keys.iter().map(|k| &k.at).collect::<Vec<_>>() == wanted
            });
            if !same {
                let content = serde_json::to_vec(&GrantContent { attributes: member.attributes.clone(), keys })?;
                let sealed = if encrypted { Sealed::new(&[&member.identity, &gm.public()], &content)? } else { Sealed::plain(&content) };
                self.control(world_id, Item::Grant { member: member.identity.clone(), sealed }, "")?;
            }
        }
        Ok(())
    }

    // --- Templates ----------------------------------------------------

    /// The templates of a world.
    ///
    /// A freshly created world has none, and none are shipped either: which
    /// types there should be is known only to whoever builds the world.
    pub fn load_templates(&self, world_id: &str) -> Result<Vec<Template>> {
        if !self.holds(world_id) {
            return Ok(Vec::new());
        }
        let mut all = self.with_journal(world_id, |journal| Ok(journal.templates().unwrap_or_default()))?;
        for template in &mut all {
            template.normalize();
        }
        Ok(all)
    }

    /// Saves the types — each piece sealed for whoever may see it, or all
    /// of them as they are in an unencrypted world.
    pub fn save_templates(&self, world_id: &str, templates: &[Template]) -> Result<()> {
        if !self.is_encrypted(world_id)? {
            let (frame, pieces) = journal::types::plain(templates)?;
            return self.control(world_id, Item::Templates { frame, pieces }, "");
        }
        let mut ring = self.load_keyring(world_id)?;
        let (frame, pieces) = journal::types::seal(templates, &mut ring)?;
        self.save_keyring(world_id, &ring)?;
        self.control(world_id, Item::Templates { frame, pieces }, "")
    }

    /// Seals the types and the navigation anew — after keys turned over, so
    /// that what is new opens with the new keys.
    pub fn reseal(&self, world_id: &str) -> Result<()> {
        if let Some(navigation) = self.navigation(world_id)? {
            self.seal_navigation(world_id, &navigation)?;
        }
        let templates = self.load_templates(world_id)?;
        if templates.is_empty() {
            return Ok(());
        }
        self.save_templates(world_id, &templates)
    }

    // --- Navigation ---------------------------------------------------

    /// The side bar's entries as far as this device may see them — `None`
    /// while the game master has set none.
    pub fn navigation(&self, world_id: &str) -> Result<Option<Vec<NavEntry>>> {
        if !self.holds(world_id) {
            return Ok(None);
        }
        self.with_journal(world_id, |journal| Ok(journal.navigation()))
    }

    /// Sets the side bar's entries — the game master's.
    pub fn set_navigation(&self, world_id: &str, entries: &[NavEntry]) -> Result<()> {
        if !self.is_gm(world_id) {
            return Err(StoreError::Journal("Only the GM sets the navigation.".into()));
        }
        aw_model::navigation::check(entries).map_err(|e| StoreError::Journal(e.to_string()))?;
        if self.navigation(world_id)?.as_deref() == Some(entries) {
            return Ok(());
        }
        self.seal_navigation(world_id, entries)
    }

    fn seal_navigation(&self, world_id: &str, entries: &[NavEntry]) -> Result<()> {
        if !self.is_encrypted(world_id)? {
            // No views: the players get it whole.
            let all = journal::types::SealedFor::plain(&entries)?;
            return self.control(world_id, Item::Navigation { players: all.clone(), gm: all }, "");
        }
        let mut ring = self.load_keyring(world_id)?;
        // Every member has `all` and `players`, like the frame of the types.
        let policy = |text: &str| text.parse::<Policy>().map_err(|e| StoreError::Journal(e.to_string()));
        let players = journal::types::SealedFor::new(&policy("all OR players")?, &mut ring, &aw_model::navigation::for_players(entries))?;
        let gm = journal::types::SealedFor::new(&policy("gm")?, &mut ring, &entries)?;
        self.save_keyring(world_id, &ring)?;
        self.control(world_id, Item::Navigation { players, gm }, "")
    }

    /// Deletes a world along with its world token, keyring and journal.
    ///
    /// Irrevocable: with the world token goes any chance of ever decrypting
    /// anything from it again.
    pub fn delete_world(&self, world_id: &str) -> Result<()> {
        let dir = self.world_dir(world_id);
        if !dir.exists() {
            return Err(StoreError::NoSuchWorld(world_id.to_string()));
        }
        let _away = Away::start(world_id)?;
        self.close(world_id);
        // What writes in the background stops at its next file; then it goes.
        for _ in 0..300 {
            if !crate::readable::writing(world_id) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        remove_tree(&dir)?;
        remove_tree(&self.private_world_dir(world_id))?;
        // Kept in a folder, or noted to fetch again, it would come right back.
        self.note_fleeting(world_id, false)?;
        let mut settings = self.load_network()?;
        settings.shared.retain(|w| w != world_id);
        settings.homes.remove(world_id);
        settings.uploading.retain(|w| w != world_id);
        self.save_network(&settings)?;
        Ok(())
    }

    /// Moves a world's folder into another location, with all it holds. On
    /// the same drive a rename; else copied beside it first and only then
    /// swapped in — broken off, the world stays where it was. Tells how far
    /// it is, in files.
    pub fn move_world(&self, world_id: &str, root: &Path, tell: &dyn Fn(usize, usize)) -> Result<()> {
        self.destination(root)?;
        let from = self.world_dir(world_id);
        if !self.is_current(&from) {
            return Err(StoreError::NoSuchWorld(world_id.to_string()));
        }
        if from.parent() == Some(root) {
            return Ok(());
        }
        let name = dir_name(world_id);
        let to = root.join(&name);
        if to.exists() {
            return Err(StoreError::Journal("A folder of this world lies there already.".into()));
        }
        let _away = Away::start(world_id)?;
        // Whoever writes meanwhile waits — and finds the world gone after.
        let open = self.journal(world_id)?;
        let _held = open.lock().map_err(poisoned)?;
        let moved = std::fs::rename(&from, &to).or_else(|_| {
            let beside = root.join(format!(".moving-{name}"));
            let _ = std::fs::remove_dir_all(&beside);
            let copied = copy_dir(&from, &beside, tell);
            if copied.is_err() {
                let _ = std::fs::remove_dir_all(&beside);
            }
            copied?;
            let old = from.with_file_name(format!(".moved-{name}"));
            std::fs::rename(&from, &old)?;
            if let Err(e) = std::fs::rename(&beside, &to) {
                let _ = std::fs::rename(&old, &from);
                return Err(e);
            }
            std::fs::remove_dir_all(&old)
        });
        if let Ok(mut all) = open_journals().lock() {
            all.remove(&(self.profile.clone(), from));
        }
        Ok(moved?)
    }

    // --- Avatar --------------------------------------------------------

    fn avatar_path(&self) -> PathBuf {
        self.profile.join("avatar")
    }

    /// Stores an avatar. Expects the raw bytes and the media type.
    pub fn save_avatar(&self, bytes: &[u8], media_type: &str) -> Result<String> {
        std::fs::create_dir_all(&self.profile)?;
        let record = AvatarFile { media_type: media_type.to_string(), data: hex::encode(bytes) };
        std::fs::write(self.avatar_path(), serde_json::to_vec(&record)?)?;
        Ok(record.as_data_url())
    }

    /// The avatar as a `data:` URL, if one is set.
    pub fn load_avatar(&self) -> Result<Option<String>> {
        let path = self.avatar_path();
        if !path.exists() {
            return Ok(None);
        }
        let record: AvatarFile = serde_json::from_slice(&std::fs::read(path)?)?;
        Ok(Some(record.as_data_url()))
    }

    pub fn remove_avatar(&self) -> Result<()> {
        let path = self.avatar_path();
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    // --- Keys ----------------------------------------------------------

    /// The keys this world is opened with here.
    ///
    /// As game master the full keyring, as player what was granted to them.
    /// In both cases the only thing that decides is what they decrypt.
    pub fn view_keys(&self, world_id: &str, identity: &Identity) -> Result<HeldKeys> {
        self.require_online_access(world_id)?;
        if let Some(acting) = crate::host::acting() {
            return Ok(acting.held);
        }
        if self.is_gm(world_id) {
            return Ok(self.load_keyring(world_id)?.everything());
        }
        let (keys, encrypted) = self.with_journal(world_id, |journal| {
            let keys = journal
                .grant_content(identity.id().as_str(), identity.seal_secret())
                .map(|g| HeldKeys::unseal(identity.seal_secret(), &g.keys))
                .unwrap_or_default();
            Ok((keys, journal.encrypted()))
        })?;
        // Unencrypted, nothing needs a key.
        if keys.is_empty() && encrypted {
            return Err(StoreError::NoKeys);
        }
        Ok(keys)
    }

    /// The number of documents — without reading any of them.
    pub fn article_count(&self, world_id: &str) -> Result<usize> {
        if !self.holds(world_id) {
            return Ok(0);
        }
        self.with_journal(world_id, |journal| Ok(journal.article_count()))
    }

    // --- Articles ------------------------------------------------------

    /// The articles as they stand, each layer encrypted with its data key —
    /// so whatever opens them decides what can be read, exactly as before.
    pub fn load_articles(&self, world_id: &str) -> Result<Vec<StoredArticle>> {
        if !self.holds(world_id) {
            return Ok(Vec::new());
        }
        let open = self.journal(world_id)?;
        let mut guard = open.lock().map_err(poisoned)?;
        let version = guard.journal.version();
        if let Some((v, articles)) = &guard.projection {
            if *v == version {
                return Ok(articles.clone());
            }
        }
        // Only what changed since the last projection is built again — with
        // ten thousand documents, one edit should not cost all of them.
        let changed = guard.projection.as_ref().and_then(|(v, _)| guard.journal.changed_since(*v));
        let before: HashMap<ArticleId, StoredArticle> = match (&changed, guard.projection.take()) {
            (Some(_), Some((_, articles))) => articles.into_iter().map(|a| (a.id, a)).collect(),
            _ => HashMap::new(),
        };
        let mut before = before;
        // The first time in this run: what was built at an earlier start, as
        // far as nothing it hangs on changed since.
        let mut earlier = if changed.is_none() { self.read_projection(world_id) } else { HashMap::new() };
        // Nothing built before — a world opened the first time, a session
        // read from its server: every document is opened, on every core.
        if changed.is_none() && earlier.is_empty() {
            let parts = guard.journal.all_parts();
            guard.journal.open_many(&parts);
        }
        let mut out = Vec::new();
        let mut built = 0;
        let mut prints = BTreeMap::new();
        for (article, article_type, slug) in guard.journal.article_list() {
            let kept = changed.as_ref().filter(|c| !c.contains(&article)).and_then(|_| before.remove(&article));
            match kept {
                Some(same) if same.article_type == article_type && same.slug == slug => out.push(same),
                _ => {
                    let print = guard.journal.article_fingerprint(article, &article_type, &slug);
                    match earlier.remove(&article.to_string()) {
                        Some((p, same)) if p == print => out.push(same),
                        _ => {
                            out.push(stored(guard.journal.article_state(article, article_type, slug))?);
                            built += 1;
                        }
                    }
                    prints.insert(article.to_string(), print);
                }
            }
        }
        guard.projection = Some((version, out.clone()));
        // Kept for the next start — in the background: thousands of
        // documents take a moment to write.
        if changed.is_none() && built > 0 {
            let (store, world, articles) = (self.clone(), world_id.to_string(), out.clone());
            std::thread::spawn(move || store.write_projection(&world, prints, &articles));
        }
        Ok(out)
    }

    /// The projection an earlier start kept: article → (fingerprint, state).
    fn read_projection(&self, world_id: &str) -> HashMap<String, (String, StoredArticle)> {
        std::fs::read(self.private_world_dir(world_id).join(PROJECTION))
            .ok()
            .and_then(|raw| serde_json::from_slice::<Projection>(&raw).ok())
            .filter(|p| p.version == 1)
            .map(|p| p.articles)
            .unwrap_or_default()
    }

    fn write_projection(&self, world_id: &str, mut prints: BTreeMap<String, String>, articles: &[StoredArticle]) {
        if moving(world_id) {
            return;
        }
        let kept = Projection {
            version: 1,
            articles: articles.iter().filter_map(|a| Some((a.id.to_string(), (prints.remove(&a.id.to_string())?, a.clone())))).collect(),
        };
        if let Ok(raw) = serde_json::to_vec(&kept) {
            let _ = aw_store::durable::write(&self.private_world_dir(world_id).join(PROJECTION), &raw);
        }
    }

    /// The articles as they stand — with one of them as it stood with a
    /// version. So everything that reads articles shows that version too.
    pub fn load_articles_at(&self, world_id: &str, article: ArticleId, session: &str, time: u64) -> Result<Vec<StoredArticle>> {
        let mut all = self.load_articles(world_id)?;
        let then = self.with_journal(world_id, |journal| stored(journal.article_at(article, session, time)?))?;
        match all.iter_mut().find(|a| a.id == article) {
            Some(slot) => *slot = then,
            None => all.push(then),
        }
        Ok(all)
    }

    /// A new article, with its parts — the game master's.
    #[cfg(test)]
    pub fn create_article(
        &self,
        world_id: &str,
        article_type: String,
        slug: String,
        parts: Vec<NewPart>,
        session: &str,
    ) -> Result<ArticleId> {
        self.create_article_as(world_id, ArticleId::new(), article_type, slug, parts, session)
    }

    /// A new article under an id chosen beforehand — where its parts' views
    /// name it already (`self`, see `own_view`).
    pub fn create_article_as(
        &self,
        world_id: &str,
        article: ArticleId,
        article_type: String,
        slug: String,
        parts: Vec<NewPart>,
        session: &str,
    ) -> Result<ArticleId> {
        self.control(world_id, Item::Article { article }, session)?;
        // What the document is lies in its base layer, for its readers only.
        let about = About { article_type, slug };
        for part in parts {
            let about = part.is_base.then_some(&about);
            self.add_part(world_id, article, part, about, session)?;
        }
        Ok(article)
    }

    /// A new part in an article — with fresh keys. A base part carries what
    /// the document is.
    pub fn add_part(&self, world_id: &str, article: ArticleId, part: NewPart, about: Option<&About>, session: &str) -> Result<LayerId> {
        let layer = LayerId::new();
        // The images its text names come with their keys.
        let plain = self.with_images(world_id, &Plain::default(), &part.plain)?;
        let encrypted = self.is_encrypted(world_id)?;
        let mut ring = self.load_keyring(world_id)?;
        let before = ring.to_json()?;
        let head = journal::new_head(
            &mut ring,
            article,
            layer,
            part.is_base,
            part.level,
            1,
            &part.view,
            &part.access,
            &plain,
            about,
            encrypted,
        )?;
        // A new part mostly needs no new key: the ring, and every member's
        // grant made from it, stay as they are.
        if ring.to_json()? != before {
            self.save_keyring(world_id, &ring)?;
        }
        self.control(world_id, Item::Layer(head), session)?;
        Ok(layer)
    }

    /// Who sees or edits a part changed: a new generation of it, with fresh
    /// keys and its content so far.
    pub fn rekey(
        &self,
        world_id: &str,
        layer: LayerId,
        view: &Policy,
        access: Access,
        level: Level,
        session: &str,
    ) -> Result<()> {
        let (head, plain, about, from) = self.with_journal(world_id, |journal| {
            let head = journal.head(layer).cloned().ok_or_else(|| StoreError::Journal("There is no such part.".into()))?;
            let about = if head.is_base { journal.about(head.article) } else { None };
            let from = journal.state(layer).map(|(_, heads)| heads).unwrap_or_default();
            Ok((head, journal.plain(layer)?, about, from))
        })?;
        let encrypted = self.is_encrypted(world_id)?;
        let mut ring = self.load_keyring(world_id)?;
        let mut next = journal::new_head(
            &mut ring,
            head.article,
            layer,
            head.is_base,
            level,
            head.generation + 1,
            view,
            &access,
            &plain,
            about.as_ref(),
            encrypted,
        )?;
        // What it starts from: so what was written there counts as taken along.
        next.from = from;
        self.save_keyring(world_id, &ring)?;
        self.control(world_id, Item::Layer(next), session)
    }

    /// Removes an article, or one of its parts — for good.
    pub fn remove(&self, world_id: &str, article: ArticleId, layer: Option<LayerId>, session: &str) -> Result<()> {
        self.control(world_id, Item::Removed { article, layer }, session)
    }

    /// A layer's generation and heads as they stand — where an editor starts.
    pub fn layer_state(&self, world_id: &str, layer: LayerId) -> Result<Option<(u32, Vec<String>)>> {
        self.with_journal(world_id, |journal| Ok(journal.state(layer)))
    }

    /// Who could see and edit a layer as an editor saw it.
    pub fn access_at(&self, world_id: &str, layer: LayerId, generation: u32) -> Result<(Access, Level)> {
        self.with_journal(world_id, |journal| Ok(journal.access_at(layer, generation)?))
    }

    /// A layer's content as an editor saw it.
    pub fn plain_at(&self, world_id: &str, layer: LayerId, generation: u32, heads: &[String]) -> Result<Plain> {
        self.with_journal(world_id, |journal| Ok(journal.plain_at(layer, generation, heads)?))
    }

    /// Writes a layer's content, on top of `base` — what the writer saw.
    #[allow(clippy::too_many_arguments)]
    pub fn edit_layer(
        &self,
        world_id: &str,
        layer: LayerId,
        generation: u32,
        base: &[String],
        wanted: &Plain,
        author: &Identity,
        session: &str,
    ) -> Result<()> {
        active_sessions()
            .lock()
            .map_err(poisoned)?
            .insert((self.profile.clone(), world_id.to_string(), session.to_string()), now());
        // In a player's name: only what changes needs their right — as the journal holds it.
        if crate::host::acting().is_some() {
            let now = self.with_journal(world_id, |journal| {
                Ok(if base.is_empty() { journal.plain(layer).ok() } else { journal.plain_at(layer, generation, base).ok() })
            })?;
            if now.as_ref() != Some(wanted) {
                crate::host::may_write(self, world_id, layer, generation).map_err(|e| StoreError::Journal(e.0))?;
            }
        }
        self.with_journal(world_id, |journal| {
            journal.edit(layer, generation, base, wanted, author, session, now())?;
            Ok(())
        })
    }

    /// A stable place in a layer's text.
    pub fn cursor(&self, world_id: &str, layer: LayerId, position: usize) -> Result<String> {
        self.with_journal(world_id, |journal| Ok(journal.cursor(layer, position)?))
    }

    /// Where a stable place stands in a layer's text now.
    pub fn position(&self, world_id: &str, layer: LayerId, cursor: &str) -> Result<usize> {
        self.with_journal(world_id, |journal| Ok(journal.position(layer, cursor)?))
    }

    /// Whether players are ever meant to read a layer — where the game
    /// master writes in one that is not, nobody else needs to know.
    pub fn for_players(&self, world_id: &str, layer: LayerId) -> Result<bool> {
        self.with_journal(world_id, |journal| Ok(journal.head(layer).is_some_and(|h| h.level.for_players())))
    }

    /// The versions of an article.
    pub fn sessions(&self, world_id: &str, article: ArticleId) -> Result<Vec<journal::Session>> {
        self.with_journal(world_id, |journal| Ok(journal.sessions(article)))
    }

    /// A layer before and after a session, with who may see and edit it.
    pub fn compare(&self, world_id: &str, layer: LayerId, session: &str) -> Result<(Plain, Plain)> {
        self.with_journal(world_id, |journal| Ok(journal.compare(layer, session)?))
    }

    /// Carries what a session wrote into a generation a layer has left over
    /// into the current one.
    pub fn carry(&self, world_id: &str, layer: LayerId, session: &str, author: &Identity) -> Result<()> {
        self.with_journal(world_id, |journal| {
            journal.carry(layer, session, author, now())?;
            Ok(())
        })
    }

    /// One's own lines a new generation left behind — carried over where one
    /// may still write. What one may not write any more stays in the history
    /// for the game master to decide on.
    fn carry_own(&self, world_id: &str) -> Result<()> {
        let me = self.load_identity()?;
        self.with_journal(world_id, |journal| {
            for (layer, session) in journal.stale_of(me.id().as_str()) {
                let _ = journal.carry(layer, &session, &me, now());
            }
            Ok(())
        })
    }

    /// Takes a session's changes to a layer back.
    pub fn revert(&self, world_id: &str, layer: LayerId, session: &str, author: &Identity, new_session: &str) -> Result<()> {
        self.with_journal(world_id, |journal| {
            journal.revert(layer, session, author, new_session, now())?;
            Ok(())
        })
    }

    // --- Passing on ----------------------------------------------------

    /// Every shared record here — control and versions — in an order a
    /// receiver can take them in. Everything goes to every member; their keys
    /// decide what they read.
    pub fn all_records(&self, world_id: &str) -> Result<Vec<String>> {
        self.with_journal(world_id, |journal| {
            let ids = journal.in_order(&journal.shared());
            Ok(ids.iter().filter_map(|id| journal.raw(id)).map(|r| String::from_utf8_lossy(&r).to_string()).collect())
        })
    }

    /// Records by name — what `ids` names and is here, in order.
    pub fn records(&self, world_id: &str, ids: &[String]) -> Result<Vec<String>> {
        self.with_journal(world_id, |journal| {
            Ok(journal
                .in_order(ids)
                .iter()
                .filter_map(|id| journal.raw(id))
                .map(|r| String::from_utf8_lossy(&r).to_string())
                .collect())
        })
    }

    /// Of `ids`, those here, in an order a receiver can take them in.
    pub fn in_order(&self, world_id: &str, ids: &[String]) -> Result<Vec<String>> {
        self.with_journal(world_id, |journal| Ok(journal.in_order(ids)))
    }

    /// The names of everything here — drafts too, for writing together over
    /// the network.
    pub fn inventory(&self, world_id: &str) -> Result<Vec<String>> {
        self.require_online_access(world_id)?;
        if !self.holds(world_id) {
            return Ok(Vec::new());
        }
        self.with_journal(world_id, |journal| Ok(journal.inventory()))
    }

    /// Takes in records from elsewhere. A world not here yet comes into
    /// being — as long as its records carry the world's own signature.
    /// Returns how many were new.
    pub fn receive(&self, world_id: &str, records: &[String]) -> Result<usize> {
        self.require_online_access(world_id)?;
        if !self.holds(world_id) {
            // Checked against the world's key before anything is kept: a
            // forged world leaves nothing behind.
            let public = journal::world_of(records, world_id)
                .map_err(|_| StoreError::Journal("This is not the world asked for.".into()))?;
            let private = self.private_world_dir(world_id).join("state.json");
            if private.exists() {
                let local = self.read_local(&private)?;
                if local.public.world_id != public.world_id || local.public.owner_key != public.owner_key {
                    return Err(StoreError::Journal("Private state does not match the received world.".into()));
                }
                self.save_descriptor(&public)?;
            } else {
                self.save_local(&Local { version: STORE_VERSION, public, owner: String::new(), members: Vec::new(), invited: Vec::new() })?;
            }
        }
        let open = self.journal(world_id)?;
        let new = {
            let mut guard = open.lock().map_err(poisoned)?;
            guard.journal.add_all(&records.iter().map(|r| r.as_bytes()).collect::<Vec<_>>())?
        };
        // A new grant brings keys: open again with them.
        if new > 0 {
            {
                let mut guard = open.lock().map_err(poisoned)?;
                let keys = self.device_keys(world_id, &guard.journal)?;
                guard.journal.set_keys(keys);
            }
            // A new generation may have left lines of one's own behind.
            if !self.online_cache {
                self.carry_own(world_id)?;
                self.prune(world_id, false)?;
            }
        }
        Ok(new)
    }

    // --- Members -------------------------------------------------------

    /// Takes a member out — or an id let in ahead of time. What they had they
    /// keep; every key they held turns over, so nothing new reaches them.
    pub fn remove_member(&self, world_id: &str, id: &str) -> Result<WorldMeta> {
        let mut meta = self.load_world_meta(world_id)?;
        let gone = meta.member(id).cloned();
        if gone.as_ref().is_some_and(|m| m.role == Role::Gm) {
            return Err(StoreError::Journal("The game master cannot be removed — hand the world over first.".into()));
        }
        meta.members.retain(|m| m.identity.id.as_str() != id);
        meta.invited.retain(|i| i != id);
        self.save_world_meta(&meta)?;
        // Nobody takes their place, nor they anyone's.
        for (new, old) in self.takeovers(world_id)? {
            if new == id || old == id {
                self.set_takeover(world_id, &new, None)?;
            }
        }
        if self.pending_handover(world_id)?.as_deref() == Some(id) {
            self.set_handover(world_id, None)?;
        }
        // Every device learns of it: the others' stop passing the world on.
        if self.has_grant(world_id, id)? {
            self.control(world_id, Item::Revoked { member: id.to_string() }, "")?;
        }
        if let Some(gone) = gone {
            let mut ring = self.load_keyring(world_id)?;
            for attribute in gone.attributes.iter().filter_map(|a| a.parse::<aw_model::Attribute>().ok()) {
                if ring.current_epoch(&attribute).is_some() {
                    ring.rotate(&attribute)?;
                }
            }
            self.save_keyring(world_id, &ring)?;
            self.reseal(world_id)?;
        }
        Ok(meta)
    }

    // --- Review ---------------------------------------------------------

    /// Whether players' writing waits for the game master.
    pub fn moderated(&self, world_id: &str) -> Result<bool> {
        self.with_journal(world_id, |journal| Ok(journal.moderated()))
    }

    /// Switches review on or off. What stands when it starts stays as it is.
    pub fn set_moderation(&self, world_id: &str, on: bool) -> Result<()> {
        let settled = if on { self.with_journal(world_id, |journal| Ok(journal.sessions_by(None)))? } else { Vec::new() };
        self.control(world_id, Item::Moderation { on, settled }, "")
    }

    /// Whether a member's writing counts without waiting.
    pub fn trusted(&self, world_id: &str, member: &str) -> Result<bool> {
        self.with_journal(world_id, |journal| Ok(journal.trusted(member)))
    }

    /// Trusts a member — or no longer: what they wrote until then stays.
    pub fn set_trust(&self, world_id: &str, member: &str, trusted: bool) -> Result<()> {
        let settled =
            if trusted { Vec::new() } else { self.with_journal(world_id, |journal| Ok(journal.sessions_by(Some(member))))? };
        self.control(world_id, Item::Trust { member: member.to_string(), trusted, settled }, "")
    }

    /// The game master's word on a waiting version — and on the ones that
    /// go with it.
    pub fn verdict(&self, world_id: &str, article: ArticleId, session: &str, accepted: bool) -> Result<()> {
        let (versions, keys) = self.with_journal(world_id, |journal| {
            let mut sessions = journal.bound(session, accepted);
            sessions.push(session.to_string());
            let versions: Vec<String> = sessions.iter().flat_map(|s| journal.waiting_versions(s)).collect();
            // Taken in, what was sealed opens for everyone who may read it.
            let keys = if accepted { journal.reveal(&versions)? } else { BTreeMap::new() };
            Ok((versions, keys))
        })?;
        if versions.is_empty() {
            return Ok(());
        }
        self.control(world_id, Item::Verdict { article, accepted, versions, keys }, "")
    }

    /// The versions waiting on this device: for the game master, everything
    /// players wrote that waits; for a player, their own.
    pub fn waiting(&self, world_id: &str) -> Result<Vec<(ArticleId, String)>> {
        self.with_journal(world_id, |journal| Ok(journal.waiting()))
    }

    /// Where a version and now differ in a layer's text, by the characters
    /// themselves — `None` where they cannot be compared that way, because
    /// the layer changed its generation in between.
    pub fn layer_marks(&self, world_id: &str, layer: LayerId, session: &str, time: u64) -> Result<Option<(Vec<Range>, Vec<Range>)>> {
        self.with_journal(world_id, |journal| {
            let (Some((then_generation, then)), Some((now_generation, now))) =
                (journal.heads_at(layer, session, time)?, journal.shown_state(layer))
            else {
                return Ok(None);
            };
            if then_generation != now_generation {
                return Ok(None);
            }
            Ok(Some(journal.marks(layer, now_generation, &then, &now)?))
        })
    }

    // --- Keeping it small ---------------------------------------------------

    /// How many versions of each document stay.
    pub fn version_limit(&self, world_id: &str) -> Result<u32> {
        self.with_journal(world_id, |journal| Ok(journal.version_limit()))
    }

    /// Asks the game master's device for a new document — see `journal::Proposal`.
    pub fn propose(&self, world_id: &str, author: &Identity, content: &[u8]) -> Result<String> {
        let at = now();
        self.with_journal(world_id, |journal| Ok(journal.propose(author, content, at)?))
    }

    /// The new documents members asked for: the proposal's id, who asked,
    /// when, and what — where this device may open it.
    pub fn proposals(&self, world_id: &str, me: &Identity) -> Result<Vec<(String, IdentityPublic, u64, Option<Vec<u8>>)>> {
        self.with_journal(world_id, |journal| {
            Ok(journal.proposals().map(|(id, p)| (id.clone(), p.author.clone(), p.at, p.open(me.seal_secret()))).collect())
        })
    }

    /// Proposals answered: gone, everywhere — the game master's.
    pub fn forget_proposals(&self, world_id: &str, ids: Vec<String>) -> Result<()> {
        if !ids.is_empty() {
            self.control(world_id, Item::Deleted { records: ids }, "")?;
        }
        Ok(())
    }

    /// The dice the world rolls with, by sides — empty for the usual set.
    pub fn dice(&self, world_id: &str) -> Result<Vec<u32>> {
        self.with_journal(world_id, |journal| Ok(journal.dice()))
    }

    /// Sets the dice the world rolls with — the game master's.
    pub fn set_dice(&self, world_id: &str, sides: Vec<u32>) -> Result<()> {
        if self.dice(world_id)? != sides {
            self.control(world_id, Item::Dice { sides }, "")?;
        }
        Ok(())
    }

    /// Sets how many versions of each document stay — and keeps to it.
    pub fn set_version_limit(&self, world_id: &str, max: u32) -> Result<()> {
        if max == 0 {
            return Err(StoreError::Journal("At least one version stays.".into()));
        }
        if self.version_limit(world_id)? != max {
            self.control(world_id, Item::Versions { max }, "")?;
        }
        self.prune(world_id, true)
    }

    /// Deletes the versions of a session — the game master's. The newest
    /// stays, and so does what waits.
    pub fn delete_version(&self, world_id: &str, article: ArticleId, session: &str) -> Result<()> {
        let (ids, removable) = self.with_journal(world_id, |journal| {
            Ok((journal.versions_of(article, session), journal.removable(article)))
        })?;
        if ids.is_empty() {
            return Ok(());
        }
        if ids.iter().any(|id| !removable.contains(id)) {
            return Err(StoreError::Journal("The newest version stays.".into()));
        }
        self.control(world_id, Item::Deleted { records: ids }, "")?;
        self.restart(world_id, article)
    }

    /// Lets go of what something newer replaced and nothing needs any more —
    /// the game master's, for everyone. Returns how many records went.
    pub fn tidy(&self, world_id: &str) -> Result<usize> {
        if !self.is_gm(world_id) {
            return Ok(0);
        }
        let gone = self.with_journal(world_id, |journal| Ok(journal.superseded()))?;
        // Each deletion reads the world anew: as few as fit in a record.
        for piece in gone.chunks(200_000) {
            self.control(world_id, Item::Deleted { records: piece.to_vec() }, "")?;
        }
        Ok(gone.len())
    }

    /// Keeps each document to the limit — on the game master's device. The
    /// oldest versions go once a document has half as many again; `now`
    /// keeps to it at once. Records something newer replaced go along.
    pub fn prune(&self, world_id: &str, now: bool) -> Result<()> {
        if !self.is_gm(world_id) {
            return Ok(());
        }
        let (gone, articles) = self.with_journal(world_id, |journal| {
            let max = journal.version_limit() as usize;
            let over = if now { max } else { max + max.div_ceil(2) };
            let mut gone = Vec::new();
            let mut articles = Vec::new();
            for article in journal.over(over) {
                let excess = journal.kept_versions(article).len().saturating_sub(max);
                let oldest: Vec<String> = journal.removable(article).into_iter().take(excess).collect();
                if !oldest.is_empty() {
                    gone.extend(oldest);
                    articles.push(article);
                }
            }
            if !gone.is_empty() {
                gone.extend(journal.superseded());
            }
            Ok((gone, articles))
        })?;
        if gone.is_empty() {
            return Ok(());
        }
        self.control(world_id, Item::Deleted { records: gone }, "")?;
        for article in articles {
            self.restart(world_id, article)?;
        }
        Ok(())
    }

    /// A fresh start for a document: each part begins a new generation with
    /// what it holds now and the same rights — so versions from here on no
    /// longer carry the history before. Not while versions wait: they build
    /// on the history.
    fn restart(&self, world_id: &str, article: ArticleId) -> Result<()> {
        let parts = self.with_journal(world_id, |journal| {
            if journal.has_waiting(article) {
                return Ok(Vec::new());
            }
            let mut out = Vec::new();
            for (layer, generation) in journal.parts_of(article) {
                out.push((layer, journal.access_at(layer, generation)?));
            }
            Ok(out)
        })?;
        for (layer, (access, level)) in parts {
            let view: Policy = access.own.parse().map_err(|_| StoreError::Journal("A part without a readable view.".into()))?;
            self.rekey(world_id, layer, &view, access, level, "")?;
        }
        Ok(())
    }

    /// Where a version stands with the game master.
    pub fn review(&self, world_id: &str, session: &str) -> Result<Review> {
        self.with_journal(world_id, |journal| Ok(journal.session_review(session)))
    }

    // --- Handing the world over ----------------------------------------

    /// To whom the game master hands the world over, once they show up.
    pub fn pending_handover(&self, world_id: &str) -> Result<Option<String>> {
        let path = self.private_world_dir(world_id).join("handover.json");
        if !path.exists() {
            return Ok(None);
        }
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }

    pub fn set_handover(&self, world_id: &str, to: Option<&str>) -> Result<()> {
        let path = self.private_world_dir(world_id).join("handover.json");
        match to {
            Some(to) => aw_store::durable::write(&path, &serde_json::to_vec(to)?)?,
            None if path.exists() => std::fs::remove_file(path)?,
            None => {}
        }
        Ok(())
    }

    /// The whole world — token, keyring, members, every record — sealed for
    /// `to` alone. Whatever carries it sees nothing but ciphertext.
    pub fn handover_pack(&self, world_id: &str, to: &IdentityPublic) -> Result<HandoverPackage> {
        if !self.is_gm(world_id) {
            return Err(StoreError::Journal("Only the game master can hand the world over.".into()));
        }
        let local = self.load_local(world_id)?;
        if !local.members.iter().any(|m| m.identity.id == to.id) {
            return Err(StoreError::Journal("The world goes only to a member.".into()));
        }
        // What one wrote goes along as versions.
        let me = self.load_identity()?;
        self.with_journal(world_id, |journal| {
            for (session, _) in journal.open_sessions(me.id().as_str()) {
                journal.close(&session, &me)?;
            }
            Ok(())
        })?;
        let dir = self.private_world_dir(world_id);
        let handover = Handover {
            token: std::fs::read_to_string(dir.join("owner.key"))?,
            keyring: self.load_keyring(world_id)?.to_json()?,
            members: local.members,
            invited: local.invited,
            requests: self.load_requests(world_id)?,
            records: self.all_records(world_id)?,
        };
        let key = ContentKey::generate()?;
        Ok(HandoverPackage {
            world: world_id.to_string(),
            to: to.id.to_string(),
            key: seal_key(&to.seal_public()?, &key)?,
            data: hex::encode(encrypt(&key, &serde_json::to_vec(&handover)?)?),
        })
    }

    /// Takes over a world handed to `me`: from now on this device is its game
    /// master, and whoever was becomes a player. Every key the former game
    /// master no longer needs turns over.
    pub fn handover_take(&self, package: &HandoverPackage, me: &Identity) -> Result<String> {
        if package.to != me.id().as_str() {
            return Err(StoreError::Journal("This world was handed to someone else.".into()));
        }
        let key = unseal_key(me.seal_secret(), &package.key)?;
        let raw = hex::decode(&package.data).map_err(|e| StoreError::Journal(e.to_string()))?;
        let handover: Handover = serde_json::from_slice(&decrypt(&key, &raw)?)?;
        let world = package.world.as_str();

        self.receive(world, &handover.records)?;
        let public = self.load_local(world)?.public;
        let token = WorldToken::from_json(handover.token.as_bytes())?;
        if token.public().owner_key != public.owner_key || token.world_id() != &public.world_id {
            return Err(StoreError::Journal("The world token does not belong to this world.".into()));
        }
        let ring = Keyring::from_json(&handover.keyring)?;
        let dir = self.private_world_dir(world);
        aw_store::durable::write(&dir.join("keyring.json"), ring.to_json()?.as_bytes())?;
        self.save_token(&token)?;

        let mut former = Vec::new();
        let mut members: Vec<Member> = handover
            .members
            .into_iter()
            .filter(|m| m.identity.id != *me.id())
            .map(|mut m| {
                if m.role == Role::Gm {
                    m.role = Role::Player;
                    m.attributes.retain(|a| a != "gm");
                    former.push(m.attributes.clone());
                }
                m
            })
            .collect();
        members.push(Member {
            identity: me.public(),
            role: Role::Gm,
            attributes: vec!["gm".into(), "players".into(), "all".into()],
        });
        let owner = self.load_local(world)?.owner;
        self.save_local(&Local { version: STORE_VERSION, public, owner, members, invited: handover.invited })?;
        self.save_requests(world, &handover.requests)?;

        // The journal names the new game master; the members get their keys from them.
        let meta = self.load_world_meta(world)?;
        self.save_world_meta(&meta)?;
        let mut ring = self.load_keyring(world)?;
        let kept: Vec<String> = former.into_iter().flatten().collect();
        let dropped: Vec<aw_model::Attribute> = ring.attributes().filter(|a| !kept.contains(&a.to_string())).cloned().collect();
        for attribute in dropped {
            ring.rotate(&attribute)?;
        }
        self.save_keyring(world, &ring)?;
        self.reseal(world)?;
        Ok(world.to_string())
    }

    /// Whether the world went to someone else: this device holds the token,
    /// but the journal names another game master.
    pub fn handed_over(&self, world_id: &str) -> Result<bool> {
        if !self.is_gm(world_id) {
            return Ok(false);
        }
        let me = self.load_identity()?;
        self.with_journal(world_id, |journal| {
            Ok(journal.world_item().is_some_and(|(.., gm)| gm.id != *me.id()))
        })
    }

    /// Steps down as game master: token, keyring and what only the game
    /// master keeps leave this device. The journal stays — from now on read
    /// with the keys the new game master grants.
    pub fn step_down(&self, world_id: &str) -> Result<()> {
        let dir = self.private_world_dir(world_id);
        for file in ["owner.key", "keyring.json", "requests.json", "handover.json"] {
            if dir.join(file).exists() {
                std::fs::remove_file(dir.join(file))?;
            }
        }
        let mut local = self.load_local(world_id)?;
        local.members.clear();
        local.invited.clear();
        self.save_local(&local)?;
        let mut settings = self.load_network()?;
        if settings.shared.iter().any(|w| w == world_id) {
            settings.shared.retain(|w| w != world_id);
            self.save_network(&settings)?;
        }
        Ok(())
    }

    // --- Network -------------------------------------------------------

    pub fn load_network(&self) -> Result<NetworkSettings> {
        let path = self.profile.join("network.json");
        if !path.exists() {
            return Ok(NetworkSettings::default());
        }
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }

    pub fn save_network(&self, settings: &NetworkSettings) -> Result<()> {
        std::fs::create_dir_all(&self.profile)?;
        aw_store::durable::write(&self.profile.join("network.json"), &serde_json::to_vec_pretty(settings)?)?;
        Ok(())
    }

    pub fn load_requests(&self, world_id: &str) -> Result<Vec<JoinRequest>> {
        let path = self.private_world_dir(world_id).join("requests.json");
        if !path.exists() {
            return Ok(Vec::new());
        }
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }

    pub fn save_requests(&self, world_id: &str, requests: &[JoinRequest]) -> Result<()> {
        aw_store::durable::write(&self.private_world_dir(world_id).join("requests.json"), &serde_json::to_vec_pretty(requests)?)?;
        Ok(())
    }

    /// How this device's member is shown in the world: their colour — on
    /// their name and on the documents assigned to them (`journal::Badge`).
    pub fn set_badge(&self, world_id: &str, colour: &str) -> Result<()> {
        let me = self.load_identity()?;
        let of: Vec<String> = self
            .load_world_meta(world_id)?
            .member(me.id().as_str())
            .map(|m| m.attributes.iter().filter(|a| a.starts_with("doc:")).cloned().collect())
            .unwrap_or_default();
        self.with_journal(world_id, |journal| {
            journal.badge(&me, colour, of, now())?;
            Ok(())
        })
    }

    /// Members' colours by the terms that name them: `player:<name>`, and
    /// the documents their badges name — one two badges name, by neither.
    pub fn colours(&self, world_id: &str) -> Result<std::collections::BTreeMap<String, String>> {
        if !self.holds(world_id) {
            return Ok(Default::default());
        }
        self.with_journal(world_id, |journal| {
            let mut out = std::collections::BTreeMap::new();
            let mut claimed: std::collections::BTreeMap<String, Vec<String>> = Default::default();
            for badge in journal.badges() {
                out.insert(format!("player:{}", badge.author.name.to_lowercase()), badge.colour.clone());
                for doc in &badge.of {
                    claimed.entry(doc.clone()).or_default().push(badge.colour.clone());
                }
            }
            for (doc, colours) in claimed {
                if colours.len() == 1 {
                    out.insert(doc, colours[0].clone());
                }
            }
            Ok(out)
        })
    }

    /// Ids that take a player's place once they are let in: new id → old id.
    /// For someone whose device was set up anew without their keys.
    pub fn takeovers(&self, world_id: &str) -> Result<std::collections::BTreeMap<String, String>> {
        let path = self.private_world_dir(world_id).join(TAKEOVERS);
        if !path.exists() {
            return Ok(Default::default());
        }
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }

    /// `new` takes `old`'s place once let in — or no longer, with `None`.
    pub fn set_takeover(&self, world_id: &str, new: &str, old: Option<&str>) -> Result<()> {
        let mut all = self.takeovers(world_id)?;
        match old {
            Some(old) => all.insert(new.to_string(), old.to_string()),
            None => all.remove(new),
        };
        aw_store::durable::write(&self.private_world_dir(world_id).join(TAKEOVERS), &serde_json::to_vec_pretty(&all)?)?;
        Ok(())
    }
}

/// See [`Store::takeovers`].
pub const TAKEOVERS: &str = "takeovers.json";

/// An article as it lies on disk: each layer sealed with its data key — as
/// far as this device can open it.
fn stored(article: journal::ArticleState) -> Result<StoredArticle> {
    let mut layers = Vec::new();
    for layer in article.layers {
        let sealed = match layer.open {
            Some((dek, access, plain)) => dek.seal(&serde_json::to_vec(&LayerContent::of(access, plain))?)?,
            None => String::new(),
        };
        layers.push(StoredLayer {
            id: layer.head.layer,
            header: layer.head.header,
            level: layer.head.level,
            sealed,
            is_base: layer.head.is_base,
            plain: layer.head.plain,
        });
    }
    Ok(StoredArticle { id: article.article, article_type: article.article_type, slug: article.slug, layers })
}

#[cfg(test)]
mod tests;

mod descriptor;

mod images;
pub use images::{named_images, GalleryImage};
