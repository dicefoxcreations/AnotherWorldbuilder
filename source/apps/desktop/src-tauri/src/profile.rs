//! Where this app keeps its data.
//!
//! Two folders: the **profile** — identity, avatar, settings — and the
//! **worlds**. On first start the user chooses: one folder with both below it
//! (`profile/` and `worlds/`), or each on its own — the profile on this
//! device, say, and the worlds on an external drive. Only the choice itself
//! lies at a fixed place: `locations.json` in the system's config folder.
//!
//! A folder that is not there — a drive not connected — is never made anew
//! behind the user's back; the app says so and waits. While it runs, the app
//! locks the profile and uses a local lock for the canonical worlds path, so
//! another copy under the same OS user cannot write the same data concurrently.
//!
//! `AW_PROFILE_ROOT` moves the fixed place elsewhere — so two copies of the
//! app can run side by side on one device, each as a different person, for
//! trying the network out.

use super::*;
use std::sync::{Mutex, OnceLock};

/// The two folders.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Locations {
    pub profile: PathBuf,
    pub worlds: PathBuf,
    #[serde(default)]
    pub additional: Vec<PathBuf>,
}

impl Locations {
    fn roots(&self) -> Vec<PathBuf> {
        std::iter::once(self.worlds.clone()).chain(self.additional.clone()).collect()
    }
    fn store(&self) -> Store { Store::with_roots(self.profile.clone(), self.roots()) }
}

/// The locations in use, with the locks held in them.
struct InUse {
    at: Locations,
    _locks: Vec<(PathBuf, std::fs::File)>,
}

static IN_USE: OnceLock<Mutex<Option<InUse>>> = OnceLock::new();

fn in_use() -> &'static Mutex<Option<InUse>> {
    IN_USE.get_or_init(Default::default)
}

pub fn profile_root() -> Option<PathBuf> {
    std::env::var_os("AW_PROFILE_ROOT").map(PathBuf::from)
}

#[cfg(feature = "gui")]
/// The fixed place where the choice is remembered.
pub(crate) fn config_dir(app: &tauri::AppHandle) -> Answer<PathBuf> {
    match profile_root() {
        Some(root) if !root.is_absolute() => Err(AppError("AW_PROFILE_ROOT must be absolute.".into())),
        Some(root) => Ok(root),
        None => Ok(app.path().app_config_dir()?),
    }
}

#[cfg(feature = "gui")]
fn remembered(app: &tauri::AppHandle) -> Answer<Option<Locations>> {
    let path = config_dir(app)?.join("locations.json");
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(&std::fs::read(path)?)?))
}

#[cfg(feature = "gui")]
fn remember(app: &tauri::AppHandle, at: &Locations) -> Answer<()> {
    aw_store::durable::write(&config_dir(app)?.join("locations.json"), &serde_json::to_vec_pretty(at)?)?;
    Ok(())
}

/// Why locations cannot be used.
enum Refused {
    /// A folder is not there.
    Missing(String),
    /// Another copy of the app uses it.
    Busy(String),
}

impl From<Refused> for AppError {
    fn from(r: Refused) -> Self {
        match r {
            Refused::Missing(m) | Refused::Busy(m) => AppError(m),
        }
    }
}

/// Canonical world root => one local lock, even with different profiles or aliases.
/// Takes a profile and a worlds folder into use without the app's window —
/// the server's. Refused while another copy uses them.
pub(crate) fn use_folders(profile: &Path, worlds: &Path, locks: &Path) -> Answer<Store> {
    let at = Locations { profile: profile.to_path_buf(), worlds: worlds.to_path_buf(), additional: Vec::new() };
    claim(&at, locks).map_err(|e| match e {
        Refused::Missing(m) | Refused::Busy(m) => AppError(m),
    })?;
    Ok(at.store())
}

fn world_lock_path(root: &Path, canonical_worlds: &Path) -> PathBuf {
    let id = aw_crypto::WorldId::derive("storage", &[canonical_worlds.as_os_str().as_encoded_bytes()]);
    root.join(format!("{}.lock", store::dir_name(id.as_str())))
}

