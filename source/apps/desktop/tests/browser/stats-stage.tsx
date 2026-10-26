import { useState } from "react";
import type { ArticleView, DocumentText, Template, TemplateField, WorldInfo } from "../../src/api/client";
import { ArticlePage } from "../../src/features/wiki/ArticlePage";
import { VisualEditor } from "../../src/features/wiki/Document";
import { WorldDataContext } from "../../src/features/worlds/WorldData";
import { DiceButton, DiceTray } from "../../src/features/dice/DiceTray";
import { LinkPreview } from "../../src/features/wiki/LinkPreview";
import { setFixture } from "./core";

/**
 * Stat blocks, rolls and numbers that feats add to — a goblin as the rule
 * book prints it, and a character whose feat raises his Strength. The
 * abilities are a property group every type takes in.
 */

const WORLD: WorldInfo = {
  id: "w",
  name: "Test",
  icon: "🌍",
  publicKey: "",
  tokenFile: "",
  folder: "",
  storage: "local",
  cloudFolders: [],
  role: "gm",
  isGm: true,
  attributes: [],
  members: [],
  articleCount: 0,
  home: "",
  requests: 0,
  invited: [],
  handover: null,
  moderated: false,
  waiting: 0,
  versionLimit: 50,
};

const ABILITIES = ["Strength", "Dexterity", "Constitution", "Intelligence", "Wisdom", "Charisma"];
const number = (label: string): TemplateField => ({ key: label.toLowerCase().replace(/ /g, "-"), label, kind: "number" });

const base = {
  supertype: "",
  group: false,
  rules: [],
  description: "",
  auth: null,
  namePattern: "%%input%%",
  page: "",
};

/**
 * The six abilities — one group, taken in by every type below — and the
 * rules of the game they are reckoned by: nothing of it is the app's.
 */
const ABILITY_GROUP: Template = {
  ...base,
  group: true,
  id: "abilities",
  name: "Abilities",
  icon: "🧩",
  properties: ABILITIES.map(number),
  rules: [
    { name: "mod", params: ["x"], formula: "floor((x - 10) / 2)" },
    { name: "check", params: ["x"], formula: "1d20 + x" },
    { name: "crit", params: [], formula: "20" },
    { name: "fumble", params: [], formula: "1" },
  ],
  infobox: { align: "none", width: "auto", show: [] },
  blocks: [],
};

const abilities = (label = "Abilities"): TemplateField => ({ key: label.toLowerCase(), label, kind: "group", group: "abilities" });

const MONSTER: Template = {
  ...base,
  id: "monster",
  name: "Monster",
  icon: "👹",
  properties: [
    number("Armor class"),
    number("Hit points"),
    abilities(),
    number("Proficiency bonus"),
    { key: "skills", label: "Skills", kind: "reference", articleType: "skill", multiple: true, parent: false, count: "Proficiency", every: true },
  ],
  infobox: { align: "none", width: "auto", show: [] },
  blocks: [{ key: "stats", title: "", style: "statblock", parts: [{ key: "stats", text: "", view: "all", edit: "gm", level: "open" }] }],
};

const FEAT: Template = {
  ...base,
  id: "feat",
  name: "Feat",
  icon: "⭐",
  properties: [abilities()],
  infobox: { align: "none", width: "auto", show: [] },
  blocks: [],
};

const ITEM: Template = {
  ...FEAT,
  id: "item",
  name: "Item",
  icon: "🧤",
  infobox: { align: "right", width: "auto", show: [{ element: "number", property: "strength", label: "" }] },
  blocks: [{ key: "stats", title: "", style: "text", parts: [{ key: "stats", text: "", view: "all", edit: "gm", level: "open" }] }],
};

const ref = (key: string, label: string, articleType: string, more: Partial<{ multiple: boolean; adds: boolean }> = {}): TemplateField =>
  ({ key, label, kind: "reference", articleType, multiple: false, parent: false, ...more });

