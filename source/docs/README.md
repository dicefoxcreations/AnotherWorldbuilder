# Documentation

## Development

- [Project layout](development/project-layout.md): responsibilities, paths and where things go.
- [Building and distributing](development/builds.md): local Linux and Windows builds and finished packages.
- [Interface](development/ui.md): colour tokens, button variants and the rules for them.
- [Licenses](development/licenses.md): which libraries are inside, notices in the app, open points before a release.

## Architecture

- [The journal of a world](architecture/journal.md): signed entries, read and write keys, versions, writing together.
- [Storage locations](architecture/storage.md): profile and worlds, the choice on first start, moving.
- [World format 3](architecture/world-format.md): one file per version, drafts in the profile, version limit, deleting.
- [Images](architecture/images.md): gallery, names, visibility through sections, metadata removed, originals in parts.
- [Windows beside](architecture/windows.md): documents and sections in their own windows, kept in step with each other.
- [Calendar and timeline](architecture/calendar-timeline.md): world calendar, date with time, `<timeline>`, lists from properties, family trees.
- [Property groups](architecture/property-groups.md): properties that several types include (`Scores: group Abilities`), nested, with a layout (`<view of=…>`) and computed values (`= [[…]]`, `Class.Hit die`).
- [Stat blocks, dice and bonuses](architecture/stat-blocks.md): `style=statblock`, `<abilities>`, `[[…]]` and dice in the text, dice tray with log and favourites, 3D dice, `adds`.
- [Maps](architecture/maps.md): `<map>` to pan and zoom, pins with a name and a target, layers that switch to the map of another document.
- [Assigning players](architecture/assignment.md): documents, types and attributes per player, `view=self` for what belongs to the assigned player.

## Testing

- [Finding and joining worlds on the LAN](testing/lan-join.md): network switch, join requests, a test run with two profiles.
- [Local and mounted cloud folders](testing/folder-sync.md): several storage locations and working directly on world folders.
- [Directly over the internet](testing/internet-direct.md): router port, STUN, meeting through the shared folder, hole punching without a relay.
- [Host mode](testing/host.md): players work online against the GM's device, without their own copy; the host enforces views and rights.
- [Server on a NAS](testing/server.md): `aw-server` – a door keeper without keys for the worlds of several GMs, with an admin; Docker image for Synology.
- [Browser test bench](testing/browser-testbench.md): checking the UI with real components and the Rust parser.
