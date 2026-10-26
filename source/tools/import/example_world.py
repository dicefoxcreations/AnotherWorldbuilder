"""An example world to show what the app does: a small, made-up coast with
places, people, a family, a creature, events and sessions — and a map.

    python3 tools/import/example_world.py [out.awpack]

Without an argument it writes `build/packaging/Example world.awpack`. Needs
the test bench running (`cargo run -p aw-testbench`), and Pillow and NumPy
for the map, which is drawn here from noise. Everything in it is invented
for this example; it is free to use as one likes.
"""

import json
import os
import sys

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

sys.path.insert(0, os.path.dirname(__file__))
import awpack  # noqa: E402
from awpack import article_id, entry, link, part  # noqa: E402

HERE = os.path.dirname(__file__)
OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "..", "..", "build", "packaging", "Example world.awpack")


def doc(name):
    return article_id("example", name)


def day(d, month, year=412):
    """A day of the usual calendar: twelve months of thirty days."""
    return (year - 1) * 360 + (month - 1) * 30 + (d - 1)


# --- The map -----------------------------------------------------------------


def draw_map(path, seed=7, w=1600, h=1000):
    rng = np.random.default_rng(seed)
    noise = np.zeros((h, w))
    for octave in range(6):
        cells = 3 * 2**octave
        grid = rng.random((cells + 1, int(cells * w / h) + 2))
        small = Image.fromarray((grid * 255).astype(np.uint8)).resize((w, h), Image.BICUBIC)
        noise += np.asarray(small, dtype=float) / 255 / 2**octave
    noise /= noise.max()
    # Land to the west, the sea to the east, a bay in the middle.
    x = np.linspace(0, 1, w)[None, :]
    y = np.linspace(0, 1, h)[:, None]
    bay = np.exp(-((x - 0.55) ** 2 / 0.02 + (y - 0.5) ** 2 / 0.05))
    height = noise * 0.9 + (0.75 - x) * 1.1 - bay * 0.6
    land = height > 0.45
    sea = np.array([196, 214, 210])
    shore = np.array([226, 214, 178])
    field = np.array([214, 202, 160])
    hill = np.array([178, 160, 120])
    img = np.zeros((h, w, 3))
    img[:] = sea
    depth = np.clip((0.45 - height) * 3, 0, 1)[..., None]
    img = img * (1 - depth * 0.25)
    t = np.clip((height - 0.45) * 4, 0, 1)[..., None]
    land_colour = shore * (1 - t) + field * t
    t2 = np.clip((height - 0.8) * 3, 0, 1)[..., None]
    land_colour = land_colour * (1 - t2) + hill * t2
    img = np.where(land[..., None], land_colour, img)
    picture = Image.fromarray(img.clip(0, 255).astype(np.uint8))
    # The coastline, inked.
    edge = Image.fromarray((land * 255).astype(np.uint8)).filter(ImageFilter.FIND_EDGES).filter(ImageFilter.MaxFilter(3))
    ink = Image.new("RGB", (w, h), (92, 74, 52))
    picture.paste(ink, mask=edge)
    # Reeds in the marsh, hills in the west, inked like the coast.
    d = ImageDraw.Draw(picture)
    ink_colour = (92, 74, 52)
    for _ in range(140):
        px, py = int(rng.uniform(0.12, 0.36) * w), int(rng.uniform(0.45, 0.75) * h)
        if land[py, px]:
            d.line([(px, py), (px - 3, py - 10)], fill=ink_colour, width=2)
            d.line([(px, py), (px + 3, py - 9)], fill=ink_colour, width=2)
            d.line([(px, py), (px, py - 12)], fill=ink_colour, width=2)
    for _ in range(45):
        px, py = int(rng.uniform(0.03, 0.3) * w), int(rng.uniform(0.08, 0.4) * h)
        if land[py, px]:
            d.arc([px - 26, py - 16, px + 26, py + 22], 200, 340, fill=ink_colour, width=3)
    for _ in range(60):
        px, py = int(rng.uniform(0.3, 0.45) * w), int(rng.uniform(0.15, 0.45) * h)
        if land[py, px]:
            d.ellipse([px - 7, py - 14, px + 7, py], outline=ink_colour, width=2)
            d.line([(px, py), (px, py + 6)], fill=ink_colour, width=2)
    # Parchment grain and a frame.
    grain = (rng.random((h, w)) * 18).astype(np.uint8)
    picture = Image.fromarray(np.clip(np.asarray(picture, dtype=int) - grain[..., None], 0, 255).astype(np.uint8))
    d = ImageDraw.Draw(picture)
    for gx in range(0, w, 200):
        d.line([(gx, 0), (gx, h)], fill=(150, 140, 120), width=1)
    for gy in range(0, h, 200):
        d.line([(0, gy), (w, gy)], fill=(150, 140, 120), width=1)
    d.rectangle([4, 4, w - 5, h - 5], outline=(92, 74, 52), width=6)
    picture.save(path, quality=88)
    return land