const SPECIES: Template = { ...FEAT, id: "species", name: "Species", icon: "🧬", properties: [abilities(), { ...number("Speed"), unit: "m" } as TemplateField] };
/** What a class brings, each from a level on. */
const features = (): TemplateField => ({ ...ref("features", "Features", "feature", { multiple: true }), count: "From level" }) as TemplateField;
const CLASS: Template = { ...FEAT, id: "class", name: "Class", icon: "⚔", properties: [number("Hit die"), features()] };
const SUBCLASS: Template = {
  ...FEAT,
  id: "subclass",
  name: "Subclass",
  icon: "🔰",
  properties: [{ ...ref("class", "Class", "class"), parent: true } as TemplateField, features()],
};
const FEATURE: Template = { ...FEAT, id: "feature", name: "Feature", icon: "✨", properties: [] };
/**
 * A skill, or a saving throw: its base is a formula of its own —
 * `[[Dexterity mod]]` — reckoned for whoever has it, and with how
 * well they have it.
 */
const PROBE = (id: string, name: string, icon: string): Template => ({
  ...FEAT,
  id,
  name,
  icon,
  properties: [number("Base"), { ...number("Bonus"), default: "[[Base + floor(Proficiency * Proficiency bonus)]]" } as TemplateField],
});
const SKILL = PROBE("skill", "Skill", "🎯");
const SAVE = PROBE("saving-throw", "Saving throw", "🛡");

/**
 * A character's values: the abilities, what they fight with — reckoned
 * from class, level and Constitution — and what is picked. Its layout is
 * the sheet every character shows with `<view of="Character stats">`.
 */
const COMBAT: Template = {
  ...ABILITY_GROUP,
  id: "combat",
  name: "Combat",
  properties: [
    number("Armor class"),
    // The first class's die in full, then each class's average — or the dice as rolled.
    {
      ...number("Hit points"),
      default:
        "[[first(Classes.Hit die) + or(Rolled, Classes.(Level * (Hit die / 2 + 1)) - first(Classes.(Hit die / 2 + 1))) + Level * Constitution mod]]",
    } as TemplateField,
    { ...number("Speed"), unit: "m", default: "[[Species.Speed]]" } as TemplateField,
  ],
};
const SHEET = `# %%Name%%

---

**Armor class** %%Armor class%%
**Hit points** %%Hit points%% ([[Classes.((Level)d(Hit die)) + Level * Constitution mod]])
**Classes** %%Classes%%
**Speed** %%Speed%%
**Proficiency bonus** +%%Proficiency bonus%%
**Passive perception** [[10 + Skills[Perception].Bonus]]

---
<abilities show=Abilities with=mod short="STR, DEX, CON, INT, WIS, CHA" also="Saving throws.Bonus" heading=Abilities also-heading=Saves>
---

## Skills

<list from=Skills show="●Proficiency, [[+ Bonus]]" columns=3>

## Features

<list from=Classes.Features when="From level <= Level">
<list from=Subclasses.Features when="From level <= Classes[Class].Level">

## Feats

%%Feats%%

## Equipment

%%Equipment%%`;
const CHARACTER_STATS: Template = {
  ...ABILITY_GROUP,
  id: "character-stats",
  name: "Character stats",
  properties: [
    abilities(),
    { key: "combat", label: "Combat", kind: "group", group: "combat" },
    ref("feats", "Feats", "feat", { multiple: true, adds: true }),
    ref("equipment", "Equipment", "item", { multiple: true, adds: true }),
  ],
  blocks: [{ key: "layout", title: "", style: "text", parts: [{ key: "layout", text: SHEET, view: "all", edit: "all", level: "open" }] }],
};