/// Takes locations into use: both folders there, neither used by another
/// copy of the app. What was in use before is let go.
fn claim(at: &Locations, lock_root: &Path) -> Result<(), Refused> {
    let mut slot = in_use().lock().map_err(|e| Refused::Missing(e.to_string()))?;
    for (dir, what) in [(&at.profile, "profile"), (&at.worlds, "worlds")] {
        if !dir.is_dir() {
            return Err(Refused::Missing(format!(
                "The {what} folder is not there: {}. Is the drive connected?",
                dir.display()
            )));
        }
    }
    let profile = std::fs::canonicalize(&at.profile).map_err(|e| Refused::Missing(e.to_string()))?;
    std::fs::create_dir_all(lock_root).map_err(|e| Refused::Missing(e.to_string()))?;
    let mut roots = Vec::new();
    for root in at.roots() {
        // A disconnected additional drive remains listed and is never recreated.
        if !root.exists() { continue; }
        let root = std::fs::canonicalize(root).map_err(|e| Refused::Missing(e.to_string()))?;
        if !root.is_dir() || profile.starts_with(&root) || root.starts_with(&profile) {
            return Err(Refused::Missing("Profile and world locations must be separate folders, neither inside the other.".into()));
        }
        if roots.iter().any(|p: &PathBuf| root.starts_with(p) || p.starts_with(&root)) {
            return Err(Refused::Missing("World locations must not overlap or refer to the same folder.".into()));
        }
        roots.push(root);
    }
    let mut locks = Vec::new();
    let mut paths = vec![profile.join(".lock")];
    paths.extend(roots.iter().map(|root| world_lock_path(lock_root, root)));
    for path in paths {
        if let Some((_, file)) = slot.as_ref().and_then(|u| u._locks.iter().find(|(p, _)| p == &path)) {
            locks.push((path, file.try_clone().map_err(|e| Refused::Missing(e.to_string()))?));
            continue;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|e| Refused::Missing(format!("Cannot open storage lock {}: {e}", path.display())))?;
        match file.try_lock() {
            Ok(()) => locks.push((path, file)),
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(Refused::Busy(format!(
                    "Another copy of the app is using {} right now.",
                    path.display()
                )))
            }
            // A drive that knows no locks: go on without.
            Err(std::fs::TryLockError::Error(_)) => locks.push((path, file)),
        }
    }
    // A profile taken up anew: what it held only while online goes — it
    // was held for the last time the app ran.
    let fresh = slot.as_ref().is_none_or(|u| u.at.profile != at.profile);
    *slot = Some(InUse { at: at.clone(), _locks: locks });
    drop(slot);
    if fresh {
        at.store().forget_fleeting();
        crate::online::purge_sessions(&at.store());
    }
    Ok(())
}

#[cfg(feature = "gui")]
/// The data, where it lies now.
pub fn store(app: &tauri::AppHandle) -> Answer<Store> {
    if let Some(store) = store_now() {
        return Ok(store);
    }
    let at = remembered(app)?.ok_or_else(|| AppError("Choose first where the app keeps its data.".into()))?;
    claim(&at, &app.path().app_local_data_dir()?.join("storage-locks"))?;
    Ok(at.store())
}

/// The data, if a place is in use — for what runs in the background.
pub fn store_now() -> Option<Store> {
    let slot = in_use().lock().ok()?;
    slot.as_ref().map(|u| u.at.store())
}

// --- Commands ---------------------------------------------------------------

/// Where the data lies, as the start screen and the settings show it.
#[derive(Serialize)]
pub struct StorageView {
    /// `unset`: nothing chosen yet. `missing`: a folder is not there.
    /// `busy`: another copy of the app uses it. `ready`.
    pub state: String,
    pub profile: String,
    pub worlds: String,
    pub locations: Vec<LocalLocation>,
    /// When both lie side by side below one folder: that folder.
    pub together: Option<String>,
    /// Where to keep everything, if nothing is chosen yet.
    pub suggested: String,
    pub problem: Option<String>,
}

#[derive(Serialize)]
pub struct LocalLocation {
    pub path: String,
    pub available: bool,
    pub primary: bool,
}

#[cfg(feature = "gui")]
fn suggested(app: &tauri::AppHandle) -> PathBuf {
    profile_root()
        .or_else(|| app.path().document_dir().ok().map(|d| d.join("AnotherWorldbuilder")))
        .or_else(|| app.path().app_data_dir().ok())
        .unwrap_or_default()
}

fn together(at: &Locations) -> Option<String> {
    let base = at.profile.parent()?;
    (at.profile.file_name()? == "profile" && at.worlds == base.join("worlds")).then(|| base.display().to_string())
}

