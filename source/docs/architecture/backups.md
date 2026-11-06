# Backups

As of October 2026. Code: `apps/desktop/src-tauri/src/backup.rs`, the
server's in `keeper.rs`; interface: `features/worlds/Backups.tsx`
(*Options → World → Backups*), *Restore a backup …* in the main menu.

**On this device** the GM backs a world up by hand (*Back up now*); every
backup stays until the GM deletes it. They go into `backups/` beside the
worlds' folder, or a folder of one's choice, each world in a folder of its
own: `<name> – world-<id>/2026-10-23 06-15-00 UTC/`.

**What is in one.** `world/` — `world.sqlite`, copied whole while it goes on
being written, and the pictures. `backup.awbackup` — world, name, time, the
GM's id. Like the world, a backup is readable without the app.

**Room.** A picture never changes under its name: one the last backup holds
already is a hard link to it there. A backup takes the room of what is new
since the last one — on the same drive; elsewhere it is copied.

**A world on a server** its GM backs up here the same way, by hand: fetched
from the server whole (`Files { taking: false }` — it stays open), a picture
the last backup holds already linked, not fetched again.

**By itself — on a server.** A server backs up the worlds it holds, daily
unless their GM says otherwise (*Off / Daily / Weekly*, *Keep*, 7 at first;
the oldest go) — the world's GM or the server's admin sets it
(`Ask::Backups`). A minute after the server starts and then every quarter of
an hour it looks whether a world is due. They lie in `backups/` in the
server's data folder. To bring one back, stop the server and put its
`world/` back as `worlds/world-<id>/`.

**Restoring.** *Restore a backup …* takes the `backup.awbackup` of a backup
and brings the world back beside the others — where it is not here already.

A backup is for losing a world — a broken drive, a deleted folder. Going back
to an earlier text is what the versions of each document are for.

**Tests:** `backup::tests` — three backups by hand, the second links the
unchanged pictures, all stay; deleted and restored, the world opens as
before.
