# The journal of a world

As of October 2026. The sync design for the whole world; it replaces the earlier text prototype and the handing over of whole world packages. Code: `crates/aw-store/src/journal/`. How the entries are kept is described in [world format 4](world-format.md).

## Basic idea

A world is not a file that gets overwritten but a set of **signed entries**. Every entry is named after the hash of its bytes and is never changed: control entries, and one entry per version of a document, kept in the world's state and update files ([world format](world-format.md)). What a session writes until its end lies in the profile as a draft. Two devices compare what they have by the names and exchange what is missing – over the LAN, directly over the internet or through a [shared folder](../testing/folder-sync.md). Every device checks every entry itself, no matter who passed it on.

| Kind | Who writes | Signed by | Content |
| --- | --- | --- | --- |
| **Control** | only the GM | world token | world name, calendar, icon, start page; types; which articles and sections exist; key headers; grants (attribute keys per member) and their revocation on removal (`revoked`, replaces the grant); review, trust, verdicts |
| **Version** | whoever edited the document | the author; every section by the write key of whoever wrote it | the whole document after a session: every section as an encrypted Automerge document |
| **Draft** | whoever may edit the section | the author **and** the section's write key | encrypted Automerge changes of a running session to one section; only in the profile and on the LAN |

**Everything goes to everyone.** Every member gets every entry; what they can read is decided by their keys alone. There is no filter on delivery any more – which is why every device, a relay or a shared folder can pass on everything without knowing who may do what. For this to hold, nothing revealing stands in plain text:

- **What a document is** (type and slug) lies encrypted in the head of its base section (`About`). Whoever cannot open the document only sees *that* there is one.
- **Grants** are sealed (`Sealed`) – for the member and the GM who wrote it. Nobody else sees who has which attributes.
- **Waiting proposals** are sealed for GM and author, not encrypted with the section's key (see *Review*).
- **GM-only sections** travel along, readable only for the GM. The level (`GmOnly`, `Sealed`, `Open`) now only says whether players are ever meant to read them; presence in a GM-only section still does not go to players.
- **Key headers name nobody.** They carry neither the rule nor attribute names: a slot is addressed with blinded identifiers that only someone holding the attribute key can compute (HKDF over key and salt). Every header has its own salt – the same attribute looks different in every header, so sections cannot be matched to each other through it. The rule itself stands encrypted in the section.
- **Types follow the rights** (`journal/types.rs`): the frame of a type – name, fields and parts for everyone – is sealed for all members; every part and every property with a view of its own is sealed for exactly that view, and a block whose parts are all restricted for everyone who may see one of them. Whoever opens frame and parts puts together what they may see; the GM gets the whole type. After keys are rotated, the types are sealed anew.

What stays open: number, size and time of the entries, and the level of a section. Whoever may read a section also sees its whole rule – with `class:rogue OR knows:x` also the name of the second attribute.

Of every control item the newest entry counts (GM counter `seq`). Versions and drafts merge by CRDT: whoever writes at different places at the same time keeps both. Every change carries its session and its time itself – which version it belongs to stays recognisable, no matter which file it stands in.

`world.json` contains nothing but the format version and the public world identifier including the public signing key. The GM's member list and advance invitations are under `profile/worlds/<world>/state.json`; private keys also stay in the profile. The [storage locations](storage.md) describe the separation. `articles.json`, `templates.json`, `held.json` no longer exist – everything shared stands in the journal.

## Players create documents

Which documents and sections exist, and their key headers, only the GM writes (control). If a player may create documents of a type (`create=` on the type, e.g. `players`), they write a **proposal** instead (`Proposal`, `proposals/<hash>.json`): the document as they filled it in, sealed for the GM and themselves, signed by them. It travels like everything else, on the LAN and through folders; every device accepts it only from a member. Other members have the file but cannot read it.

As soon as the GM's device has it (at the sync, which runs every 2 s anyway), it creates the document (`answer_proposals`):

- only if the author is a member and the type allows them to create (their attributes plus `players` and `all`);
- with the rights of the **type**, not those asked for: base section and properties like `<article>`, properties with a view of their own like that view, every section as in the type. A section the type does not know is dropped.

Afterwards the GM deletes the proposal (`Deleted`), accepted or not; it disappears on all devices.