#[cfg(feature = "gui")]
fn view(app: &tauri::AppHandle, at: Option<&Locations>, state: &str, problem: Option<String>) -> StorageView {
    StorageView {
        state: state.into(),
        profile: at.map(|a| a.profile.display().to_string()).unwrap_or_default(),
        worlds: at.map(|a| a.worlds.display().to_string()).unwrap_or_default(),
        locations: at.map(|a| a.roots().iter().enumerate().map(|(i, root)| LocalLocation {
            path: root.display().to_string(), available: root.is_dir(), primary: i == 0,
        }).collect()).unwrap_or_default(),
        together: at.and_then(together),
        suggested: suggested(app).display().to_string(),
        problem,
    }
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn storage_status(app: tauri::AppHandle) -> Answer<StorageView> {
    let Some(at) = remembered(&app)? else { return Ok(view(&app, None, "unset", None)) };
    Ok(match claim(&at, &app.path().app_local_data_dir()?.join("storage-locks")) {
        Ok(()) => view(&app, Some(&at), "ready", None),
        Err(Refused::Missing(m)) => view(&app, Some(&at), "missing", Some(m)),
        Err(Refused::Busy(m)) => view(&app, Some(&at), "busy", Some(m)),
    })
}

/// Where to put things: one folder for both, or each on its own.
#[derive(Deserialize)]
pub struct Choice {
    pub base: Option<String>,
    pub profile: Option<String>,
    pub worlds: Option<String>,
    pub additional: Option<Vec<String>>,
}

impl Choice {
    fn locations(&self) -> Answer<Locations> {
        let absolute = |p: &str| -> Answer<PathBuf> {
            let path = PathBuf::from(p.trim());
            if p.trim().is_empty() || !path.is_absolute() {
                return Err(AppError(format!("Not a full path: {p:?}")));
            }
            Ok(path)
        };
        match (&self.base, &self.profile, &self.worlds) {
            (Some(base), _, _) => {
                let base = absolute(base)?;
                Ok(Locations { profile: base.join("profile"), worlds: base.join("worlds"), additional: Vec::new() })
            }
            (None, Some(profile), Some(worlds)) => Ok(Locations { profile: absolute(profile)?, worlds: absolute(worlds)?, additional: self.additional.as_deref().unwrap_or_default().iter().map(|p| absolute(p)).collect::<Answer<_>>()? }),
            _ => Err(AppError("Choose a folder.".into())),
        }
    }
}

#[cfg(feature = "gui")]
/// Chooses where the data lies — on first start, or when the place
/// remembered is gone. Takes whatever already lies there; copies nothing.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn storage_choose(app: tauri::AppHandle, choice: Choice) -> Answer<StorageView> {
    let at = choice.locations()?;
    std::fs::create_dir_all(&at.profile)?;
    std::fs::create_dir_all(&at.worlds)?;
    claim(&at, &app.path().app_local_data_dir()?.join("storage-locks"))?;
    remember(&app, &at)?;
    // The network may be switched on in a profile found there.
    network::start(&app)?;
    Ok(view(&app, Some(&at), "ready", None))
}

/// Whether a folder already holds data of this app.
fn holds(dir: &Path, profile: bool) -> bool {
    if profile {
        return dir.join("identity.json").exists() || dir.join("worlds").exists();
    }
    std::fs::read_dir(dir)
        .map(|entries| entries.filter_map(|e| e.ok()).any(|e| e.path().join("world.json").exists()))
        .unwrap_or(false)
}

/// For the settings: whether the profile and the worlds would be taken from
/// there, rather than brought along.
#[derive(Serialize)]
pub struct Peek {
    pub profile: bool,
    pub worlds: bool,
}

#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn storage_peek(choice: Choice) -> Answer<Peek> {
    let at = choice.locations()?;
    Ok(Peek { profile: holds(&at.profile, true), worlds: holds(&at.worlds, false) })
}

/// Copies a folder's content, but this app's lock.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<usize> {
    std::fs::create_dir_all(to)?;
    let mut copied = 0;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        if entry.file_name() == ".lock" {
            continue;
        }
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copied += copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &target)?;
            copied += 1;
        }
    }
    Ok(copied)
}

#[cfg(feature = "gui")]
/// Changes where the profile or the worlds lie. With `carry`, this device's
/// data is copied there first — into a folder that holds none of it yet.
/// Without, the folder is used as it is. The old folder stays either way.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn storage_move(app: tauri::AppHandle, choice: Choice, carry: bool) -> Answer<StorageView> {
    let old = store(&app)?;
    let old = Locations { profile: old.profile_dir().to_path_buf(), worlds: old.worlds_dir(), additional: old.local_roots().iter().skip(1).cloned().collect() };
    let at = choice.locations()?;
    for (from, to, profile, what) in
        [(&old.profile, &at.profile, true, "profile"), (&old.worlds, &at.worlds, false, "worlds")]
    {
        if from == to || !carry {
            continue;
        }
        if to.starts_with(from) || from.starts_with(to) {
            return Err(AppError(format!("The new {what} folder lies inside the old one, or the other way round.")));
        }
        if holds(to, profile) {
            return Err(AppError(format!("There already are {what} in {}.", to.display())));
        }
        copy_tree(from, to)?;
    }
    std::fs::create_dir_all(&at.profile)?;
    std::fs::create_dir_all(&at.worlds)?;
    claim(&at, &app.path().app_local_data_dir()?.join("storage-locks"))?;
    remember(&app, &at)?;
    network::start(&app)?;
    Ok(view(&app, Some(&at), "ready", None))
}

