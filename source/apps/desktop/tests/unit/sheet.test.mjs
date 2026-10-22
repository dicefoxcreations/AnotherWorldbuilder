// Numbers that feats add to and items set: `shared/Sheet`.
import assert from "node:assert/strict";
import test from "node:test";
import { explain, lookupFor, sumOf, summedValues } from "../../src/shared/Sheet.ts";
import { parse, reckon } from "../../src/shared/Dice.ts";

const type = (id, properties, rules = []) => ({ id, name: id, supertype: "", properties, rules, infobox: { align: "none", width: "auto", show: [] }, blocks: [] });
/** What a modifier is — the world's to say, here as in the rule books. */
const MOD = { name: "mod", params: ["x"], formula: "floor((x - 10) / 2)" };
const str = { key: "str", label: "Stärke", kind: "number" };
const templates = [
  type("char", [str, { key: "gear", label: "Ausrüstung", kind: "reference", articleType: "item", multiple: true, parent: false, adds: true }, { key: "father", label: "Vater", kind: "reference", articleType: "char", multiple: false, parent: false }], [MOD]),
  type("item", [str]),
];
const doc = (id, articleType, fields) => ({ id, name: id, articleType, layers: [{ section: "properties", fields }] });

const athlet = doc("Athlet", "item", { str: "+1" });
const gauntlets = doc("Krafthandschuhe", "item", { str: ">=19" });
const belt = doc("Gürtel", "item", { str: "=21" });
const weakness = doc("Schwäche", "item", { str: "<=8" });
const curse = doc("Fluch", "item", { str: "-2" });
const statue = doc("Statue", "item", { str: 30 });
const father = doc("Vater", "char", { str: 20 });
const all = [athlet, gauntlets, belt, weakness, father, curse, statue];
const strength = (base, gear) => sumOf(templates, all, doc("Brakka", "char", { str: base, gear, father: "Vater" }), "Stärke");

test("a feat adds", () => {
  const s = strength(15, ["Athlet"]);
  assert.equal(s.total, 16);
  assert.equal(explain(s), "15 + 1 Athlet");
});

test("gauntlets set a floor that a feat does not lift", () => {
  assert.equal(strength(15, ["Athlet", "Krafthandschuhe"]).total, 19);
  assert.equal(explain(strength(15, ["Athlet", "Krafthandschuhe"])), "15 + 1 Athlet, at least 19 Krafthandschuhe");
});

test("unless what adds up is more", () => {
  assert.equal(strength(18, ["Athlet", "Krafthandschuhe"]).total, 19);
  assert.equal(strength(20, ["Athlet", "Krafthandschuhe"]).total, 21);
});

test("a reference without adds adds nothing — not the father's strength", () => {
  assert.equal(strength(15, []).total, 15);
});

test("what sets it sets it; at most holds it down", () => {
  assert.equal(strength(15, ["Athlet", "Gürtel"]).total, 21, "a feat does not lift what is set");
  assert.equal(explain(strength(15, ["Athlet", "Gürtel"])), "15 + 1 Athlet, = 21 Gürtel");
  assert.equal(strength(15, ["Athlet", "Schwäche"]).total, 8);
  assert.equal(strength(6, ["Schwäche"]).total, 6);
  assert.equal(strength(15, ["Gürtel", "Schwäche"]).total, 8, "set, then held to at most");
});

test("formulas see the result, and the type's rules", () => {
  const brakka = doc("Brakka", "char", { str: 15, gear: ["Athlet", "Krafthandschuhe"] });
  const look = lookupFor(templates, [...all, brakka], brakka);
  assert.equal(look("Stärke"), 19);
  assert.deepEqual(look.rules, [MOD]);
  assert.equal(reckon(parse("Stärke mod", look.rules), look).total, 4);
  assert.equal(reckon(parse("mod(Stärke) + 1", look.rules), look).total, 5);
  assert.throws(() => reckon(parse("Stärke mod"), look), /no number/, "without the rule, no modifier");
});

test("a curse lowers, and a bare number on a pick is its own — it changes nothing", () => {
  assert.equal(strength(15, ["Fluch"]).total, 13);
  assert.equal(explain(strength(15, ["Fluch"])), "15 − 2 Fluch");
  assert.equal(strength(15, ["Statue"]).total, 15);
});

test("read on its own, an operator is its number", () => {
  assert.equal(sumOf(templates, all, curse, "Stärke").total, -2);
  assert.equal(sumOf(templates, all, gauntlets, "Stärke").total, 19);
});

test("raising needs a value to raise; a floor stands on its own", () => {
  assert.equal(sumOf(templates, all, doc("NSC", "char", { gear: ["Athlet"] }), "Stärke").total, undefined);
  assert.equal(sumOf(templates, all, doc("NSC", "char", { gear: ["Athlet", "Krafthandschuhe"] }), "Stärke").total, 19);
});