# --- Types ---------------------------------------------------------------------

T = awpack.Types()
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
    return SKELETON.format(
        props=props,
        shown="".join(f"      {s}\n" for s in shown),
        blocks="".join(f"    {b}\n    </block>\n" for b in blocks),
    )


T.add("relation", "Relation", "🔗", description="What someone is to someone else.")
T.add("abilities", "Abilities", "💪", group=True, text="""Might: number
Agility: number
Wits: number
Resolve: number
mod(x) = x
check(x) = 2d6 + x
""")
T.add("character", "Character", "🧑", type_text(
    """Role: text
Home: @@Place@@
Faction: @@Faction@@
Born: date
Died: date
Kinship: @@Character@@ as @@Relation@@
""",
    ["<text bind=Role>", "<select bind=Home>", "<select bind=Faction>", "<date bind=Born>", "<date bind=Died>"],
    ['<block title="Appearance">', '<block title="Story">', '<block title="Secrets" view=gm>'],
))
T.add("place", "Place", "📍", type_text(
    """Kind: text
Region: @@Place@@
""",
    ["<text bind=Kind>", "<select bind=Region>"],
    ['<block title="Description">', '<block title="Secrets" view=gm>'],
))
T.add("faction", "Faction", "⚑", type_text(
    """Leader: @@Character@@
Seat: @@Place@@
""",
    ["<select bind=Leader>", "<select bind=Seat>"],
    ['<block title="Aims">'],
))
T.add("creature", "Creature", "🦑", type_text(
    """Threat: text
Hit points: number
Abilities: group Abilities
Found at: @@Place@@ multiple
""",
    ["<text bind=Threat>", '<select bind="Found at">'],
    ['<block title="" style=statblock top>',
     '<block title="Lore">'],
))
T.add("event", "Event", "📅", type_text(
    """Date: date
Until: date
Where: @@Place@@
Involved: @@Character@@ multiple
""",
    ["<date bind=Date>", "<date bind=Until>", "<select bind=Where>", "<select bind=Involved>"],
    ['<block title="What happened">'],
))
T.add("session", "Session", "📜", type_text(
    """Played on: datetime
In-game date: date
Present: @@Character@@ multiple
""",
    ['<date bind="Played on">', '<date bind="In-game date">', "<select bind=Present>"],
    ['<block title="Summary">'],
))

TYPES = {t["id"]: t for t in T.compile()}


def keys_of(type_id):
    out = {}
    todo = [type_id]
    while todo:
        t = TYPES[todo.pop()]
        for f in t["properties"]:
            out[f["label"]] = f["key"]
            if f.get("kind") == "group":
                todo.append(f["group"])
        if t.get("supertype"):
            todo.append(t["supertype"])
    return out


def section(type_id, title):
    for b in TYPES[type_id]["blocks"]:
        if b["title"] == title:
            return b["key"]
    raise KeyError(f"{type_id}: {title}")


P = awpack.Package("The Gullwind Coast")
P.types = list(TYPES.values())


def add(type_id, name, fields=None, texts=None, own=None):
    """A document: its properties by label, its type's sections by title,
    and sections of its own (title, text, view)."""
    keys = keys_of(type_id)
    props = {keys[k]: v for k, v in (fields or {}).items() if v not in (None, "", [])}
    parts = [part("properties", fields=props)] if props else []
    names = {k.lower(): v for k, v in keys.items()}
    for title, text in (texts or {}).items():
        text = awpack.stored_names(text, names)
        block = section(type_id, title)
        view = "gm" if title == "Secrets" else "all"
        parts.append(part(block, text, view=view))
    for n, (title, text) in enumerate(own or [], 1):
        parts.append(part(f"notes-{n}", text, title=title))
    P.document(doc(name), type_id, name, parts)


