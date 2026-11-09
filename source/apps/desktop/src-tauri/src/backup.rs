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

const FORMAT: &str = "aw-backup";
/// The backups of world format 5.
const VERSION: u32 = 2;
/// Describes a backup; its folder holds `world/` beside it.
pub const FILE: &str = "backup.awbackup";
/// A backup as one file — a phone's: one file outlasts the app, and the
/// system's picker hands it back. `backup.awbackup` first, then `world/`.
pub const PACKED: &str = "tar";
/// Where the plan of a world's backups lies, in its private folder.
const PLAN: &str = "backup.json";

/// Where a world's backups go.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Plan {
    /// Empty: `backups/` beside the worlds' folder.
    #[serde(default)]
    pub folder: String,
}

/// When a server's plan last came round, at or before `now` (ms since
/// 1970, UTC): today's or yesterday's time, this week's or last week's day.
/// None: off.
pub(crate) fn last_due(plan: &aw_sync::world::BackupPlan, now: u64) -> Option<u64> {
    const DAY: u64 = 24 * 3600 * 1000;
    let at = u64::from(plan.at.min(24 * 60 - 1)) * 60_000;
    let today = now - now % DAY;
    let (start, length) = match plan.every.as_str() {
        "daily" => (today, DAY),
        // 1 January 1970 was a Thursday: Monday is three days before.
        "weekly" => (today - (now / DAY + 3) % 7 * DAY + u64::from(plan.day.min(6)) * DAY, 7 * DAY),
        _ => return None,
    };
    let due = start + at;
    Some(if due > now { due.saturating_sub(length) } else { due })
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

/// A backup found where backups go by default — of any world, also one no
/// longer here.
#[derive(Serialize, Clone, Debug)]
pub struct Found {
    pub at: u64,
    pub name: String,
    pub world: String,
    /// Its `backup.awbackup`, for `backup_restore`.
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

/// Where backups go unless the GM chose a folder: `backups/` beside the
/// worlds' folder — on a phone in its Documents, where they outlast the app
/// and a file manager or a computer on its cable finds them.
fn default_root(store: &Store) -> PathBuf {
    #[cfg(target_os = "android")]
    if let Some(documents) = profile::phone_documents().map(|d| d.join("backups")).filter(|d| std::fs::create_dir_all(d).is_ok()) {
        return documents;
    }
    store.worlds_dir().parent().map(|p| p.join("backups")).unwrap_or_else(|| store.worlds_dir().join("backups"))
}

/// The folder of a world's backups.
fn folder(store: &Store, world: &str, plan: &Plan) -> Answer<PathBuf> {
    let root = if plan.folder.trim().is_empty() {
        default_root(store)
    } else {
        PathBuf::from(plan.folder.trim())
    };
    let name: String = world_name(store, world)?.chars().map(|c| if c.is_alphanumeric() || " -_.".contains(c) { c } else { '_' }).collect();
    Ok(root.join(format!("{} – {}", name.trim(), store::dir_name(world))))
}

/// A world's name — here, or as known of its server.
fn world_name(store: &Store, world: &str) -> Answer<String> {
    if store.holds(world) {
        return Ok(store.load_world_meta(world)?.name);
    }
    network::remote_of(store, world).map(|r| r.name).ok_or_else(|| AppError("There is no such world.".into()))
}

/// Whether this device leads a world — here, or on its server.
fn leads(store: &Store, world: &str) -> bool {
    if store.holds(world) {
        return store.is_gm(world);
    }
    network::remote_worlds(store).is_ok_and(|all| all.iter().any(|w| w.id == world && w.is_gm))
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
            let path = e.path();
            let raw = if packed(&path) { about_packed(&path)? } else { std::fs::read(path.join(FILE)).ok()? };
            let about: About = serde_json::from_slice(&raw).ok()?;
            (about.format == FORMAT).then_some((about, path))
        })
        .collect();
    out.sort_by_key(|(about, _)| std::cmp::Reverse(about.at));
    out
}

fn packed(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == PACKED) && path.is_file()
}

/// What `restore` is given for a backup: its `backup.awbackup` — or the one file.
fn restorable(path: &Path) -> PathBuf {
    if packed(path) { path.to_path_buf() } else { path.join(FILE) }
}

/// `backup.awbackup` of a packed backup — its first entry.
fn about_packed(file: &Path) -> Option<Vec<u8>> {
    let mut archive = tar::Archive::new(std::fs::File::open(file).ok()?);
    let mut first = archive.entries().ok()?.next()?.ok()?;
    if first.path().ok()?.as_os_str() != FILE {
        return None;
    }
    let mut raw = Vec::new();
    std::io::Read::read_to_end(&mut first, &mut raw).ok()?;
    Some(raw)
}

/// A finished backup folder as one file beside it; the folder goes.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn pack(folder: &Path) -> Answer<PathBuf> {
    let file = PathBuf::from(format!("{}.{PACKED}", folder.display()));
    let partial = PathBuf::from(format!("{}.partial", file.display()));
    let mut archive = tar::Builder::new(std::fs::File::create(&partial)?);
    archive.append_path_with_name(folder.join(FILE), FILE)?;
    archive.append_dir_all("world", folder.join("world"))?;
    archive.into_inner()?.sync_all()?;
    std::fs::rename(&partial, &file)?;
    std::fs::remove_dir_all(folder)?;
    Ok(file)
}

