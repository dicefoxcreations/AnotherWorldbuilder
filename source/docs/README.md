# Documentation

- [Running a server](server-setup.md): `aw-server` with Docker, on any machine or a Synology NAS.

## Development

- [Project layout](development/project-layout.md): responsibilities, paths and where things go.
- [Building and distributing](development/builds.md): local Linux and Windows builds and finished packages.
- [Interface](development/ui.md): colour tokens, button variants and the rules for them.
- [Licenses](development/licenses.md): which libraries are inside, notices in the app, open points before a release.

## Architecture

- [World format 5](architecture/world-format.md): a world as a plain SQLite database, the one view check, writing together, versions and review.
- [The network](architecture/network.md): one place per world, the keeper, servers and the GM's app, finding, joining, moving, the admin.
- [Mailboxes](architecture/mailboxes.md): worlds without a server, through a cloud folder; switching to and from a server.
- [Storage locations](architecture/storage.md): profile and worlds, the choice on first start, moving.
- [Images](architecture/images.md): gallery, names, visibility through sections, metadata removed, originals unchanged.
- [Windows beside](architecture/windows.md): documents and sections in their own windows, kept in step with each other.
- [Calendar and timeline](architecture/calendar-timeline.md): world calendar, date with time, `<timeline>`, lists from properties, family trees.
- [Property groups](architecture/property-groups.md): properties that several types include (`Scores: group Abilities`), nested, with a layout (`<view of=…>`) and computed values (`= [[…]]`, `Class.Hit die`).
- [Stat blocks, dice and bonuses](architecture/stat-blocks.md): `style=statblock`, `<abilities>`, `[[…]]` and dice in the text, dice tray with log and favourites, 3D dice, `adds`.
- [Maps](architecture/maps.md): `<map>` to pan and zoom, pins with a name and a target, layers that switch to the map of another document.
- [Assigning players](architecture/assignment.md): documents, types and attributes per player, `view=self` for what belongs to the assigned player.
- [Links](architecture/links.md): `@@Type:Name@@`, a property's value, anchors in the text (`@@#^c70ff9|the inn@@`).
- [Backups](architecture/backups.md): on this device by hand; on a server daily or weekly by itself, how many stay; restoring.

## Testing

- [The network by hand](testing/network.md): two copies of the app, a world on the GM's app, a local `aw-server`, moving.
- [Mailboxes by hand](testing/mailboxes.md): a world without a server through a folder, a player offline, switching to a server.
- [Players: a new id](testing/players.md): a player's new id takes the old one's place, with what was assigned to them.
- [Browser test bench](testing/browser-testbench.md): checking the UI with real components and the Rust parser.
