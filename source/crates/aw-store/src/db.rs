//! A world as a plain database: world format 5.
//!
//! ```text
//! <world>/world.sqlite    everything that is said, in plain columns
//! <world>/media/          the pictures, as ordinary files
//! ```
//!
//! Nothing here is encrypted or packed: whoever opens `world.sqlite` with any
//! SQLite program reads the world. Nested things — a type, the calendar, the
//! navigation — stand there as JSON text.
//!
//! Who sees what is not kept by keys but asked of the one who holds the
//! world: `Reader` says what a member may see and change, and every reading
//! that leaves the holder goes through it (`Document::seen_by`).
//!
//! Two who write at once: a save names the text it started from, and
//! `merge` puts what was written meanwhile and what is saved now together,
//! letter by letter.

use aw_model::{Attribute, Policy, Template};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// The world format this module writes and reads.
pub const FORMAT: u32 = 5;
/// The database's name in a world's folder.
pub const FILE: &str = "world.sqlite";
/// The pictures' folder in a world's folder.
pub const MEDIA: &str = "media";
/// Versions kept per document when the game master has not said otherwise.
pub const VERSION_LIMIT: u32 = 50;

fn copy_members(conn: &Connection) -> Result<Vec<String>> {
    Ok(conn.prepare("SELECT id FROM members")?.query_map([], |r| r.get(0))?.collect::<std::result::Result<_, _>>()?)
}

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS world(key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS members(
        id TEXT PRIMARY KEY, name TEXT NOT NULL, role TEXT NOT NULL, attributes TEXT NOT NULL DEFAULT '[]',
        colour TEXT NOT NULL DEFAULT '', trusted INTEGER NOT NULL DEFAULT 0, verify_key TEXT NOT NULL DEFAULT '',
        slot TEXT NOT NULL DEFAULT '', joined INTEGER NOT NULL DEFAULT 0);
    CREATE TABLE IF NOT EXISTS roles(name TEXT PRIMARY KEY, label TEXT NOT NULL DEFAULT '');
    CREATE TABLE IF NOT EXISTS types(id TEXT PRIMARY KEY, position INTEGER NOT NULL, definition TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS documents(
        id TEXT PRIMARY KEY, type TEXT NOT NULL DEFAULT '', slug TEXT NOT NULL DEFAULT '',
        created INTEGER NOT NULL, created_by TEXT NOT NULL, changed INTEGER NOT NULL, changed_by TEXT NOT NULL,
        proposed INTEGER NOT NULL DEFAULT 0);
    CREATE TABLE IF NOT EXISTS layers(
        document TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE, id TEXT NOT NULL,
        position INTEGER NOT NULL, base INTEGER NOT NULL DEFAULT 0, section TEXT NOT NULL,
        title TEXT NOT NULL DEFAULT '', view TEXT NOT NULL, edit TEXT NOT NULL,
        text TEXT NOT NULL DEFAULT '', fields TEXT NOT NULL DEFAULT '{}', images TEXT NOT NULL DEFAULT '[]',
        PRIMARY KEY(document, id));
    CREATE TABLE IF NOT EXISTS versions(
        id INTEGER PRIMARY KEY, document TEXT NOT NULL, session TEXT NOT NULL, author TEXT NOT NULL,
        started INTEGER NOT NULL, ended INTEGER NOT NULL, arrived INTEGER NOT NULL,
        review TEXT NOT NULL DEFAULT 'counted', content TEXT NOT NULL, signature TEXT NOT NULL DEFAULT '',
        UNIQUE(document, session));
    CREATE INDEX IF NOT EXISTS versions_document ON versions(document, ended);
    CREATE TABLE IF NOT EXISTS images(
        id TEXT PRIMARY KEY, file TEXT NOT NULL, preview TEXT NOT NULL DEFAULT '', name TEXT NOT NULL DEFAULT '',
        folder TEXT NOT NULL DEFAULT '', mime TEXT NOT NULL, width INTEGER NOT NULL DEFAULT 0, height INTEGER NOT NULL DEFAULT 0,
        size INTEGER NOT NULL DEFAULT 0, uploaded INTEGER NOT NULL DEFAULT 0, uploaded_by TEXT NOT NULL DEFAULT '',
        defaults TEXT NOT NULL DEFAULT '{}');
    CREATE TABLE IF NOT EXISTS folders(id TEXT PRIMARY KEY, name TEXT NOT NULL, parent TEXT NOT NULL DEFAULT '');
    CREATE TABLE IF NOT EXISTS requests(id TEXT PRIMARY KEY, name TEXT NOT NULL, verify_key TEXT NOT NULL, seal_key TEXT NOT NULL DEFAULT '', at INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS invited(id TEXT PRIMARY KEY);
";

/// Who may see and change something, as written: `players`, `gm`,
/// `class:rogue OR gm`. An empty or unreadable rule is the game master's.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Access {
    pub view: String,
    pub edit: String,
}

impl Default for Access {
    fn default() -> Self {
        Access { view: "all".into(), edit: "all".into() }
    }
}

/// A member's place in the world.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Gm,
    /// Lets players in, gives and takes their roles — see the draft of
    /// 26 October 2026, section 2.
    Supporter,
    #[default]
    Player,
}

impl Role {
    fn as_str(self) -> &'static str {
        match self {
            Role::Gm => "gm",
            Role::Supporter => "supporter",
            Role::Player => "player",
        }
    }

    fn parse(text: &str) -> Role {
        match text {
            "gm" => Role::Gm,
            "supporter" => Role::Supporter,
            _ => Role::Player,
        }
    }
}

/// A member of the world.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    /// Their identity's id; empty for a place not yet taken (`slot`).
    pub id: String,
    pub name: String,
    pub role: Role,
    /// Roles given to them, as written: `class:rogue`, `doc:01ABC`.
    pub attributes: Vec<String>,
    pub colour: String,
    /// Their writing counts without waiting for the game master.
    pub trusted: bool,
    /// Their identity's public signing key, hex — checks what they sign.
    pub verify_key: String,
    /// The name of their mailbox in a world without a server.
    pub slot: String,
    pub joined: u64,
}

impl Member {
    /// What names a player by themselves: `player:<name>`.
    pub fn own_attribute(&self) -> Option<String> {
        let own = format!("player:{}", self.name.trim().to_lowercase());
        own.parse::<Attribute>().is_ok().then_some(own)
    }
}

