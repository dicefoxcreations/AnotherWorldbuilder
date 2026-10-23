# Backups

As of October 2026. Code: `apps/desktop/src-tauri/src/backup.rs`; interface: `features/worlds/Backups.tsx` (*Options → World → Backups*), *Restore a backup …* in the main menu (`app/MainMenu.tsx`).

The GM backs a world up — by hand (*Back up now*), or daily or weekly by itself — and says how many stay (*Keep*, 7 at first; the oldest go). They go into `backups/` beside the worlds' folder, or a folder of one's choice, each world in a folder of its own: `<name> – world-<id>/2026-10-23 06-15-00 UTC/`.

**What is in one.** `world/` — the world's folder as it was, everything that is on this device, images too. `keys.sealed` — what only this profile keeps of the world (world token, keyring, members, invitations, requests, takeovers), encrypted with a data key that is sealed for the GM's identity: the backup opens with it alone and lies no more open than the world. `backup.awbackup` — world, name, time, the GM's id.

**Room.** The world's files are named after their content and never change. A file the last backup holds already becomes a hard link to it: a backup takes the room of what is new since the last one. On the same drive only; elsewhere it is copied.

**By itself.** A minute after the app starts and then every quarter of an hour it looks whether a world is due — a day or a week since its newest backup. Only while the app runs; a backup broken off is left as `.partial-…` and cleared the next time.

**Restoring.** *Restore a backup …* takes the `backup.awbackup` of a backup and brings the world back next to the other worlds, with its keys — where it is not here: a world that is here stays as it is. Only the GM who made the backup restores it; on a new device first their keys (*Restore keys* at the first start, from *Back up keys …* in the profile menu), then the backup.

A backup is for losing a world — a broken drive, a deleted folder. Going back to an earlier text is done by the versions of each document.

**Tests:** `backup::tests` — backed up twice, the second links the unchanged files; two kept; not due again the same day; deleted and restored, the world opens with its page as before; someone else's identity does not open it.
