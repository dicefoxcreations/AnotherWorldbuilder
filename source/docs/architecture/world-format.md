# World format 4: a state and updates

As of October 2026. Code: `crates/aw-store/src/shelf.rs` (where entries are kept), `crates/aw-store/src/journal/` (`mod.rs` entries and loading, `writing.rs` drafts, versions and cleaning up, `history.rs` version list), `store.rs` (end of session, limit, new base, the next state). Rights, keys, generations and review are in the [journal](journal.md). No migration in the app: worlds in format 3 (one file per entry) are turned into format 4 once with `cargo run --release -p aw-store --example convert_world -- <world folder> <profile folder of the world>`.

## Why

Up to format 2 every save (autosave after a 0.6 s pause) was a file of its own, all flat in `journal/`. To show a document, the app had to read and replay all entries of its sections; on opening it read the whole world.

Goals:

- **One file per version of a document.** It contains the whole document; one file is enough for the current state.
- **One layout** for local and shared folders — safe in a cloud folder that copies whole files, whether or not the folder is shared.
- **Few files:** a big world is one state file, not a hundred thousand small ones.
- **Limited storage:** the GM sets how many versions remain and can delete versions.
- **Live writing and presence stay** as they are.

## Layout

```text
world-<id>/
  world.json                                  public descriptor: world ID, public key
  state-<generation>-<hash>.sqlite            the entries, consolidated
  updates/<device>-<number>.awu               entries one device added since
  media/parts/                                images, in parts named by their bytes
```

Every entry is signed and named after the hash of its bytes, as before: GM entries (world, types, grants, key headers, verdicts, deletions), one complete entry per version of a document, the gallery, proposals and badges. Article IDs stand in plain text; title, type, slug and content stay encrypted.

**Nothing in the folder is ever changed in place.** A cloud folder copies whole files and cannot merge two writers, and a file may arrive in pieces:

- **The state** is a SQLite file with one row per entry (`records`: id, kind, article, arrival, bytes compressed with zstd) and what it holds of each device's updates (`meta`). Its name carries the hash of its bytes: a state that has not fully arrived does not check out, and the one before is read.
- **An update file** holds what one device wrote at once — a version, a change to the world — with a checksum at its end. Each device writes only its own files and numbers them without gaps. A file still on its way waits.
- **Consolidating:** the GM's device writes the next state once nobody wrote for a minute — the long part without holding the world, which stays open to work in: the last state, every device's updates in order up to the first gap, without what the GM deleted. Then it lets go of what the state before already held — whoever is a state behind still finds what they need.
- **Reading:** the newest whole state plus every update it does not hold.

A server, and a session reading a world from its server, keep the state alone: one SQLite file changed in place, transaction by transaction, without updates.

## The version file

A version is one session of one author: from "Edit" until leaving it, as before.

| Part | Plain text | Content |
| --- | --- | --- |
| Head | yes | world, article, predecessors (one or more version IDs), author, session, time |
| Sections | no | per section: generation and the Automerge state, encrypted with the section's data key, signed with its write key |
| Signature | – | the author's, over the whole file |

- **Unchanged sections are taken over as ciphertext**, with the write signature of their last writer. So a player who cannot read the GM section still writes a complete version.
- **Merging instead of replacing:** the state of a section is the Automerge merge of all blocks of that section in the version and its predecessors. A copied older block therefore turns nothing back.
- If the predecessors differ in a section the author cannot read, they take over all blocks; merging happens on reading.
- Waiting versions (review switched on) are sealed for GM and author; a `Verdict` names versions, the keys are under the name of the respective section block.
- **Who wrote what:** new changes in a version – those not in any predecessor – must be by the version's author. If they come from someone else (writing together on the LAN), that person's signed draft travels along in the version (`drafts`). A section that does not keep to this stays out. If a version builds on a deleted one, this can no longer be checked; then the author's word counts.

## Simultaneous versions

Every version names its predecessors. If two versions build on the same one (offline, at the same time), there are two **tips**. The app shows the merge of both; the next save names both as predecessors and closes the fork. Overlaps are marked with ⚠ as before.

A local **index in the profile** (`index.json`) remembers the head of each checked version – document, author, session, predecessors, sections – and which section blocks passed their check. On opening a world, the GM entries and the drafts are read, of the versions only the ids. On opening, a section reads only the tip(s); the version list and comparisons read the rest when opened. If a tip has not passed its check here yet, all versions of the section are read and checked in order. If the index is gone, that happens for all.

## Session and live writing

- During the session, small signed Automerge changes keep arising (with author and write key, as before). They lie **in the profile** – to survive a crash – and **not in the world folder**.
- On the LAN they go to the others with the frequent digest request, together with presence (who, which section, cursor; forgotten after 5 s). Presence in GM-only sections still reaches no player.
- Whoever joins in the middle of a session gets the tip plus the running changes.
- On leaving "Edit" – or automatically after a longer pause – every author writes their version. If three people write together, three versions arise that build on each other.
- After a crash, the open session is closed as a version from the profile at the next start.
- Only finished versions go through folders. Others see a change there after the session ends; directly connected, already while typing.

## Version limit

The GM sets per world how many versions remain per document (control entry `Versions { max }`, default 50).

- Cleaning up happens **in batches** on the GM's device: when a document clearly exceeds the limit, back down to the limit. If the GM's device is offline, versions pile up.
- Waiting versions do not count and are not dropped automatically. Tips are never dropped.
- **New base:** every version carries the Automerge history. So that deleting saves space, the GM sets a new base when cleaning up – a new generation of the sections with the same rights and the current state as the start, as with a change of rights. New versions build on it and no longer carry the old history. Older kept versions still carry it until they are dropped too.
- What was written offline on an older base is carried over as text, as with a change of generation (`carry`).
- Comparisons across a base go through the text, as across generations.

## Deleting versions

- Only the GM, only older versions. The newest cannot be deleted; for that there is *Undo*, which writes a new version.
- Deleting writes a signed control entry `Deleted { article, versions }`. Every device removes the named files and does not accept them again – otherwise the next device would put them back on syncing.
- Deleting by hand also sets a new base. The content of a deleted version can still be in the history of older kept versions until they are dropped.
- What someone has backed up or exported themselves is not reached by deleting.

## Control entries

Of every control item, the newest counts (`seq`), as before. When cleaning up, the GM removes outdated entries that no kept version needs any more, likewise with `Deleted`.

## What goes away

- One entry per autosave in the world folder.
- The flat `journal/` and `Journal::open`, which reads every file of the world.

## Settled

- A session ends on leaving "Edit", after 10 minutes without writing, and at the next start if the app was closed before.
- Cleaning up happens as soon as a document reaches 1.5 times the limit; immediately when the limit changes.
- Options → Basics → *Versions per document*; in the version list the GM deletes older versions with the bin.

## Open

- Very many small files: a cloud app syncs them one by one (one provider through rclone about 0.4 files per second, [measured](../testing/folder-sync.md)). Bundling entries into pack files would be the next step.
- Images and other attachments are not part of this format.
