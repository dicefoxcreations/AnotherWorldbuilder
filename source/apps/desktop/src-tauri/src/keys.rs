//! A backup of one's keys, for another device or a new start: the identity
//! — what proves to every world one is its GM, a supporter or a player. The
//! worlds themselves are not in it; they come from their folder.
//!
//! Locked with a passphrase (`aw_crypto::passphrase`): whoever finds the
//! file needs it, and nobody can bring it back without.

use super::*;

const FORMAT: &str = "aw-keys";
/// The identity alone — worlds keep no keys of their own any more.
const VERSION: u32 = 2;
/// Short passphrases are guessed, however slow each guess.
const SHORTEST: usize = 10;

#[derive(Serialize, Deserialize)]
struct Backup {
    format: String,
    version: u32,
    /// `identity.json` as it lies.
    identity: String,
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
    let backup = Backup { format: FORMAT.into(), version: VERSION, identity };
    Ok(aw_crypto::passphrase::lock(passphrase, &serde_json::to_vec(&backup)?)?)
}

#[cfg(feature = "gui")]
/// Brings the identity back, where this profile has none yet.
#[cfg_attr(feature = "gui", tauri::command(async))]
pub fn keys_restore(app: tauri::AppHandle, path: String, passphrase: String) -> Answer<IdentityInfo> {
    keys_restore_in(&store(&app)?, path, passphrase)
}

/// `keys_restore` for whom the store is given.
pub(crate) fn keys_restore_in(store: &Store, path: String, passphrase: String) -> Answer<IdentityInfo> {
    restore(&store, &std::fs::read(&path)?, &passphrase)?;
    Ok(info_from(&store, &store.load_identity()?))
}

pub(crate) fn restore(store: &Store, locked: &[u8], passphrase: &str) -> Answer<()> {
    let plain = aw_crypto::passphrase::unlock(passphrase, locked).map_err(|_| AppError("Wrong passphrase — or not a key backup.".into()))?;
    let backup: Backup = serde_json::from_slice(&plain)?;
    if backup.format != FORMAT || backup.version != VERSION {
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identity_comes_back_into_a_new_profile_and_is_the_worlds_gm() {
        let dir = tempfile::tempdir().unwrap();
        let here = Store::new(dir.path().join("here"));
        let gm = Identity::create("Marla").unwrap();
        here.save_identity(&gm).unwrap();
        let world = new_world(&here, &gm, "Eldermere".into()).unwrap().id;
        assert!(backup(&here, "short").is_err(), "a short passphrase is refused");
        let locked = backup(&here, "a long passphrase").unwrap();

        // A new install: the same worlds folder, an empty profile.
        let there = Store::at(dir.path().join("new-profile"), here.worlds_dir());
        assert!(restore(&there, &locked, "the wrong passphrase").is_err());
        restore(&there, &locked, "a long passphrase").unwrap();
        assert_eq!(there.load_identity().unwrap().id(), gm.id());
        assert!(there.is_gm(&world), "the world knows them as its GM");
        assert!(!there.db(&world).unwrap().document_ids().unwrap().is_empty());

        // Someone else's profile is not taken over.
        let other = Store::at(dir.path().join("other"), here.worlds_dir());
        other.save_identity(&Identity::create("Eve").unwrap()).unwrap();
        assert!(restore(&other, &locked, "a long passphrase").is_err());
    }
}
