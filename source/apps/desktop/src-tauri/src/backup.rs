//! Backups of a world: its folder as it was at a time, in a folder of the
//! GM's choice — by hand, or daily or weekly by themselves, as many kept as
//! asked for.
//!
//! The world's files are named by their content and never change, so a file
//! the last backup holds already is a hard link to it there: each backup
//! takes the room of what is new only. What only this profile keeps — the
//! world token, the keyring, the members — goes along sealed for the GM's
//! identity: the backup opens with it alone, and lies no more open than the
//! world itself.

use super::*;
use std::collections::BTreeMap;

const FORMAT: &str = "aw-backup";
/// Describes a backup; its folder holds `world/` and the sealed keys beside it.
pub const FILE: &str = "backup.awbackup";
const KEYS: &str = "keys.sealed";
/// Where the plan of a world's backups lies, in its private folder.
const PLAN: &str = "backup.json";

/// How often a world is backed up by itself, how many backups stay, and where.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Plan {
    /// `off`, `daily` or `weekly`.
    pub every: String,
    pub keep: u32,
    /// Empty: `backups/` beside the worlds' folder.
    #[serde(default)]
    pub folder: String,
}

impl Default for Plan {
    fn default() -> Self {
        Plan { every: "off".into(), keep: 7, folder: String::new() }
    }
}

impl Plan {
    fn period(&self) -> Option<u64> {
        match self.every.as_str() {
            "daily" => Some(24 * 3600 * 1000),
            "weekly" => Some(7 * 24 * 3600 * 1000),
            _ => None,
        }
    }
}

/// What a backup says of itself.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct About {
    format: String,
    version: u32,
    world: String,
    name: String,
    /// Milliseconds since 1970.
    at: u64,
    /// The GM whose identity opens the keys.
    gm: String,
}

/// The keys of a world, sealed: a data key for the GM, the files under it.
#[derive(Serialize, Deserialize)]
struct SealedKeys {
    key: aw_crypto::WrappedKey,
    /// Hex: the files by name, encrypted with the data key.
    data: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct Entry {
    pub at: u64,
    pub path: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct View {
    pub plan: Plan,
    /// Where they go — `folder` or the default.
    pub folder: String,
    /// Newest first.
    pub backups: Vec<Entry>,
}

pub(crate) fn plan(store: &Store, world: &str) -> Answer<Plan> {
    let path = store.private_world_dir(world).join(PLAN);
    Ok(match std::fs::read(&path) {
        Ok(raw) => serde_json::from_slice(&raw)?,
        Err(_) => Plan::default(),
    })
}

/// The folder of a world's backups.
fn folder(store: &Store, world: &str, plan: &Plan) -> Answer<PathBuf> {
    let root = if plan.folder.trim().is_empty() {
        store.worlds_dir().parent().map(|p| p.join("backups")).unwrap_or_else(|| store.worlds_dir().join("backups"))
    } else {
        PathBuf::from(plan.folder.trim())
    };
    let name: String = store.load_world_meta(world)?.public.name.chars().map(|c| if c.is_alphanumeric() || " -_.".contains(c) { c } else { '_' }).collect();
    Ok(root.join(format!("{} – {}", name.trim(), store::dir_name(world))))
}

pub(crate) fn view(store: &Store, world: &str) -> Answer<View> {
    let plan = plan(store, world)?;
    let at = folder(store, world, &plan)?;
    let backups = list(&at).into_iter().map(|(about, path)| Entry { at: about.at, path: path.display().to_string() }).collect();
    Ok(View { folder: at.display().to_string(), plan, backups })
}

/// The finished backups in a folder, newest first.
fn list(folder: &Path) -> Vec<(About, PathBuf)> {
    let mut out: Vec<(About, PathBuf)> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let about: About = serde_json::from_slice(&std::fs::read(e.path().join(FILE)).ok()?).ok()?;
            (about.format == FORMAT).then_some((about, e.path()))
        })
        .collect();
    out.sort_by(|a, b| b.0.at.cmp(&a.0.at));
    out
}