for role in ["Mother", "Father", "Daughter", "Son", "Wife", "Husband", "Sister", "Brother"]:
    P.document(doc("relation " + role), "relation", role, [])


def kin(role, own_role, other):
    return {"with": doc(other), "role": doc("relation " + role), "own_role": doc("relation " + own_role)}


# --- Places ----------------------------------------------------------------------

os.makedirs(os.path.join(os.path.dirname(OUT) or ".", "example-images"), exist_ok=True)
MAP_FILE = os.path.join(os.path.dirname(OUT) or ".", "example-images", "gullwind-coast.jpg")
draw_map(MAP_FILE)
MAP = P.image(MAP_FILE, "Map of the Gullwind Coast")

COAST_MAP = f"""<map src={MAP} height=400>
<pin at=50,63 label="Port Hallow" to={doc("Port Hallow")} icon=anchor color=amber>
<pin at=94,65 label="Greyspire Lighthouse" label-at=left to={doc("Greyspire Lighthouse")} icon=tower color=teal>
<pin at=24,58 label="Saltreed Marsh" to={doc("Saltreed Marsh")} icon=tree color=green>
<pin at=67,85 label="Tidecaller Caves" to={doc("Tidecaller Caves")} icon=cave color=violet label-at=bottom>
</map>"""

add("place", "The Gullwind Coast", {"Kind": "Region"}, {
    "Description": "A ragged coast of salt marsh, chalk cliffs and fishing towns. Storms come in from the east without warning; the lighthouses are older than anyone remembers who built them.\n\n" + COAST_MAP,
    "Secrets": "Below the bay lies a drowned town. Its bells still ring at the lowest tides.",
})
add("place", "Port Hallow", {"Kind": "Harbour town", "Region": doc("The Gullwind Coast")}, {
    "Description": f"The largest town on the coast: tar, gulls and too many taverns. The {link(doc('The Lantern Guild'), 'Lantern Guild')} keeps its hall on the harbour wall.",
})
add("place", "Greyspire Lighthouse", {"Kind": "Lighthouse", "Region": doc("The Gullwind Coast")}, {
    "Description": "A black tower on a chalk spur. Its lamp burns blue on nights the keepers will not talk about.",
    "Secrets": f"The lamp is fed by something in the {link(doc('Tidecaller Caves'), 'Tidecaller Caves')}.",
})
add("place", "Saltreed Marsh", {"Kind": "Marsh", "Region": doc("The Gullwind Coast")}, {
    "Description": "Reeds taller than a rider, paths that move with the tide, and the smoke of eel-smokers' huts.",
})
add("place", "Tidecaller Caves", {"Kind": "Sea caves", "Region": doc("The Gullwind Coast")}, {
    "Description": "Reachable only at low tide. The walls are carved with spirals that seem to turn when lamplight moves.",
})
add("place", "The Drowned Chapel", {"Kind": "Ruin", "Region": doc("The Gullwind Coast")}, {
    "Description": "A chapel roof that shows above the water once a month.",
    "Secrets": "The Brine Syndicate meets here. The chapel's bell is the signal.",
})

# --- People, a family ------------------------------------------------------------

