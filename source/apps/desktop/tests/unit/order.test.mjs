// The order of a type's blocks, inherited ones among them — `shared/types`.
import assert from "node:assert/strict";
import test from "node:test";
import { effective } from "../../src/shared/types.ts";

const block = (key) => ({ key, title: key, style: "text", parts: [] });
const type = (id, supertype, blocks, order) => ({ id, name: id, supertype, group: false, rules: [], properties: [], infobox: { align: "none", width: "auto", show: [] }, blocks: blocks.map(block), order });

const character = type("character", "", ["looks", "life"]);
const hero = type("hero", "character", ["goals"], ["goals", "looks"]);
const champion = type("champion", "hero", ["deeds"]);
const keys = (t, all) => effective(all, t).blocks.map((b) => b.key);

test("without an order: the supertype's first, then the type's own", () => {
  assert.deepEqual(keys(type("hero", "character", ["goals"]), [character]), ["looks", "life", "goals"]);
});

test("an order puts inherited blocks between its own; those not named follow", () => {
  const all = [character, hero, champion];
  assert.deepEqual(keys(hero, all), ["goals", "looks", "life"]);
  assert.deepEqual(keys(champion, all), ["goals", "looks", "life", "deeds"], "the nearest order holds below too");
  assert.deepEqual(keys(character, all), ["looks", "life"], "the supertype keeps its own");
});

// The order of documents a list sets: `sort="Role desc, name"`.
import { orderBy, sortWritten } from "../../src/shared/types.ts";

const role = type("role", "", []);
const person = { ...type("person", "", []), properties: [{ key: "r1", label: "Role", kind: "reference", multiple: false, to: "role" }] };
const doc = (id, articleType, name, fields = {}) => ({ id, articleType, name, slug: id, visibility: "all", edit: "gm", layerTotal: 1, layers: [{ id, title: name, section: "properties", mayEdit: true, text: "", fields, policy: "all", edit: "gm", level: "open", epoch: 0 }] });

test("a list's sort: a pick by its document's name, turned round by desc, then by name", () => {
  const roles = [doc("npc", "role", "NPC"), doc("player", "role", "Player"), doc("companion", "role", "Companion")];
  const people = [doc("a", "person", "Yen", { r1: "player" }), doc("b", "person", "Glom", { r1: "companion" }), doc("c", "person", "Durnan", { r1: "npc" }), doc("d", "person", "Argaen", { r1: "player" }), doc("e", "person", "Nobody")];
  const keys = sortWritten(person, ["Role desc", "name", "Unknown"]);
  assert.deepEqual(keys, [{ key: "r1", desc: true }, { key: "name", desc: false }], "what the type lacks is left out");
  const order = orderBy([role, person], [...roles, ...people], keys);
  assert.deepEqual([...people].sort(order).map((p) => p.name), ["Argaen", "Yen", "Durnan", "Glom", "Nobody"], "one without a role last");
});