const CHARACTER: Template = {
  ...base,
  id: "character",
  name: "Character",
  icon: "🧑",
  properties: [
    ref("species", "Species", "species", { adds: true }),
    { ...ref("classes", "Classes", "class", { multiple: true }), count: "Level" } as TemplateField,
    ref("subclasses", "Subclasses", "subclass", { multiple: true }),
    { ...number("Level"), default: "[[Classes.Level]]" } as TemplateField,
    { ...number("Proficiency bonus"), default: "[[steps(Level, 1, 2, 5, 3, 9, 4, 13, 5, 17, 6)]]" } as TemplateField,
    { ...ref("saving-throws", "Saving throws", "saving-throw", { multiple: true }), count: "Proficiency", every: true } as TemplateField,
    { ...ref("skills", "Skills", "skill", { multiple: true }), count: "Proficiency", every: true } as TemplateField,
    number("Rolled"),
    { key: "character-stats", label: "Character stats", kind: "group", group: "character-stats" },
  ],
  infobox: {
    align: "right",
    width: "auto",
    show: [...ABILITIES.slice(0, 3).map((a) => ({ element: "number", property: a.toLowerCase(), label: "" })), { element: "select", property: "feats", label: "" }, { element: "select", property: "equipment", label: "" }],
  },
  blocks: [
    { key: "stats", title: "Stats", style: "statblock", place: "top", parts: [{ key: "stats", text: "", view: "all", edit: "gm", level: "open" }] },
    {
      key: "story",
      title: "Story",
      style: "text",
      parts: [
        { key: "story", text: "", view: "all", edit: "gm", level: "open" },
        { key: "story-2", text: "", view: "gm", edit: "gm", level: "open" },
      ],
    },
    { key: "notes", title: "Notes", style: "text", new: ["Character"], parts: [{ key: "notes", text: "", view: "all", edit: "gm", level: "open" }] },
    { key: "family", title: "Family", style: "text", place: "bottom", parts: [{ key: "family", text: "", view: "all", edit: "gm", level: "open" }] },
  ],
};

const TEMPLATES = [ABILITY_GROUP, COMBAT, CHARACTER_STATS, MONSTER, FEAT, ITEM, SPECIES, CLASS, SUBCLASS, FEATURE, SKILL, SAVE, CHARACTER];

const GOBLIN_TEXT = `# Goblin
*Small humanoid (goblinoid), neutral evil*

---

**Armor class** %%Armor class%% (leather armor, shield)
**Hit points** %%Hit points%% (2d6)
**Speed** 9 m

---
<abilities show=Abilities with=mod short="STR, DEX, CON, INT, WIS, CHA">
---

**Skills** <list from=Skills when=Proficiency style=inline show="[[+ Bonus]]">
**Senses** darkvision 18 m, passive perception [[10 + Wisdom mod]]
**Languages** Common, Goblin
**Challenge** 1/4 (50 XP)

---

***Nimble Escape.*** The goblin can take the Disengage or Hide action as a bonus action on each of its turns.

## Actions

***Scimitar.*** *Melee Weapon Attack:* [[+ Dexterity mod + 2]] to hit, reach 1.5 m, one target. *Hit:* 5 (1d6 + 2) slashing damage.

***Shortbow.*** *Ranged Weapon Attack:* [[+4]] to hit, range 24/96 m, one target. *Hit:* 5 (1d6 + 2) piercing damage.

<view of="Character stats">
`;

const BRAKKA_TEXT = `<view of="Character stats">

## Actions

***Axe.*** [[+ Strength mod + 2]] to hit, *Hit:* [[1d8 + Strength mod]] slashing damage.
`;

function layer(owner: string, section: string, text: string, fields: Record<string, unknown> = {}) {
  return { id: `${owner}-${section}`, title: "", section, mayEdit: true, text, fields, policy: "all", edit: "gm", level: "open", epoch: 1, parent: null } as unknown as ArticleView["layers"][number];
}

function article(id: string, name: string, articleType: string, fields: Record<string, unknown>, text = ""): ArticleView {
  return {
    id,
    articleType,
    slug: id,
    name,
    visibility: "all",
    edit: "gm",
    layerTotal: 2,
    layers: [layer(id, "properties", "", fields), ...(text ? [layer(id, "stats", text)] : [])],
  };
}

const scores = (values: number[]) => Object.fromEntries(ABILITIES.map((a, i) => [a.toLowerCase(), values[i]]));