#[cfg(feature = "gui")]
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn storage_locations(app: tauri::AppHandle, paths: Vec<String>) -> Answer<StorageView> {
    let mut at = remembered(&app)?.ok_or_else(|| AppError("Choose storage first.".into()))?;
    let mut additional = Vec::new();
    for path in paths {
        let path = PathBuf::from(path);
        if !path.is_absolute() { return Err(AppError("Choose an absolute folder path.".into())); }
        if !path.is_dir() && !at.additional.contains(&path) {
            return Err(AppError(format!("Folder is not available: {}", path.display())));
        }
        if path != at.worlds && !additional.contains(&path) { additional.push(path); }
    }
    at.additional = additional;
    claim(&at, &app.path().app_local_data_dir()?.join("storage-locks"))?;
    remember(&app, &at)?;
    network::republish();
    Ok(view(&app, Some(&at), "ready", None))
}

#[cfg(feature = "gui")]
/// Shows a folder in the system's file manager — the profile, the worlds, or
/// a shared folder; nothing else.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn storage_open(app: tauri::AppHandle, path: String) -> Answer<()> {
    let store = store(&app)?;
    let path = PathBuf::from(path);
    // A world's readable copy too: `<location>/<world>/text`.
    // Or one's own beside the profile: `<profile>/worlds/<world>/text`.
    let copy = path.file_name().is_some_and(|n| n == "text")
        && path
            .parent()
            .and_then(|w| w.parent())
            .is_some_and(|root| store.local_roots().iter().any(|r| r == root) || root == store.profile_dir().join("worlds"));
    let known = path == store.profile_dir()
        || store.local_roots().contains(&path)
        || copy;
    if !known || !path.is_dir() {
        return Err(AppError("That folder is not there.".into()));
    }
    reveal(&path)
}

/// Opens a folder in the file manager.
pub(crate) fn reveal(path: &Path) -> Answer<()> {
    #[cfg(target_os = "windows")]
    let opener = "explorer";
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let opener = "xdg-open";
    std::process::Command::new(opener)
        .arg(&path)
        .spawn()
        .map_err(|e| AppError(format!("The file manager did not open: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_leaves_the_lock_behind() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("a");
        std::fs::create_dir_all(from.join("world-1/control")).unwrap();
        std::fs::write(from.join("world-1/world.json"), "{}").unwrap();
        std::fs::write(from.join("world-1/control/x.json"), "{}").unwrap();
        std::fs::write(from.join(".lock"), "").unwrap();
        assert_eq!(copy_tree(&from, &dir.path().join("b")).unwrap(), 2);
        assert!(holds(&dir.path().join("b"), false));
        assert!(!dir.path().join("b/.lock").exists());
    }

    #[test]
    fn one_folder_for_both_is_recognised() {
        let base = PathBuf::from("/data/aw");
        let both = Locations { profile: base.join("profile"), worlds: base.join("worlds"), additional: Vec::new() };
        assert_eq!(together(&both).as_deref(), Some("/data/aw"));
        let apart = Locations { profile: base.join("profile"), worlds: PathBuf::from("/mnt/disk/worlds"), additional: Vec::new() };
        assert_eq!(together(&apart), None);
    }

    #[test]
    fn world_locks_are_local_and_still_exclude_other_instances() {
        let dir = tempfile::tempdir().unwrap();
        let at = Locations { profile: dir.path().join("profile"), worlds: dir.path().join("worlds"), additional: Vec::new() };
        std::fs::create_dir_all(&at.profile).unwrap();
        std::fs::create_dir_all(&at.worlds).unwrap();
        let root = dir.path().join("local-locks");
        assert!(claim(&at, &root).is_ok());
        assert!(!at.worlds.join(".lock").exists());
        let path = world_lock_path(&root, &std::fs::canonicalize(&at.worlds).unwrap());
        let other = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        assert!(matches!(other.try_lock(), Err(std::fs::TryLockError::WouldBlock)));
        *in_use().lock().unwrap() = None;
        other.try_lock().unwrap();
        // Private data must not be placed inside a shareable world tree.
        let nested = Locations { profile: at.worlds.join("profile"), worlds: at.worlds, additional: Vec::new() };
        std::fs::create_dir_all(&nested.profile).unwrap();
        assert!(claim(&nested, &root).is_err());
    }

    #[test]
    fn a_second_copy_finds_the_folders_busy() {
        let dir = tempfile::tempdir().unwrap();
        let file = std::fs::File::create(dir.path().join(".lock")).unwrap();
        file.try_lock().unwrap();
        let other = std::fs::OpenOptions::new().write(true).open(dir.path().join(".lock")).unwrap();
        assert!(matches!(other.try_lock(), Err(std::fs::TryLockError::WouldBlock)));
    }
}
