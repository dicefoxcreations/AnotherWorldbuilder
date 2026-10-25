//! Where a world's records are kept.
//!
//! One SQLite file holds them: the state. Where nobody else writes — a
//! server, a session, this device's own drafts — it changes in place, one
//! transaction after the other (`Mode::Alone`). In a folder others may share
//! — a cloud folder, say, which copies whole files and cannot merge two
//! writers — nothing is ever changed in place (`Mode::Shared`): each device
//! adds what it writes as files of its own (`updates/`), and the game
//! master's device consolidates them now and then into the next state, a new
//! file under a new name. What is only half there — a file still on its way
//! — does not check out and waits.

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Only this process writes: the state changes in place.
    Alone,
    /// Others may write the same folder: files of one's own, consolidated.
    Shared,
}

/// A record as kept: its id, its kind — what the journal orders by — the
/// article it belongs to, when it came, and its bytes, where they were asked
/// for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stored {
    pub id: String,
    pub kind: u8,
    pub article: Option<String>,
    pub at: u64,
    pub raw: Option<Vec<u8>>,
}

const ALONE: &str = "state.sqlite";
const UPDATES: &str = "updates";
const MAGIC: &[u8; 4] = b"AWU1";

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS records(id BLOB NOT NULL UNIQUE, kind INTEGER NOT NULL, article TEXT, at INTEGER NOT NULL, raw BLOB NOT NULL);
    CREATE INDEX IF NOT EXISTS records_article ON records(article) WHERE article IS NOT NULL;
    CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value BLOB NOT NULL);";

/// A state file of a shared folder: `state-<generation>-<hash>.sqlite`, the
/// hash that of its bytes — so whoever reads it knows it is whole.
#[derive(Clone, Debug, PartialEq, Eq)]
struct StateName {
    generation: u64,
    hash: String,
    name: String,
}

impl StateName {
    fn parse(name: &str) -> Option<Self> {
        let rest = name.strip_prefix("state-")?.strip_suffix(".sqlite")?;
        let (generation, hash) = rest.split_once('-')?;
        if generation.len() != 10 || hash.len() != 16 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        Some(StateName { generation: generation.parse().ok()?, hash: hash.to_string(), name: name.to_string() })
    }

    fn new(generation: u64, hash: &str) -> Self {
        let name = format!("state-{generation:010}-{hash}.sqlite");
        StateName { generation, hash: hash.to_string(), name }
    }
}

/// An update file: `updates/<device>-<number>.awu` — numbered without gaps
/// per device, so a state can say how far it holds each device's.
fn update_name(device: &str, number: u64) -> String {
    format!("{device}-{number:010}.awu")
}

fn parse_update_name(name: &str) -> Option<(String, u64)> {
    let (device, number) = name.strip_suffix(".awu")?.rsplit_once('-')?;
    if device.len() != 16 || !device.bytes().all(|b| b.is_ascii_hexdigit()) || number.len() != 10 {
        return None;
    }
    Some((device.to_string(), number.parse().ok()?))
}

/// What an update file holds, if it is whole: its records, by its own
/// checksum at the end.
fn read_update(path: &Path, at: u64) -> Option<Vec<Stored>> {
    let bytes = std::fs::read(path).ok()?;
    let (body, sum) = bytes.split_at_checked(bytes.len().checked_sub(32)?)?;
    if body.get(..4)? != MAGIC || blake3::hash(body).as_bytes() != sum {
        return None;
    }
    let mut out = Vec::new();
    let mut rest = &body[4..];
    let mut take = |n: usize| -> Option<&[u8]> {
        let (head, tail) = rest.split_at_checked(n)?;
        rest = tail;
        Some(head)
    };
    while let Some(id) = take(32) {
        let id = hex::encode(id);
        let kind = take(1)?[0];
        let len = take(1)?[0] as usize;
        let article = (len > 0).then(|| String::from_utf8(take(len)?.to_vec()).ok()).flatten();
        let len = u32::from_le_bytes(take(4)?.try_into().ok()?) as usize;
        let raw = take(len)?.to_vec();
        out.push(Stored { id, kind, article, at, raw: Some(raw) });
    }
    Some(out)
}

