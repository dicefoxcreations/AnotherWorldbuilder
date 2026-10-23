# Links

As of October 2026. Code: `crates/aw-model/src/link.rs` (reading and writing), `markdown.rs` (anchors), `apps/desktop/src/shared/Links.ts`, `shared/Anchors.ts`, `features/wiki/MarkdownText.tsx`.

`@@` opens a link, a second `@@` closes it:

| Written | Shows |
| --- | --- |
| `@@Place:Waterdeep@@` | the document, with its type in front for documents of the same name |
| `@@Waterdeep|the city@@` | the document, under text of its own |
| `@@Character:Tim#Status@@` | the value of Tim's property *Status* |
| `@@Session 1#^c70ff9|the inn@@` | the line of *Session 1* closed by the anchor `^c70ff9` |
| `@@#^c70ff9|the inn@@` | the same, in this document |

**Anchors.** A line ending in `^` and a name of letters, digits and `-` — `… and waits at the inn. ^c70ff9` — carries an anchor there: in a paragraph, a list item, a table cell. Reading, it is not shown; in the editor it stands faint at the line's end. A link to it scrolls there and lights the line up for a moment, in another document once that one is open. It is Obsidian's notation (`[[#^c70ff9|the inn]]` there); imported notes keep their anchors and links.