add("character", "Ilsa Marrow", {
    "Role": "Harbourmaster", "Home": doc("Port Hallow"), "Faction": doc("The Lantern Guild"), "Born": day(14, 3, 371),
    "Kinship": [kin("Mother", "Daughter", "Agna Marrow"), kin("Father", "Daughter", "Tobin Marrow"), kin("Brother", "Sister", "Corin Marrow")],
}, {
    "Appearance": "Weathered, sharp-eyed, a brass whistle on a cord around her neck.",
    "Story": f"Ilsa runs the harbour of {link(doc('Port Hallow'), 'Port Hallow')} with a ledger and a loud voice. She lost her father to {link(doc('The Great Storm'), 'the Great Storm')} and has distrusted the lighthouses ever since.",
    "Secrets": f"She pays the {link(doc('The Brine Syndicate'), 'Brine Syndicate')} to leave her ships alone — and keeps a copy of every payment.",
})
add("character", "Corin Marrow", {
    "Role": "Smuggler", "Home": doc("Saltreed Marsh"), "Faction": doc("The Brine Syndicate"), "Born": day(2, 9, 376),
    "Kinship": [kin("Mother", "Son", "Agna Marrow"), kin("Father", "Son", "Tobin Marrow"), kin("Sister", "Brother", "Ilsa Marrow")],
}, {
    "Appearance": "Charming, soaked, always smells of eel smoke.",
    "Story": "Ilsa's younger brother. Officially an eel-smoker in the marsh.",
    "Secrets": "Corin rings the bell of the Drowned Chapel.",
})
add("character", "Agna Marrow", {
    "Role": "Net-mender", "Home": doc("Port Hallow"), "Born": day(20, 1, 348),
    "Kinship": [kin("Husband", "Wife", "Tobin Marrow"), kin("Daughter", "Mother", "Ilsa Marrow"), kin("Son", "Mother", "Corin Marrow")],
}, {
    "Story": "Mends nets on the harbour steps and knows every rumour before it is one.",
})
add("character", "Tobin Marrow", {
    "Role": "Lighthouse keeper", "Home": doc("Greyspire Lighthouse"), "Born": day(5, 6, 344), "Died": day(11, 10, 402),
    "Kinship": [kin("Wife", "Husband", "Agna Marrow"), kin("Daughter", "Father", "Ilsa Marrow"), kin("Son", "Father", "Corin Marrow")],
}, {
    "Story": f"Keeper of {link(doc('Greyspire Lighthouse'), 'Greyspire')} for thirty years. Lost in {link(doc('The Great Storm'), 'the Great Storm')}.",
    "Secrets": "Tobin did not drown. He walked into the Tidecaller Caves and did not come out.",
})
add("character", "Brother Odo", {"Role": "Keeper of the lamp", "Home": doc("Greyspire Lighthouse"), "Faction": doc("The Lantern Guild"), "Born": day(30, 11, 380)}, {
    "Appearance": "Thin, soot-stained hands, speaks in a whisper.",
    "Story": "Took over the lighthouse after Tobin. Says the blue light is a blessing.",
})
add("character", "Wren", {"Role": "Player character · scout", "Home": doc("Saltreed Marsh"), "Born": day(8, 4, 391)}, {
    "Story": "Grew up in the marsh. Owes Corin a favour.",
})

ilsa, corin, agna, tobin = (doc(n) for n in ["Ilsa Marrow", "Corin Marrow", "Agna Marrow", "Tobin Marrow"])
FAMILY = f"""<graph via=Kinship show="type, roles">
<person who={tobin} at=0,0>
<person who={agna} at=16,0>
<person who={ilsa} at=0,8>
<person who={corin} at=16,8>
<line between="{tobin}, {agna}" sides="right, left" shape=straight>
<line between="{tobin} & {agna}, {ilsa} & {corin}" shape=corner say="Parents, Children">
</graph>"""
P.documents[ilsa]["parts"].append(part("notes-1", FAMILY, title="Family"))

# --- Factions -------------------------------------------------------------------

add("faction", "The Lantern Guild", {"Leader": doc("Brother Odo"), "Seat": doc("Port Hallow")}, {
    "Aims": "Keep the lights burning, the ships safe — and the guild's tolls paid.",
})
add("faction", "The Brine Syndicate", {"Seat": doc("The Drowned Chapel")}, {
    "Aims": "Smuggling salt, silver and secrets past the guild's lights.",
})

# --- A creature -----------------------------------------------------------------

