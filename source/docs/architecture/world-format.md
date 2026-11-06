# World format 5: a world as a plain database

As of October 2026. Code: `crates/aw-store/src/db.rs` (the database, the
one view check, merging), `apps/desktop/src-tauri/src/store.rs` (worlds,
settings, who a command runs for), `document.rs` (opening, saving,
versions), `store/images.rs` (pictures).

## Layout

```text
world-<id>/
  world.sqlite        the world: documents, types, members, versions, gallery
  media/              pictures as they came, with a small preview beside each
```

That is all. Any SQLite program opens `world.sqlite`; every text, rule and
setting stands in it as plain text. Nothing is encrypted or packed: the
world's file is its readable copy. Whoever may read the folder may read the
world — so who sees what is decided where the world is held (on this device,
by a server — see [the network](network.md)), and a stolen drive is answered
by the operating system's disk encryption.

## Tables

| Table | Rows |
| --- | --- |
| `world` | settings by key: `id`, `name`, `icon`, `home`, `calendar`, `navigation`, `moderated`, `version_limit`, `dice`, `listed`, `roles`, `takeovers` |
| `members` | id, name, role (`gm`, `supporter`, `player`), attributes, colour, trusted, signing key, mailbox (`slot`), joined |
| `types` | the types, each as its JSON definition, in order |
| `documents` | id, type, slug, made and changed (when, by whom), proposed |
| `layers` | the parts of each document: base or not, section, title, `view`, `edit`, text, fields, pictures named |
| `versions` | one row per editing session: author, from–to, `counted` / `waiting` / `rejected`, the layers it touched before and after |
| `images`, `folders` | the gallery: file, preview, name, folder, size, uploader, defaults; the GM's folder tree |
| `requests`, `invited` | who asked to join; whom the GM let in ahead of time |

A document is a stack of **layers**. The base layer carries its name, its
properties and its own rule; every section, and every region with a rule of
its own inside a section (`<block view=gm>…</block>`), is a layer. A rule is
written as people write it: `all`, `players`, `gm`, `class:rogue OR gm`,
`doc:<id>`, `self` — see [assigning players](assignment.md).

## Who sees what: one check

`Reader` (`db.rs`) is the one place that decides. A reader holds `all`,
`players`, their attributes and `player:<name>`; the GM holds everything.
`documents_for(reader)` gives each document without the layers the reader may
not see, and nothing at all of a document whose base they may not see. Every
command that shows something — the page, the editor, the gallery, a package,
a mailbox's copy — goes through it; there is no second check in the
interface.

## Saving and writing together

The editor gets a document as text with, for each layer, the hash of its
state as handed out. Saving sends the text back with those hashes:

- What changed in a layer is measured against the state the editor saw —
  never against now — and merged letter by letter with what others wrote
  since (`merge`, Automerge in memory). Two people typing in one paragraph
  both keep what they wrote.
- A layer the writer may not edit stays as it is; a part with a rule of its
  own is the GM's to add or change (who sees it a writer may set, who edits
  it not).
- The handed-out states live in memory (the last 20,000). Written elsewhere —
  in a [mailbox](mailboxes.md) — a save carries its states along, and the
  holder takes them in under their hash.

## Versions

Every editing session is one version: the layers it touched as they were the
first time it touched them, and as it left them.

- **Review:** with *Review changes* on, what a player who is not trusted
  writes waits (`waiting`). The document stays as it was for everyone else;
  its author sees it as written. The GM accepts or turns it down.
- **Going back** puts a version's change back onto the document as it stands
  now, merged with what others wrote since.
- **Limit:** the GM sets how many versions each document keeps (50 at
  first); the oldest counted ones go. Waiting ones stay. The GM may delete
  older versions by hand, never the newest.

## Moving and copying

A world is its folder. Moving it into another folder of this device copies
the folder and lets the old one go; onto a server the database and the
pictures travel in pieces, and it lies there alone ([the network](network.md)).
A copy is a new world with a new id: types, documents and pictures, without
players and versions.

## No migration

Worlds of earlier formats are not read. A world of format 4 comes over as a
package: exported by the app that holds it (*Options → Transfer → Export*,
everything), taken into a new world here (*Import*). Types, documents with
their rules, pictures, navigation, calendar and start page come along;
members are added again, versions start anew.
