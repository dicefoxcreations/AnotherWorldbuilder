"""The D&D 5e template as a package to ship: types, value lists, an outline —
no content.

    python3 tools/import/dnd_template.py [out.awpack]

Without an argument it writes the published one, `templates/D&D 5e.awpack`
beside `source/`.

Needs the test bench running (`cargo run -p aw-testbench`). Nothing here
comes from a book or an online library; a library import of one's own (not
part of the app) adds that, for one's own worlds only. Ids agree with such a
blueprint's, so a world made from this template can take it later.
"""

import json
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
import awpack  # noqa: E402
from awpack import entry  # noqa: E402
from dnd_types import VALUES, T, data_id, data_type_id  # noqa: E402

OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), "..", "..", "..", "templates", "D&D 5e.awpack")

P = awpack.Package("D&D 5e")
P.types = T.compile()
for kind, values in VALUES.items():
    for value in values:
        P.document(data_id(kind, value), data_type_id(kind), value, [])

P.navigation = [
    entry("story", "group", label="Story", children=[
        entry("sessions", "type", "session-log", "Sessions"), entry("quests", "type", "quest", "Quests"),
        entry("rumours", "type", "rumour", "Rumours"), entry("events", "type", "event", "Events"), entry("kills", "type", "kill", "Killboard")]),
    entry("world", "group", label="World", children=[
        entry("characters", "type", "character", "Characters"), entry("locations", "type", "location", "Locations"),
        entry("organizations", "type", "organization", "Organizations"), entry("monsters", "type", "monster", "Monsters")]),
    entry("things", "group", label="Items & magic", children=[
        entry("items", "type", "item", "Items"), entry("spells", "type", "spell", "Spells"), entry("magic-items", "type", "magic-item", "Magic items"),
        entry("equipment", "type", "equipment")]),
    entry("rules", "group", label="Rules", children=[
        entry("species", "type", "species"), entry("classes", "type", "class"), entry("backgrounds", "type", "background"),
        entry("feats", "type", "feat"), entry("rules-glossary", "type", "rule")]),
]

P.write(OUT)
print(json.dumps({"types": len(P.types), "documents": len(P.documents)}))