**At once at the player's:** the player's device picks the document ID itself and sends it along in the proposal; the GM's device creates the document under exactly this ID (`create_document_as`). Until then the player's device shows its own waiting proposals as documents (`pending_views`): in lists, in the read view with "Waits for the GM", as the target of links. Links in their texts already become this ID when proposing (`known_to_me` knows the waiting ones). So they can create and link character, NPCs and home, even if the GM's device is not online for two weeks; the others see it as soon as the GM's device has worked through the proposals, and all links are right.

- **Images:** in the form, image fields and texts take images, uploaded or from the gallery; the GM's device takes over their keys on creating (see [images](images.md)).
- **Changing:** ✎ opens a waiting document in the form it was asked with. Saving sends a new proposal with the same ID; the newest counts. The editor and versions exist only once the document has been created.
- If a proposal only arrived when the document was already created, what was created stays; changes then go through the editor.
- **Who sees a part, by an editor:** whoever may edit a part and is not the GM picks who sees it — the document's head, a section, a part; who edits it stays the GM's. In an encrypted world the keys are the GM's: the editor's device writes a proposal (`Asked.shown`), the GM's device sets the view when it has it (`set_shown`) — if the editor still may edit the part.
- **A type's access for its documents:** the type editor's preview sets who sees, edits and makes documents of a type; *Apply to N documents* gives who sees and edits to the documents there are (`type_access_apply`), with their sections that had the same access — a section with access of its own keeps it.

## Reading and writing are separate keys

Every section has, per **generation**:

- a **data key**, wrapped for the view rule (as before),
- a **write key pair**: the private part wrapped for the edit rule, the public part open in the header.

A change is co-signed by the write key. Every device checks it against the public part – without knowing who has which attributes. Without the right there is no valid signature; such a change is refused everywhere (test `a_change_without_the_write_key_is_refused`).

If the GM changes who may see or edit a section, a **new generation** begins with fresh keys; its start state is the previous content. Whoever loses a right keeps the past but can neither read nor write what follows.

**Written offline while the rights changed:** such changes belong to the old generation and would otherwise only stand in the history. As soon as the author's device learns of the new generation, it carries its own lines over – as the same version, by the same author (test `what_bob_wrote_offline_reaches_the_text_even_when_access_changed_meanwhile`). If the author may no longer write there, the version stays marked "not in the text yet", and the GM can take it over with *Take into the text*.

## Versions

From "Edit" until leaving is one **session** – one version. Saving happens automatically while typing (after a 0.6 s pause, when writing together after 0.15 s; whoever keeps typing without a pause at least every second – so the others see the text coming); there is no more save and discard. Per document:

- **Versions**: a list of all sessions with author, time and changed parts.
- **Comparison in the document**: a click on a version places it in the document itself beside the current state – this version on the left, now on the right. In read mode as finished pages, in edit mode as code or visually (a switch instead of "one/two pages"); the right side stays editable. What has gone since stands red on the left, what has been added green on the right, within changed lines word by word. (What is written is letter by letter, the fewest edits — `diff::edits`: what stays is never taken out and put back, so what someone else writes beside it at the same moment stays where they wrote it.) In text fields the marks lie on a congruent layer behind the field (`shared/editor/MarkedField.tsx`); on the page the Markdown rendering draws them (`shared/Marks.ts`), without cutting links, tags or headings apart. The state of a version comes from the journal: every section as it stood after that session or at its time (`Journal::article_at`). Times in the journal are milliseconds.
- **Arrived later**: every version stands at the time it was written. If it only arrives days later (written offline), it also carries "arrived …", and the *Versions* button shows a number until the list has been opened; there it is marked "new" – otherwise it would get lost, because it stands *below* newer versions in the list. Undo and take-into-the-text are at the selected entry of the list. The arrival time is the time at which one's own device got the entry.
- **Undo**: takes back exactly the changes of this version, as a new change. What others wrote afterwards stays (the CRDT deletes the inserted characters themselves, not "what stands there now").
- **Marks**: what distinguishes a version from now is decided by the journal through the characters themselves: every character of an Automerge text has a fixed identifier, and inserted is what exists in one state and not in the other (`LayerDoc::marks`). A character deleted and typed again right away is therefore a new one. The ranges are carried into the document code, the section texts and the read page (`version_marks`). Only where a section changed its generation between the version and now do the identifiers not match; there the text is compared. While typing in the editor is not saved yet, the frontend compares by itself until the next save delivers new marks.
- **Saved as read**: when saving, the editor does not write Automerge's smallest character difference but the difference as a person reads it (`journal/diff.rs`): first lines, in them words, and letters only for similar words (`aaaa` → `aaabbb`: one `a` gone, `bbb` new). Two words with little in common are replaced whole – otherwise single letters would remain.
- **Overlaps**: two sessions of different people who did not see each other (offline, at the same time) and changed the same lines or fields are marked with ⚠. Both stand in the text; the GM sees where she should look.

