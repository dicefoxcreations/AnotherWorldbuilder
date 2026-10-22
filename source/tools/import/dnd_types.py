"""The D&D 5e types — a monster, a spell, a character, a session log — and
the small value lists they choose from.

Shared by `dnd_template.py` (types alone, to ship) and library imports of
one's own, outside the app. Everything here is written for
this app; nothing comes from a book.
"""

import awpack
from awpack import article_id

T = awpack.Types()
DATA = ["Source", "Size", "Creature type", "Alignment", "Spell school", "Rarity", "Equipment category", "Feat category", "Environment"]
DATA_ICONS = {"Source": "📚", "Size": "📏", "Creature type": "🧬", "Alignment": "⚖️", "Spell school": "🔮", "Rarity": "💠", "Equipment category": "🗃️", "Feat category": "🏅", "Environment": "🌲"}


def data_type_id(name):
    return name.lower().replace(" ", "-")




for name in DATA:
    T.add(data_type_id(name), name, DATA_ICONS[name])

T.add("abilities", "Abilities", "💪", group=True, text="""Strength: number
Dexterity: number
Constitution: number
Intelligence: number
Wisdom: number
Charisma: number
mod(x) = floor((x - 10) / 2)
check(x) = 1d20 + x
crit = 20
fumble = 1
""")

SOURCE = 'Source: @@Source@@\nPage: number\n'
SKELETON = """---
{props}---

<article view=all edit=gm>
  <name>
  <body>
    <infobox align=right width=narrow>
{shown}    </infobox>
{blocks}  </body>
</article>
"""


def type_text(props, shown, blocks):
    shown_lines = "".join(f"      {s}\n" for s in shown)
    block_lines = "".join(f"    {b}\n    </block>\n" for b in blocks)
    return SKELETON.format(props=props + SOURCE, shown=shown_lines + "      <select bind=Source>\n", blocks=block_lines)


T.add("monster", "Monster", "🐉", type_text(
    """Image: image
Size: @@Size@@
Creature type: @@Creature type@@
Tags: text
Alignment: @@Alignment@@
Armor class: number
Armor: text
Initiative: text
Hit points: number
Hit dice: text
Speed: text
Abilities: group Abilities
Saving throws: text
Skills: text
Vulnerabilities: text
Resistances: text
Immunities: text
Condition immunities: text
Gear: text
Senses: text
Languages: text
Challenge rating: text
Experience: number
Proficiency bonus: number
Environment: @@Environment@@ multiple
Encountered at: @@Location@@ multiple
""",
    ["<image bind=Image>", '<select bind="Creature type">', "<select bind=Size>", "<select bind=Alignment>", '<text bind="Challenge rating">', "<select bind=Environment>", '<select bind="Encountered at">'],
    ['<block title="Stat block" style=statblock>', '<block title="Description">', '<block title="Notes">'],
))
# What one does a deed with — a spell, a thing, a feature of one's class,
# a rule anyone has (Unarmed Strike): a kill names it.
T.add("means", "Means", "🎯", description="What one does something with: a spell, an item, a feature or a rule.")
T.add("spell", "Spell", "✨", type_text(
    """Level: text
School: @@Spell school@@
Casting time: text
Range: text
Components: text
Duration: text
Attack or save: text
Damage or effect: text
Classes: @@Class@@ multiple
Subclasses: @@Subclass@@ multiple
""",
    ["<text bind=Level>", "<select bind=School>", '<text bind="Casting time">', "<text bind=Range>", "<text bind=Components>", "<text bind=Duration>", '<text bind="Attack or save">', '<text bind="Damage or effect">', "<select bind=Classes>"],
    ['<block title="Description">'],
), supertype="means")
# Things: one tree for what the books know and what a table makes up. The
# kind is the type — a weapon, a ring —, magic shows in the rarity, and
# whose a thing is in Owner. A table's own things of a kind lie below it
# (the homebrew types further down).
ITEM_SHOWN = ["<image bind=Image>", "<select bind=Category>", "<select bind=Rarity>", "<text bind=Attunement>"]
ITEM_END = ["<text bind=Cost>", "<text bind=Weight>", "<select bind=Owner>", "<select bind=Acquired>"]
T.add("item", "Item", "🎒", type_text(
    """Image: image
Category: @@Equipment category@@
Rarity: @@Rarity@@
Attunement: text
Cost: text
Weight: text
Owner: @@Character@@ multiple
Acquired: @@Session log@@ multiple
""",
    ITEM_SHOWN + ITEM_END,
    ['<block title="Description">', '<block title="Notes">'],
), supertype="means")