/// What one reader may see and change — the one place views are decided.
#[derive(Clone, Debug)]
pub struct Reader {
    held: BTreeSet<Attribute>,
    gm: bool,
    /// Each rule asked about, and the answer: reading a world asks the same
    /// few rules for every document and part, tens of thousands of times.
    asked: std::sync::Arc<Mutex<std::collections::HashMap<String, bool>>>,
}

/// More rules than this asked of one reader: they are asked afresh.
const ASKED: usize = 20_000;

impl Reader {
    /// The game master: everything.
    pub fn gm() -> Self {
        Reader { held: [Attribute::All, Attribute::Players, Attribute::Gm].into_iter().collect(), gm: true, asked: Default::default() }
    }

    /// A member, with every role they hold.
    pub fn of(member: &Member) -> Self {
        if member.role == Role::Gm {
            return Reader::gm();
        }
        let mut held: BTreeSet<Attribute> = [Attribute::All, Attribute::Players].into_iter().collect();
        held.extend(member.attributes.iter().filter_map(|a| a.parse().ok()));
        if let Some(own) = member.own_attribute().and_then(|a| a.parse().ok()) {
            held.insert(own);
        }
        Reader { held, gm: false, asked: Default::default() }
    }

    /// A reader holding exactly these roles — a preview of what a player sees.
    pub fn holding(attributes: &[String]) -> Self {
        let mut held: BTreeSet<Attribute> = [Attribute::All, Attribute::Players].into_iter().collect();
        held.extend(attributes.iter().filter_map(|a| a.parse().ok()));
        Reader { held, gm: false, asked: Default::default() }
    }

    pub fn is_gm(&self) -> bool {
        self.gm
    }

    /// What tells two readers apart: two with the same key see the same.
    pub fn key(&self) -> String {
        if self.gm {
            return "gm".into();
        }
        self.held.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(" ")
    }

    /// Whether a rule lets this reader in. What does not read is the game
    /// master's alone.
    pub fn allows(&self, rule: &str) -> bool {
        if self.gm {
            return true;
        }
        let mut asked = self.asked.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(&known) = asked.get(rule) {
            return known;
        }
        let answer = rule.parse::<Policy>().map(|p| p.satisfied_by(&self.held)).unwrap_or(false);
        if asked.len() >= ASKED {
            asked.clear();
        }
        asked.insert(rule.to_string(), answer);
        answer
    }

    pub fn sees(&self, access: &Access) -> bool {
        self.allows(&access.view)
    }

    pub fn may_edit(&self, access: &Access) -> bool {
        self.allows(&access.view) && self.allows(&access.edit)
    }
}

/// A document in the trash: deleted by someone who may edit it, there for
/// nobody until the GM brings it back or deletes it for good. Its base
/// layer's field — the document, its texts and versions stay as they are.
pub const TRASHED: &str = "trashed";

/// When a document went to the trash, and who put it there.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trashed {
    pub at: u64,
    pub by: String,
}

/// The section of a document's cover: what those who may not open it know of
/// it — an alias as its `title`, and in its access who may follow links to it.
pub const COVER: &str = "cover";

/// A layer of a document: a stretch of it with its own access. The base
/// layer carries the document's name, who sees it as a whole and its own
/// settings; the others its parts and its property values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: String,
    /// The document's own: its name (`title`), its view and edit rule.
    pub base: bool,
    /// The section of the type it belongs to — `properties` for the property
    /// values, `properties:<key>` for one with access of its own.
    pub section: String,
    pub title: String,
    pub access: Access,
    pub text: String,
    /// Field key → value: property values, a base layer's own settings.
    pub fields: serde_json::Map<String, serde_json::Value>,
    /// The pictures it shows, by id.
    pub images: Vec<String>,
}

/// A document, whole or as one reader sees it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    /// Its type by id — empty for a document without one.
    pub doc_type: String,
    pub slug: String,
    /// The base layer first, then the others in their order.
    pub layers: Vec<Layer>,
    pub created: u64,
    pub created_by: String,
    pub changed: u64,
    pub changed_by: String,
    /// Asked for by a player, not taken in by the game master yet.
    pub proposed: bool,
}

impl Document {
    pub fn base(&self) -> Option<&Layer> {
        self.layers.iter().find(|l| l.base)
    }

    /// When it went to the trash and by whom — see `TRASHED`.
    pub fn trashed(&self) -> Option<Trashed> {
        serde_json::from_value(self.base()?.fields.get(TRASHED)?.clone()).ok()
    }

    /// Its name, as the base layer says — or, seen from outside, its cover.
    pub fn name(&self) -> &str {
        self.base().or_else(|| self.cover()).map(|l| l.title.as_str()).unwrap_or_default()
    }

    pub fn cover(&self) -> Option<&Layer> {
        self.layers.iter().find(|l| l.section == COVER)
    }

    /// Whether this is no more than the cover of a document the reader may
    /// not open.
    pub fn is_cover(&self) -> bool {
        self.base().is_none() && self.cover().is_some()
    }

    /// The document as `reader` may see it — `None` where they may not see
    /// it at all: whoever does not see its base does not see it, not even
    /// where a part of it would be theirs. Layers they may not see are not
    /// there. Who sees its cover gets that alone.
    pub fn seen_by(&self, reader: &Reader) -> Option<Document> {
        // In the trash it is there for nobody — the GM finds it in the trash.
        if self.trashed().is_some() {
            return None;
        }
        // A player's copy holds no more than the cover where that is all they get.
        if !self.base().is_some_and(|b| reader.sees(&b.access)) {
            return self.cover_for(reader);
        }
        let mut seen = self.clone();
        seen.layers.retain(|l| reader.sees(&l.access));
        Some(seen)
    }

    /// The cover alone, under the alias or else the name — without type or
    /// slug, which would tell more.
    fn cover_for(&self, reader: &Reader) -> Option<Document> {
        let mut cover = self.cover().filter(|l| reader.sees(&l.access))?.clone();
        if cover.title.trim().is_empty() {
            cover.title = self.base()?.title.clone();
        }
        if cover.title.trim().is_empty() {
            return None;
        }
        cover.text.clear();
        cover.fields.clear();
        cover.images.clear();
        Some(Document { doc_type: String::new(), slug: String::new(), layers: vec![cover], proposed: false, ..self.clone() })
    }
}

