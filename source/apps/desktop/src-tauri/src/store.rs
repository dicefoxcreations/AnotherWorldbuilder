//! Storage on disk.
//!
//! ```text
//! <profile>/identity.json          identity and private keys
//! <profile>/avatar.json            the profile's picture
//! <profile>/network.json           network settings
//! <profile>/worlds/<world>/        what this device alone keeps of a world
//! <worlds>/<world>/world.sqlite    the world, in plain columns (`aw_store::db`)
//! <worlds>/<world>/media/          its pictures, as ordinary files
//! ```
//!
//! A world lives where it was made: in one of the folders this device keeps
//! worlds in, served by this app's own server — to itself alone, or to the
//! members over the network — or on a server elsewhere, where this device
//! holds nothing of it. Nothing is encrypted at rest: who sees what is the
//! holder's to decide, by `Reader`.

use aw_crypto::{Identity, IdentityPublic};
use aw_model::navigation::NavEntry;
use aw_model::{Template, WorldCalendar};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

pub use aw_store::db::{Access, Document, Folder, Image, Layer, Member, Reader, Role, Version, VersionContent, WorldDb};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("file not readable or writable: {0}")]
    Io(#[from] std::io::Error),

    #[error("file is malformed: {0}")]
    Format(#[from] serde_json::Error),

    #[error(transparent)]
    Crypto(#[from] aw_crypto::CryptoError),

    #[error("world {0} does not exist here")]
    NoSuchWorld(String),

    #[error("{0}")]
    Other(String),
}

impl From<Box<dyn std::error::Error + Send + Sync>> for StoreError {
    fn from(e: Box<dyn std::error::Error + Send + Sync>) -> Self {
        StoreError::Other(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, StoreError>;

fn err(text: impl Into<String>) -> StoreError {
    StoreError::Other(text.into())
}

/// Default icon of a world.
pub const WORLD_ICON: &str = "🌍";

/// A world's settings as the app works with them.
#[derive(Clone, Debug)]
pub struct WorldMeta {
    pub id: String,
    pub name: String,
    /// The world's emoji. Freely chosen, the default is the globe.
    pub icon: String,
    /// The world's home page — a document like any other.
    pub home: String,
    pub calendar: WorldCalendar,
    pub members: Vec<Member>,
    /// Ids let in ahead of time.
    pub invited: Vec<String>,
}

impl WorldMeta {
    pub fn member(&self, id: &str) -> Option<&Member> {
        self.members.iter().find(|m| m.id == id)
    }

    pub fn gm(&self) -> Option<&Member> {
        self.members.iter().find(|m| m.role == Role::Gm)
    }
}

/// What this device does on the network. Everything is off until switched
/// on — the network as a whole, and then each world by itself.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkSettings {
    #[serde(default)]
    pub enabled: bool,
    /// The worlds this app's server offers to their members, by id.
    #[serde(default)]
    pub shared: Vec<String>,
    /// Reachable from the internet: the router's port, STUN, IPv6.
    #[serde(default)]
    pub internet: bool,
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
    /// The worlds worked in on a server — or on a GM's app — by id: this
    /// device holds nothing of them but where they are.
    #[serde(default)]
    pub remote: BTreeMap<String, RemoteWorld>,
    /// The GM's worlds without a server, by id: the folder their mailboxes
    /// lie in, which the cloud app keeps in step.
    #[serde(default)]
    pub posts: BTreeMap<String, String>,
    /// The worlds this device takes part in through a mailbox, by id.
    #[serde(default)]
    pub mailboxes: BTreeMap<String, Mailbox>,
}

/// A player's mailbox in a world without a server.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Mailbox {
    /// The folder shared with this player: `ein/` and `aus/` in it.
    pub folder: String,
}

/// A world on a server, as this device knows of it.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct RemoteWorld {
    pub name: String,
    #[serde(default)]
    pub icon: String,
    /// The server's code.
    pub server: String,
    /// Asked to join; not let in yet.
    #[serde(default)]
    pub asked: bool,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        NetworkSettings {
            enabled: false,
            shared: Vec::new(),
            internet: false,
            port: default_port(),
            stun: stun_servers(),
            servers: Vec::new(),
            remote: BTreeMap::new(),
            posts: BTreeMap::new(),
            mailboxes: BTreeMap::new(),
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
        format!("data:{};base64,{}", self.media_type, base64(&raw))
    }
}

/// A small Base64 encoder without an extra dependency.
pub fn base64(bytes: &[u8]) -> String {
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

/// Milliseconds since 1970.
pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// A fresh world id: `world:<16 hex>`, at random.
pub fn new_world_id() -> Result<String> {
    let seed: [u8; 32] = aw_crypto::random_bytes()?;
    Ok(aw_crypto::Id::derive("world", &[&seed]).to_string())
}

// --- Moving worlds ----------------------------------------------------------

/// How far moving a world is — for the interface to show.
fn moving_worlds() -> &'static Mutex<std::collections::HashSet<String>> {
    static MOVING: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    MOVING.get_or_init(Default::default)
}

/// Whether a world is being moved or deleted right now.
pub fn moving(world_id: &str) -> bool {
    moving_worlds().lock().is_ok_and(|all| all.contains(world_id))
}

/// A world noted as moving while this lives: closed to every command, and
/// to the members asking for it — they are told it moves.
pub(crate) struct Away(String);

impl Away {
    pub(crate) fn start(world_id: &str) -> Result<Self> {
        moving_worlds().lock().map_err(|_| err("a lock was poisoned"))?.insert(world_id.to_string());
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
            if path.is_dir() {
                dirs.push(path)
            } else {
                files.push(path)
            }
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

// --- Acting for a member ----------------------------------------------------

thread_local! {
    /// The member a command runs for, where it is not this device's own:
    /// this app's server answering a member over the network.
    static ACTING: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Runs `f` for the member `id` — what it reads and writes is theirs to.
pub fn acting_for<T>(id: &str, f: impl FnOnce() -> T) -> T {
    let before = ACTING.with(|a| a.replace(Some(id.to_string())));
    let out = f();
    ACTING.with(|a| *a.borrow_mut() = before);
    out
}

/// The member a command runs for, where it is not this device's own.
pub fn acting() -> Option<String> {
    ACTING.with(|a| a.borrow().clone())
}

/// Types as `reader` may see them — see `Store::templates_for`.
pub fn types_for(templates: Vec<Template>, reader: &Reader) -> Vec<Template> {
    if reader.is_gm() {
        return templates;
    }
    let sees = |view: &str| reader.allows(view);
    templates
        .into_iter()
        .map(|mut t| {
            let hidden: Vec<(String, String)> = t
                .properties
                .iter()
                .filter(|f| f.access.as_ref().is_some_and(|a| !sees(&a.view)))
                .map(|f| (f.key.clone(), f.label.clone()))
                .collect();
            t.properties.retain(|f| !hidden.iter().any(|(k, _)| *k == f.key));
            t.infobox.show.retain(|s| !hidden.iter().any(|(k, l)| s.property == *k || s.property == *l));
            t.blocks = std::mem::take(&mut t.blocks)
                .into_iter()
                .filter_map(|mut b| {
                    let had = b.parts.len();
                    b.parts.retain(|p| sees(&p.view));
                    (had == 0 || !b.parts.is_empty()).then_some(b)
                })
                .collect();
            t
        })
        .collect()
}

// --- The store --------------------------------------------------------------

/// The open databases of this process, by the world's folder.
fn open_worlds() -> &'static Mutex<HashMap<PathBuf, Arc<WorldDb>>> {
    static OPEN: OnceLock<Mutex<HashMap<PathBuf, Arc<WorldDb>>>> = OnceLock::new();
    OPEN.get_or_init(Default::default)
}

/// Where the data lies: the profile — identity, avatar, settings — and the
/// worlds, in one or more folders.
#[derive(Clone, Debug)]
pub struct Store {
    profile: PathBuf,
    worlds: PathBuf,
    roots: Vec<PathBuf>,
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
        Store { profile, worlds, roots }
    }

    pub fn local_roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// New worlds go to the selected root; existing worlds retain their location.
    pub fn destination(&self, root: &Path) -> Result<Self> {
        if !self.roots.iter().any(|p| p == root) {
            return Err(err("Choose a configured world location."));
        }
        if !root.is_dir() {
            return Err(err("The world location is not available."));
        }
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

    /// The folder a world lies in — where it is, or where it would go.
    pub fn world_dir(&self, world_id: &str) -> PathBuf {
        let name = dir_name(world_id);
        self.roots
            .iter()
            .map(|root| root.join(&name))
            .find(|dir| dir.join(aw_store::db::FILE).exists())
            .unwrap_or_else(|| self.worlds_dir().join(name))
    }

    /// Whether this device holds the world.
    pub fn holds(&self, world_id: &str) -> bool {
        self.world_dir(world_id).join(aw_store::db::FILE).exists()
    }

    /// What this device alone keeps of a world, independent of where it lies.
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

    fn avatar_path(&self) -> PathBuf {
        self.profile.join("avatar.json")
    }

    /// Keeps the profile's picture; returns it as a `data:` URL.
    pub fn save_avatar(&self, bytes: &[u8], media_type: &str) -> Result<String> {
        let file = AvatarFile { media_type: media_type.to_string(), data: hex::encode(bytes) };
        aw_store::durable::write(&self.avatar_path(), &serde_json::to_vec(&file)?)?;
        Ok(file.as_data_url())
    }

    pub fn load_avatar(&self) -> Result<Option<String>> {
        let path = self.avatar_path();
        if !path.exists() {
            return Ok(None);
        }
        let file: AvatarFile = serde_json::from_slice(&std::fs::read(path)?)?;
        Ok(Some(file.as_data_url()))
    }

    pub fn remove_avatar(&self) -> Result<()> {
        let path = self.avatar_path();
        if path.exists() {
            std::fs::remove_file(path)?;
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

    // --- Worlds --------------------------------------------------------

    /// The world's database, opened once and kept.
    pub fn db(&self, world_id: &str) -> Result<Arc<WorldDb>> {
        if moving(world_id) {
            return Err(err("The world is being moved."));
        }
        let dir = self.world_dir(world_id);
        if !dir.join(aw_store::db::FILE).exists() {
            return Err(StoreError::NoSuchWorld(world_id.to_string()));
        }
        let mut open = open_worlds().lock().map_err(|_| err("a lock was poisoned"))?;
        if let Some(db) = open.get(&dir) {
            return Ok(db.clone());
        }
        let db = Arc::new(WorldDb::open(&dir)?);
        open.insert(dir, db.clone());
        Ok(db)
    }

    /// Lets go of an open world — it is gone or moves.
    pub(crate) fn close(&self, world_id: &str) {
        if let Ok(mut all) = open_worlds().lock() {
            all.remove(&self.world_dir(world_id));
        }
    }

    /// The worlds lying here, by id.
    pub fn held_worlds(&self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        for root in &self.roots {
            if !root.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(root)? {
                let dir = entry?.path();
                if !dir.join(aw_store::db::FILE).exists() {
                    continue;
                }
                let Ok(db) = WorldDb::open(&dir) else { continue };
                let Some(id) = db.get("id")? else { continue };
                if dir.file_name().and_then(|n| n.to_str()) != Some(dir_name(&id).as_str()) {
                    continue;
                }
                if out.contains(&id) {
                    return Err(err(format!("World {id} exists in more than one storage location. Remove the duplicate location from Storage.")));
                }
                out.push(id);
            }
        }
        Ok(out)
    }

    pub fn list_worlds(&self) -> Result<Vec<WorldMeta>> {
        let mut out: Vec<WorldMeta> = self.held_worlds()?.iter().filter_map(|id| self.load_world_meta(id).ok()).collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    /// A new world here, with `me` as its game master.
    pub fn create_world(&self, name: &str, me: &Identity) -> Result<WorldMeta> {
        let id = new_world_id()?;
        let gm = Member { id: me.id().to_string(), name: me.name().to_string(), role: Role::Gm, verify_key: me.public().verify_key, ..Default::default() };
        let db = WorldDb::create(&self.world_dir(&id), &id, name.trim(), &gm)?;
        db.set("icon", WORLD_ICON)?;
        drop(db);
        self.load_world_meta(&id)
    }

    pub fn load_world_meta(&self, world_id: &str) -> Result<WorldMeta> {
        let db = self.db(world_id)?;
        Ok(WorldMeta {
            id: world_id.to_string(),
            name: db.get("name")?.unwrap_or_default(),
            icon: db.get("icon")?.unwrap_or_else(|| WORLD_ICON.to_string()),
            home: db.get("home")?.unwrap_or_default(),
            calendar: db.get_json("calendar")?,
            members: db.members()?,
            invited: db.invited()?,
        })
    }

    /// Keeps the world's own settings — name, icon, home, calendar, members,
    /// those let in ahead of time.
    pub fn save_world_meta(&self, meta: &WorldMeta) -> Result<()> {
        let db = self.db(&meta.id)?;
        db.set("name", &meta.name)?;
        db.set("icon", &meta.icon)?;
        db.set("home", &meta.home)?;
        db.set_json("calendar", &meta.calendar)?;
        let before = db.members()?;
        for gone in before.iter().filter(|b| !meta.members.iter().any(|m| m.id == b.id)) {
            db.remove_member(&gone.id)?;
        }
        for member in &meta.members {
            db.put_member(member)?;
        }
        db.set_invited(&meta.invited)?;
        Ok(())
    }

    /// Whom a command runs for in a world: the member acted for, or this
    /// device's own identity.
    pub fn actor(&self) -> Result<String> {
        match acting() {
            Some(id) => Ok(id),
            None => Ok(self.load_identity()?.id().to_string()),
        }
    }

    /// What the one a command runs for may see in a world. Whoever is no
    /// member reads nothing.
    pub fn reader(&self, world_id: &str) -> Result<Reader> {
        let me = self.actor()?;
        self.db(world_id)?.reader(&me)?.ok_or_else(|| err("You are not a member of this world."))
    }

    /// Whether the one a command runs for is the world's game master.
    pub fn is_gm(&self, world_id: &str) -> bool {
        self.reader(world_id).is_ok_and(|r| r.is_gm())
    }

    pub fn load_templates(&self, world_id: &str) -> Result<Vec<Template>> {
        Ok(self.db(world_id)?.types()?)
    }

    /// The types as `reader` may see them: a property or a part with a rule
    /// they do not meet is not there for them — not its label, not its
    /// starting text — and a block of such parts alone is not either.
    pub fn templates_for(&self, world_id: &str, reader: &Reader) -> Result<Vec<Template>> {
        Ok(types_for(self.load_templates(world_id)?, reader))
    }

    /// The navigation as `reader` may see it: without the GM's entries.
    pub fn navigation_for(&self, world_id: &str, reader: &Reader) -> Result<Option<Vec<NavEntry>>> {
        Ok(self.navigation(world_id)?.map(|n| if reader.is_gm() { n } else { aw_model::navigation::for_players(&n) }))
    }

    pub fn save_templates(&self, world_id: &str, templates: &[Template]) -> Result<()> {
        Ok(self.db(world_id)?.save_types(templates)?)
    }

    /// The navigation as the game master set it — `None`: not set yet.
    pub fn navigation(&self, world_id: &str) -> Result<Option<Vec<NavEntry>>> {
        let db = self.db(world_id)?;
        Ok(match db.get("navigation")? {
            Some(text) => Some(serde_json::from_str(&text)?),
            None => None,
        })
    }

    pub fn set_navigation(&self, world_id: &str, entries: &[NavEntry]) -> Result<()> {
        Ok(self.db(world_id)?.set_json("navigation", &entries)?)
    }

    /// A setting of the world, read as JSON — `default` where it is not set.
    pub fn setting<T: for<'de> Deserialize<'de>>(&self, world_id: &str, key: &str, default: T) -> Result<T> {
        Ok(match self.db(world_id)?.get(key)? {
            Some(text) => serde_json::from_str(&text).unwrap_or(default),
            None => default,
        })
    }

    pub fn set_setting<T: Serialize>(&self, world_id: &str, key: &str, value: &T) -> Result<()> {
        Ok(self.db(world_id)?.set_json(key, value)?)
    }

    /// The dice the world rolls with, by sides — empty for the usual set.
    pub fn dice(&self, world_id: &str) -> Result<Vec<u32>> {
        self.setting(world_id, "dice", Vec::new())
    }

    /// How many versions of each document stay.
    pub fn version_limit(&self, world_id: &str) -> Result<u32> {
        self.setting(world_id, "version_limit", aw_store::db::VERSION_LIMIT)
    }

    /// Whether players' writing waits for the game master.
    pub fn moderated(&self, world_id: &str) -> Result<bool> {
        self.setting(world_id, "moderated", false)
    }

    /// Whether a server names the world to everyone who asks.
    pub fn on_the_list(&self, world_id: &str) -> Result<bool> {
        self.setting(world_id, "listed", false)
    }

    /// Deletes a world from this device, for good.
    pub fn delete_world(&self, world_id: &str) -> Result<()> {
        let _away = Away::start(world_id)?;
        let dir = self.world_dir(world_id);
        self.close(world_id);
        remove_tree(&dir)?;
        remove_tree(&self.private_world_dir(world_id))?;
        Ok(())
    }

    /// Moves a world into another of this device's folders: copied whole,
    /// then the old one goes.
    pub fn move_world(&self, world_id: &str, root: &Path, tell: &dyn Fn(usize, usize)) -> Result<()> {
        let from = self.world_dir(world_id);
        let to = root.join(dir_name(world_id));
        if from == to {
            return Ok(());
        }
        if to.exists() {
            return Err(err("A world with this id lies there already."));
        }
        let _away = Away::start(world_id)?;
        self.close(world_id);
        copy_dir(&from, &to, tell)?;
        remove_tree(&from)?;
        Ok(())
    }

    /// Members' colours by the terms that name them: `player:<name>`, and
    /// the documents assigned to one member alone.
    pub fn colours(&self, world_id: &str) -> Result<BTreeMap<String, String>> {
        let mut out = BTreeMap::new();
        let mut claimed: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for m in self.db(world_id)?.members()?.into_iter().filter(|m| !m.colour.is_empty()) {
            if let Some(own) = m.own_attribute() {
                out.insert(own, m.colour.clone());
            }
            for doc in m.attributes.iter().filter(|a| a.starts_with("doc:")) {
                claimed.entry(doc.clone()).or_default().push(m.colour.clone());
            }
        }
        for (doc, colours) in claimed {
            if colours.len() == 1 {
                out.insert(doc, colours[0].clone());
            }
        }
        Ok(out)
    }
}

/// The image ids a text or a field names.
pub fn named_images(text: &str) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    let mut rest = text;
    while let Some(at) = rest.find("img:") {
        let tail = &rest[at..];
        let end = tail.find(|c: char| !(c.is_ascii_alphanumeric() || c == ':' || c == '-')).unwrap_or(tail.len());
        if aw_store::media::is_id(&tail[..end]) {
            out.insert(tail[..end].to_string());
        }
        rest = &tail[4..];
    }
    out
}

/// A member of a world as an identity makes them.
pub fn member_of(identity: &IdentityPublic, role: Role) -> Member {
    Member { id: identity.id.to_string(), name: identity.name.clone(), role, verify_key: identity.verify_key.clone(), ..Default::default() }
}

mod images;
pub use images::GalleryImage;