def kind_text(props, shown):
    """A kind of thing: its own properties, the infobox whole."""
    return SKELETON.format(props=props, shown="".join(f"      {x}\n" for x in shown + ["<select bind=Source>"]), blocks="")


T.add("weapon", "Weapon", "🗡️", kind_text(
    """Damage: text
Damage type: text
Properties: text
Mastery: text
""",
    ITEM_SHOWN + ["<text bind=Damage>", '<text bind="Damage type">', "<text bind=Properties>", "<text bind=Mastery>"] + ITEM_END), supertype="item")
T.add("armor", "Armor", "🛡️", kind_text("Armor class: text\n", ITEM_SHOWN + ['<text bind="Armor class">'] + ITEM_END), supertype="item")
ITEM_KINDS = {"wondrous-item": ("Wondrous item", "🏺"), "potion": ("Potion", "🧪"), "ring": ("Ring", "💍"), "rod": ("Rod", "🥢"),
              "scroll": ("Scroll", "📜"), "staff": ("Staff", "🦯"), "wand": ("Wand", "🪄"), "gear": ("Gear", "🧰")}
for kind_id, (kind_name, kind_icon) in ITEM_KINDS.items():
    T.add(kind_id, kind_name, kind_icon, supertype="item")
T.add("feat", "Feat", "⭐", type_text(
    """Category: @@Feat category@@
Prerequisite: text
""",
    ["<select bind=Category>", "<text bind=Prerequisite>"],
    ['<block title="Description">'],
))
T.add("background", "Background", "📜", type_text(
    """Image: image
Ability scores: text
Feat: @@Feat@@
Skill proficiencies: text
Tool proficiencies: text
Languages: text
Equipment: text
""",
    ["<image bind=Image>", '<text bind="Ability scores">', "<select bind=Feat>", '<text bind="Skill proficiencies">', '<text bind="Tool proficiencies">', "<text bind=Languages>", "<text bind=Equipment>"],
    ['<block title="Description">'],
))
T.add("species", "Species", "🧝", type_text(
    """Image: image
Creature type: @@Creature type@@
Size: text
Speed: text
""",
    ["<image bind=Image>", '<select bind="Creature type">', "<text bind=Size>", "<text bind=Speed>"],
    ['<block title="Description">'],
))
T.add("class", "Class", "🛡️", type_text(
    """Image: image
Primary ability: text
Hit point die: text
Saving throws: text
Skill proficiencies: text
Weapon proficiencies: text
Armor training: text
""",
    ["<image bind=Image>", '<text bind="Primary ability">', '<text bind="Hit point die">', '<text bind="Saving throws">'],
    ['<block title="Description">', '<block title="Features">', '<block title="Subclasses">\n      <list type=Subclass below=self>'],
))
T.add("subclass", "Subclass", "🔰", type_text(
    """Class: @@Class@@ parent
""",
    ["<select bind=Class>"],
    ['<block title="Description">'],
))
T.add("feature", "Feature", "🔹", type_text(
    """Image: image
Class: @@Class@@
Subclass: @@Subclass@@
Species: @@Species@@
Level: number
""",
    ["<image bind=Image>", "<select bind=Class>", "<select bind=Subclass>", "<select bind=Species>", "<number bind=Level>"],
    ['<block title="Description">'],
), supertype="means")
# --- What a campaign writes: the same in every world --------------------------

CAMPAIGN_DATA = {
    "Life status": ("❤️", ["Alive", "Dead", "Missing", "Unknown"]),
    "Quest status": ("🚩", ["Open", "Active", "Done", "Failed", "Declined"]),
    "Accuracy": ("🎯", ["True", "Good", "Mediocre", "False"]),
    "Kill kind": ("☠️", ["Boss", "Major", "Minion"]),
}
for name, (icon, _) in CAMPAIGN_DATA.items():
    T.add(data_type_id(name), name, icon)


