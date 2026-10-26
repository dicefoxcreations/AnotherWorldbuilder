//! Backups of a world: its folder as it was at a time, in a folder of the
//! GM's choice. On this device they are made by hand and all of them stay;
//! a server backs up the worlds it holds by itself, daily or weekly, as many
//! kept as their GM asks for (with `copy_world` from here).
//!
//! A backup is the world's database, copied whole while it goes on being
//! written, and its pictures. A picture never changes under its name, so one
//! the last backup holds already is a hard link to it there: each backup
//! takes the room of what is new only. Like the world, a backup is readable
//! without the app.

use super::*;
use std::collections::BTreeMap;

const FORMAT: &str = "aw-backup";
/// The backups of world format 5.
const VERSION: u32 = 2;
/// Describes a backup; its folder holds `world/` beside it.
pub const FILE: &str = "backup.awbackup";
/// Where the plan of a world's backups lies, in its private folder.
const PLAN: &str = "backup.json";

/// Where a world's backups go.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Plan {
    /// Empty: `backups/` beside the worlds' folder.
    #[serde(default)]
    pub folder: String,
}

/// How often a backup comes by itself — on a server.
pub(crate) fn period(every: &str) -> Option<u64> {
    match every {
        "daily" => Some(24 * 3600 * 1000),
        "weekly" => Some(7 * 24 * 3600 * 1000),
        _ => None,
    }
}

/// What a backup says of itself.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct About {
    format: String,
    version: u32,
    world: String,
    name: String,
    /// Milliseconds since 1970.
    pub(crate) at: u64,
    /// Who made it, by id — empty for a server's own.
    gm: String,
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
    let name: String = store.load_world_meta(world)?.name.chars().map(|c| if c.is_alphanumeric() || " -_.".contains(c) { c } else { '_' }).collect();
    Ok(root.join(format!("{} – {}", name.trim(), store::dir_name(world))))
}

pub(crate) fn view(store: &Store, world: &str) -> Answer<View> {
    let plan = plan(store, world)?;
    let at = folder(store, world, &plan)?;
    let backups = list(&at).into_iter().map(|(about, path)| Entry { at: about.at, path: path.display().to_string() }).collect();
    Ok(View { folder: at.display().to_string(), plan, backups })
}

/// The finished backups in a folder, newest first.
pub(crate) fn list(folder: &Path) -> Vec<(About, PathBuf)> {
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
    aw_store::durable::write(&store.private_world_dir(world).join(PLAN), &serde_json::to_vec_pretty(&plan)?)?;
    view(store, world)
}

/// Backs a world up now — by hand; every backup stays.
pub(crate) fn backup(store: &Store, world: &str) -> Answer<View> {
    if !store.is_gm(world) {
        return Err(AppError("Only the GM backs up a world.".into()));
    }
    if store::moving(world) {
        return Err(AppError("The world is being moved — back it up afterwards.".into()));
    }
    let plan = plan(store, world)?;
    let into = folder(store, world, &plan)?;
    let name = store.load_world_meta(world)?.name;
    copy_world(&*store.db(world)?, &into, world, &name, &store.actor()?, None)?;
    view(store, world)
}