const ARTICLES: ArticleView[] = [
  article("goblin", "Goblin", "monster", { "armor-class": 15, "hit-points": 7, ...scores([8, 14, 10, 10, 8, 8]), "proficiency-bonus": 2, skills: [{ with: "stealth", count: 2 }] }, GOBLIN_TEXT),
  article("athlete", "Athlete", "feat", { strength: "+1" }),
  article("tough", "Tough", "feat", { constitution: "+2" }),
  article("ogre-gauntlets", "Ogre gauntlets", "item", { strength: ">=19" }, "Coarse gloves of grey-green leather, far too big for any hand — until one puts them on. Stronger than any @@Feat:Athlete@@."),
  article("dwarf", "Dwarf", "species", { speed: 7.5, constitution: "+2" }),
  article("fighter", "Fighter", "class", {
    "hit-die": 10,
    features: [{ with: "fighting-style", count: 1 }, { with: "second-wind", count: 1 }, { with: "extra-attack", count: 5 }],
  }),
  article("rogue", "Rogue", "class", { "hit-die": 8, features: [{ with: "sneak-attack", count: 1 }] }),
  article("champion", "Champion", "subclass", { class: "fighter", features: [{ with: "improved-critical", count: 3 }] }),
  article("fighting-style", "Fighting style", "feature", {}),
  article("second-wind", "Second wind", "feature", {}),
  article("extra-attack", "Extra attack", "feature", {}),
  article("sneak-attack", "Sneak attack", "feature", {}),
  article("improved-critical", "Improved critical", "feature", {}),
  article("athletics", "Athletics", "skill", { base: "[[Strength mod]]" }),
  article("stealth", "Stealth", "skill", { base: "[[Dexterity mod]]" }),
  article("perception", "Perception", "skill", { base: "[[Wisdom mod]]" }),
  article("st-strength", "Strength", "saving-throw", { base: "[[Strength mod]]" }),
  article("st-constitution", "Constitution", "saving-throw", { base: "[[Constitution mod]]" }),
  article("st-wisdom", "Wisdom", "saving-throw", { base: "[[Wisdom mod]]" }),
  article(
    "brakka",
    "Brakka",
    "character",
    {
      ...scores([15, 12, 12, 10, 13, 8]),
      species: "dwarf",
      classes: [{ with: "fighter", count: 3 }, { with: "rogue", count: 1 }],
      subclasses: ["champion"],
      "saving-throws": [{ with: "st-strength", count: 1 }, { with: "st-constitution", count: 1 }],
      skills: [{ with: "athletics", count: 1 }, { with: "perception", count: 2 }, { with: "stealth", count: 0.5 }],
      feats: ["athlete", "tough"],
      equipment: ["ogre-gauntlets"],
    },
    BRAKKA_TEXT,
  ),
].map((a) => (a.id === "brakka" ? { ...a, layers: [
          ...a.layers,
          // Narrower than the document: only players see it.
          { ...layer("brakka", "family", "Son of Thráin, grandson of Thrór."), policy: "players" },
          // Headings fold what follows them, on into the next part.
          layer("brakka", "story", "# Youth\nBorn under the mountain.\n\n## Exile\nThe Blue Mountains.\n\n# Deeds\nRetook Erebor."),
          layer("brakka", "story-2", "Hid the map."),
        ] } : a));

const GOBLIN_DOC: DocumentText = {
  name: "Goblin",
  access: { view: "all", edit: "gm", level: "open" },
  properties: [
    { name: "Armor class", value: "15" },
    { name: "Hit points", value: "7" },
    ...ABILITIES.map((a, i) => ({ name: a, value: String([8, 14, 10, 10, 8, 8][i]), group: ["Abilities"] })),
  ],
  blocks: [{ key: "stats", title: "", style: "statblock", parts: [{ key: "stats", text: GOBLIN_TEXT, view: "all", edit: "gm", level: "open" }] }],
};