## Review: changes only after approval

The GM can switch on for a world that player changes only become visible to others after her approval (Options → Players → *Review changes*). Single members can be marked as *Trusted*; their changes count at once.

| Entry (control) | Meaning |
| --- | --- |
| `Moderation { on, settled }` | Review on/off. `settled` are the sessions that already existed when it was switched on – they keep counting as before. |
| `Trust { member, trusted, settled }` | A member whose changes count at once. On withdrawal their previous sessions stay (`settled`). |
| `Verdict { article, accepted, versions, keys }` | The GM's decision on waiting versions – accepted with the keys of their sections, encrypted with the key of the respective section. |

Decisions are made **per version**, that is after the end of a session. If a player keeps writing in the same session afterwards, a further version arises that waits again. A session is **waiting** while anything of it waits, **turned down** if what was decided was turned down, otherwise **counted** (`Review`).

- **What waits is sealed.** The author's device encrypts drafts and new sections of the version with a key of its own and wraps it only for GM and author. Everyone else has the file – like everything – but cannot read it.
- **Not in the text:** the visible state is made of all changes without the waiting and turned-down ones (`Journal::visible`); only the author sees their waiting ones. What the GM writes builds on this state – never on an open proposal.
- **Taking in** writes a `Verdict` with the keys of the sections, encrypted with the key of the respective section. From then on everyone who may read the section reads them. Versions it builds on go along (`Journal::bound`) – without them it would not stand.
- **Turning down** takes the version out of the text, at the author's too. Versions that build on it go along.
- In the version list a waiting version stands as *waits for review*. When selected it shows on the left the text as it would be with it: green, what it adds; on the right, what it takes out of the present text, red. The GM has *Take in* and *Turn down* there.

## Writing together

On saving, the editor writes only the difference against the state it started on (`base`: generation and heads per section). What others wrote meanwhile stays and comes back; typing during the save is laid on top (`shared/editor/Merge.ts`, the same rule in Rust for generation changes).

Where the others are writing right now travels as **presence** with the frequent digest request – never stored, forgotten after 5 s without renewal. The spot is an Automerge cursor: it stays at the character even when someone writes before it. Presence in GM-only sections reaches no player.

## Changing members and the GM

- **Removing**: the GM takes a member (or an advance invitation) off the list. Over the network it gets nothing more after that; every attribute it had is rotated. What it already has, it keeps.
- **Handing over the GM role**: the GM hands the whole world to a member – world token, keyring, member list, requests and all journal entries, the GM-internal ones too. The package is encrypted with a fresh key, and that key is sealed for the recipient (`HandoverPackage`). Whoever transports it – LAN, file, later a relay – sees only ciphertext.
  - **Over the network**: the handover waits (`handover.json`) until the recipient shows up; the digest tells them, they fetch the package (`Ask::Handover`). Then they write a new world entry naming them as GM and give it back at the next sync. As soon as the old GM device sees a world entry naming someone else, it steps down: token, keyring and requests leave the device, the journal stays. Until then the GM can take the handover back.
  - **As a file**: the device steps down at once. If the file is lost, the world is lost.
  - Whoever was GM so far keeps playing with `players` and `all`; every other attribute is rotated by the new GM, so that what comes is reachable only through the new keys.

## Encrypted or not

Every world is encrypted or not, freely chosen, no matter where it lies (*Options → World → Encrypted*, GM only). The app cannot know whether a folder is local, a NAS or a cloud drive; whoever puts a world into the cloud should keep it encrypted.

