// Property groups: properties several types take in — `shared/types`.
import assert from "node:assert/strict";
import test from "node:test";
import { effective, unfold, usersOf } from "../../src/shared/types.ts";
import { sumOf } from "../../src/shared/Sheet.ts";

const type = (id, properties, more = {}) => ({ id, name: id, supertype: "", group: false, rules: [], properties, infobox: { align: "none", width: "auto", show: [] }, blocks: [], ...more });
const number = (key, label) => ({ key, label, kind: "number" });
const uses = (label, group) => ({ key: label.toLowerCase(), label, kind: "group", group });

const abilities = type("abilities", [number("str", "Stärke"), number("dex", "Geschicklichkeit")], { group: true });
const character = type("character", [{ key: "title", label: "Titel", kind: "text", multiline: false }, uses("Werte", "abilities"), { key: "feats", label: "Talente", kind: "reference", articleType: "feat", multiple: true, parent: false, adds: true }]);
const feat = type("feat", [uses("Attribute", "abilities")]);
const all = [abilities, character, feat];

test("a group stands unfolded, under the name the type gives it", () => {
  const props = effective(all, character).properties;
  assert.deepEqual(props.map((p) => [p.label, (p.inGroup ?? []).join(" › ")]), [["Titel", ""], ["Stärke", "Werte"], ["Geschicklichkeit", "Werte"], ["Talente", ""]]);
  assert.equal(props[1].key, "str", "the group's own keys — the same in every type");
});

test("what is in a group adds up by name, as before", () => {
  const athlet = { id: "athlet", name: "Athlet", articleType: "feat", layers: [{ section: "properties", fields: { str: "+1" } }] };
  const brakka = { id: "brakka", name: "Brakka", articleType: "character", layers: [{ section: "properties", fields: { str: 15, feats: ["athlet"] } }] };
  assert.equal(sumOf(all, [athlet, brakka], brakka, "Stärke").total, 16);
});

test("a group brings its rules, through groups and supertypes", () => {
  const mod = { name: "mod", params: ["x"], formula: "floor((x - 10) / 2)" };
  const check = { name: "check", params: ["x"], formula: "1d20 + x" };
  const scores = type("scores", [number("str", "Stärke")], { group: true, rules: [mod] });
  const values = type("values", [uses("Attribute", "scores")], { group: true, rules: [check] });
  const creature = type("creature", [uses("Werte", "values")]);
  const monster = type("monster", [], { supertype: "creature", rules: [{ name: "crit", params: [], formula: "20" }] });
  const names = effective([scores, values, creature, monster], monster).rules.map((r) => r.name);
  assert.deepEqual(names, ["crit", "check", "mod"]);
});

test("a group in a group keeps its path", () => {
  const values = type("values", [uses("Attribute", "abilities"), number("speed", "Bewegungsrate")], { group: true });
  const hero = type("hero", [uses("Charakterwerte", "values")]);
  assert.deepEqual(effective([abilities, values, hero], hero).properties.map((p) => p.inGroup.join(" › ")), [
    "Charakterwerte › Attribute",
    "Charakterwerte › Attribute",
    "Charakterwerte",
  ]);
});

test("a group that contains itself ends", () => {
  const a = type("a", [uses("B", "b")], { group: true });
  const b = type("b", [uses("A", "a"), number("x", "X")], { group: true });
  assert.deepEqual(unfold([a, b], a.properties).map((p) => p.label), ["X"]);
});

test("who takes a group in", () => {
  assert.deepEqual(usersOf(all, "abilities").map((t) => t.id), ["character", "feat"]);
});
