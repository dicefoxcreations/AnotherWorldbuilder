# Stat blocks, dice and bonuses

As of October 2026. Code: `crates/aw-model/src/markdown.rs` (`[[…]]`, dice in the text, `<abilities>`), `template.rs` / `template_text.rs` (`style=statblock`, `adds`, `Rule`), `apps/desktop/src-tauri/src/network.rs` (rolls in the session); interface: `shared/Dice.ts` (formulas, rolling), `shared/Sheet.ts` (bonuses), `features/dice/DiceTray.tsx` (dice tray), `features/wiki/MarkdownText.tsx` (dice, ability row), `features/wiki/ListCard.tsx` (`AbilitiesCard`), `shared/editor/FlowText.ts` (dice and lines in the editor).

The model is the stat block of rulebooks: name, values, ability row, actions and rolls one clicks. The look is the app's: a card like `box`, sections in teal, the abilities as tiles.

**No game system is built in.** What a modifier is, what a check rolls, when a roll is critical and what an item does with a value, the world says in its [rules](#rules) and values. The app knows only numbers, dice, formulas and operators.

## Stat block

A block of a type with `style=statblock` looks like a printed stat block:

```text
<block title="" style=statblock>
  # Goblin
  *Small humanoid (goblinoid), neutral evil*

  ---

  **Armor class** %%Armor class%% (leather armor, shield)
  **Hit points** %%Hit points%% (2d6)

  ---
  <abilities show=Abilities with=mod short="STR, DEX, CON, INT, WIS, CHA">
  ---

  ## Actions

  ***Scimitar.*** *Melee weapon attack:* [[+ Dexterity mod + 2]] to hit. *Hit:* 5 (1d6 + 2) slashing damage.
</block>
```

- `#` is the name, `##` a section such as *Actions*: small, in capitals, teal and with a line below, like block titles elsewhere. `---` is a fine separating line.
- The content is ordinary text with values, links and rolls. The editor shows it as it is read.
- The app's tokens apply; the block has no colours of its own.
- Like `box` and `quote`, the style belongs to the type's block. A monster type brings the stat block along, and every monster has it.

## Top, bottom and folded

```text
<block title=Sheet style=statblock top fold=folded view="self OR gm">
```

- `top` puts the block above infobox and text, across the whole width of the page. A stat block at the top is no longer limited to 560 px; abilities and lists use the width.
- `bottom` puts it below infobox and text, likewise across the whole width: for family trees, timelines and everything that would be too narrow beside the infobox. Without either, a block stands in the text beside the infobox. Both at once is an error.
- Every block with a title folds with a click on the title. `fold=folded` lets it start folded, `fold=off` never folds it. What a reader folded open or shut, the device remembers.
- `new="Place, Region"` puts a ＋ beside the title that creates documents of these types (see [create buttons](../development/ui.md#create-buttons)).
- `heading=2` to `heading=4` sets the title smaller. Without it, it stands in the size of `#` in the text, `4` in the size of the text (see [headings](../development/ui.md#headings)).
- In the editor a click on the title opens the block's settings on the right: title, size (*H1* to *H4*), style, place (*Top*, *In text*, *Bottom*) and folding (*Open*, *Folded*, *Never*). The arrow before it folds. The settings belong to the type, for an inherited block to the parent type, and apply at once to all its documents. A block without a title shows its label *Block* as soon as the pointer is over it.
- `view="self OR gm"`: the sheet is seen by whoever is assigned to the document, and by the GM (see [assignment](assignment.md)).

## Order

**New sections.** In the editor, *＋ Section below* in a block's side bar (or *New section below* in the right-click menu) creates a section "New section". It goes into the document's type, with its access, directly below the block it was created from, also between inherited ones (then through `<order>`). Its settings open right away to name it. It applies to every document of the type; where nothing is in it, the read view does not show it.

**Sections of a document.** Sections the type does not set out belong to the document alone. A document without a type, such as the world's start page, has only such sections. There, *New section below* creates a section of the document. A click on its title (or, for a section without a title, on the label *Block*) opens its settings, the same as for a section of the type: title, size (*H1* to *H4*), style, place, *↑ Up* / *↓ Down* among its own sections, folding, *New*, plus *＋ Section below* and *Remove the section*. In the code they are the same settings as in the type: `<block title=Places style=box bottom fold=folded heading=2 new=Place>`. A new section with a name is kept even when empty, so that one can write into it; in the read view it only appears with text. The order is held by the document's base layer (`order`, the keys of the sections), which everyone who sees the document reads. In a document with a type, its own sections come after those of the type. In the read view they carry their title and fold like the others. How an own section looks stands in the field `shape` of its first layer (`style`, `place`, `fold`, `heading`, `new` with type IDs); what matches the default is missing there. `##` headings become sections of their own by a right click on one of them (*## headings to sections*), each as large as its heading was.

Without a setting, the blocks of the parent types come first, then the type's own. In the editor there are two ways to rearrange them:

- Drag a block by its title onto another. Dropped on its upper half, it stands before it, on the lower half after it. A line shows where it lands. If the target is in another place (top, in text, bottom), the block moves there.
- In the block's side bar, *↑ Up* and *↓ Down*, among the blocks in the same place.

Where this is stored:

- If both blocks belong to the same type and no order is set in the chain, the order changes in that type, for all types below it too.
- Otherwise the document's type gets an order of its own, which includes the inherited blocks too. So *Goals* (player character) comes before *Life* (character). In the template text it stands as `<order of="Appearance, Goals, Life">` with the titles. The nearest order in the chain applies. Blocks not named follow in their usual sequence; an unknown title is an error.

**Order of documents.** `<sort by="In-game start, In-game end, Session date, name">` in the template text sets how the documents of a type stand in lists, in the side bar and, on the same day, in the timeline: by these properties (stored by key), then by name, numbers as numbers. A reference counts with the order of the referenced document, so an event stands with its session. Whoever lacks a value stands behind those who have it. Subtypes sort the same as long as they set nothing themselves.

**Sorting, counting, embedding lists.** `<list … sort="Role desc, Location, name">` gives a list a starting order of its own (otherwise the type's); `desc` reverses a step. In tables a click on the column head sorts (again: reversed), ↺ in the name head goes back to the starting order. `count=Killer by=Kind` counts instead of listing: one row per value, one column per value of `by`, a total, a total row; values the reader cannot open are not counted. From 13 entries on, a list has a search field that searches names and the shown columns. `style=tree` shows every entry with its image (otherwise its type's symbol) and the number of entries below it; the first level stands open, deeper branches fold open. `<embed of=Sessions>` shows the text of another document under its name (foldable); a type page brings its ＋ button along. `fold=folded` starts folded, `fold=off` never folds, `title=off` leaves out the name (side bar *Look*). What the reader cannot open is missing entirely, without a hint, even when the GM looks as a player. In the visual editor, *Order* (↑ Up / ↓ Down) moves every element past the paragraph or element before or after it. A section with the place `side` (`<block … side width=380>`, for own sections "Place: Side" in the side panel) stands in a column of its own beside the text and folds away sideways. Types without a page get the usual type page on package import.

Packages (`transfer.rs`) carry the world calendar too. A world takes it over on import only while it still has the default calendar.

## Ability row

`also="Saving throws.Bonus"` gives the row a second bar: under each ability the number of the choice named like the ability, here the saving throw with its proficiency dot, to click. So the saving throws stand in the order of the abilities. `heading=Abilities also-heading=Saves` writes above both bars what they are.

How good someone is at something is shown by small circles: empty (not proficient), ◐ half (half proficiency, number 0.5), ● proficient (1), ●● expertise (2). The bonus rounds down: `[[Base + floor(Proficiency * Proficiency bonus)]]`.

`<abilities show=Abilities with=mod short="STR, DEX, …">` shows number properties of this document side by side, each as a tile: abbreviation, value and below it what the rule `with` makes of it (`STR · 16 · +3`).

- `show=` names a group (its numbers) or single names. Without `show=` the row shows nothing.
- `with=` names a rule with one value, mostly the modifier. Without `with=` only the value stands there.
- `short=` are the abbreviations in the same order. Without them, the first three letters stand there.
- A click on a tile rolls a check if the rules know `check`: `check(mod(Strength))`, without `with=` `check(Strength)`.
- In the editor the row is a surface. The side bar picks the numbers by click, in the order one clicks them, and under *Below* the rule.

## Rolling and calculating

| Written | Shows | A click |
| --- | --- | --- |
| `2d6 + 3`, `1d20`, `d8` in the text | as written | rolls |
| `[[1d20 + Strength mod]]` | `1d20 + 3` | rolls |
| `[[+4]]`, `[[+ Strength mod]]` | `+4` | rolls the rule `check` with 4, e.g. 1d20 + 4 ("+4 to hit"). Without `check` only the number |
| `[[10 + mod(Dexterity)]]` | `12` | – (only calculated) |

- Rust only cuts out rolls (`Node::Roll`). Calculating happens in the display, because only there are the values known that this reader has.
- Formulas: numbers, dice (`d` or `W`), `+ - * /` (dividing rounds down), brackets, `floor`, `ceil`, `round`, `abs`, `min`, `max` and the world's rules. Also:
  - **Comparisons** `<`, `<=`, `>`, `>=`, `=` (or `==`), `!=`: 1 or 0. They bind weakest: `Level >= 5`.
  - **`if(condition, then, else)`** calculates only the branch that applies. Also `and(…)` and `not(x)`.
  - **`steps(x, from1, value1, from2, value2, …)`**: a table in steps, the value of the highest step up to `x`. The proficiency bonus by total level is `steps(Level, 1, 2, 5, 3, 9, 4, 13, 5, 17, 6)` (or `2 + floor((Level - 1) / 4)`).
  - **Keeping dice:** `2d20kh1` counts the higher (advantage), `2d20kl1` the lower (disadvantage), `4d6kh3` the three highest. The others stand struck through in the tray: `[4̶, 17]`.
  - **Dice one by one:** as a number a roll is its sum, but it knows every die. `map(rule, 5d6)` applies a rule to every die and gives a list. `sum`, `count`, `min`, `max`, `first`, `last` take a list value by value. `[Courage, Wits, Intuition]` is a list of values. Several lists go in pairs; a single number applies to every entry.
  - Dice that go to a rule fall **once**, even if the rule names them several times.
  - Properties stand there by name. A name continues over single spaces and hyphens as long as a letter follows: `Strength - 10` is a difference, `Strength-Mod` a name.
- A roll is named after the bold or italic words at the start of its line, so *Scimitar* or *Hit points*. A roll from the ability row is named after the ability.
- What cannot be read stands there dashed in red. Hovering shows the reason.
- Rolling uses `crypto.getRandomValues`, without bias.

## Dice

The 🎲 in the header opens the dice bar on the right. There:

- **A line for rolling:** `1d20 + 3`, with a name `Attack: 1d20 + 5`. Enter or *Roll* rolls. Below it, buttons add dice; a second click on the same one turns `1d20` into `2d20`. Which buttons stand there the GM chooses under *Options → Basics → Dice* for the whole world (for a dice pool system e.g. only `d6`, also unusual ones like `d7`); without a choice they are d4, d6, d8, d10, d12, d20 and d100. The choice travels to everyone as a GM control entry. Any die can be written; the choice only concerns the buttons.
- **Favourites:** ☆ keeps the line; a click on a favourite rolls it, × removes it. They apply to this device and this world.
- **The log:** all rolls with name, document, the dice and the sum, those from documents too. A check is judged by the rules `crit` (green), `fumble` (red) and `success` (✓ or ✗), see [rules](#rules). The verdict is calculated by whoever rolls, with the document's rules, and it travels with the roll.
- **Everyone / Only me**: with *Everyone* a roll goes to everyone in the session, with *Only me* it stays here. The device remembers the choice.
- **Clear log** (GM only): empties the log here and on every device in the session.

When the bar is closed, the last roll shows at the bottom right for a few seconds, and the 🎲 counts the others' rolls that came since.

**3D dice.** Every roll tumbles across the window and comes to rest on the result (`features/dice/Dice3D.ts`, three.js). The app has already rolled by then, with the world's rules; the dice only show it, they decide nothing. Shapes exist for d4, d6, d8, d10, d12, d20 and d100 (as a tens and a ones die); other dice only appear in the log. The d4 shows the number of the top corner, 6 and 9 are underlined. Under *Options → Dice* every device sets whose rolls tumble (*Everyone's*, *Mine*, *Off*) and the colour of die and number; *Throw* shows a set as a sample. Everything is built into the app, nothing is loaded from outside.

**Where what lies.**

- Rolls from a part only the GM sees (`level=gm-only` or `view=gm`) always stay here (🔒). How a secret monster hits stays secret.
- The rolls travel with the digest request that runs every 2 s anyway. A player sends theirs to the host, the host passes them on to the others. A host only accepts rolls in the asker's own name. On the way, a roll is forgotten after a minute. No rolls go through folders; they belong to the live session.
- Every device keeps the log for itself, in the private part of the profile (`dice.json` beside the world, at most 300 rolls, with the favourites). It is not synced.
- The GM's *Clear log* travels along as a special roll (`mark=clear`). A device empties its log only if it comes from the world's GM as the journal names her. A player cannot send it in the GM's name, because the host only accepts rolls in one's own name.

## Bonuses through references

A number says with its operator what it does:

| Written | Meaning | Example |
| --- | --- | --- |
| `Strength: 15` | the own value, the starting value | character, ability scores |
| `Strength: +1` | raises | feat |
| `Strength: -1` | lowers | curse |
| `Strength: =21` | sets, whatever it was before | a magic belt |
| `Strength: >=19` (or `≥19`) | sets to at least 19 | gauntlets of power |
| `Strength: <=8` (or `≤8`) | sets to at most 8 | curse of weakness |

The operators take effect where a reference with `adds` picks the document:

```text
Feats: @@Feat@@ multiple adds
Equipment: @@Item@@ multiple adds
```

Then: own value + everything that raises or lowers. If a picked document sets the value (`=`), that applies instead (with several, the last one). After that, "at least" holds the value from below, "at most" from above:

| Starting value | Feat | Gauntlets | Result |
| --- | --- | --- | --- |
| 15 | +1 | – | 16 |
| 15 | +1 | >=19 | 19 |
| 18 | +1 | >=19 | 19 |
| 20 | +1 | >=19 | 21 |

What effect an item has stands at the item. That gauntlets make "at least 19" is a rule of one game. The app does not know it; the world writes it down that way.

The result stands everywhere: in the infobox, at `%%Strength%%`, in tables, in the ability row and in every formula. Hovering shows where the number comes from (`15 + 1 Athlete, at least 19 Gauntlets`).

- A bare number is stored as a number, a number with an operator as text (`"+1"`, `"-1"`, `"=21"`, `">=19"`, `"<=8"`). Read by itself, `-5` is −5 and `>=19` is 19.
- Raising needs an own value: an NPC without Strength gets none through "Human +1". A set value applies even without an own one.
- A bare number at a picked document is its own value and changes nothing. A statue with Strength 30 does not make its owner stronger.
- What is compared is the property's name. The feat type needs the properties it can change. The easiest way is for every type to include the same [property group](property-groups.md).
- What a picked document picks itself counts too, e.g. a belt that grants a feat. Circles are recognised.
- Only references with `adds` count. The father's Strength is not the son's.
- The editor shows the own value, the read view the result.
- When read, a number with an operator stands as `+1`, `−1`, `= 21`, `≥ 19` or `≤ 8`.
- What a picked document does is shown by the preview when hovering over the link (see below).
- Values can come from formulas, through references too (`Class.Hit die`), see [property groups](property-groups.md#calculated-values).

## Preview

If the pointer rests on a link, after a moment the **whole page** of the document appears, as it looks when read: head, infobox, all sections. It is laid out narrower (808 px) and shrunk to 90 %.

- A link in a preview opens the next one above it, like a submenu. If the pointer goes back to a lower card, those above close; Escape closes the top one.
- The same rights apply as on the page. What the reader cannot decrypt is missing; if nothing is readable, only the head remains. As GM you therefore also see secret sections. 👁 (preview as player) shows what a player would see.
- Only for looking: no rights choice, no deleting, no image choice, no backlinks.
- A card stays open while the pointer is on the link, on it or on a card above it. One can scroll in it, and its links work. Escape, a click beside it or scrolling the page closes it.
- Every element with `data-preview=<id>` has it: links in the text, in the infobox, in lists and in the editor. A single listener (`features/wiki/LinkPreview.tsx`) draws them with `ArticlePage glance`.

## Rules

A game's rules stand as formulas in a type or a [property group](property-groups.md), at the top among the properties:

```text
Strength: number
Dexterity: number
mod(x) = floor((x - 10) / 2)
check(x) = 1d20 + x
crit = 20
fumble = 1
```

- A rule has a one-word name, values in brackets (or none) and after `=` a formula. A line in which `=` comes before any colon is a rule, otherwise a property.
- Every formula of a document can call them: `mod(Strength)`. A rule with one value also after a name: `Strength mod`. If the document has a property with exactly that name, the property applies.
- Rules may call rules. A rule that needs itself comes to nothing.
- A document knows the rules of its type, the parent types and all included groups. Duplicate names are refused on saving, as are the built-in ones (`floor`, `min`, `or` …).
- Five names have a fixed task. The verdicts get `d`, the dice of the check (dropped ones not counted; as a number their sum, for `map` and `count` each one), `x`, the value checked, and `r`, what the check came to. A rule takes the first one, two or three of these:

| Rule | What for | As a number | As a rule |
| --- | --- | --- | --- |
| `check(x)` | what `[[+4]]` and a click on an ability tile roll. Called with several values in the text: `[[check(Courage, Wits, Intuition, Climbing)]]` | – | `1d20 + x` |
| `crit` | critical (green) | `d` at least this much | `crit(d, x) = d <= x / 5` |
| `fumble` | fumble (red) | `d` at most this much | `fumble(d) = d >= 96` |
| `success` | succeeded ✓ / failed ✗ | `r` at least this much | `success(d, x) = d <= x`, `success(d, x, r) = r >= 0` |
| `shown` | what the button of a check shows, without a sign | – | `shown(x) = x` (instead of `+50`), `shown(a, b, c, sv) = sv` |

- Other kinds of games write other rules:

| Kind of game | Rules |
| --- | --- |
| d20 plus modifier | `check(x) = 1d20 + x`, `crit = 20`, `fumble = 1` |
| d20 with advantage as a check of its own | `advantage(x) = 2d20kh1 + x`, in the text `[[advantage(Strength mod)]]` |
| Roll under the value (percentile) | `check(x) = 1d100`, `success(d, x) = d <= x`, `crit(d, x) = d <= x / 5`, `fumble(d) = d >= 96` |
| Fixed threshold (2d6) | `check(x) = 2d6 + x`, `success = 10` |
| 3d6 | `check(x) = 3d6 + x`, `crit = 17` |
| Dice pool | `hit(w) = w >= 5`, `check(x) = sum(map(hit, (x)d6))`, `success = 1`, `one(w) = w == 1`, `fumble(d) = sum(map(one, d)) * 2 > count(d)`, `shown(x) = x` |
| Three abilities rolled under on 3d20 | `over(w, e) = max(0, w - e)`, `check(a, b, c, sv) = sv - sum(map(over, 3d20, [a, b, c]))`, `success(d, x, r) = r >= 0`, `one(w) = w == 1`, `crit(d) = sum(map(one, d)) >= 2`, `twenty(w) = w == 20`, `fumble(d) = sum(map(twenty, d)) >= 2`, `shown(a, b, c, sv) = sv`. In the text `[[check(Courage, Wits, Intuition, Climbing)]]`: the button shows the skill value, the tray the points left and ✓/✗ |

- A world with two systems has two groups with rules of their own.
- A rule may read properties: `crit = Crit at` with `Crit at: number = [[20]]` in the combat values. An item with `Crit at: <=19` (through `adds`) then makes a 19 critical already.
- Calculating happens in the interface (`shared/Dice.ts`): when the formula is read, a call is replaced by the rule's formula. Rust only checks form and names.

## Open

- Exploding dice (a 6 rolls again).
- Text in formulas: `shown` shows `50`, but not yet `50 %`.
- A stat block of another document embedded, such as the monsters of an encounter in the session log (`<view of=… for=document>`).
- A block style that a single document chooses itself, not only its type.