- **Encrypted:** as described above. Views and rights apply, everyone opens what they have keys for.
- **Unencrypted:** the control entry `Encryption { on: false }` applies; new section generations are `plain`: access, start state, short name and drafts lie unencrypted (hex) in the entries, there is no data key, `header`/`write` are empty, the seed of the write key stands open in `seed`. Types and outline lie there whole and unencrypted (`SealedFor::plain`), as do gallery names, folders and requests (`Sealed::plain`). Images lie as a whole file, unchanged (one part, without a key) – any image program opens them. There are no views and no rights: whoever has the world reads everything; every member may edit everything. The views that were set stay stored in the access and apply again as soon as the world is encrypted. No key lies in the world.
- **Only members write:** the write key lies open, so every device additionally checks that a draft or version is signed by the GM or a member with an admission (grant); otherwise the entry waits (the admission may still be on its way) and does not count.
- **Supporters act in their own name:** a supporter's act (`Support`: a grant, a removal, keys turned over) is signed by the supporter, not with the world token. It waits until its author is a supporter, counts only while they are one, and outweighs the GM's grant or removal of that member only where it came after it (`after`, the newest control count its author knew). The GM's own word written later takes over; keys turned over stay for good.

**Image parts** are named after their content; an unencrypted image therefore has the same name at every switch. A deleted part therefore only disappears when no living gallery entry names it any more (`needed_parts`) – otherwise, after switching twice, the journal would clear away the images just written.

**Switching** writes the `Encryption` entry, seals types, outline and gallery folders anew, brings every image into the new form (new gallery entry, new parts), starts every section still in the old form in a new generation (`restart_all`, the image references in the content along with it) and finally deletes the old image entries with their parts (`Item::Deleted`) – so when encrypting, no plain-text file stays behind. New heads arise on all cores (`make_head`) and go into the journal in one go (`Journal::control_all`, `durable::write_all`); images in packs of 64 (`media::put_parts`, `Journal::put_images`), and while switching the journal does not look at the disk on every access (`store::quietly`). With 11,000 documents, 35,000 sections and 5,000 images it takes a good two minutes in each direction (before: 11–14); the dialog shows step, counter and time (`world_switch_progress`). Until everything is done, `switching` lies in the private world folder; if it breaks off, the next opening of the world continues. What lay open for a while stays readable for anyone who took a copy back then. Older versions keep their old image references; the version view no longer shows their images afterwards.

**Readable copy** (`apps/desktop/src-tauri/src/readable.rs`): one text per document (the code as the code editor shows it; images as `../images/<name>.<ext>`) and the images as files, ordered by type:

```text
<world>/text/<type>/<document>.md
<world>/text/images/<image>.<ext>
```

For an unencrypted world the GM's device always keeps it in the world folder; the images there are hard links to the image files, so not stored a second time. Every device can also keep one of its own beside the profile (*Options → World → Readable copy*), for an encrypted world too – then with what it can read, in plain text only on this device. It follows the world at every sync tick in both directions: what changes in the app is written anew; a file that changed elsewhere (in a text editor or another Markdown app) becomes a version – merged on the state the file showed (`readable.json` in the profile holds file, hash and state per document). A deleted file comes back; only the app deletes documents. In the world folder only the GM's device keeps it: in a shared folder two devices would otherwise take each other's files for changes by hand. On encrypting, the copy in the world folder disappears.

## Limits

- Writing at the same spot at the same time is merged technically, only marked as to content.
- A GM with several devices is not provided for (one counter).
- After an attribute rotation, new members get only the new keys; older sections are not re-encrypted automatically (as before).
- Waiting proposals are sealed for the GM who was GM at the time of writing. If the world is handed over, the new GM cannot read them – decide before.
- Players do not sync with each other directly; only the GM's device answers for a world, on the LAN and over the [direct connection](../testing/internet-direct.md), under the same rules as a server. Through a [shared folder](../testing/folder-sync.md) their changes meet in the folder's files.

## Conflicts

What can merge, merges: concurrent writing in the same part meets in its Automerge document. What reaches no one is a **conflict** (`Journal::conflicts`), listed by editing session under *Options → Conflicts* (there while there are any, with a count):

- **Written to a document removed meanwhile** — a version that came after the removal (offline, say, on a device that had not heard of it yet). It shows the document as its author saw it: a version that wrote in a document saw it standing. *Keep as new document* makes a document of the same type from it — the GM's at once, a player's as a proposal for the GM.
- **Waiting** longer than ten minutes for what it builds on — a part's head or a version before it that never came.

The GM sees all of them and can discard the changes for good (`Item::Deleted`) — never a document; everyone else sees their own. A world read from its server a piece at a time shows no waiting ones: there they wait only for the rest of their document.