def campaign_text(props, shown, blocks, sort="", players=False, name=""):
    """`players`: what the players keep themselves — they make and change
    these; the GM can still hide one. `name`: the name pattern."""
    access = "view=all edit=players create=players" if players else "view=all edit=gm"
    shown_lines = "".join(f"      {x}\n" for x in shown)
    block_lines = (f"    <sort by=\"{sort}\">\n" if sort else "") + "".join(f"    {b}\n    </block>\n" for b in blocks)
    return f"""---
{props}---

<article {access}>
  <name{f'="{name}"' if name else ""}>
  <body>
    <infobox align=right width=narrow>
{shown_lines}    </infobox>
{block_lines}  </body>
</article>
"""


CHARACTER_SHOWN = ["<image bind=Image>", "<text bind=Aliases>", "<select bind=Species>", "<text bind=Lineage>", "<select bind=Class>", "<select bind=Subclass>",
                   "<text bind=Gender>", "<text bind=Pronouns>", "<number bind=Age>", '<select bind="Life status">', "<select bind=Home>", "<select bind=Location>",
                   "<select bind=Organizations>", "<text bind=Languages>"]
T.add("character", "Character", "🧑", campaign_text(
    """Image: image
Aliases: text
Species: @@Species@@
Lineage: text
Class: @@Class@@
Subclass: @@Subclass@@
Gender: text
Pronouns: text
Age: number
Languages: text
Life status: @@Life status@@
Home: @@Location@@
Location: @@Location@@
Organizations: @@Organization@@ multiple
Spells: @@Spell@@ multiple
""",
    CHARACTER_SHOWN,
    ['<block title="Appearance">', '<block title="Personality">', '<block title="Biography">', '<block title="Relationships">', '<block title="Motivation">', '<block title="Notes">',
     '<block title="Items" bottom fold=folded>\n      <list type=Item where="Owner=self" style=table show="Type, Rarity">',
     '<block title="Kills" bottom fold=folded>\n      <list type=Kill where="Killer=self" style=table show="Kind, Enemy, With, How, Session">'],
))
# Who someone is at the table: a player's character, the GM's people, those
# who travel along — and the people of a character's own story, below them.
T.add("player-character", "Player character", "🧙", supertype="character")
T.add("npc", "NPC", "👤", supertype="character")
T.add("companion", "Companion", "🤝", supertype="character")
T.add("backstory-npc", "Backstory NPC", "👥", """---
Backstory of: @@Player character@@ parent
---

<article view=all edit=gm>
  <name>
  <body>
    <infobox align=right width=narrow>
""" + "".join(f"      {x}\n" for x in CHARACTER_SHOWN + ['<select bind="Backstory of">']) + """    </infobox>
  </body>
</article>
""", supertype="character")
T.add("location", "Location", "🗺️", campaign_text(
    """Image: image
Aliases: text
Lies in: @@Location@@ parent
Leader: @@Character@@
""",
    ["<image bind=Image>", "<text bind=Aliases>", '<select bind="Lies in">', "<select bind=Leader>"],
    ['<block title="Notes">', '<block title="Places here" bottom>\n      <list type=Location below=self>', '<block title="Who is here" bottom fold=folded>\n      <list type=Character where="Location=self">'],
))
T.add("organization", "Organization", "🏛️", campaign_text(
    """Image: image
Aliases: text
Kind: text
Leader: @@Character@@
Headquarters: @@Location@@
Active in: @@Location@@ multiple
""",
    ["<image bind=Image>", "<text bind=Aliases>", "<text bind=Kind>", "<select bind=Leader>", "<select bind=Headquarters>", '<select bind="Active in">'],
    ['<block title="Notes">', '<block title="Members" bottom>\n      <list type=Character where="Organizations=self">'],
))
T.add("session-log", "Session log", "📓", campaign_text(
    """Image: image
Session date: datetime
In-game start: date
In-game end: date
Characters: @@Character@@ multiple
Locations: @@Location@@ multiple
Organizations: @@Organization@@ multiple
Previous: @@Session log@@ = last
Next: @@Session log@@ from Previous
""",
    ["<image bind=Image>", '<date bind="Session date">', '<date bind="In-game start">', '<date bind="In-game end">', "<select bind=Characters>", "<select bind=Locations>", "<select bind=Organizations>",
     "<select bind=Previous>", "<select bind=Next>"],
    ['<block title="Journal entry">', '<block title="Plans">', '<block title="Notes">',
     '<block title="Items found" bottom fold=folded>\n      <list type=Item where="Acquired=self" style=table show="Type, Owner">',
     '<block title="Events" bottom fold=folded>\n      <list type=Event where="Session=self">',
     '<block title="Kills" bottom fold=folded>\n      <list type=Kill where="Session=self" style=table show="Killer, Kind, Enemy, With, How">'],
    sort="In-game start, In-game end, Session date, name", players=True, name="Session %%count%%",
))
T.add("event", "Event", "📅", campaign_text(
    """Image: image
Date: date
Until: date
Session: @@Session log@@
Characters: @@Character@@ multiple
Locations: @@Location@@ multiple
""",
    ["<image bind=Image>", "<date bind=Date>", "<date bind=Until>", "<select bind=Session>", "<select bind=Characters>", "<select bind=Locations>"],
    ['<block title="Description">'],
    sort="Date, Session, name",
))
T.add("quest", "Quest", "🧭", campaign_text(
    """Image: image
Status: @@Quest status@@
Characters: @@Character@@ multiple
Organizations: @@Organization@@ multiple
Locations: @@Location@@ multiple
Monsters: @@Monster@@ multiple
""",
    ["<image bind=Image>", "<select bind=Status>", "<select bind=Characters>", "<select bind=Organizations>", "<select bind=Locations>", "<select bind=Monsters>"],
    ['<block title="Summary">', '<block title="Details">'], players=True,
))
T.add("rumour", "Rumour", "💬", campaign_text(
    """Image: image
Accuracy: @@Accuracy@@
Characters: @@Character@@ multiple
Organizations: @@Organization@@ multiple
Locations: @@Location@@ multiple
Monsters: @@Monster@@ multiple
""",
    ["<image bind=Image>", "<select bind=Accuracy>", "<select bind=Characters>", "<select bind=Organizations>", "<select bind=Locations>", "<select bind=Monsters>"],
    ['<block title="Summary">', '<block title="Details">'], players=True,
))
T.add("kill", "Kill", "💀", campaign_text(
    """Killer: @@Character@@
Kind: @@Kill kind@@
Enemy: @@Monster@@
With: @@Means@@ multiple
How: text
Session: @@Session log@@
""",
    ["<select bind=Killer>", "<select bind=Kind>", "<select bind=Enemy>", "<select bind=With>", "<text bind=How>", "<select bind=Session>"],
    [], players=True,
))
# A table's own things, monsters and rules: below the kind the books know.
HOMEBREW = ["weapon", "armor", *[k for k in ITEM_KINDS], "monster", "species", "class", "subclass", "background", "feat", "spell", "feature"]
for base in HOMEBREW:
    T.add(f"homebrew-{base}", f"Homebrew {T.by_id[base]['name'].lower()}", T.by_id[base]["icon"], supertype=base)