/// One editing session on a document, as it ended.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Version {
    pub document: String,
    pub session: String,
    pub author: String,
    pub started: u64,
    pub ended: u64,
    pub arrived: u64,
    /// `counted`, `waiting` (for the game master) or `rejected`.
    pub review: String,
    /// The parts and properties as the session left them.
    pub content: VersionContent,
    pub signature: String,
}

/// What a version holds: the layers it changed, whole — as it left them,
/// and as they were before it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VersionContent {
    /// As the session left them; a layer it removed is not here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<Layer>,
    /// As they were the first time the session touched them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before: Vec<Layer>,
    /// The layers it made, by id — there was nothing before.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<String>,
}

impl Version {
    /// The version as `reader` may see it: only the layers they may see —
    /// `None` where nothing of it is left.
    pub fn seen_by(&self, document: &Document, reader: &Reader) -> Option<Version> {
        reader.sees(&document.base()?.access).then_some(())?;
        let mut seen = self.clone();
        seen.content.layers.retain(|l| reader.sees(&l.access));
        seen.content.before.retain(|l| reader.sees(&l.access));
        let ids: BTreeSet<String> = seen.content.layers.iter().chain(&seen.content.before).map(|l| l.id.clone()).collect();
        seen.content.added.retain(|id| ids.contains(id));
        (!ids.is_empty()).then_some(seen)
    }
}

/// A picture of the world: its file in `media/`, and what the gallery says of it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Image {
    pub id: String,
    /// The original, without its metadata — a file name in `media/`.
    pub file: String,
    /// A small picture for the gallery — empty where there is none.
    pub preview: String,
    pub name: String,
    pub folder: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub size: u64,
    pub uploaded: u64,
    pub uploaded_by: String,
    /// How it is put into a text, as JSON — see `media::ImageDefaults`.
    pub defaults: serde_json::Value,
}

/// A folder of the gallery; `parent` empty: at the top.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub parent: String,
}

/// Someone who asked to join, waiting.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    pub name: String,
    /// Their identity's public keys, hex — what their id is made of.
    pub verify_key: String,
    pub seal_key: String,
    pub at: u64,
}

/// A world's database.
pub struct WorldDb {
    dir: PathBuf,
    conn: Mutex<Connection>,
    /// Counts up with every change of a document — for what is worked out
    /// from all of them and kept until something changes.
    revision: std::sync::atomic::AtomicU64,
    /// Counts up with every write of anything.
    writes: std::sync::atomic::AtomicU64,
    /// Which document each write changed — `None`: what changes how every
    /// document is seen, the types. For whoever asks what changed since.
    journal: Mutex<Journal>,
    /// This opening of the world: what was counted in another means nothing here.
    epoch: u64,
}

/// What changed, write by write — the newest so many.
#[derive(Default)]
struct Journal {
    entries: std::collections::VecDeque<(u64, Option<String>)>,
    /// The last write no longer noted: before it, nothing can be said.
    forgotten: u64,
}

/// How many writes the journal holds.
const JOURNAL: usize = 20_000;

/// What changed since a write.
#[derive(Debug, PartialEq)]
pub enum Since {
    /// These documents — made, changed or gone.
    Documents(Vec<String>),
    /// Too much, or what changes how all of them are seen: everything anew.
    Whole,
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn json<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}

fn from_json<T: for<'de> Deserialize<'de> + Default>(text: &str) -> T {
    serde_json::from_str(text).unwrap_or_default()
}

/// A world at a glance — see `WorldDb::glance`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Glance {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub documents: u64,
    pub members: u64,
}

/// The columns of a document's own row, as `document_from` reads them.
const DOCUMENT_COLUMNS: &str = "id, type, slug, created, created_by, changed, changed_by, proposed";
/// The columns of a layer's row, as `layer_from` reads them.
const LAYER_COLUMNS: &str = "id, base, section, title, view, edit, text, fields, images";

/// A document's own row, without its layers.
fn document_from(r: &rusqlite::Row) -> rusqlite::Result<Document> {
    Ok(Document {
        id: r.get(0)?,
        doc_type: r.get(1)?,
        slug: r.get(2)?,
        layers: Vec::new(),
        created: r.get(3)?,
        created_by: r.get(4)?,
        changed: r.get(5)?,
        changed_by: r.get(6)?,
        proposed: r.get(7)?,
    })
}

/// A layer's row, its columns from `at` on.
fn layer_from(r: &rusqlite::Row, at: usize) -> rusqlite::Result<Layer> {
    Ok(Layer {
        id: r.get(at)?,
        base: r.get(at + 1)?,
        section: r.get(at + 2)?,
        title: r.get(at + 3)?,
        access: Access { view: r.get(at + 4)?, edit: r.get(at + 5)? },
        text: r.get(at + 6)?,
        fields: from_json(&r.get::<_, String>(at + 7)?),
        images: from_json(&r.get::<_, String>(at + 8)?),
    })
}

/// A member's row.
fn version_from(r: &rusqlite::Row) -> rusqlite::Result<Version> {
    Ok(Version {
        document: r.get(0)?,
        session: r.get(1)?,
        author: r.get(2)?,
        started: r.get(3)?,
        ended: r.get(4)?,
        arrived: r.get(5)?,
        review: r.get(6)?,
        content: from_json(&r.get::<_, String>(7)?),
        signature: r.get(8)?,
    })
}

fn member_from(r: &rusqlite::Row) -> rusqlite::Result<Member> {
    Ok(Member {
        id: r.get(0)?,
        name: r.get(1)?,
        role: Role::parse(&r.get::<_, String>(2)?),
        attributes: from_json(&r.get::<_, String>(3)?),
        colour: r.get(4)?,
        trusted: r.get(5)?,
        verify_key: r.get(6)?,
        slot: r.get(7)?,
        joined: r.get(8)?,
    })
}