// A number reckoned from others: hit points from class and level.
const hp = { key: "hp", label: "Trefferpunkte", kind: "number", default: "[[Klasse.Trefferwürfel + (Stufe - 1) * (Klasse.Trefferwürfel / 2 + 1) + Stufe * Konstitution mod]]" };
const rules = [
  type("pc", [
    { key: "con", label: "Konstitution", kind: "number" },
    { key: "lvl", label: "Stufe", kind: "number" },
    hp,
    { key: "class", label: "Klasse", kind: "reference", articleType: "class", multiple: false, parent: false },
    { key: "feats", label: "Talente", kind: "reference", articleType: "feat", multiple: true, parent: false, adds: true },
  ]),
  type("class", [{ key: "hd", label: "Trefferwürfel", kind: "number" }]),
  type("feat", [hp]),
].map((t) => (t.id === "pc" ? { ...t, rules: [MOD] } : t));
const ranger = doc("Waldläufer", "class", { hd: 10 });
const tough = doc("Zäh", "feat", { hp: "+6" });
const ilva = (fields) => doc("Ilva", "pc", { con: 14, lvl: 3, class: "Waldläufer", ...fields });

test("a number with a formula reckons itself, through what it picks", () => {
  const s = sumOf(rules, [ranger, tough], ilva({}), "Trefferpunkte");
  assert.equal(s.total, 28);
  assert.equal(explain(s), "28 by formula");
});

test("feats add to a reckoned number, as to any", () => {
  const s = sumOf(rules, [ranger, tough], ilva({ feats: ["Zäh"] }), "Trefferpunkte");
  assert.equal(s.total, 34);
  assert.equal(explain(s), "28 by formula + 6 Zäh");
});

test("a value of its own stands instead — rolled hit points", () => {
  assert.equal(sumOf(rules, [ranger], ilva({ hp: 31 }), "Trefferpunkte").total, 31);
});

test("without what the formula needs, there is no number", () => {
  assert.equal(sumOf(rules, [], doc("Niemand", "pc", { con: 10 }), "Trefferpunkte").total, undefined);
  assert.equal(lookupFor(rules, [ranger], ilva({}))("Klasse.Trefferwürfel"), 10, "a dot reads what is picked");
});

test("a formula that needs itself gives nothing, and ends", () => {
  const loop = [type("x", [{ key: "a", label: "A", kind: "number", default: "[[B + 1]]" }, { key: "b", label: "B", kind: "number", default: "[[A + 1]]" }])];
  assert.equal(sumOf(loop, [], doc("X", "x", {}), "A").total, undefined);
});

test("what a value does to others is read as written, even beside a formula", () => {
  const item = [type("gear", [str, hp])];
  assert.equal(summedValues(item, [], doc("Handschuhe", "gear", { str: "=19" })).str, "=19");
});

// Multiclassing: levels in several classes, each with its hit die.
const AVERAGE = "first(Klassen.Trefferwürfel) + or(Gewürfelt, Klassen.(Stufe * (Trefferwürfel / 2 + 1)) - first(Klassen.(Trefferwürfel / 2 + 1))) + Klassen.Stufe * Konstitution mod";
const multi = [
  type("pc", [
    { key: "con", label: "Konstitution", kind: "number" },
    { key: "classes", label: "Klassen", kind: "reference", articleType: "class", multiple: true, parent: false, count: "Stufe" },
    { key: "lvl", label: "Stufe", kind: "number", default: "[[Klassen.Stufe]]" },
    { key: "rolled", label: "Gewürfelt", kind: "number" },
    { key: "hp", label: "Trefferpunkte", kind: "number", default: `[[${AVERAGE}]]` },
  ]),
  type("class", [{ key: "hd", label: "Trefferwürfel", kind: "number" }]),
].map((t) => (t.id === "pc" ? { ...t, rules: [MOD] } : t));
const fighter = doc("Kämpfer", "class", { hd: 10 });
const rogue = doc("Schurke", "class", { hd: 8 });
const hero = (fields) => doc("Held", "pc", { con: 14, classes: [{ with: "Kämpfer", count: 3 }, { with: "Schurke", count: 2 }], ...fields });

test("levels in several classes: each its own number", () => {
  const look = lookupFor(multi, [fighter, rogue], hero({}));
  assert.equal(look("Stufe"), 5, "the level is what the classes add up to");
  assert.equal(look("Klassen.Stufe"), 5);
  assert.equal(look("Klassen.Trefferwürfel"), 18, "a path adds up");
});