fn encode_update(records: &[Stored]) -> Result<Vec<u8>> {
    let mut out = MAGIC.to_vec();
    for r in records {
        let raw = r.raw.as_deref().ok_or("a record without its bytes")?;
        out.extend_from_slice(&hex::decode(&r.id)?);
        out.push(r.kind);
        let article = r.article.as_deref().unwrap_or("");
        out.push(u8::try_from(article.len()).map_err(|_| "an article id too long")?);
        out.extend_from_slice(article.as_bytes());
        out.extend_from_slice(&u32::try_from(raw.len()).map_err(|_| "a record too big")?.to_le_bytes());
        out.extend_from_slice(raw);
    }
    let sum = blake3::hash(&out);
    out.extend_from_slice(sum.as_bytes());
    Ok(out)
}

fn millis(time: std::time::SystemTime) -> u64 {
    time.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn now() -> u64 {
    millis(std::time::SystemTime::now())
}

/// A path as SQLite takes it in a URI — to open a file read-only, without
/// any lock: a state never changes, and a cloud drive may know no locks.
fn uri(path: &Path) -> String {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let text = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from(if text.starts_with('/') { "file:" } else { "file:///" });
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~:".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out.push_str("?immutable=1");
    out
}

fn schema(db: &Connection) -> Result<()> {
    db.execute_batch(SCHEMA)?;
    Ok(())
}

fn included_of(db: &Connection) -> Result<BTreeMap<String, u64>> {
    let raw: Option<Vec<u8>> = db.query_row("SELECT value FROM meta WHERE key = 'included'", [], |r| r.get(0)).optional()?;
    Ok(raw.and_then(|r| serde_json::from_slice(&r).ok()).unwrap_or_default())
}

pub struct Shelf {
    dir: PathBuf,
    private: PathBuf,
    mode: Mode,
    db: Connection,
    /// Shared: the state read — none before the first.
    state: Option<StateName>,
    /// Shared: how far the state holds each device's updates.
    included: BTreeMap<String, u64>,
    /// Shared: the update files taken in, by device and number, with the
    /// ids they held.
    files: BTreeMap<(String, u64), Vec<String>>,
    /// Shared: what update files hold that the state does not.
    loose: HashMap<String, Stored>,
    /// Shared: this device's name in its update files, and its next number.
    device: String,
    next: u64,
    /// When this device last wrote.
    written: Option<std::time::Instant>,
    /// When the folders were last looked at — unchanged, nothing came.
    stamps: (Option<std::time::SystemTime>, Option<std::time::SystemTime>),
}

impl Shelf {
    /// The records of the world in `dir`; this device's own notes on it —
    /// its name, what it checked — in `private`.
    pub fn open(dir: &Path, private: &Path, mode: Mode) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let mut shelf = match mode {
            Mode::Alone => {
                let db = Connection::open(dir.join(ALONE))?;
                db.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;")?;
                schema(&db)?;
                Shelf::with(dir, private, mode, db)
            }
            Mode::Shared => {
                let db = Connection::open_in_memory()?;
                schema(&db)?;
                let mut shelf = Shelf::with(dir, private, mode, db);
                shelf.device = shelf.device_name()?;
                shelf.adopt_newest()?;
                shelf.read_updates()?;
                shelf.next = shelf.next_number();
                shelf
            }
        };
        shelf.stamps = shelf.stamps_now();
        Ok(shelf)
    }

    fn with(dir: &Path, private: &Path, mode: Mode, db: Connection) -> Self {
        Shelf {
            dir: dir.to_path_buf(),
            private: private.to_path_buf(),
            mode,
            db,
            state: None,
            included: BTreeMap::new(),
            files: BTreeMap::new(),
            loose: HashMap::new(),
            device: String::new(),
            next: 1,
            written: None,
            stamps: (None, None),
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// This device's name in the folder — made once, kept privately.
    fn device_name(&self) -> Result<String> {
        let path = self.private.join("device");
        if let Ok(name) = std::fs::read_to_string(&path) {
            let name = name.trim();
            if name.len() == 16 && name.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Ok(name.to_string());
            }
        }
        let seed = format!("{:?}-{}-{}", std::time::SystemTime::now(), std::process::id(), path.display());
        let name = blake3::hash(seed.as_bytes()).to_hex()[..16].to_string();
        crate::durable::write(&path, name.as_bytes())?;
        Ok(name)
    }

    fn next_number(&self) -> u64 {
        let mine = self.files.keys().filter(|(d, _)| *d == self.device).map(|(_, n)| *n);
        let here = self.update_files().into_iter().filter(|(d, _, _)| *d == self.device).map(|(_, n, _)| n);
        mine.chain(here).chain(self.included.get(&self.device).copied()).max().unwrap_or(0) + 1
    }

    fn stamps_now(&self) -> (Option<std::time::SystemTime>, Option<std::time::SystemTime>) {
        let stamp = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        (stamp(&self.dir), stamp(&self.dir.join(UPDATES)))
    }

    /// The state files lying here, newest first.
    fn state_files(&self) -> Vec<StateName> {
        let mut out: Vec<StateName> = std::fs::read_dir(&self.dir)
            .map(|entries| entries.filter_map(|e| StateName::parse(e.ok()?.file_name().to_str()?)).collect())
            .unwrap_or_default();
        out.sort_by(|a, b| (b.generation, &b.name).cmp(&(a.generation, &a.name)));
        out
    }

    /// The update files lying here: device, number, path.
    fn update_files(&self) -> Vec<(String, u64, PathBuf)> {
        let Ok(entries) = std::fs::read_dir(self.dir.join(UPDATES)) else { return Vec::new() };
        entries
            .filter_map(|e| {
                let e = e.ok()?;
                let (device, number) = parse_update_name(e.file_name().to_str()?)?;
                Some((device, number, e.path()))
            })
            .collect()
    }

    /// Whether a state file is whole — its bytes what its name says. Checked
    /// once per device: the answer is kept privately.
    fn whole(&self, state: &StateName) -> bool {
        let path = self.dir.join(&state.name);
        let Ok(meta) = std::fs::metadata(&path) else { return false };
        let mark = format!("{} {}", state.name, meta.len());
        let kept = self.private.join("whole");
        if std::fs::read_to_string(&kept).is_ok_and(|k| k == mark) {
            return true;
        }
        let Ok(mut file) = std::fs::File::open(&path) else { return false };
        let mut hasher = blake3::Hasher::new();
        if hasher.update_reader(&mut file).is_err() || hasher.finalize().to_hex()[..16] != state.hash {
            return false;
        }
        let _ = crate::durable::replace(&kept, mark.as_bytes());
        true
    }

    /// Reads the newest whole state, if it is not the one read already.
    /// Returns the records it holds that were not here before.
    fn adopt_newest(&mut self) -> Result<Vec<Stored>> {
        let Some(newest) = self.state_files().into_iter().find(|s| self.whole(s)) else { return Ok(Vec::new()) };
        if self.state.as_ref() == Some(&newest) {
            return Ok(Vec::new());
        }
        let db = Connection::open_with_flags(uri(&self.dir.join(&newest.name)), OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI)?;
        let before: HashSet<Vec<u8>> = if self.state.is_some() {
            let mut q = self.db.prepare("SELECT id FROM records")?;
            let ids = q.query_map([], |r| r.get::<_, Vec<u8>>(0))?.collect::<std::result::Result<_, _>>()?;
            ids
        } else {
            HashSet::new()
        };
        let mut new = Vec::new();
        if self.state.is_some() {
            let mut q = db.prepare("SELECT id, kind, article, at, raw FROM records")?;
            let rows = q.query_map([], |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, u8>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, i64>(3)?, r.get::<_, Vec<u8>>(4)?)))?;
            for row in rows {
                let (id, kind, article, at, raw) = row?;
                if before.contains(&id) {
                    continue;
                }
                let id = hex::encode(id);
                if self.loose.contains_key(&id) {
                    continue;
                }
                new.push(Stored { id, kind, article, at: at as u64, raw: Some(raw) });
            }
        }
        self.included = included_of(&db)?;
        self.db = db;
        self.state = Some(newest);
        // What the state holds now is loose no more.
        let held: Vec<(String, u64)> = self.files.keys().filter(|(d, n)| self.included.get(d).is_some_and(|i| n <= i)).cloned().collect();
        for key in held {
            for id in self.files.remove(&key).unwrap_or_default() {
                self.loose.remove(&id);
            }
        }
        Ok(new)
    }

    /// Takes in the whole update files not read yet, beyond what the state
    /// holds. Returns their records that are new here.
    fn read_updates(&mut self) -> Result<Vec<Stored>> {
        let mut new = Vec::new();
        let mut files = self.update_files();
        files.sort();
        for (device, number, path) in files {
            if self.files.contains_key(&(device.clone(), number)) || self.included.get(&device).is_some_and(|i| number <= *i) {
                continue;
            }
            let at = std::fs::metadata(&path).and_then(|m| m.modified()).map(millis).unwrap_or_else(|_| now());
            let Some(records) = read_update(&path, at) else { continue };
            let mut ids = Vec::with_capacity(records.len());
            for record in records {
                ids.push(record.id.clone());
                if self.loose.contains_key(&record.id) || self.in_state(&record.id)? {
                    continue;
                }
                self.loose.insert(record.id.clone(), record.clone());
                new.push(record);
            }
            self.files.insert((device, number), ids);
        }
        Ok(new)
    }

    fn in_state(&self, id: &str) -> Result<bool> {
        Ok(self.db.query_row("SELECT 1 FROM records WHERE id = ?1", [hex::decode(id)?], |_| Ok(())).optional()?.is_some())
    }

    /// Every record here — with its bytes where `with_raw` wants them for
    /// its kind; the others are fetched with `get` when needed.
    pub fn all(&self, with_raw: impl Fn(u8) -> bool) -> Result<Vec<Stored>> {
        let mut q = self.db.prepare("SELECT id, kind, article, at, raw FROM records")?;
        let mut kinds: Vec<u8> = (0..=u8::MAX).filter(|k| with_raw(*k)).collect();
        kinds.sort();
        let rows = q.query_map([], |r| {
            let kind: u8 = r.get(1)?;
            let raw = if kinds.binary_search(&kind).is_ok() { Some(r.get::<_, Vec<u8>>(4)?) } else { None };
            Ok(Stored { id: hex::encode(r.get::<_, Vec<u8>>(0)?), kind, article: r.get(2)?, at: r.get::<_, i64>(3)? as u64, raw })
        })?;
        let mut out: Vec<Stored> = rows.collect::<std::result::Result<_, _>>()?;
        for loose in self.loose.values() {
            let mut record = loose.clone();
            if !with_raw(record.kind) {
                record.raw = None;
            }
            out.push(record);
        }
        Ok(out)
    }

    /// What came meanwhile — from other devices sharing the folder: their
    /// update files, or a newer state. Nothing comes in a folder of one's own.
    pub fn fresh(&mut self) -> Result<Vec<Stored>> {
        if self.mode == Mode::Alone {
            return Ok(Vec::new());
        }
        let stamps = self.stamps_now();
        if stamps == self.stamps && stamps.0.is_some() {
            return Ok(Vec::new());
        }
        self.stamps = stamps;
        let mut new = self.adopt_newest()?;
        new.extend(self.read_updates()?);
        Ok(new)
    }

    /// A record's bytes.
    pub fn get(&self, id: &str) -> Option<Vec<u8>> {
        if let Some(loose) = self.loose.get(id) {
            return loose.raw.clone();
        }
        let key = hex::decode(id).ok()?;
        self.db.query_row("SELECT raw FROM records WHERE id = ?1", [key], |r| r.get(0)).optional().ok()?
    }

    /// Keeps records — on the disk when this returns.
    pub fn put(&mut self, records: &[Stored]) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        match self.mode {
            Mode::Alone => {
                let tx = self.db.transaction()?;
                {
                    let mut q = tx.prepare("INSERT OR IGNORE INTO records(id, kind, article, at, raw) VALUES (?1, ?2, ?3, ?4, ?5)")?;
                    for r in records {
                        q.execute(params![hex::decode(&r.id)?, r.kind, r.article, r.at as i64, r.raw.as_deref().ok_or("a record without its bytes")?])?;
                    }
                }
                tx.commit()?;
            }
            Mode::Shared => {
                let name = update_name(&self.device, self.next);
                crate::durable::write(&self.dir.join(UPDATES).join(&name), &encode_update(records)?)?;
                self.files.insert((self.device.clone(), self.next), records.iter().map(|r| r.id.clone()).collect());
                self.next += 1;
                for r in records {
                    self.loose.insert(r.id.clone(), r.clone());
                }
                self.written = Some(std::time::Instant::now());
            }
        }
        Ok(())
    }

    /// Lets records go. Alone, at once; shared, they stay until the next
    /// consolidation leaves them out.
    pub fn remove(&mut self, ids: &[String]) -> Result<()> {
        if self.mode == Mode::Shared || ids.is_empty() {
            return Ok(());
        }
        let tx = self.db.transaction()?;
        {
            let mut q = tx.prepare("DELETE FROM records WHERE id = ?1")?;
            for id in ids {
                q.execute([hex::decode(id)?])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Update files the state does not hold yet.
    pub fn pending(&self) -> usize {
        self.files.keys().filter(|(d, n)| self.included.get(d).is_none_or(|i| n > i)).count()
    }

    /// Whether the next state is due: updates wait, and this device wrote
    /// nothing for `quiet` — a state is written while nobody writes.
    pub fn due(&self, quiet: std::time::Duration) -> bool {
        self.pending() > 0 && self.written.is_none_or(|at| at.elapsed() >= quiet)
    }

    /// Writes the next state: the last one, the updates that followed it —
    /// each device's without a gap — and without `drop`. The game master's
    /// device does this; any other leaves it. Then the files two states back
    /// go: whoever is a state behind still finds what they need. Returns
    /// whether a new state was written.
    pub fn consolidate(&mut self, drop: &HashSet<String>) -> Result<bool> {
        if self.mode == Mode::Alone {
            let ids: Vec<String> = drop.iter().cloned().collect();
            self.remove(&ids)?;
            return Ok(!ids.is_empty());
        }
        self.fresh()?;
        // Each device's updates in order, up to the first that is not here.
        let mut taken: Vec<(String, u64)> = Vec::new();
        let mut included = self.included.clone();
        for (device, number) in self.files.keys() {
            let upto = included.entry(device.clone()).or_insert(0);
            if *number == *upto + 1 {
                *upto = *number;
                taken.push((device.clone(), *number));
            }
        }
        let dropping = drop.iter().any(|id| self.in_state(id).unwrap_or(false));
        if taken.is_empty() && !dropping {
            return Ok(false);
        }
        let generation = self.state.as_ref().map_or(0, |s| s.generation) + 1;
        let part = self.dir.join(format!(".state-{generation:010}-{}.part", self.device));
        let _ = std::fs::remove_file(&part);
        if let Some(state) = &self.state {
            std::fs::copy(self.dir.join(&state.name), &part)?;
        }
        {
            let mut db = Connection::open(&part)?;
            db.execute_batch("PRAGMA journal_mode = DELETE; PRAGMA synchronous = OFF;")?;
            schema(&db)?;
            let tx = db.transaction()?;
            {
                let mut put = tx.prepare("INSERT OR IGNORE INTO records(id, kind, article, at, raw) VALUES (?1, ?2, ?3, ?4, ?5)")?;
                for key in &taken {
                    for id in self.files.get(key).into_iter().flatten() {
                        let Some(r) = self.loose.get(id) else { continue };
                        if drop.contains(id) {
                            continue;
                        }
                        put.execute(params![hex::decode(&r.id)?, r.kind, r.article, r.at as i64, r.raw.as_deref().ok_or("a record without its bytes")?])?;
                    }
                }
                let mut gone = tx.prepare("DELETE FROM records WHERE id = ?1")?;
                for id in drop {
                    gone.execute([hex::decode(id)?])?;
                }
                tx.execute(
                    "INSERT OR REPLACE INTO meta(key, value) VALUES ('included', ?1), ('generation', ?2)",
                    params![serde_json::to_vec(&included)?, generation.to_string().into_bytes()],
                )?;
            }
            tx.commit()?;
            // Much let go: the file shrinks.
            let (free, pages): (i64, i64) = db.query_row("SELECT freelist_count, page_count FROM pragma_freelist_count(), pragma_page_count()", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
            if free * 4 > pages {
                db.execute_batch("VACUUM")?;
            }
        }
        std::fs::File::open(&part)?.sync_all()?;
        let mut hasher = blake3::Hasher::new();
        hasher.update_reader(std::fs::File::open(&part)?)?;
        let name = StateName::new(generation, &hasher.finalize().to_hex()[..16]);
        std::fs::rename(&part, self.dir.join(&name.name))?;
        crate::durable::sync_parent(&self.dir.join(&name.name))?;
        let _ = crate::durable::replace(&self.private.join("whole"), format!("{} {}", name.name, std::fs::metadata(self.dir.join(&name.name))?.len()).as_bytes());
        let previous = std::mem::take(&mut self.included);
        let previous_generation = self.state.as_ref().map(|s| s.generation);
        self.adopt_newest()?;
        self.tidy(&previous, previous_generation);
        Ok(true)
    }

    /// Lets go of what a state two back held: update files the previous
    /// state held already, and states before it.
    fn tidy(&self, previous: &BTreeMap<String, u64>, previous_generation: Option<u64>) {
        for (device, number, path) in self.update_files() {
            if previous.get(&device).is_some_and(|i| number <= *i) {
                let _ = std::fs::remove_file(path);
            }
        }
        if let Some(keep) = previous_generation {
            for state in self.state_files() {
                if state.generation < keep {
                    let _ = std::fs::remove_file(self.dir.join(&state.name));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(text: &str, kind: u8) -> Stored {
        let raw = text.as_bytes().to_vec();
        Stored { id: blake3::hash(&raw).to_hex().to_string(), kind, article: Some("art:1".into()), at: 1, raw: Some(raw) }
    }

    fn ids(records: &[Stored]) -> Vec<String> {
        let mut out: Vec<String> = records.iter().map(|r| String::from_utf8(r.raw.clone().unwrap()).unwrap()).collect();
        out.sort();
        out
    }

    fn files_in(dir: &Path) -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_dir(dir).map(|e| e.filter_map(|e| e.ok()?.file_name().into_string().ok()).collect()).unwrap_or_default();
        out.sort();
        out
    }

    #[test]
    fn alone_it_keeps_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let (world, private) = (dir.path().join("w"), dir.path().join("p"));
        let mut shelf = Shelf::open(&world, &private, Mode::Alone).unwrap();
        shelf.put(&[record("a", 0), record("b", 1)]).unwrap();
        shelf.remove(&[record("a", 0).id]).unwrap();
        drop(shelf);
        let shelf = Shelf::open(&world, &private, Mode::Alone).unwrap();
        assert_eq!(ids(&shelf.all(|_| true).unwrap()), vec!["b"]);
        assert_eq!(shelf.all(|k| k != 1).unwrap()[0].raw, None, "a version's bytes only when asked");
        assert_eq!(shelf.get(&record("b", 1).id).unwrap(), b"b");
    }

    #[test]
    fn shared_each_device_writes_its_own_files_and_the_game_master_consolidates() {
        let dir = tempfile::tempdir().unwrap();
        let world = dir.path().join("w");
        let mut gm = Shelf::open(&world, &dir.path().join("gm"), Mode::Shared).unwrap();
        let mut bob = Shelf::open(&world, &dir.path().join("bob"), Mode::Shared).unwrap();
        gm.put(&[record("world", 0)]).unwrap();
        bob.put(&[record("bob 1", 1)]).unwrap();
        assert_eq!(ids(&bob.fresh().unwrap()), vec!["world"]);
        assert_eq!(ids(&gm.fresh().unwrap()), vec!["bob 1"]);

        // Consolidated: one state, nothing lost, nothing twice.
        assert!(gm.consolidate(&HashSet::new()).unwrap());
        assert_eq!(ids(&gm.all(|_| true).unwrap()), vec!["bob 1", "world"]);
        assert!(bob.fresh().unwrap().is_empty(), "nothing new for whoever had it all");
        assert_eq!(ids(&bob.all(|_| true).unwrap()), vec!["bob 1", "world"]);
        assert_eq!(gm.pending(), 0);

        // Bob writes on, on the old state; the next one takes it in, and the
        // files of two states back go.
        bob.put(&[record("bob 2", 1)]).unwrap();
        gm.fresh().unwrap();
        assert!(gm.consolidate(&[record("world", 0).id].into_iter().collect()).unwrap());
        assert_eq!(ids(&gm.all(|_| true).unwrap()), vec!["bob 1", "bob 2"]);
        let updates = files_in(&world.join(UPDATES));
        assert_eq!(updates.len(), 1, "only what the latest state holds newly stays a while: {updates:?}");
        let fresh = Shelf::open(&world, &dir.path().join("eve"), Mode::Shared).unwrap();
        assert_eq!(ids(&fresh.all(|_| true).unwrap()), vec!["bob 1", "bob 2"]);
        assert_eq!(files_in(&world).iter().filter(|f| f.starts_with("state-")).count(), 2, "the state before stays a while");

        // Bob's numbers go on — never one that is gone again.
        drop(bob);
        let mut bob = Shelf::open(&world, &dir.path().join("bob"), Mode::Shared).unwrap();
        bob.put(&[record("bob 3", 1)]).unwrap();
        assert!(files_in(&world.join(UPDATES)).iter().any(|f| f.ends_with("0000000003.awu")));
    }

    #[test]
    fn what_is_only_half_there_waits() {
        let dir = tempfile::tempdir().unwrap();
        let world = dir.path().join("w");
        let mut gm = Shelf::open(&world, &dir.path().join("gm"), Mode::Shared).unwrap();
        gm.put(&[record("one", 0)]).unwrap();
        gm.consolidate(&HashSet::new()).unwrap();
        gm.put(&[record("two", 0)]).unwrap();
        gm.consolidate(&HashSet::new()).unwrap();

        // The newest state arrives cut short; an update file too.
        let newest = files_in(&world).into_iter().filter(|f| f.starts_with("state-")).max().unwrap();
        let bytes = std::fs::read(world.join(&newest)).unwrap();
        std::fs::write(world.join(&newest), &bytes[..bytes.len() / 2]).unwrap();
        let half = encode_update(&[record("three", 0)]).unwrap();
        let name = world.join(UPDATES).join(update_name("00000000000000aa", 1));
        std::fs::write(&name, &half[..half.len() - 5]).unwrap();

        let mut bob = Shelf::open(&world, &dir.path().join("bob"), Mode::Shared).unwrap();
        assert_eq!(ids(&bob.all(|_| true).unwrap()), vec!["one", "two"], "the state before, whole, and the update it lacks");
        // Then it is whole.
        std::fs::write(world.join(&newest), &bytes).unwrap();
        std::fs::write(&name, &half).unwrap();
        // Another moment: a folder's time may not have moved.
        bob.stamps = (None, None);
        assert_eq!(ids(&bob.fresh().unwrap()), vec!["three"]);
        assert_eq!(ids(&bob.all(|_| true).unwrap()), vec!["one", "three", "two"]);
    }

    #[test]
    fn a_state_opens_from_a_path_with_any_letters() {
        let dir = tempfile::tempdir().unwrap();
        let world = dir.path().join("Daniel ist 100% – #1 ?");
        let mut gm = Shelf::open(&world, &dir.path().join("gm"), Mode::Shared).unwrap();
        gm.put(&[record("one", 0)]).unwrap();
        gm.consolidate(&HashSet::new()).unwrap();
        let bob = Shelf::open(&world, &dir.path().join("bob"), Mode::Shared).unwrap();
        assert_eq!(ids(&bob.all(|_| true).unwrap()), vec!["one"]);
    }
}
