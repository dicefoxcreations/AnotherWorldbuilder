# Storage locations and the world library

As of October 2026. Code: `profile.rs`, `store.rs`, `network.rs`; interface:
`Storage.tsx`, `MainMenu.tsx`.

## Profile and worlds

| Place | Content |
| --- | --- |
| `profile/identity.json` | one's identity: name, id, signing key and its second key |
| `profile/network.json` | network settings: local network, internet, port, servers, the worlds offered, the worlds worked in on servers and through mailboxes |
| `profile/endpoint.key` | this device's network key — its code stays the same |
| `profile/worlds/world-<id>/` | what this device alone keeps of a world: where its backups go, its dice log, a mailbox's state |
| `profile/host/server.json` | the app's own server: who came, who is turned away, where a world went |
| `logs/` beside the profile | the app's log, a file a day, the last fourteen kept (`applog.rs`): its start, commands that failed, refused mailbox changes. Names worlds and commands, never their text. *Options → About → Open the log folder* |
| `<location>/world-<id>/` | a world: `world.sqlite` and `media/` ([world format 5](world-format.md)) |

Worlds on a server and those of a mailbox's player lie nowhere here but
in `network.json` (where they are) — a player's mailbox world has its own
copy in the default location, made anew with each delivery.

**No migration.** Worlds of earlier formats are not read; see
[world format 5](world-format.md#no-migration).

## Several local storage locations

On first start, the profile and the default world folder are chosen. Under
**Settings → Storage → Add world locations** more folders can be added — an
external drive, a network drive. What is chosen is the parent folder the
`world-<id>` folders lie in.

- The profile and local world roots must not lie inside each other, nor
  overlap.
- Locations not available stay listed and are not created anew. An
  unavailable default location leads to the storage choice at start.
- **Remove location** removes the assignment; the files stay.
- The same world id in two locations is reported as a conflict.
- **Move** for the profile or the default location copies the data on
  request; the old folder stays.

The choice is stored in `locations.json` in the system's configuration
folder (or under `AW_PROFILE_ROOT`). Locks in the profile and in
`storage-locks/` keep two copies of the app from using the same folders.

## The world library

The start page shows the worlds on this device, those on other devices (a
GM's app, a mailbox) and each server with its worlds. **New world:** on this
device, in another location, or on a server one may bring worlds to.
**Open world:** add a folder, restore a backup, or open a mailbox.

## Backing up

The profile — above all `identity.json` — and the world folders belong in
the personal backup. Without the identity, one is someone else to the worlds
one plays in. *Profile → Back up keys …* writes the identity into a file
`.awkeys`, locked with a passphrase (Argon2id, then XChaCha20-Poly1305; at
least 10 characters); *Restore your keys from a backup* takes it back on a
new device. A world itself is backed up as a [backup](backups.md).

Deleting a world removes its folder and this device's private data of it. A
world on a server: its GM deletes it there; a player leaves it.