test("hit points by the average: the first class's die in full, then each class's average", () => {
  // 10 + 2 at level 1, 2 × (6 + 2) as a fighter, 2 × (5 + 2) as a rogue.
  assert.equal(sumOf(multi, [fighter, rogue], hero({}), "Trefferpunkte").total, 42);
});

test("rolled hit dice stand instead of the average, where entered", () => {
  assert.equal(sumOf(multi, [fighter, rogue], hero({ rolled: 20 }), "Trefferpunkte").total, 40);
});

test("the hit dice, written per class", async () => {
  const { rollFormula } = await import("../../src/shared/Dice.ts");
  const r = rollFormula("Klassen.((Stufe)W(Trefferwürfel)) + Stufe * Konstitution mod", lookupFor(multi, [fighter, rogue], hero({})), (s) => s);
  assert.equal(r.formula, "3W10 + 2W8 + 10");
  assert.equal(r.total, 56);
  assert.equal(r.dice.length, 5);
});

// A player's sheet: skills that reckon for whoever has them, a proficiency
// bonus by total level, features by class level.
const { picksAlong } = await import("../../src/shared/Sheet.ts");
const ref = (key, label, articleType, more = {}) => ({ key, label, kind: "reference", articleType, multiple: true, parent: false, ...more });
const player = [
  type("pc", [
    { key: "dex", label: "Geschicklichkeit", kind: "number" },
    { key: "wis", label: "Weisheit", kind: "number" },
    ref("classes", "Klassen", "class", { count: "Stufe" }),
    ref("subs", "Unterklassen", "sub"),
    { key: "lvl", label: "Stufe", kind: "number", default: "[[Klassen.Stufe]]" },
    { key: "pb", label: "Übungsbonus", kind: "number", default: "[[steps(Stufe, 1, 2, 5, 3, 9, 4, 13, 5, 17, 6)]]" },
    ref("skills", "Fertigkeiten", "skill", { count: "Übung" }),
  ], [MOD]),
  type("skill", [{ key: "base", label: "Grundwert", kind: "number" }, { key: "bonus", label: "Bonus", kind: "number", default: "[[Grundwert + Übung * Übungsbonus]]" }]),
  type("class", [ref("feats", "Merkmale", "feature", { count: "Ab Stufe" })]),
  type("sub", [{ key: "of", label: "Klasse", kind: "reference", articleType: "class", multiple: false, parent: true }, ref("feats", "Merkmale", "feature", { count: "Ab Stufe" })]),
  type("feature", []),
];
const stealth = doc("Heimlichkeit", "skill", { base: "[[Geschicklichkeit mod]]" });
const perception = doc("Wahrnehmung", "skill", { base: "[[Weisheit mod]]" });
const favored = doc("Bevorzugter Feind", "feature", {});
const hunter = doc("Beute des Jägers", "feature", {});
const extra = doc("Zusätzlicher Angriff", "feature", {});
const sneak = doc("Hinterhältiger Angriff", "feature", {});
const ranger2 = doc("Waldläufer", "class", { feats: [{ with: "Bevorzugter Feind", count: 1 }, { with: "Zusätzlicher Angriff", count: 5 }] });
const rogue2 = doc("Schurke", "class", { feats: [{ with: "Hinterhältiger Angriff", count: 1 }] });
const huntress = doc("Jägerin", "sub", { of: "Waldläufer", feats: [{ with: "Beute des Jägers", count: 3 }] });
const docs2 = [stealth, perception, favored, hunter, extra, sneak, ranger2, rogue2, huntress];
const ilva2 = doc("Ilva", "pc", {
  dex: 16,
  wis: 14,
  classes: [{ with: "Waldläufer", count: 3 }, { with: "Schurke", count: 2 }],
  subs: ["Jägerin"],
  skills: [{ with: "Heimlichkeit", count: 2 }, { with: "Wahrnehmung", count: 0 }],
});

test("a proficiency bonus by total level, as a table of steps", () => {
  const look = lookupFor(player, docs2, ilva2);
  assert.equal(look("Stufe"), 5);
  assert.equal(look("Übungsbonus"), 3);
});

test("a skill's formula reckons for whoever has it, with the pick's number", () => {
  const skills = picksAlong(player, docs2, ilva2, "Fertigkeiten");
  assert.deepEqual(skills.map((s) => [s.pick.name, s.look("Übung"), s.look("Bonus")]), [["Heimlichkeit", 2, 3 + 2 * 3], ["Wahrnehmung", 0, 2]]);
  // Seen on its own, the skill has no dexterity: no number.
  assert.equal(lookupFor(player, docs2, stealth)("Bonus"), undefined);
  // A choice among the picks by name.
  const look = lookupFor(player, docs2, ilva2);
  assert.equal(reckon(parse("10 + Fertigkeiten[Wahrnehmung].Bonus", look.rules), look).total, 12, "passive perception");
});