pub(crate) fn set_plan(store: &Store, world: &str, plan: Plan) -> Answer<View> {
    if !store.is_gm(world) {
        return Err(AppError("Only the GM backs up a world.".into()));
    }
    if !["off", "daily", "weekly"].contains(&plan.every.as_str()) {
        return Err(AppError("Off, daily or weekly.".into()));
    }
    let plan = Plan { keep: plan.keep.clamp(1, 365), ..plan };
    aw_store::durable::write(&store.private_world_dir(world).join(PLAN), &serde_json::to_vec_pretty(&plan)?)?;
    view(store, world)
}

/// A file named by its content: the same name, the same bytes, for good.
fn unchanging(path: &Path) -> bool {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    stem.len() == 64 && stem.chars().all(|c| c.is_ascii_hexdigit())
}

/// Backs a world up now, then lets go of the oldest beyond what is kept.
pub(crate) fn backup(store: &Store, world: &str) -> Answer<View> {
    if !store.is_gm(world) {
        return Err(AppError("Only the GM backs up a world.".into()));
    }
    if store::moving(world) {
        return Err(AppError("The world is being moved — back it up afterwards.".into()));
    }
    let plan = plan(store, world)?;
    let into = folder(store, world, &plan)?;
    std::fs::create_dir_all(&into)?;
    // A backup broken off before lies under a name starting with a dot.
    for e in std::fs::read_dir(&into)?.flatten() {
        if e.file_name().to_string_lossy().starts_with(".partial-") {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
    let at = store::now();
    let last = list(&into).into_iter().next().map(|(_, p)| p.join("world"));
    let partial = into.join(format!(".partial-{at}"));
    let from = store.world_dir(world);
    let mut dirs = vec![from.clone()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                dirs.push(path);
                continue;
            }
            let rel = path.strip_prefix(&from).map_err(|e| AppError(e.to_string()))?;
            let target = partial.join("world").join(rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let before = last.as_ref().map(|l| l.join(rel)).filter(|b| unchanging(&path) && b.is_file());
            if before.is_some_and(|b| std::fs::hard_link(&b, &target).is_ok()) {
                continue;
            }
            std::fs::copy(&path, &target)?;
        }
    }
    let me = store.load_identity()?;
    std::fs::write(partial.join(KEYS), serde_json::to_vec(&seal_keys(store, world, &me)?)?)?;
    let about = About { format: FORMAT.into(), version: 1, world: world.into(), name: store.load_world_meta(world)?.public.name, at, gm: me.id().to_string() };
    aw_store::durable::write(&partial.join(FILE), &serde_json::to_vec_pretty(&about)?)?;
    // Two in the same second: the second gets a number.
    let mut name = stamp(at);
    for n in 2.. {
        if !into.join(&name).exists() {
            break;
        }
        name = format!("{} ({n})", stamp(at));
    }
    std::fs::rename(&partial, into.join(name))?;
    for (_, old) in list(&into).into_iter().skip(plan.keep.max(1) as usize) {
        let _ = std::fs::remove_dir_all(old);
    }
    view(store, world)
}

fn seal_keys(store: &Store, world: &str, me: &Identity) -> Answer<SealedKeys> {
    let mut files = BTreeMap::new();
    for name in keys::KEPT {
        if let Ok(text) = std::fs::read_to_string(store.private_world_dir(world).join(name)) {
            files.insert(name.to_string(), text);
        }
    }
    let dek = aw_crypto::ContentKey::generate().map_err(|e| AppError(e.to_string()))?;
    let data = aw_crypto::encrypt(&dek, &serde_json::to_vec(&files)?).map_err(|e| AppError(e.to_string()))?;
    let recipient = me.public().seal_public().map_err(|e| AppError(e.to_string()))?;
    let key = aw_crypto::seal_key(&recipient, &dek).map_err(|e| AppError(e.to_string()))?;
    Ok(SealedKeys { key, data: hex::encode(data) })
}

/// `2026-10-23 06-15-00 UTC` — a folder name that sorts by time.
fn stamp(ms: u64) -> String {
    let secs = ms / 1000;
    let (h, m, s) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
    // Days since 1970 to a date (H. Hinnant's `civil_from_days`).
    let z = (secs / 86400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {h:02}-{m:02}-{s:02} UTC")
}

/// Brings a world back from a backup — where it is not here: next to the
/// other worlds, with its keys, which only the GM who made the backup opens.
pub(crate) fn restore(store: &Store, file: &Path) -> Answer<String> {
    let about: About = serde_json::from_slice(&std::fs::read(file)?).map_err(|_| AppError("Not a backup of a world.".into()))?;
    if about.format != FORMAT || about.version != 1 {
        return Err(AppError("Not a backup this version knows.".into()));
    }
    let backup = file.parent().ok_or_else(|| AppError("Not a backup of a world.".into()))?;
    let me = store.load_identity()?;
    if me.id().as_str() != about.gm {
        return Err(AppError("Only the GM who made the backup opens it — restore their keys first.".into()));
    }
    if store.holds(&about.world) {
        return Err(AppError(format!("“{}” is here already. Delete it first to bring the backup back.", about.name)));
    }
    let sealed: SealedKeys = serde_json::from_slice(&std::fs::read(backup.join(KEYS))?)?;
    let dek = aw_crypto::unseal_key(me.seal_secret(), &sealed.key).map_err(|_| AppError("The keys of this backup do not open.".into()))?;
    let plain = aw_crypto::decrypt(&dek, &hex::decode(&sealed.data).map_err(|e| AppError(e.to_string()))?).map_err(|_| AppError("The keys of this backup do not open.".into()))?;
    let files: BTreeMap<String, String> = serde_json::from_slice(&plain)?;

    let target = store.worlds_dir().join(store::dir_name(&about.world));
    let partial = store.worlds_dir().join(format!(".restoring-{}", store::dir_name(&about.world)));
    let _ = std::fs::remove_dir_all(&partial);
    store::copy_dir(&backup.join("world"), &partial, &|_, _| {})?;
    let private = store.private_world_dir(&about.world);
    std::fs::create_dir_all(&private)?;
    for (name, text) in &files {
        if keys::KEPT.contains(&name.as_str()) {
            aw_store::durable::write(&private.join(name), text.as_bytes())?;
        }
    }
    std::fs::rename(&partial, &target)?;
    Ok(about.world)
}

/// Backs up, by themselves, the worlds whose time has come — looked at now
/// and then while the app runs.
pub(crate) fn due(store: &Store) {
    let Ok(worlds) = store.list_worlds() else { return };
    for meta in worlds {
        let world = meta.public.world_id.to_string();
        if !store.is_gm(&world) {
            continue;
        }
        let Ok(plan) = plan(store, &world) else { continue };
        let Some(period) = plan.period() else { continue };
        let Ok(at) = folder(store, &world, &plan) else { continue };
        let last = list(&at).first().map(|(a, _)| a.at).unwrap_or(0);
        if store::now().saturating_sub(last) >= period {
            if let Err(e) = backup(store, &world) {
                eprintln!("backup of {world}: {}", e.0);
            }
        }
    }
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn backup_view(app: tauri::AppHandle, world: String) -> Answer<View> {
    view(&store(&app)?, &world)
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn backup_plan_set(app: tauri::AppHandle, world: String, plan: Plan) -> Answer<View> {
    set_plan(&store(&app)?, &world, plan)
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn backup_now(app: tauri::AppHandle, world: String) -> Answer<View> {
    backup(&store(&app)?, &world)
}

#[cfg(feature = "gui")]
/// Opens the folder of a world's backups.
#[cfg_attr(feature = "gui", tauri::command)]
pub fn backup_open(app: tauri::AppHandle, world: String) -> Answer<()> {
    let store = store(&app)?;
    let at = folder(&store, &world, &plan(&store, &world)?)?;
    std::fs::create_dir_all(&at)?;
    profile::reveal(&at)
}

#[cfg(feature = "gui")]
/// Brings a world back from the backup whose `backup.awbackup` is at `path`.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn backup_restore(app: tauri::AppHandle, path: String) -> Answer<WorldInfo> {
    let store = store(&app)?;
    let world = restore(&store, Path::new(&path))?;
    let me = store.load_identity()?;
    world_info(&store, &store.load_world_meta(&world)?, me.id().as_str())
}

#[cfg(feature = "gui")]
/// Looks now and then whether a world is due — a minute after the start, then every quarter hour.
pub fn start(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(60));
        loop {
            if let Ok(store) = store(&app) {
                due(&store);
            }
            std::thread::sleep(std::time::Duration::from_secs(15 * 60));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backup_holds_the_world_and_brings_it_back_with_its_keys() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("gm"));
        let gm = Identity::create("Marla").unwrap();
        store.save_identity(&gm).unwrap();
        let meta = new_world(&store, &gm, "Eldermere".into()).unwrap();
        let world = meta.public.world_id.to_string();
        let page = |s: &Store| document::code_of(s, &gm, &world, &meta.home).unwrap().code;
        let before = page(&store);

        set_plan(&store, &world, Plan { every: "daily".into(), keep: 2, folder: dir.path().join("safe").display().to_string() }).unwrap();
        let first = backup(&store, &world).unwrap();
        assert_eq!(first.backups.len(), 1);
        // The next links what did not change: the same file, not a copy.
        std::thread::sleep(std::time::Duration::from_millis(5));
        let second = backup(&store, &world).unwrap();
        let record = |b: &Entry| {
            let mut dirs = vec![PathBuf::from(&b.path).join("world")];
            while let Some(d) = dirs.pop() {
                for e in std::fs::read_dir(d).unwrap().flatten() {
                    if e.path().is_dir() { dirs.push(e.path()) } else if unchanging(&e.path()) { return e.path() }
                }
            }
            panic!("no record");
        };
        let (a, b) = (record(&second.backups[0]), record(&second.backups[1]));
        use std::os::unix::fs::MetadataExt;
        assert_eq!(std::fs::metadata(&a).unwrap().ino(), std::fs::metadata(b.parent().unwrap().join(a.file_name().unwrap())).unwrap().ino());
        // Kept: two, the oldest goes.
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert_eq!(backup(&store, &world).unwrap().backups.len(), 2);
        // Not due again today.
        let count = std::fs::read_dir(dir.path().join("safe")).unwrap().count();
        due(&store);
        assert_eq!(std::fs::read_dir(dir.path().join("safe")).unwrap().count(), count);

        // Lost: the world and its keys. The backup brings both back.
        let newest = PathBuf::from(&view(&store, &world).unwrap().backups[0].path).join(FILE);
        assert!(restore(&store, &newest).is_err(), "here already");
        store.delete_world(&world).unwrap();
        assert_eq!(restore(&store, &newest).unwrap(), world);
        assert!(store.is_gm(&world));
        assert_eq!(page(&store), before);

        // Someone else's identity does not open it.
        let other = Store::new(dir.path().join("eve"));
        other.save_identity(&Identity::create("Eve").unwrap()).unwrap();
        assert!(restore(&other, &newest).is_err());
    }

    #[test]
    fn a_stamp_is_the_date_and_time() {
        assert_eq!(stamp(0), "1970-01-01 00-00-00 UTC");
        assert_eq!(stamp(1_792_736_100_000), "2026-10-23 06-15-00 UTC");
    }
}