/// A new backup of a world in `into`: its database copied whole, its
/// pictures beside it — one the last backup holds already is a hard link to
/// it there. `keep`: how many stay, the oldest let go; `None`: all.
pub(crate) fn copy_world(db: &store::WorldDb, into: &Path, world: &str, name: &str, gm: &str, keep: Option<u32>) -> Answer<PathBuf> {
    std::fs::create_dir_all(into)?;
    // A backup broken off before lies under a name starting with a dot.
    for e in std::fs::read_dir(into)?.flatten() {
        if e.file_name().to_string_lossy().starts_with(".partial-") {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
    let at = store::now();
    let last = list(into).into_iter().next().map(|(_, p)| p.join("world").join(aw_store::db::MEDIA));
    let partial = into.join(format!(".partial-{at}"));
    let media = partial.join("world").join(aw_store::db::MEDIA);
    std::fs::create_dir_all(&media)?;
    db.copy_to(&partial.join("world").join(aw_store::db::FILE))?;
    for entry in std::fs::read_dir(db.dir().join(aw_store::db::MEDIA))?.flatten() {
        let path = entry.path();
        let Some(file) = path.file_name() else { continue };
        let target = media.join(file);
        let before = last.as_ref().map(|l| l.join(file)).filter(|b| b.is_file());
        if before.is_some_and(|b| std::fs::hard_link(&b, &target).is_ok()) {
            continue;
        }
        std::fs::copy(&path, &target)?;
    }
    let about = About { format: FORMAT.into(), version: VERSION, world: world.into(), name: name.into(), at, gm: gm.into() };
    aw_store::durable::write(&partial.join(FILE), &serde_json::to_vec_pretty(&about)?)?;
    // Two in the same second: the second gets a number.
    let mut stamped = stamp(at);
    for n in 2.. {
        if !into.join(&stamped).exists() {
            break;
        }
        stamped = format!("{} ({n})", stamp(at));
    }
    let done = into.join(stamped);
    std::fs::rename(&partial, &done)?;
    if let Some(keep) = keep {
        for (_, old) in list(into).into_iter().skip(keep.max(1) as usize) {
            let _ = std::fs::remove_dir_all(old);
        }
    }
    Ok(done)
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
/// other worlds, as it was.
pub(crate) fn restore(store: &Store, file: &Path) -> Answer<String> {
    let about: About = serde_json::from_slice(&std::fs::read(file)?).map_err(|_| AppError("Not a backup of a world.".into()))?;
    if about.format != FORMAT || about.version != VERSION {
        return Err(AppError("Not a backup this version knows.".into()));
    }
    let backup = file.parent().ok_or_else(|| AppError("Not a backup of a world.".into()))?;
    if store.holds(&about.world) {
        return Err(AppError(format!("“{}” is here already. Delete it first to bring the backup back.", about.name)));
    }
    let target = store.worlds_dir().join(store::dir_name(&about.world));
    let partial = store.worlds_dir().join(format!(".restoring-{}", store::dir_name(&about.world)));
    let _ = std::fs::remove_dir_all(&partial);
    store::copy_dir(&backup.join("world"), &partial, &|_, _| {})?;
    std::fs::rename(&partial, &target)?;
    Ok(about.world)
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
#[cfg_attr(feature = "gui", tauri::command(async))]
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
    world_now(&store, &world)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backup_holds_the_world_and_brings_it_back() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("gm"));
        let gm = Identity::create("Marla").unwrap();
        store.save_identity(&gm).unwrap();
        let meta = new_world(&store, &gm, "Eldermere".into()).unwrap();
        let world = meta.id.clone();
        store.upload_image(&world, &tiny_png(), "Map", "").unwrap();
        let page = |s: &Store| document::code_of(s, &world, &meta.home).unwrap().code;
        let before = page(&store);

        set_plan(&store, &world, Plan { folder: dir.path().join("safe").display().to_string() }).unwrap();
        let first = backup(&store, &world).unwrap();
        assert_eq!(first.backups.len(), 1);
        // The next links what did not change: the same picture, not a copy.
        std::thread::sleep(std::time::Duration::from_millis(5));
        let second = backup(&store, &world).unwrap();
        let picture = |b: &Entry| std::fs::read_dir(PathBuf::from(&b.path).join("world").join(aw_store::db::MEDIA)).unwrap().flatten().next().unwrap().path();
        let (a, b) = (picture(&second.backups[0]), picture(&second.backups[1]));
        use std::os::unix::fs::MetadataExt;
        assert_eq!(std::fs::metadata(&a).unwrap().ino(), std::fs::metadata(&b).unwrap().ino());
        // By hand, every one stays.
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert_eq!(backup(&store, &world).unwrap().backups.len(), 3);

        // Lost: the world. The backup brings it back.
        let newest = PathBuf::from(&view(&store, &world).unwrap().backups[0].path).join(FILE);
        assert!(restore(&store, &newest).is_err(), "here already");
        store.delete_world(&world).unwrap();
        assert_eq!(restore(&store, &newest).unwrap(), world);
        assert!(store.is_gm(&world));
        assert_eq!(page(&store), before);
        assert_eq!(store.gallery(&world).unwrap().len(), 1, "the picture came back too");
    }

    fn tiny_png() -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        image::RgbImage::new(2, 2).write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn a_server_keeps_as_many_as_asked_and_lets_go_of_the_oldest() {
        let dir = tempfile::tempdir().unwrap();
        let gm = store::Member { id: "id:gm".into(), name: "Marla".into(), ..Default::default() };
        let db = store::WorldDb::create(&dir.path().join("world"), "world:1", "Eldermere", &gm).unwrap();
        let into = dir.path().join("backups");
        for _ in 0..4 {
            copy_world(&db, &into, "world:1", "Eldermere", "", Some(2)).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let left = list(&into);
        assert_eq!(left.len(), 2);
        assert!(left[0].0.at > left[1].0.at, "the newest first");
    }

    #[test]
    fn a_stamp_is_the_date_and_time() {
        assert_eq!(stamp(0), "1970-01-01 00-00-00 UTC");
        assert_eq!(stamp(1_792_736_100_000), "2026-10-23 06-15-00 UTC");
    }
}