pub(crate) fn set_plan(store: &Store, world: &str, plan: Plan) -> Answer<View> {
    if !leads(store, world) {
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
    let _done = copy_world(&*store.db(world)?, &into, world, &name, &store.actor()?, None)?;
    #[cfg(target_os = "android")]
    pack(&_done)?;
    view(store, world)
}

/// Backs a world on a server up here, by hand: fetched from there whole —
/// a picture the last backup holds already is linked, not fetched again.
pub(crate) async fn backup_remote(store: &Store, world: &str, tell: &(dyn Fn(u64, u64) + Sync)) -> Answer<View> {
    let remote = network::remote_of(store, world).ok_or_else(|| AppError("This world is not on a server.".into()))?;
    if !leads(store, world) {
        return Err(AppError("Only the GM backs up a world.".into()));
    }
    let into = folder(store, world, &plan(store, world)?)?;
    std::fs::create_dir_all(&into)?;
    let at = store::now();
    let last = list(&into).into_iter().next().map(|(_, p)| p.join("world"));
    let partial = into.join(format!(".partial-{at}"));
    let _ = std::fs::remove_dir_all(&partial);
    let fetched = network::fetch(store, &remote.server, world, &partial.join("world"), false, last.as_deref(), tell).await;
    if let Err(e) = fetched {
        let _ = std::fs::remove_dir_all(&partial);
        return Err(e);
    }
    let about = About { format: FORMAT.into(), version: VERSION, world: world.into(), name: remote.name, at, gm: store.load_identity()?.id().to_string() };
    aw_store::durable::write(&partial.join(FILE), &serde_json::to_vec_pretty(&about)?)?;
    std::fs::rename(&partial, into.join(stamp(at)))?;
    #[cfg(target_os = "android")]
    pack(&into.join(stamp(at)))?;
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
    if packed(file) {
        // Unpacked beside the worlds, brought back from there, let go of.
        let unpacked = store.worlds_dir().join(format!(".unpacking-{}", store::now()));
        let _ = std::fs::remove_dir_all(&unpacked);
        let done = tar::Archive::new(std::fs::File::open(file)?)
            .unpack(&unpacked)
            .map_err(|_| AppError("Not a backup of a world.".into()))
            .and_then(|()| restore(store, &unpacked.join(FILE)));
        let _ = std::fs::remove_dir_all(&unpacked);
        if file.file_name().is_some_and(|n| n == RECEIVED) {
            let _ = std::fs::remove_file(file);
        }
        return done;
    }
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
pub async fn backup_now(app: tauri::AppHandle, world: String) -> Answer<View> {
    use tauri::Emitter;
    let store = store(&app)?;
    if network::remote_of(&store, &world).is_some() {
        let tell = |done: u64, total: u64| {
            let _ = app.emit("copy-progress", ("transfer", done, total));
        };
        return backup_remote(&store, &world, &tell).await;
    }
    backup(&store, &world)
}

#[cfg(feature = "gui")]
/// Opens the folder of a world's backups.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn backup_open(app: tauri::AppHandle, world: String) -> Answer<()> {
    backup_open_in(&store(&app)?, world)
}

/// `backup_open` for whom the store is given.
pub(crate) fn backup_open_in(store: &Store, world: String) -> Answer<()> {
    let at = folder(store, &world, &plan(store, &world)?)?;
    std::fs::create_dir_all(&at)?;
    profile::reveal(&at)
}

#[cfg(feature = "gui")]
/// Brings a world back from the backup whose `backup.awbackup` is at `path`.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn backup_restore(app: tauri::AppHandle, path: String) -> Answer<WorldInfo> {
    backup_restore_in(&store(&app)?, path)
}

#[cfg(feature = "gui")]
/// The backups where they go by default, newest first — to bring one back
/// without choosing a file, as on a phone.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn backup_found(app: tauri::AppHandle) -> Answer<Vec<Found>> {
    Ok(found(&default_root(&store(&app)?)))
}

pub(crate) fn found(root: &Path) -> Vec<Found> {
    let mut out: Vec<Found> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .flat_map(|e| list(&e.path()))
        .map(|(about, path)| Found { at: about.at, name: about.name, world: about.world, path: restorable(&path).display().to_string() })
        .collect();
    out.sort_by_key(|x| std::cmp::Reverse(x.at));
    out
}

/// Where a backup handed over in pieces lies until it is brought back.
const RECEIVED: &str = ".received.tar";

#[cfg(feature = "gui")]
/// A packed backup handed over in pieces, each in Base64 — a phone's picker
/// gives no path, and Android's bridge carries text only: `first` starts it
/// anew. Answers where it lies, for `backup_restore`, which lets go of it.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn backup_receive(app: tauri::AppHandle, piece: String, first: bool) -> Answer<String> {
    use base64::Engine;
    use std::io::Write;
    let bytes = base64::engine::general_purpose::STANDARD.decode(piece).map_err(|_| AppError("The file came broken.".into()))?;
    let path = store(&app)?.worlds_dir().join(RECEIVED);
    let mut file = std::fs::OpenOptions::new().create(true).write(true).append(!first).truncate(first).open(&path)?;
    file.write_all(&bytes)?;
    Ok(path.display().to_string())
}

/// `backup_restore` for whom the store is given.
pub(crate) fn backup_restore_in(store: &Store, path: String) -> Answer<WorldInfo> {
    let world = restore(store, Path::new(&path))?;
    // Not theirs to open — someone else's world: it does not stay behind unseen.
    world_now(store, &world).inspect_err(|_| {
        let _ = std::fs::remove_dir_all(store.worlds_dir().join(store::dir_name(&world)));
    })
}

#[cfg(test)]
#[path = "backup_tests.rs"]
mod tests;
