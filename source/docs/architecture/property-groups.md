# Property groups

As of October 2026. Code: `crates/aw-model/src/template.rs` (`FieldKind::Group`, `Template::group`), `types.rs` (`unfold`, checks), `template_text.rs` (`group …`, `group_from_text`), `document.rs` (indented in the code); interface: `shared/types.ts` (`unfold`, `usersOf`), `features/settings/Options.tsx` (list), `features/templates/TemplateEditor.tsx` (editing a group), `features/wiki/Document.tsx` (folded in the properties).

Properties that several types need stand once in a **group**. The six ability scores are needed by the character, the monster, the feat, the item and the species. If the group changes, every type that includes it changes.

A group can also have a **layout**: how its values look. A document includes it with `<view of=Group>`. So a sheet is defined in one place and builds itself in every document from that document's properties.

None of this is tied to a rule system. Groups, formulas, [rules](stat-blocks.md#rules) and layouts are data of the world. A group can also bring the rules its values calculate by (`mod(x) = …`). They apply in every type that includes the group.

## Creating

Under *Options › Types › Property groups*. A group has only properties, one per line, as at the top of a type:

```text
Strength: number
Dexterity: number
Constitution: number
Intelligence: number
Wisdom: number
Charisma: number
```

It has no documents, no page, no parent type and no infobox. Its only block is its layout. It cannot be picked (`@@Group@@`) or inherited from. It can only be deleted once no type includes it any more.

## Including

```text
---
Species: @@Species@@ adds
Scores: group Abilities
Feats: @@Feat@@ multiple adds
---
```

- `Scores: group Abilities` puts the properties of the group in this place. `Scores` is the heading they stand under.
- `Abilities: group` means: the group with the same name as the line.
- The properties keep their names and their keys. `%%Strength%%`, `<number bind=Strength>`, `[[+ Strength mod]]` and the bonuses through `adds` read them like their own. Because the keys belong to the group, they are the same in every type.
- A group may contain groups. Every property knows its path (`Character scores › Abilities`), and editor and code show the levels nested. A group that contains itself is refused.
- No name may occur twice, not from group and type, nor from group and parent type. Saving a group checks this too: it may not make a name occur twice in any type that includes it.
- Defaults (`= …`) and rights (`view=`) stand at the properties in the group, not at the `group` line.

## In the document

- **Editor:** under *Properties* the group stands folded under its heading, with the number of its properties. Unfolded, the names are indented, the fields stay in the column of the others.
- **Code:** the values stand indented under the heading:

  ```text
  ---
  Species: @@Species:Human@@
  Scores:
    Strength: 14
    Dexterity: 15
  Feats: @@Feat:Athlete@@
  ---
  ```

  A name without a value with indented lines below it is a heading, not a property. The indentation is only for the eye; a property is found by its name.

## Layout

Below the properties, separated by `---`, stands the group's layout:

```text
---
Abilities: group
Combat: group
Feats: @@Feat@@ multiple adds
Equipment: @@Item@@ multiple adds
---

# %%Name%%

**Armor class** %%Armor class%%
**Hit points** %%Hit points%% ([[(Level)d(Class.Hit die) + Level * Constitution mod]])
**Speed** %%Speed%%

---
<abilities show=Abilities with=mod short="STR, DEX, CON, INT, WIS, CHA">
---

## Feats
%%Feats%%
```

- It is ordinary text with everything text can do: `%%…%%`, rolls, `<abilities>`, links and also `<view of=…>` of another group.
- `<view of=Character scores>` draws the layout with the values of the document it stands in. The frame comes from the block it stands in, e.g. `style=statblock`. What only this document has (actions, traits), it writes below in the same block.
- `%%Name%%` is the document's name, where no property has that name.
- `<abilities show=Abilities with=mod>` shows the numbers of a group, below each what the rule `mod` makes of it.
- It is stored with keys and IDs like any text. A renamed ability or group stays connected.
- A layout that shows itself stops.
- In the editor `<view>` is a surface. The side bar picks the group; the layout itself is changed in the group.
- The values in the layout can be changed in place in the editor: numbers, texts, choices including the number per choice (the level per class). Under every ability tile stands the own value, above it what bonuses make of it.

## Calculated values

A default in `[[…]]` is a formula, not a value:

```text
Hit points: number = [[Class.Hit die + (Level - 1) * (Class.Hit die / 2 + 1) + Level * Constitution mod]]
Speed: number m = [[Species.Speed]]
```

- A document without a value of its own has what the formula comes to. A value of its own, such as rolled hit points, stands instead.
- Bonuses through `adds` are added as to any own value: Tough +6 on a calculated 28 makes 34. The tooltip says `28 by formula + 6 Tough`.
- **Through references with a dot:** `Class.Hit die` reads the property of the document that *Class* picks. If a reference picks several, they are added up.
- If something the formula needs is missing, e.g. for a monster without a class, there is no value and the field is empty. A formula that needs itself comes to nothing.
- In the editor the empty field shows the result as a placeholder, the formula is in the tooltip.
- Dice with a calculated count or number of sides: `(Level)d(Class.Hit die)`, `1d(…)`, `d(…)`. `W` is accepted for `d` as well.

## A number per choice

`Classes: @@Class@@ multiple with Level`: every chosen class carries a number of its own, here its level. In the document it stands after the link:

```text
Classes: @@Class:Fighter@@ 3, @@Class:Rogue@@ 2
```

A choice is then stored as `{"with": id, "count": 3}`. The same fits everything that is there several times, e.g. "Healing potion 3" in the inventory. In the editor and in the infobox a small number field stands beside each name. If a class is added, the others keep their level.

In formulas:

| Written | Meaning |
| --- | --- |
| `Classes.Level` | the numbers of all choices together: the total level |
| `Classes.Hit die` | a property of the chosen documents, added up |
| `Classes.(Level * (Hit die / 2 + 1))` | calculated per choice, then added up. Inside, the number of the choice, its properties and, for what it does not know, those of the document apply |
| `first(Classes.Hit die)` | the first choice. Likewise `last`, `sum`, `count`, `min`, `max` |
| `or(Rolled, …)` | the first that has a number |
| `Classes.((Level)d(Hit die))` | dice per choice: `3d10 + 2d8` |

How hit points come about is a matter of the game, not of the app. For a d20 fantasy game (the full die of the first class, after that the average per level, unless someone rolls and enters the sum of their rolls) the world writes:

```text
Hit points: number = [[first(Classes.Hit die) + or(Rolled, Classes.(Level * (Hit die / 2 + 1)) - first(Classes.(Hit die / 2 + 1))) + Level * Constitution mod]]
Level: number = [[Classes.Level]]
```

Fighter 3 and Rogue 2 with Constitution 14: 10 + 3 × 6 + 2 × 5 − 6 + 5 × 2 = 42.

Another system writes another formula, e.g. "Constitution + Level × 5", and needs neither `first` nor the average. If a table does not grant the average, it leaves it out and always enters rolled values. The formula becomes more readable with rules the group brings along:

```text
average(d) = d / 2 + 1
Hit points: number = [[first(Classes.Hit die) + or(Rolled, Classes.(Level * average(Hit die)) - first(Classes.(average(Hit die)))) + Level * Constitution mod]]
```

Subclasses are a type of their own that belongs to its class with `Class: @@Class@@ parent`. A character picks them with `Subclasses: @@Subclass@@ multiple`. The class follows from the subclass and need not be counted again.

## For whoever picks

The formula of a picked document calculates for the document that picks it. Inside, the number of the choice, the properties of the picked document and, for what it does not know, those of the picker apply, through all levels outwards. The rules too: first those of the picked type, then those of the picker.

- **A value may be a formula**, not only a default: `Base: [[Dexterity mod]]` at the skill *Stealth*, `Armor class: [[14 + min(Dexterity mod, 2)]]` at the scale mail. Read by itself, the skill has no Dexterity and no value. For Ilva it has Ilva's.
- **Skills with proficiency:** `Skills: @@Skill@@ multiple with Proficiency`, plus at the type Skill `Bonus: number = [[Base + Proficiency * Proficiency bonus]]`. Proficiency is 0, 1 (proficient) or 2 (expertise). Saving throws work the same way.
- **Proficiency bonus by total level:** `Proficiency bonus: number = [[steps(Level, 1, 2, 5, 3, 9, 4, 13, 5, 17, 6)]]`.
- **A choice in the path:** `Skills[Perception].Bonus` is the bonus of the skill of that name, so passive perception is `[[10 + Skills[Perception].Bonus]]`. If a reference stands in the brackets, the documents it picks apply, here or further out: `Classes[Class].Level` is, in a subclass, the character's level in its class.

## Lists through the choice

```text
<list from=Skills show="[[+ Bonus]]" columns=3>
<list from=Classes.Features when="From level <= Level">
<list from=Subclasses.Features when="From level <= Classes[Class].Level">
```

- `from=` with a dot goes on through the choice: the features each chosen class brings (`Features: @@Feature@@ multiple with From level` at the class). Such a list is only read. One changes the classes.
- `when=` is a formula, calculated in every entry. What it makes 0 is missing. Here: features from level 5 only appear once the character has level 5 in the class. When the level rises, the feature is there by itself.
- `show=` may contain the number of the choice (`Proficiency`), numbers calculated for the picker (`Bonus`) and rolls in `[[…]]`. A click on `[[+ Bonus]]` rolls a check, named after the entry. In the simple list the values stand after the name.
- `●Proficiency`: a symbol before the name of a number shows it as that many symbols before the entry. ● means proficient, ●● expertise, a pale ● not at all.

## Every document of a type

```text
Skills: @@Skill@@ every with Proficiency
```

- `every` always covers every document of the type, ordered by name. There is nothing to add or take out. A skill created later is had by everyone at once, with 0.
- Only the number can be changed: in the properties (every entry with its field), in the list in the sheet while writing (the field stands in place of the dots) and in the code.
- Only numbers that are not 0 are stored and written in the code: `Skills: @@Skill:Athletics@@ 1, @@Skill:Perception@@ 2`.
- `every` needs `with`: without a number every document would have the same, and the property would say nothing.
- A short stat block shows only what counts, in one line: `**Skills** <list from=Skills when=Proficiency style=inline show="[[+ Bonus]]">` gives "Stealth +6". Whoever prefers writing by hand keeps writing by hand.

## For new documents

- A default `= *` at a choice pre-selects every document of the type, once on creating. After that it is an ordinary choice. For skills, `every` is the right thing.
- `= last` pre-selects the last document of the type by name, with numbers counting as numbers ("Session 036" after "Session 9"). A new session thus gets its previous one: `Previous: @@Session log@@ = last`.

## What points here

```text
Previous: @@Session log@@ = last
Next: @@Session log@@ from Previous
```

- `from Previous` stores nothing. The property holds the documents of the type whose "Previous" points to this one. It stands in the infobox like any choice, as a link, and can only be changed where it comes from.
- Without `multiple` it is the first by name. With `multiple` it is all of them: `Children: @@Character@@ multiple from Mother`.
- Paging on thus needs only one side: whoever sets "Previous" in the new session (or lets `= last` do it) gives the previous one its "Next".
- `<view of=Group>` shows nothing if the document's type does not include the group. The template *Character* therefore brings `<view of=Player scores>` for all, and only player characters (who have the group) show the player sheet.

## Compared with the parent type

A parent type says *what* something is: an NPC is a character, and a reference to Character offers NPC. A group only says *which properties* something has. A feat has Strength like a character but is not one.