/// An image's row.
fn image_from(r: &rusqlite::Row) -> rusqlite::Result<Image> {
    Ok(Image {
        id: r.get(0)?,
        file: r.get(1)?,
        preview: r.get(2)?,
        name: r.get(3)?,
        folder: r.get(4)?,
        mime: r.get(5)?,
        width: r.get(6)?,
        height: r.get(7)?,
        size: r.get(8)?,
        uploaded: r.get(9)?,
        uploaded_by: r.get(10)?,
        defaults: serde_json::from_str(&r.get::<_, String>(11)?).unwrap_or_default(),
    })
}

const MEMBER_COLUMNS: &str = "id, name, role, attributes, colour, trusted, verify_key, slot, joined";
const VERSION_COLUMNS: &str = "document, session, author, started, ended, arrived, review, content, signature";
/// The same, with nothing in its content.
const VERSION_BARE: &str = "document, session, author, started, ended, arrived, review, '{}', signature";
const IMAGE_COLUMNS: &str = "id, file, preview, name, folder, mime, width, height, size, uploaded, uploaded_by, defaults";

impl WorldDb {
    /// Makes a new world in `dir`, which must not hold one yet.
    pub fn create(dir: &Path, id: &str, name: &str, gm: &Member) -> Result<Self> {
        if dir.join(FILE).exists() {
            return Err(format!("a world lies in {} already", dir.display()).into());
        }
        std::fs::create_dir_all(dir.join(MEDIA))?;
        let db = Self::open_at(dir)?;
        {
            let conn = db.conn();
            conn.execute_batch(SCHEMA)?;
        }
        db.set("format", &FORMAT.to_string())?;
        db.set("id", id)?;
        db.set("name", name)?;
        db.put_member(&Member { role: Role::Gm, joined: now(), ..gm.clone() })?;
        Ok(db)
    }

    /// Opens the world in `dir`.
    pub fn open(dir: &Path) -> Result<Self> {
        if !dir.join(FILE).exists() {
            return Err(format!("no world in {}", dir.display()).into());
        }
        let db = Self::open_at(dir)?;
        let format: u32 = db.get("format")?.and_then(|f| f.parse().ok()).unwrap_or(0);
        if format != FORMAT {
            return Err(format!("world format {format}, this app reads {FORMAT}").into());
        }
        Ok(db)
    }