add("creature", "Reef Lurker", {
    "Threat": "Large sea beast, hunts at night", "Hit points": 34, "Might": 3, "Agility": 1, "Wits": -1, "Resolve": 2,
    "Found at": [doc("Tidecaller Caves"), doc("The Drowned Chapel")],
}, {
    "": """# Reef Lurker
*Large sea beast, hunts at night*

---

**Hit points** %%Hit points%%

---
<abilities show=Abilities with=mod short="MIG, AGI, WIT, RES">
---

## Actions

***Grasping tentacles.*** [[+ Might mod]] to hit. *Hit:* 2d6 + 1 crushing, and the target is held.

***Ink cloud.*** Everyone within reach rolls [[+ Wits mod]] or loses sight of it for a round.""",
    "Lore": "Fishermen say a lurker can count lamps. Where one goes dark, it comes.",
})

# --- Events and sessions --------------------------------------------------------

add("event", "The Great Storm", {"Date": day(11, 10, 402), "Until": day(13, 10, 402), "Where": doc("The Gullwind Coast"), "Involved": [tobin]}, {
    "What happened": "Three days of storm. Greyspire went dark on the second night; eleven ships were lost.",
})
add("event", "The blue light", {"Date": day(1, 1, 403), "Where": doc("Greyspire Lighthouse"), "Involved": [doc("Brother Odo")]}, {
    "What happened": "On the first night of the year, the lamp of Greyspire burned blue for the first time.",
})
add("event", "Port Hallow is founded", {"Date": day(1, 5, 210), "Where": doc("Port Hallow")}, {
    "What happened": "Fishers from the marsh build a harbour wall where the bay is calmest.",
})
add("event", "The Lantern Guild takes the lights", {"Date": day(21, 6, 288), "Where": doc("Greyspire Lighthouse")}, {
    "What happened": "The towns hand the lighthouses to the guild — and with them, the tolls.",
})
add("event", "The first bell", {"Date": day(9, 12, 398), "Where": doc("The Drowned Chapel"), "Involved": [corin]}, {
    "What happened": "At the lowest tide of the year, a bell rings under the bay. Two smugglers' boats come in that night.",
})
add("session", "Session 1 – Fog over Port Hallow", {"Played on": "2026-09-05T19:00", "In-game date": day(3, 8, 412), "Present": [doc("Wren")]}, {
    "Summary": f"Wren arrives in {link(doc('Port Hallow'), 'Port Hallow')} with a sealed letter for {link(ilsa, 'Ilsa Marrow')}. A ship comes in without a crew.",
})
add("session", "Session 2 – The bell at low tide", {"Played on": "2026-09-12T19:00", "In-game date": day(5, 8, 412), "Present": [doc("Wren")]}, {
    "Summary": f"Following the bell, Wren finds a boat hidden in the reeds of {link(doc('Saltreed Marsh'), 'Saltreed Marsh')}, and Corin's knife in it.",
})

# --- The world's page and outline -----------------------------------------------

P.home = doc("home")
P.document(P.home, "", "The Gullwind Coast", [part("notes-1", f"""Welcome to the Gullwind Coast — an example world. Everything here is invented; change it, delete it, or start your own.

{COAST_MAP}

<timeline type="Event, Session" until=Until text="What happened, Summary">""", title="")])
MONTHS = ["First Moon", "Thaw Moon", "Seed Moon", "Blossom Moon", "Pasture Moon", "Sun Moon", "Hay Moon", "Harvest Moon", "Mist Moon", "Fall Moon", "Frost Moon", "Dark Moon"]
P.calendar = {"eras": [{
    "name": "Reckoning of the Coast", "abbr": "AR", "before": "",
    "months": [{"name": m, "days": 30} for m in MONTHS],
    "weekdays": ["Firstday", "Secondday", "Thirdday", "Fourthday", "Fifthday", "Restday"],
    "leap": None, "start": 0,
    "moons": [{"name": "The Lantern", "color": "#e8dcae", "period": 29.5, "full": day(1, 1, 403)}],
}]}
P.navigation = [
    entry("story", "group", label="Story", children=[entry("sessions", "type", "session", "Sessions"), entry("events", "type", "event", "Events")]),
    entry("world", "group", label="World", children=[
        entry("places", "type", "place", "Places"), entry("characters", "type", "character", "Characters"),
        entry("factions", "type", "faction", "Factions"), entry("creatures", "type", "creature", "Creatures")]),
]

P.write(OUT)
print(json.dumps({"out": os.path.abspath(OUT), "types": len(P.types), "documents": len(P.documents)}))
