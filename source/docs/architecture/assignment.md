# Assigning players

As of October 2026. Code: `apps/desktop/src-tauri/src/lib.rs` (`member_attributes_set`, `own_view`), `store.rs` (`ensure_attributes`, `regrant`), `document.rs` (`self` on saving); interface: `features/settings/Options.tsx` (`Assigned`), `shared/ui/Visibility.tsx` (*Assigned*, names of `doc:…`).

The GM assigns documents, whole types and attributes to players — and may note their own character on themselves: it changes nothing while they are GM, and stays theirs when they hand the world over (`store::handover_take` keeps the former GM's attributes). The assignment is the right at the same time: what a section lets `self` see is seen by whoever is assigned to its document or to that document's type.

## Assigning

*Options → Players*: under each player stands what is assigned to them. The field *Assign…* finds while typing:

- **Documents**, such as Roque, the player's character. Stored as the attribute `doc:<id>`.
- **Whole types** (*All Player character*): every document of the type, future ones too. Stored as `type:<id>`.
- **Attributes** of the world, such as `class:rogue`.

Every player has `players` and `all` anyway; they are not listed. A new assignment creates its attribute. Whoever loses one sees no more of what it opens; whoever gets one sees all of it, what was written before too — the holder of the world checks every request. A player's new id takes the old one's assignments with *New ID* ([Players: a new id](../testing/players.md)).

## `self` in views

A type can say for each of its documents who sees a section:

```text
<block title=Sheet style=statblock top fold=folded view="self OR gm" edit=gm>
```

`self` means: whoever is assigned to this document or to its type. On creating and saving it becomes `doc:<id> OR type:<type>`. A document without a type (the world's start page) only gets `doc:<id>`. In the picker and the badges it is called by the document's name — on Yen's page *Yen* — and appears only in a document, among the basic choices (*Everyone*, *GM only*). *All players* is gone: every player holds both `all` and `players`, so they reach the same people; a stored `players` reads as *Everyone*. The GM holds all keys and therefore sees and changes everything anyway; so the three basic choices exclude one another, and whoever is added individually (a player, the document's own name, a type) is added to the GM, who always stays (`shared/Policies.ts`, `effective`). Something stored such as `all OR gm` the picker shows as what it means: *Everyone*. Everything else — single players, types (*All Character*), `doc:…` of other documents by their name — the search field below finds while typing; what it does not find, Enter takes as written, `a AND b` too. **Who sees, who changes.** In a document's head, behind a bar, stand an eye (who sees it) and a pen (who changes it). Beside a section title stands only what differs from the document — only the eye, only the pen, or both; otherwise what the head says applies. A region of a section with access of its own (`<block view=… edit=…>` inside the section) is framed and says whom it is for on the frame's top line, as the head does — the infobox likewise, where its access differs: on its top line, not above its fields; editing, a click on a region's line opens its access in the side bar. Players, characters and types each have a colour of their own in these bubbles, the same everywhere; the GM's stays red, everyone's green. **Own colours:** each member picks theirs under *Options → Players* (*Your colour*). It is a record of its own (`journal::Badge`), signed by them and naming the characters assigned to them (`doc:…`, which others cannot read in the grants); every device of the world takes it from a member, no GM needed, and the newest of each member counts. Their name and those characters wear it; a document two badges name wears neither. A server takes badges from 0.1.52 on. For seeing, the stricter rule applies: a section is only visible if the document is — it can be narrower, never wider. Not for changing: a section may let more people write than the document (a diary the players write in, while name and properties stay with the GM). Everyone who sees it sees that: so one knows whom to ask. In the read view it only stands there; it is changed while editing, in the same place (the GM) or at the top of the section's side bar (click on the title) under *Access*.

**What rights hang on.** On parts: a section has one part or several, each its own layer with its own rule. Text and elements (list, map, image …) stand in a part and have its rights. The line at the title shows those of the first part; a section with several parts has one line per part in its side bar, and a click into the text of such a part opens that part's own side bar. For a section of one part, a click into the text opens nothing. The side bar of an element (list, map …) shows no rights: it has those of its section. Where an element can have its own, only *Own access* stands there — on saving it then becomes a part of its own; with rights of their own they can be set there. Markers and regions of a map have their own as before.

The GM sees everything. A player sees the sheet of their character as soon as they are assigned to it, and not the sheets of other characters.

## Open

- Giving the assignment right away when admitting a player.
- `edit=self` is translated the same way, but has not been checked yet together with players.