    fn open_at(dir: &Path) -> Result<Self> {
        let conn = Connection::open(dir.join(FILE))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA synchronous=NORMAL;")?;
        // In microseconds: a number JavaScript holds exactly, as it travels.
        let epoch = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_micros() as u64).unwrap_or(1);
        Ok(WorldDb { dir: dir.to_path_buf(), conn: Mutex::new(conn), revision: Default::default(), writes: Default::default(), journal: Default::default(), epoch })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The id of the world a database file holds — read without opening
    /// it for writing.
    pub fn id_in(file: &Path) -> Result<Option<String>> {
        let conn = Connection::open_with_flags(file, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        Ok(conn.query_row("SELECT value FROM world WHERE key = 'id'", [], |r| r.get(0)).optional()?)
    }

    /// What a database file says of its world at a glance — read without
    /// opening it for writing, while another process may write it.
    pub fn glance(file: &Path) -> Result<Glance> {
        let conn = Connection::open_with_flags(file, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
        conn.busy_timeout(std::time::Duration::from_millis(200))?;
        Ok(conn.query_row(
            "SELECT (SELECT value FROM world WHERE key = 'id'), (SELECT value FROM world WHERE key = 'name'),
                    (SELECT value FROM world WHERE key = 'icon'), (SELECT count(*) FROM documents), (SELECT count(*) FROM members)",
            [],
            |r| Ok(Glance { id: r.get::<_, Option<String>>(0)?.unwrap_or_default(), name: r.get::<_, Option<String>>(1)?.unwrap_or_default(), icon: r.get::<_, Option<String>>(2)?.unwrap_or_default(), documents: r.get(3)?, members: r.get(4)? }),
        )?)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// How far the documents are — counts up with every change of one.
    pub fn revision(&self) -> u64 {
        self.revision.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn changed(&self) {
        self.revision.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// How often anything was written since the world was opened.
    pub fn writes(&self) -> u64 {
        self.writes.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn wrote(&self) {
        self.writes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// A write that changed a document — or, with `None`, how all are seen.
    /// Counted once it is done: what is read meanwhile counts as before it.
    fn noted(&self, document: Option<&str>) {
        let mut journal = self.journal.lock().unwrap_or_else(|e| e.into_inner());
        let n = self.writes.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        journal.entries.push_back((n, document.map(str::to_string)));
        while journal.entries.len() > JOURNAL {
            if let Some((gone, _)) = journal.entries.pop_front() {
                journal.forgotten = gone;
            }
        }
    }

    /// This opening of the world — see `changed_since`.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// What changed after write `since` of this opening, and how far that
    /// goes. Before what the journal holds, or after a restart: everything.
    pub fn changed_since(&self, since: u64) -> (u64, Since) {
        let journal = self.journal.lock().unwrap_or_else(|e| e.into_inner());
        let now = self.writes();
        if since > now || since < journal.forgotten {
            return (now, Since::Whole);
        }
        let mut ids: Vec<String> = Vec::new();
        for (_, document) in journal.entries.iter().filter(|(n, _)| *n > since) {
            match document {
                None => return (now, Since::Whole),
                Some(id) if !ids.contains(id) => ids.push(id.clone()),
                Some(_) => {}
            }
        }
        (now, Since::Documents(ids))
    }

    /// Something that changes how every document is seen, said from outside.
    pub fn changed_all(&self) {
        self.noted(None);
    }

    // --- The world's own settings -------------------------------------------

    pub fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self.conn().query_row("SELECT value FROM world WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        self.wrote();
        self.conn().execute("INSERT INTO world(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value", params![key, value])?;
        Ok(())
    }

    /// A setting kept as JSON — the calendar, the navigation.
    pub fn get_json<T: for<'de> Deserialize<'de> + Default>(&self, key: &str) -> Result<T> {
        Ok(self.get(key)?.map(|t| from_json(&t)).unwrap_or_default())
    }

    pub fn set_json<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.set(key, &json(value)?)
    }

    // --- Members ------------------------------------------------------------

    pub fn members(&self) -> Result<Vec<Member>> {
        let conn = self.conn();
        let mut q = conn.prepare(&format!(
            "SELECT {MEMBER_COLUMNS} FROM members ORDER BY CASE role WHEN 'gm' THEN 0 WHEN 'supporter' THEN 1 ELSE 2 END, joined, rowid"
        ))?;
        let rows = q.query_map([], member_from)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// One member — asked on every command, so by its key alone.
    pub fn member(&self, id: &str) -> Result<Option<Member>> {
        Ok(self.conn().query_row(&format!("SELECT {MEMBER_COLUMNS} FROM members WHERE id = ?1"), [id], member_from).optional()?)
    }

    /// Adds a member or changes one; a place without an id is found by its slot.
    pub fn put_member(&self, member: &Member) -> Result<()> {
        self.wrote();
        let key = if member.id.is_empty() { format!("slot:{}", member.slot) } else { member.id.clone() };
        self.conn().execute(
            "INSERT INTO members(id, name, role, attributes, colour, trusted, verify_key, slot, joined)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, role = excluded.role, attributes = excluded.attributes,
                colour = excluded.colour, trusted = excluded.trusted, verify_key = excluded.verify_key, slot = excluded.slot",
            params![key, member.name, member.role.as_str(), json(&member.attributes)?, member.colour, member.trusted, member.verify_key, member.slot, if member.joined == 0 { now() } else { member.joined }],
        )?;
        Ok(())
    }

    pub fn remove_member(&self, id: &str) -> Result<()> {
        self.wrote();
        self.conn().execute("DELETE FROM members WHERE id = ?1", [id])?;
        Ok(())
    }

    /// The reader a member is — `None` for someone who is not one.
    pub fn reader(&self, id: &str) -> Result<Option<Reader>> {
        Ok(self.member(id)?.map(|m| Reader::of(&m)))
    }

    // --- Types --------------------------------------------------------------

    pub fn types(&self) -> Result<Vec<Template>> {
        let conn = self.conn();
        let mut q = conn.prepare("SELECT definition FROM types ORDER BY position")?;
        let rows = q.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_str(&row?)?);
        }
        Ok(out)
    }

    /// Puts the types as they are now, in their order.
    pub fn save_types(&self, types: &[Template]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM types", [])?;
        for (i, t) in types.iter().enumerate() {
            tx.execute("INSERT INTO types(id, position, definition) VALUES(?1, ?2, ?3)", params![t.id, i as i64, json(t)?])?;
        }
        tx.commit()?;
        drop(conn);
        self.noted(None);
        Ok(())
    }

    // --- Documents ----------------------------------------------------------

    /// Ids of every document, in the order they were made.
    pub fn document_ids(&self) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut q = conn.prepare("SELECT id FROM documents ORDER BY created, id")?;
        let rows = q.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn document(&self, id: &str) -> Result<Option<Document>> {
        let conn = self.conn();
        let head = conn.query_row(&format!("SELECT {DOCUMENT_COLUMNS} FROM documents WHERE id = ?1"), [id], document_from).optional()?;
        let Some(mut doc) = head else { return Ok(None) };
        let mut q = conn.prepare(&format!("SELECT {LAYER_COLUMNS} FROM layers WHERE document = ?1 ORDER BY base DESC, position"))?;
        doc.layers = q.query_map([id], |r| layer_from(r, 0))?.collect::<std::result::Result<_, _>>()?;
        Ok(Some(doc))
    }

    /// Every document, whole — read in two queries, not one per document.
    pub fn document_count(&self) -> Result<usize> {
        let n: i64 = self.conn().query_row("SELECT COUNT(*) FROM documents", [], |r| r.get(0))?;
        Ok(n as usize)
    }

    /// The documents in the trash, each with its layers.
    pub fn trashed_documents(&self) -> Result<Vec<Document>> {
        self.load(
            "WHERE id IN (SELECT document FROM layers WHERE base = 1 AND json_extract(fields, '$.trashed') IS NOT NULL) ORDER BY created, id",
            params![],
        )
    }

    /// The pictures named in the parts `reader` may see, each with the
    /// documents naming it — asked of the parts that name any, not of whole
    /// documents. A part counts where its document is there for the reader:
    /// not in the trash, its base seen — a cover alone shows no picture.
    pub fn image_uses_for(&self, reader: &Reader) -> Result<std::collections::BTreeMap<String, BTreeSet<String>>> {
        let conn = self.conn();
        let mut q = conn.prepare(
            "SELECT l.document, l.view, l.images, b.view FROM layers l
             JOIN layers b ON b.document = l.document AND b.base = 1
             WHERE l.images != '[]' AND json_extract(b.fields, '$.trashed') IS NULL",
        )?;
        let mut uses: std::collections::BTreeMap<String, BTreeSet<String>> = Default::default();
        let rows = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)))?;
        for row in rows {
            let (document, view, images, base) = row?;
            if !reader.allows(&base) || !reader.allows(&view) {
                continue;
            }
            for image in from_json::<Vec<String>>(&images) {
                uses.entry(image).or_default().insert(document.clone());
            }
        }
        Ok(uses)
    }

    /// Every picture some version of a document holds — what was there once.
    pub fn images_in_versions(&self) -> Result<BTreeSet<String>> {
        let conn = self.conn();
        let mut q = conn.prepare("SELECT content FROM versions")?;
        let mut all = BTreeSet::new();
        for content in q.query_map([], |r| r.get::<_, String>(0))? {
            let content: VersionContent = from_json(&content?);
            for layer in content.layers.iter().chain(&content.before) {
                all.extend(layer.images.iter().cloned());
            }
        }
        Ok(all)
    }

    pub fn documents(&self) -> Result<Vec<Document>> {
        self.load("ORDER BY created, id", params![])
    }

    /// The next `count` documents after the one with id `after`, by id —
    /// a world read a part at a time, from where the last part ended.
    pub fn documents_after(&self, after: &str, count: usize) -> Result<Vec<Document>> {
        self.load("WHERE id > ?1 ORDER BY id LIMIT ?2", params![after, count as i64])
    }

    /// The same without the texts of their parts — but those of the
    /// section `kept` and its own sections (`kept:…`): an outline of the
    /// world, read without what it says.
    pub fn outlines_after(&self, after: &str, count: usize, kept: &str) -> Result<Vec<Document>> {
        self.load_with("WHERE id > ?1 ORDER BY id LIMIT ?2", params![after, count as i64], Some(kept))
    }

    /// Documents as `rest` picks them, each with its layers.
    fn load(&self, rest: &str, args: impl rusqlite::Params) -> Result<Vec<Document>> {
        self.load_with(rest, args, None)
    }

    /// `load`, the texts left out but those of the section `kept` names.
    fn load_with(&self, rest: &str, args: impl rusqlite::Params, kept: Option<&str>) -> Result<Vec<Document>> {
        let conn = self.conn();
        let mut q = conn.prepare(&format!("SELECT {DOCUMENT_COLUMNS} FROM documents {rest}"))?;
        let mut docs: Vec<Document> = q.query_map(args, document_from)?.collect::<std::result::Result<_, _>>()?;
        let (Some(low), Some(high)) = (docs.iter().map(|d| d.id.clone()).min(), docs.iter().map(|d| d.id.clone()).max()) else {
            return Ok(docs);
        };
        let at: std::collections::HashMap<String, usize> = docs.iter().enumerate().map(|(i, d)| (d.id.clone(), i)).collect();
        let columns = match kept {
            Some(_) => LAYER_COLUMNS.replace(", text,", ", CASE WHEN section = ?3 OR section LIKE ?3 || ':%' THEN text ELSE '' END,"),
            None => LAYER_COLUMNS.to_string(),
        };
        let mut q = conn.prepare(&format!(
            "SELECT document, {columns} FROM layers WHERE document BETWEEN ?1 AND ?2 ORDER BY document, base DESC, position"
        ))?;
        let values = [low, high].into_iter().chain(kept.map(str::to_string));
        let rows = q.query_map(rusqlite::params_from_iter(values), |r| Ok((r.get::<_, String>(0)?, layer_from(r, 1)?)))?;
        for row in rows {
            let (doc, layer) = row?;
            if let Some(&i) = at.get(&doc) {
                docs[i].layers.push(layer);
            }
        }
        Ok(docs)
    }

    /// A copy of the database as it stands, whole, in one file — while the
    /// world goes on being written.
    pub fn copy_to(&self, file: &Path) -> Result<()> {
        if file.exists() {
            std::fs::remove_file(file)?;
        }
        self.conn().execute("VACUUM INTO ?1", [file.to_string_lossy()])?;
        Ok(())
    }

    /// A copy of the world as `reader` sees it, into `dir` (its database
    /// alone): the documents and parts they see, the members named in
    /// `members`, the pictures named in `images`; no versions, no requests,
    /// nothing let in ahead of time, no gallery folders, nobody's takeovers.
    pub fn view_copy(&self, dir: &Path, reader: &Reader, members: &[String], images: &BTreeSet<String>) -> Result<()> {
        std::fs::create_dir_all(dir)?;
        let file = dir.join(FILE);
        self.copy_to(&file)?;
        let seen = self.documents_for(reader)?;
        let copy = WorldDb::open(dir)?;
        {
            let conn = copy.conn();
            conn.execute_batch("DELETE FROM documents; DELETE FROM layers; DELETE FROM versions; DELETE FROM requests; DELETE FROM invited; DELETE FROM folders; DELETE FROM world WHERE key = 'takeovers';")?;
            for id in copy_members(&conn)?.into_iter().filter(|id| !members.contains(id)) {
                conn.execute("DELETE FROM members WHERE id = ?1", [&id])?;
            }
            let all: Vec<String> = conn.prepare("SELECT id FROM images")?.query_map([], |r| r.get(0))?.collect::<std::result::Result<_, _>>()?;
            for id in all.into_iter().filter(|id| !images.contains(id)) {
                conn.execute("DELETE FROM images WHERE id = ?1", [&id])?;
            }
        }
        for doc in &seen {
            copy.put_document(doc)?;
        }
        copy.into_file()
    }

    /// Lets go of a copy as one file of its own — compacted, without a
    /// write-ahead log beside it.
    pub fn into_file(self) -> Result<()> {
        self.conn().execute_batch("VACUUM; PRAGMA journal_mode = DELETE;")?;
        Ok(())
    }

    /// Writes what the write-ahead log holds into the database file — so a
    /// copy of the folder is the world as it stands.
    pub fn checkpoint(&self) -> Result<()> {
        self.conn().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }

    /// Every document as `reader` may see it.
    pub fn documents_for(&self, reader: &Reader) -> Result<Vec<Document>> {
        Ok(self.documents()?.into_iter().filter_map(|d| d.seen_by(reader)).collect())
    }

    /// Writes a document whole, as it is now — made or changed.
    pub fn put_document(&self, doc: &Document) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO documents(id, type, slug, created, created_by, changed, changed_by, proposed)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET type = excluded.type, slug = excluded.slug,
                changed = excluded.changed, changed_by = excluded.changed_by, proposed = excluded.proposed",
            params![doc.id, doc.doc_type, doc.slug, doc.created, doc.created_by, doc.changed, doc.changed_by, doc.proposed],
        )?;
        tx.execute("DELETE FROM layers WHERE document = ?1", [&doc.id])?;
        for (i, l) in doc.layers.iter().enumerate() {
            tx.execute(
                "INSERT INTO layers(document, id, position, base, section, title, view, edit, text, fields, images)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    doc.id, l.id, i as i64, l.base, l.section, l.title, l.access.view, l.access.edit,
                    l.text, json(&l.fields)?, json(&l.images)?
                ],
            )?;
        }
        tx.commit()?;
        drop(conn);
        self.changed();
        self.noted(Some(&doc.id));
        Ok(())
    }

    /// A document goes for good — its layers and its versions with it, at once.
    pub fn delete_document(&self, id: &str) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM documents WHERE id = ?1", [id])?;
        tx.execute("DELETE FROM versions WHERE document = ?1", [id])?;
        tx.commit()?;
        drop(conn);
        self.changed();
        self.noted(Some(id));
        Ok(())
    }

    /// Id, name and type of each document `reader` may see — from their
    /// bases alone, without the rest of them. A document they know only by
    /// its cover is there under its alias, without a type.
    pub fn names_for(&self, reader: &Reader) -> Result<Vec<(String, String, String)>> {
        Ok(self
            .bases_and_covers()?
            .into_iter()
            .filter_map(|(id, name, kind, access, cover)| {
                if reader.sees(&access) {
                    return Some((id, name, kind));
                }
                let (alias, _) = cover.filter(|(_, c)| reader.sees(c))?;
                Some((id, if alias.trim().is_empty() { name } else { alias }, String::new()))
            })
            .collect())
    }

    /// Each alias `reader` may know, with who may open its document: a link
    /// to it keeps the alias as its text (see `aw_model::names`).
    pub fn aliases_for(&self, reader: &Reader) -> Result<Vec<(String, String, Access)>> {
        Ok(self
            .bases_and_covers()?
            .into_iter()
            .filter_map(|(id, _, _, access, cover)| {
                let (alias, cover) = cover?;
                (!alias.trim().is_empty() && (reader.sees(&access) || reader.sees(&cover))).then_some((id, alias, access))
            })
            .collect())
    }

    /// Each document's id, name, type and access, with its cover's alias and access.
    #[allow(clippy::type_complexity)]
    fn bases_and_covers(&self) -> Result<Vec<(String, String, String, Access, Option<(String, Access)>)>> {
        let conn = self.conn();
        let mut q = conn.prepare(
            "SELECT d.id, l.title, d.type, l.view, l.edit, c.title, c.view, c.edit FROM documents d
             JOIN layers l ON l.document = d.id AND l.base = 1
             LEFT JOIN layers c ON c.document = d.id AND c.section = ?1 AND c.base = 0
             WHERE json_extract(l.fields, '$.trashed') IS NULL
             ORDER BY d.created, d.id",
        )?;
        let rows = q.query_map([COVER], |r| {
            let cover = match r.get::<_, Option<String>>(5)? {
                Some(alias) => Some((alias, Access { view: r.get(6)?, edit: r.get(7)? })),
                None => None,
            };
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, Access { view: r.get(3)?, edit: r.get(4)? }, cover))
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// What `reader` wrote over a document's layers, merged into it as it stands.
    ///
    /// `started` holds the layers as the reader's editor began — as they saw
    /// them; `saved` what they save now. Of each layer they may change, what
    /// they changed since `started` goes in — title, text, each field — and
    /// what others wrote meanwhile stays, letter by letter. Layers they may
    /// not see or change stay untouched, whatever is sent; layers are neither
    /// made nor removed here, nor is anyone's access changed. Returns the
    /// document as it is now.
    pub fn save(&self, id: &str, reader: &Reader, by: &str, started: &[Layer], saved: &[Layer]) -> Result<Document> {
        let mut doc = self.document(id)?.ok_or_else(|| format!("no document {id}"))?;
        if !doc.base().is_some_and(|b| reader.sees(&b.access)) {
            return Err("this document is not there for you".into());
        }
        for layer in saved {
            let Some(now) = doc.layers.iter_mut().find(|l| l.id == layer.id) else { continue };
            if !reader.may_edit(&now.access) {
                continue;
            }
            let Some(base) = started.iter().find(|l| l.id == layer.id) else { continue };
            if layer.title != base.title {
                now.title = merge(&base.title, &layer.title, &now.title);
            }
            if layer.text != base.text {
                now.text = merge(&base.text, &layer.text, &now.text);
            }
            let keys: BTreeSet<&String> = layer.fields.keys().chain(base.fields.keys()).collect();
            for key in keys {
                let (was, is) = (base.fields.get(key), layer.fields.get(key));
                if was == is {
                    continue;
                }
                match (was, is, now.fields.get(key)) {
                    (Some(serde_json::Value::String(b)), Some(serde_json::Value::String(o)), Some(serde_json::Value::String(t))) => {
                        let merged = merge(b, o, t);
                        now.fields.insert(key.clone(), serde_json::Value::String(merged));
                    }
                    (_, Some(value), _) => {
                        now.fields.insert(key.clone(), value.clone());
                    }
                    (_, None, _) => {
                        now.fields.remove(key);
                    }
                }
            }
            for image in &layer.images {
                if !now.images.contains(image) {
                    now.images.push(image.clone());
                }
            }
            now.images.retain(|i| layer.images.contains(i) || !base.images.contains(i));
        }
        doc.changed = now();
        doc.changed_by = by.to_string();
        self.put_document(&doc)?;
        Ok(doc)
    }

    // --- Pictures -----------------------------------------------------------

    pub fn images(&self) -> Result<Vec<Image>> {
        let conn = self.conn();
        let mut q = conn.prepare(&format!("SELECT {IMAGE_COLUMNS} FROM images ORDER BY uploaded, id"))?;
        let rows = q.query_map([], image_from)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// One image — asked for every picture shown, so by its key alone.
    pub fn image(&self, id: &str) -> Result<Option<Image>> {
        Ok(self.conn().query_row(&format!("SELECT {IMAGE_COLUMNS} FROM images WHERE id = ?1"), [id], image_from).optional()?)
    }

    pub fn put_image(&self, i: &Image) -> Result<()> {
        self.wrote();
        self.conn().execute(
            "INSERT INTO images(id, file, preview, name, folder, mime, width, height, size, uploaded, uploaded_by, defaults)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(id) DO UPDATE SET file = excluded.file, preview = excluded.preview, name = excluded.name,
                folder = excluded.folder, mime = excluded.mime, width = excluded.width, height = excluded.height,
                size = excluded.size, defaults = excluded.defaults",
            params![i.id, i.file, i.preview, i.name, i.folder, i.mime, i.width, i.height, i.size, i.uploaded, i.uploaded_by, json(&i.defaults)?],
        )?;
        Ok(())
    }

    /// Takes a picture out, its files too — the row first: a file left over
    /// is harmless, a row naming a file that is gone is not.
    pub fn delete_image(&self, id: &str) -> Result<()> {
        self.wrote();
        let image = self.image(id)?;
        self.conn().execute("DELETE FROM images WHERE id = ?1", [id])?;
        for file in image.into_iter().flat_map(|i| [i.file, i.preview]).filter(|f| !f.is_empty()) {
            let path = self.dir.join(MEDIA).join(file);
            if path.exists() {
                std::fs::remove_file(path)?;
            }
        }
        Ok(())
    }

    pub fn folders(&self) -> Result<Vec<Folder>> {
        let conn = self.conn();
        let mut q = conn.prepare("SELECT id, name, parent FROM folders ORDER BY rowid")?;
        let rows = q.query_map([], |r| Ok(Folder { id: r.get(0)?, name: r.get(1)?, parent: r.get(2)? }))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn save_folders(&self, folders: &[Folder]) -> Result<()> {
        self.wrote();
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM folders", [])?;
        for f in folders {
            tx.execute("INSERT INTO folders(id, name, parent) VALUES(?1, ?2, ?3)", params![f.id, f.name, f.parent])?;
        }
        tx.commit()?;
        Ok(())
    }

    // --- Who would come in -----------------------------------------------------

    pub fn requests(&self) -> Result<Vec<Request>> {
        let conn = self.conn();
        let mut q = conn.prepare("SELECT id, name, verify_key, seal_key, at FROM requests ORDER BY at")?;
        let rows = q.query_map([], |r| Ok(Request { id: r.get(0)?, name: r.get(1)?, verify_key: r.get(2)?, seal_key: r.get(3)?, at: r.get(4)? }))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn put_request(&self, r: &Request) -> Result<()> {
        self.wrote();
        self.conn().execute(
            "INSERT INTO requests(id, name, verify_key, seal_key, at) VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, verify_key = excluded.verify_key, seal_key = excluded.seal_key",
            params![r.id, r.name, r.verify_key, r.seal_key, r.at],
        )?;
        Ok(())
    }

    pub fn remove_request(&self, id: &str) -> Result<()> {
        self.wrote();
        self.conn().execute("DELETE FROM requests WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Ids let in ahead of time: whoever proves one becomes a member without asking.
    pub fn invited(&self) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut q = conn.prepare("SELECT id FROM invited ORDER BY rowid")?;
        let rows = q.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn set_invited(&self, ids: &[String]) -> Result<()> {
        self.wrote();
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM invited", [])?;
        for id in ids {
            tx.execute("INSERT OR IGNORE INTO invited(id) VALUES(?1)", [id])?;
        }
        tx.commit()?;
        Ok(())
    }

    // --- Versions -----------------------------------------------------------

    /// Keeps an editing session as a version — or, while it goes on, its newest state.
    pub fn put_version(&self, v: &Version) -> Result<()> {
        self.conn().execute(
            "INSERT INTO versions(document, session, author, started, ended, arrived, review, content, signature)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(document, session) DO UPDATE SET ended = excluded.ended, review = excluded.review,
                content = excluded.content, signature = excluded.signature",
            params![v.document, v.session, v.author, v.started, v.ended, v.arrived, v.review, json(&v.content)?, v.signature],
        )?;
        // What waits for the GM stands in its author's view of the document.
        self.noted(Some(&v.document));
        Ok(())
    }

    /// A document's versions, newest last.
    pub fn versions(&self, document: &str) -> Result<Vec<Version>> {
        let conn = self.conn();
        let mut q = conn.prepare(&format!("SELECT {VERSION_COLUMNS} FROM versions WHERE document = ?1 ORDER BY ended, id"))?;
        let rows = q.query_map([document], version_from)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// The versions waiting for the game master, in every document — those
    /// of `author` alone, or everyone's. Without what they hold: asked for
    /// a list and a count, not to be read.
    pub fn waiting(&self, author: Option<&str>) -> Result<Vec<Version>> {
        let conn = self.conn();
        let mut q = conn.prepare(&format!(
            "SELECT {VERSION_BARE} FROM versions WHERE review = 'waiting' AND (?1 IS NULL OR author = ?1) ORDER BY ended DESC, id"
        ))?;
        let rows = q.query_map([author], version_from)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// How many versions wait for the game master — `author`'s, or everyone's.
    pub fn waiting_count(&self, author: Option<&str>) -> Result<usize> {
        let n: i64 = self.conn().query_row("SELECT COUNT(*) FROM versions WHERE review = 'waiting' AND (?1 IS NULL OR author = ?1)", [author], |r| r.get(0))?;
        Ok(n as usize)
    }

    /// One version goes.
    pub fn delete_version(&self, document: &str, session: &str) -> Result<()> {
        self.conn().execute("DELETE FROM versions WHERE document = ?1 AND session = ?2", [document, session])?;
        self.noted(Some(document));
        Ok(())
    }

    /// Every version of a document goes.
    pub fn delete_versions(&self, document: &str) -> Result<()> {
        self.conn().execute("DELETE FROM versions WHERE document = ?1", [document])?;
        self.noted(Some(document));
        Ok(())
    }

    /// Keeps the newest `keep` versions of a document, the others go.
    pub fn trim_versions(&self, document: &str, keep: usize) -> Result<usize> {
        self.wrote();
        let gone = self.conn().execute(
            "DELETE FROM versions WHERE document = ?1 AND review = 'counted' AND id NOT IN
               (SELECT id FROM versions WHERE document = ?1 ORDER BY ended DESC, id DESC LIMIT ?2)",
            params![document, keep as i64],
        )?;
        Ok(gone)
    }
}

/// Puts two writings of one text together: `ours` and `theirs` both grew out
/// of `base`. What each changed stays — where both changed the same spot,
/// both are kept, side by side. Letter by letter, as Automerge merges.
pub fn merge(base: &str, ours: &str, theirs: &str) -> String {
    if ours == base {
        return theirs.to_string();
    }
    if theirs == base || ours == theirs {
        return ours.to_string();
    }
    let attempt = || -> std::result::Result<String, automerge::AutomergeError> {
        use automerge::transaction::Transactable;
        use automerge::{AutoCommit, ObjType, ReadDoc, ROOT};
        let mut start = AutoCommit::new();
        let text = start.put_object(ROOT, "t", ObjType::Text)?;
        start.splice_text(&text, 0, 0, base)?;
        let mut mine = start.fork();
        let mut other = start.fork();
        for (doc, target) in [(&mut mine, ours), (&mut other, theirs)] {
            // Letter by letter, the fewest: what one did not touch stays where
            // the other wrote beside it.
            for s in crate::diff::edits(base, target) {
                doc.splice_text(&text, s.at, s.delete as isize, &s.insert)?;
            }
        }
        mine.merge(&mut other)?;
        mine.text(&text)
    };
    attempt().unwrap_or_else(|_| ours.to_string())
}

#[cfg(test)]
#[path = "db_tests.rs"]
mod tests;