T.add("rule", "Rule", "📖", """<article view=all edit=gm>
  <name>
  <body>
    <block title="Description">
    </block>
  </body>
</article>
""", supertype="means")


T.add("homebrew-rule", "Homebrew rule", "📖", supertype="rule")

# --- Values: game terms, the same in every world ------------------------------

VALUES = {
    "Size": ["Tiny", "Small", "Medium", "Large", "Huge", "Gargantuan"],
    "Creature type": ["Aberration", "Beast", "Celestial", "Construct", "Dragon", "Elemental", "Fey", "Fiend", "Giant", "Humanoid", "Monstrosity", "Ooze", "Plant", "Undead"],
    "Alignment": ["Lawful good", "Neutral good", "Chaotic good", "Lawful neutral", "Neutral", "Chaotic neutral", "Lawful evil", "Neutral evil", "Chaotic evil", "Unaligned"],
    "Spell school": ["Abjuration", "Conjuration", "Divination", "Enchantment", "Evocation", "Illusion", "Necromancy", "Transmutation"],
    "Rarity": ["Common", "Uncommon", "Rare", "Very rare", "Legendary", "Artifact"],
    "Feat category": ["Origin", "General", "Fighting style", "Epic boon"],
    "Environment": ["Arctic", "Coastal", "Desert", "Forest", "Grassland", "Hill", "Mountain", "Swamp", "Underdark", "Underwater", "Urban"],
    **{name: values for name, (_, values) in CAMPAIGN_DATA.items()},
}


def data_id(kind, name):
    """A value's document id — by its name, so worlds built apart still agree."""
    return article_id(kind, name.lower())