test("features by class level: through the picks, as far as each level reaches", () => {
  const all = picksAlong(player, docs2, ilva2, "Klassen.Merkmale");
  const kept = all.filter((s) => reckon(parse("Ab Stufe <= Stufe", s.look.rules), s.look).total !== 0);
  assert.deepEqual(kept.map((s) => s.pick.name), ["Bevorzugter Feind", "Hinterhältiger Angriff"], "level 3 ranger: no extra attack yet");
  // A subclass's features count by the level in its class — not the total.
  const sub = picksAlong(player, docs2, ilva2, "Unterklassen.Merkmale");
  const at = (s) => reckon(parse("Ab Stufe <= Klassen[Klasse].Stufe", s.look.rules), s.look).total;
  assert.deepEqual(sub.map((s) => [s.pick.name, at(s)]), [["Beute des Jägers", 1]]);
  const lower = doc("Ilva", "pc", { ...ilva2.layers[0].fields, classes: [{ with: "Waldläufer", count: 2 }, { with: "Schurke", count: 3 }] });
  const sub2 = picksAlong(player, docs2, lower, "Unterklassen.Merkmale");
  assert.equal(at(sub2[0]), 0, "ranger 2 of 5: not yet");
});

test("a value may be a formula of its own, reckoned for the wearer: armour", () => {
  const types = [
    type("pc", [{ key: "dex", label: "Geschicklichkeit", kind: "number" }, ref("gear", "Ausrüstung", "item"), { key: "ac", label: "Rüstungsklasse", kind: "number", default: "[[or(max(Ausrüstung.Rüstungsklasse), 10 + Geschicklichkeit mod)]]" }], [MOD]),
    type("item", [{ key: "ac", label: "Rüstungsklasse", kind: "number" }]),
  ];
  const scale = doc("Schuppenpanzer", "item", { ac: "[[14 + min(Geschicklichkeit mod, 2)]]" });
  const quick = doc("Flink", "pc", { dex: 18, gear: ["Schuppenpanzer"] });
  assert.equal(lookupFor(types, [scale], quick)("Rüstungsklasse"), 16);
  assert.equal(lookupFor(types, [scale], doc("Nackt", "pc", { dex: 18 }))("Rüstungsklasse"), 14);
});

test("every document of a type: those without a number count with 0, and new ones are there", () => {
  const every = player.map((t) =>
    t.id === "pc" ? { ...t, properties: t.properties.map((f) => (f.key === "skills" ? { ...f, every: true } : f)) } : t,
  );
  const only = doc("Ilva", "pc", { ...ilva2.layers[0].fields, skills: [{ with: "Heimlichkeit", count: 2 }] });
  const athletics = doc("Athletik", "skill", { base: "[[Geschicklichkeit mod]]" });
  const skills = picksAlong(every, [...docs2, athletics], only, "Fertigkeiten");
  assert.deepEqual(skills.map((s) => [s.pick.name, s.look("Übung"), s.look("Bonus")]), [["Athletik", 0, 3], ["Heimlichkeit", 2, 9], ["Wahrnehmung", 0, 2]]);
});

test("a rule may read a number — an item lowers the critical range", async () => {
  const { rollFormula } = await import("../../src/shared/Dice.ts");
  const rules = [
    { name: "check", params: ["x"], formula: "1d20 + x" },
    { name: "crit", params: [], formula: "Crit at" },
  ];
  const types = [
    type("pc", [{ key: "crit", label: "Crit at", kind: "number", default: "[[20]]" }, { key: "gear", label: "Equipment", kind: "reference", articleType: "item", multiple: true, parent: false, adds: true }], rules),
    type("item", [{ key: "crit", label: "Crit at", kind: "number" }]),
  ];
  const keen = doc("Keen blade", "item", { crit: "<=19" });
  const plain = doc("Ana", "pc", {});
  const armed = doc("Bo", "pc", { gear: ["Keen blade"] });
  const nineteen = () => 19;
  assert.equal(rollFormula("+0", lookupFor(types, [keen], plain), nineteen).mark, undefined);
  assert.equal(rollFormula("+0", lookupFor(types, [keen], armed), nineteen).mark, "crit", "the blade makes 19 a critical");
});

test("how well, as marks: none, half, once, twice", async () => {
  const { tally } = await import("../../src/shared/Tally.ts");
  assert.deepEqual([0, 0.5, 1, 2, 1.5].map((n) => tally("●", n)), [
    { text: "●", none: true },
    { text: "◐", none: false },
    { text: "●", none: false },
    { text: "●●", none: false },
    { text: "●◐", none: false },
  ]);
  assert.deepEqual(tally("★", 2.5), { text: "★★", none: false }, "only ● has a half");
});
