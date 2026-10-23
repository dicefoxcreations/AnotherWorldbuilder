# Links

As of October 2026. Code: `crates/aw-model/src/link.rs` (reading and writing), `markdown.rs` (anchors), `apps/desktop/src/shared/Links.ts`, `shared/Anchors.ts`, `features/wiki/MarkdownText.tsx`.

`@@` opens a link, a second `@@` closes it:

| Written | Shows |
| --- | --- |
| `@@Place:Gullwind@@` | the document, with its type in front for documents of the same name |
| `@@Gullwind|the city@@` | the document, under text of its own |
| `@@Character:Tim#Status@@` | the value of Tim's property *Status* |
| `@@Session 1#^c70ff9|the inn@@` | the line of *Session 1* closed by the anchor `^c70ff9` |
| `@@#^c70ff9|the inn@@` | the same, in this document |

**Anchors.** A line ending in `^` and a name of letters, digits and `-` — `… and waits at the inn. ^c70ff9` — carries an anchor there: in a paragraph, a list item, a table (in its last cell: the whole table). The same anchor closing two blocks marks everything from the first to the last.

**Making one.** Writing, select what the section is, right click, *Section → Mark as section …* and give it a name: `Harbour rumours` becomes `^harbour-rumours` at the end of the block the selection begins in — and of the block it ends in, where that is another (`shared/Anchors.ts`, `markSection`). *Section → Link to a section …* finds every section of the world by its name, its document or the words of its line (links in it read by their names), and puts in the link. A name needs to be unique only in its document — a link names the document too; a second of the same name there gets `-2` — with the selected words as its text, or the section's name. In the editor an anchor stands as a small ⚓ with its name; a link to one as a link, in bold or italic text too. A click on ⚓ chooses the section: its place is marked in the text, and the side bar shows its name to change — renamed, its anchors, this document's links to it and the links of other documents to it change along (those saved as their versions) — and *Remove the section*, which takes the anchors out and leaves the text. Reading, it is not shown; in the editor it stands faint at the line's end. A link to it scrolls there and lights the part up for a moment, in another document once that one is open. Resting the pointer on such a link shows that part alone — the paragraph, the list item, the whole table — not the page (`LinkPreview.tsx`, `PartOnly`). It is Obsidian's notation (`[[#^c70ff9|the inn]]` there); imported notes keep their anchors and links.
