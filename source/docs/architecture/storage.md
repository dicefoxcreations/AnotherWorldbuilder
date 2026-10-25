# Storage locations and world library

As of October 2026. Code: `profile.rs`, `store.rs`, `network.rs`; interface: `Storage.tsx`, `MainMenu.tsx`.

## Profile and worlds

| Place | Content |
| --- | --- |
| Profile | Identity and private keys in `identity.json`, avatar, network settings in `network.json` |
| `profile/worlds/world-<id>/` | Private world data: `state.json`, `owner.key`, `keyring.json`, `requests.json`, `handover.json` |
| `profile/worlds/world-<id>/drafts/`, `index.json` | Drafts of running sessions; what the device has already checked of the versions |
| `logs/` beside the profile | The app's log, a file a day (`2026-10-25.log`), the last fourteen kept (`applog.rs`): its start, commands that failed, a server world's reads and saves, errors of the page. Names worlds and commands, never their text or keys. *Options → About → Open the log folder*. Before a profile is chosen, beside `locations.json` |
| Local world folder `world-<id>/` | Public descriptor `world.json`, the entries in a state file and update files ([world format 4](world-format.md)), images under `media/`; contents and key grants are encrypted |

Private data is assigned by world ID, independent of the storage location. A copy of a world folder carries no GM keys. Opened journals and decrypted views are kept apart per profile.

**No automatic migration.** Earlier development formats are not converted. Old world folders stay unchanged and are not loaded as current worlds. In this development phase new worlds are created.

## Several local storage locations

On first start, the profile and the default world folder are chosen. Under **Settings → Storage → Add world locations** several more folders can be added at once. What is chosen is always the parent folder in which the `world-<id>` folders lie.

- Folders kept in step by a cloud or sync app are ordinary storage locations. That app syncs the files; Worldbuilder knows no provider.
- The profile and local world roots must not lie inside each other. World roots must not overlap or name the same folder through different paths.
- Additional drives that are not available stay visible in the settings and are not created anew. An unavailable default location leads to the storage choice at start.
- **Remove location** removes the assignment; files and private keys are kept.
- Duplicate world IDs in several active storage locations are reported as a conflict.
- **Move** for the profile or the default location copies the data on request; the old folder is kept. Additional storage locations stay assigned.

The choice is stored in `locations.json` in the system configuration folder (or under `AW_PROFILE_ROOT`). The profile lock lies in the profile; locks for world roots are in the local app data folder under `storage-locks/`. They coordinate instances of the same operating system user, not different devices. The same identity should not write on several devices at the same time.

## Shared folders and world overview

The start page shows all worlds from all storage locations and, with the LAN switched on, offers from the local network. A world in one's own default location is called **Local**; a world in an additional storage location carries a cloud symbol and the name of that folder: it lies in a **shared folder** that the group has in common through its cloud app.

- **New world:** choose the storage location – "This device" or "Shared". **Add a folder …** takes in more folders.
- **Open world:** open worlds, add folders or import an `.awworld` file.
- Whoever has the shared folder has the world: it appears by itself as soon as the cloud app has brought its files. Only those the GM gave keys can read it.
- In the shared folder the devices also arrange the direct connection (`peers/`, [directly over the internet](../testing/internet-direct.md)). Then changes go directly from device to device in seconds; the files in the folder catch up whenever the cloud app gets there.

The former direct tie-in to one cloud provider (OAuth, provider API, offline levels, loading on demand) was removed on 2026-10-20: it required the provider to review the app and tied the app to one provider.

## Backing up and deleting

The **whole profile and all local world folders** belong in the personal backup. Without the identity, world token and attribute keys, protected contents cannot be recovered. Private files are not additionally encrypted and are written with mode `0600` on Unix.

**Backing up keys:** Profile → *Back up keys …* writes a file `.awkeys` with the identity and what only this profile has per world (`owner.key`, `keyring.json`, `state.json`, `requests.json`, `handover.json`) – encrypted with a passphrase (Argon2id, 64 MiB, 3 passes; then XChaCha20-Poly1305, `aw_crypto::passphrase`), at least 10 characters. The world folders are not in it; they come from their folder. On a new device or after reinstalling: *Restore your keys from a backup* on the first screen. Restoring does not take over a foreign profile (another identity) and overwrites no existing file.

**Readable copy:** every document as text, every image as a file – readable, searchable and editable without the app. Unencrypted in the world folder (`text/`), otherwise beside the profile on request; see [journal](journal.md#encrypted-or-not).

**Projection:** `projection.json` in the private world folder holds what was built from the journal at the last start (per document a fingerprint of heads, generations and held keys). At the next start only what no longer matches its fingerprint is rebuilt – for 11,000 documents 0.6 s instead of more than 20 s. The contents lie in it as ciphertext as in the world folder, but the type and short name of the documents this device can open are in plain text – like everything in the profile (keyring, identity), only on this device. It may be deleted at any time.

Deleting a world removes its world folder and this device's private world data. If the world lies in a shared folder, the cloud app deletes it there for everyone.
