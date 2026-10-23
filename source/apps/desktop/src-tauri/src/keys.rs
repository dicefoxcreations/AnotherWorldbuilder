//! A backup of one's keys, for another device or a new start: the identity
//! and, of every world, what only this profile keeps — the game master's
//! world token and keyring, the members and invitations. The worlds
//! themselves are not in it; they come from their folder.
//!
//! Locked with a passphrase (`aw_crypto::passphrase`): whoever finds the
//! file needs it, and nobody can bring it back without.

use super::*;
use std::collections::BTreeMap;

const FORMAT: &str = "aw-keys";
/// What of a world's private folder goes along: what cannot be had again.
/// Drafts and the index are this device's own work in progress.
pub(crate) const KEPT: [&str; 6] = ["owner.key", "keyring.json", "state.json", "requests.json", "handover.json", crate::store::TAKEOVERS];
/// Short passphrases are guessed, however slow each guess.
const SHORTEST: usize = 10;

#[derive(Serialize, Deserialize)]
struct Backup {
    format: String,
    version: u32,
    /// `identity.json` as it lies.
    identity: String,
    /// World folder name → file name → contents.
    worlds: BTreeMap<String, BTreeMap<String, String>>,
}

#[cfg(feature = "gui")]
/// Writes the backup to `path`, locked with `passphrase`.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn keys_backup(app: tauri::AppHandle, path: String, passphrase: String) -> Answer<String> {
    let locked = backup(&store(&app)?, &passphrase)?;
    let path = with_extension(&path, "awkeys");
    aw_store::durable::write(&path, &locked)?;
    Ok(path.display().to_string())
}

/// The backup, locked.
fn backup(store: &Store, passphrase: &str) -> Answer<Vec<u8>> {
    if passphrase.chars().count() < SHORTEST {
        return Err(AppError(format!("The passphrase needs at least {SHORTEST} characters.")));
    }
    let identity = std::fs::read_to_string(store.identity_path())?;
    let mut worlds = BTreeMap::new();
    if let Ok(dirs) = std::fs::read_dir(store.profile_dir().join("worlds")) {
        for dir in dirs.filter_map(|d| d.ok()).filter(|d| d.path().is_dir()) {
            let mut files = BTreeMap::new();
            for name in KEPT {
                if let Ok(text) = std::fs::read_to_string(dir.path().join(name)) {
                    files.insert(name.to_string(), text);
                }
            }
            if !files.is_empty() {
                worlds.insert(dir.file_name().to_string_lossy().into_owned(), files);
            }
        }
    }
    let backup = Backup { format: FORMAT.into(), version: 1, identity, worlds };
    Ok(aw_crypto::passphrase::lock(passphrase, &serde_json::to_vec(&backup)?)?)
}

#[cfg(feature = "gui")]
/// Brings the keys back: the identity where this profile has none yet, and
/// of each world what is not here. What is here stays — a newer keyring is
/// never overwritten by an older one.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn keys_restore(app: tauri::AppHandle, path: String, passphrase: String) -> Answer<IdentityInfo> {
    let store = store(&app)?;
    restore(&store, &std::fs::read(&path)?, &passphrase)?;
    Ok(info_from(&store, &store.load_identity()?))
}

pub(crate) fn restore(store: &Store, locked: &[u8], passphrase: &str) -> Answer<()> {
    let plain = aw_crypto::passphrase::unlock(passphrase, locked).map_err(|_| AppError("Wrong passphrase — or not a key backup.".into()))?;
    let backup: Backup = serde_json::from_slice(&plain)?;
    if backup.format != FORMAT || backup.version != 1 {
        return Err(AppError("Not a key backup this version knows.".into()));
    }
    // Checked before anything is kept: keys that do not make their id are no identity.
    let probe = store.profile_dir().join(".identity-restore.json");
    aw_store::durable::write(&probe, backup.identity.as_bytes())?;
    let theirs = Identity::load(&probe);
    let _ = std::fs::remove_file(&probe);
    let theirs = theirs?;
    if store.has_identity() {
        let mine = store.load_identity()?;
        if mine.id() != theirs.id() {
            return Err(AppError("This profile already belongs to someone else. Restore into a new profile.".into()));
        }
    } else {
        store.save_identity(&theirs)?;
    }
    for (dir, files) in &backup.worlds {
        // Only world folders by their own name: nothing written outside.
        if !dir.starts_with("world-") || dir.contains(['/', '\\', '.']) {
            continue;
        }
        let into = store.profile_dir().join("worlds").join(dir);
        std::fs::create_dir_all(&into)?;
        for (name, text) in files.iter().filter(|(n, _)| KEPT.contains(&n.as_str())) {
            let target = into.join(name);
            if !target.exists() {
                aw_store::durable::write(&target, text.as_bytes())?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_come_back_into_a_new_profile_and_open_the_world() {
        let dir = tempfile::tempdir().unwrap();
        let here = Store::new(dir.path().join("here"));
        let gm = Identity::create("Marla").unwrap();
        here.save_identity(&gm).unwrap();
        let world = new_world(&here, &gm, "Eldermere".into()).unwrap().public.world_id.to_string();
        assert!(backup(&here, "short").is_err(), "a short passphrase is refused");
        let locked = backup(&here, "a long passphrase").unwrap();

        // A new install: the same worlds folder, an empty profile.
        let there = Store::at(dir.path().join("new-profile"), here.worlds_dir());
        assert!(restore(&there, &locked, "the wrong passphrase").is_err());
        restore(&there, &locked, "a long passphrase").unwrap();
        assert_eq!(there.load_identity().unwrap().id(), gm.id());
        assert!(there.is_gm(&world), "the world token came back");
        assert!(!there.load_articles(&world).unwrap().is_empty());

        // Someone else's profile is not taken over.
        let other = Store::at(dir.path().join("other"), here.worlds_dir());
        other.save_identity(&Identity::create("Eve").unwrap()).unwrap();
        assert!(restore(&other, &locked, "a long passphrase").is_err());
    }
}