const BRAKKA_DOC: DocumentText = {
  name: "Brakka",
  access: { view: "all", edit: "gm", level: "open" },
  properties: [
    { name: "Species", value: "@@Species:Dwarf@@" },
    { name: "Classes", value: "@@Class:Fighter@@ 3, @@Class:Rogue@@ 1" },
    ...ABILITIES.map((a, i) => ({ name: a, value: String([15, 12, 12, 10, 13, 8][i]), group: ["Character stats", "Abilities"] })),
    { name: "Feats", value: "@@Feat:Athlete@@, @@Feat:Tough@@", group: ["Character stats"] },
    { name: "Skills", value: "@@Skill:Athletics@@ 1, @@Skill:Perception@@ 2, @@Skill:Stealth@@ 0.5" },
    { name: "Saving throws", value: "@@Saving throw:Strength@@ 1, @@Saving throw:Constitution@@ 1" },
  ],
  blocks: [
    { key: "stats", title: "Stats", style: "statblock", place: "top", parts: [{ key: "stats", text: BRAKKA_TEXT, view: "all", edit: "gm", level: "open" }] },
    { key: "story", title: "Story", style: "text", parts: [{ key: "story", text: "Born under the mountain.", view: "all", edit: "gm", level: "open" }] },
    { key: "notes", title: "Notes", style: "text", parts: [{ key: "notes", text: "Owes the guild.", view: "all", edit: "gm", level: "open" }] },
    // A section of Brakka's own, with headings in it.
    { key: "lore", title: "Lore", style: "text", parts: [{ key: "lore", text: "Old tales.\n\n## Mines\nDeep ones.\n\n## Wars\nMany.", view: "all", edit: "gm", level: "open" }] },
  ],
};

// A type saved from the editor — a block moved — is kept for the scenario to
// look at; the types stay as they are.
const saved: Template[] = [];
// How many types there are to pick from: all but groups.
(window as unknown as { stageTypes: number }).stageTypes = TEMPLATES.filter((t) => !t.group).length;
(window as unknown as { savedTypes: Template[] }).savedTypes = saved;
setFixture("template_save", (args: { template: Template }) => {
  saved.push(args.template);
  return TEMPLATES.map((t) => (t.id === args.template.id ? args.template : t));
});

export function StatsStage() {
  const [doc, setDoc] = useState(GOBLIN_DOC);
  const [brakka, setBrakka] = useState(BRAKKA_DOC);
  const [propsOpen, setPropsOpen] = useState(false);
  const page = (id: string) => ARTICLES.find((a) => a.id === id)!;
  const data = { world: WORLD, templates: TEMPLATES, articles: ARTICLES, onPlace: () => {}, onChanged: () => {}, onApart: () => {} };
  const show = (id: string) => (
    <ArticlePage world={WORLD} article={page(id)} templates={TEMPLATES} all={ARTICLES} onPlace={() => {}} onChanged={() => {}} onError={() => {}} />
  );
  return (
    <WorldDataContext.Provider value={data}>
      <DiceTray world={WORLD}>
        <div id="dice-button">
          <DiceButton />
        </div>
        <LinkPreview />
        <div className="body-wrap stats-stage">
          <div id="goblin">{show("goblin")}</div>
          <div id="brakka">{show("brakka")}</div>
          <h4>Brakka's sheet, edited visually</h4>
          <pre id="brakka-skills">{brakka.properties.find((p) => p.name === "Skills")?.value}</pre>
          <div id="visual-brakka" className="document">
            <VisualEditor
              world={WORLD}
              article={page("brakka")}
              templates={TEMPLATES}
              all={ARTICLES}
              doc={brakka}
              propsOpen={false}
              onPropsOpen={() => {}}
              links={{ all: ARTICLES, templates: TEMPLATES, selfId: "brakka" }}
              carets={() => []}
              marks={{ kind: "added", of: () => [] }}
              onDoc={setBrakka}
              onCaret={() => {}}
            />
          </div>
          <h4>The goblin, edited visually</h4>
          <div id="visual" className="document">
            {/* In the app this stands in the document's head. */}
            <button className="ghost compact props-toggle" onClick={() => setPropsOpen(!propsOpen)}>
              Properties
            </button>
            <VisualEditor
              world={WORLD}
              article={page("goblin")}
              templates={TEMPLATES}
              all={ARTICLES}
              doc={doc}
              propsOpen={propsOpen}
              onPropsOpen={setPropsOpen}
              links={{ all: ARTICLES, templates: TEMPLATES, selfId: "goblin" }}
              carets={() => []}
              marks={{ kind: "added", of: () => [] }}
              onDoc={setDoc}
              onCaret={() => {}}
            />
          </div>
        </div>
      </DiceTray>
    </WorldDataContext.Provider>
  );
}
