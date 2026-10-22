// Relations: the text form, the graph from both sides, and the layouts.
import assert from "node:assert/strict";
import test from "node:test";
import { buildGraph, readRelations, writeRelations } from "../../src/shared/Relations.ts";
import { GRID, NODE_W, stepsLayout, withPlaces } from "../../src/shared/GraphLayout.ts";

const character = {
  id: "character",
  name: "Character",
  icon: "",
  supertype: "",
  description: "",
  properties: [{ key: "rel", label: "Relations", kind: "relation", articleType: "character", roleType: "relation" }],
  infobox: { align: "none", width: "auto", show: [] },
  auth: null,
  namePattern: "%%input%%",
  page: "",
  blocks: [],
};
const relation = { ...character, id: "relation", name: "Relation", properties: [] };
const templates = [character, relation];

const doc = (id, articleType, rel) => ({
  id,
  articleType,
  slug: id,
  name: id[0].toUpperCase() + id.slice(1),
  visibility: "all",
  layerTotal: 1,
  layers: rel ? [{ id: "l", title: "", section: "properties", mayEdit: true, text: "", fields: { rel }, policy: "all", level: "open", epoch: 1 }] : [],
});

test("the text form reads and writes both roles", () => {
  const read = readRelations("@@Character:Olga@@ Mother/Daughter, @@Ada@@ Best friend, @@Kurt@@");
  assert.deepEqual(
    read.map((w) => [w.link.name, w.role, w.ownRole]),
    [
      ["Olga", "Mother", "Daughter"],
      ["Ada", "Best friend", ""],
      ["Kurt", "", ""],
    ],
  );
  assert.equal(writeRelations(read), "@@Character:Olga@@ Mother/Daughter, @@Ada@@ Best friend, @@Kurt@@");
  // Half typed is no error.
  assert.equal(readRelations("@@Olga@@ Mother, Mot").length, 1);
});

test("both sides of a relation make one line", () => {
  const articles = [
    doc("marie", "character", [{ with: "olga", role: "mother", own_role: "daughter" }, { with: "secret", role: "aunt" }]),
    doc("olga", "character", [{ with: "marie", role: "daughter", own_role: "mother" }]),
    doc("ada", "character"),
    doc("mother", "relation"),
  ];
  const graph = buildGraph(templates, articles, { of: "character", via: [], around: "", depth: 1 });
  assert.deepEqual(graph.nodes.map((n) => n.article?.name ?? "???"), ["Ada", "Marie", "Olga", "???"]);
  const line = graph.edges.find((e) => e.a === "marie" && e.b === "olga");
  assert.deepEqual([line.atA, line.atB, line.holders], [["daughter"], ["mother"], ["marie", "olga"]]);
  // An unreadable aunt is someone of her own, not merged with anyone.
  assert.equal(graph.edges.length, 2);

  const around = buildGraph(templates, articles, { of: "", via: ["Relations"], around: "olga", depth: 1 });
  assert.deepEqual(around.nodes.map((n) => n.id).sort(), ["marie", "olga"]);
  const further = buildGraph(templates, articles, { of: "", via: [], around: "olga", depth: 2 });
  assert.equal(further.nodes.length, 3, "two steps reach the aunt");

  // Neither a middle nor a type: only who was put on it — and their unreadable kin.
  const placed = buildGraph(templates, articles, { of: "", via: [], around: "", depth: 1 }, new Map(), ["marie"]);
  assert.deepEqual(placed.nodes.map((n) => n.article?.name ?? "???"), ["Marie", "???"]);
  assert.equal(buildGraph(templates, articles, { of: "", via: [], around: "", depth: 1 }).nodes.length, 0);

});

test("without hands on it, people stand in steps from the middle", () => {
  const at = stepsLayout(["me", "a", "b", "c", "lone"], [["me", "a"], ["me", "b"], ["a", "c"]], "me");
  assert.ok(at.get("me").y < at.get("a").y);
  assert.equal(at.get("a").y, at.get("b").y);
  assert.ok(at.get("a").y < at.get("c").y);
  assert.equal(at.get("lone").y, 0, "someone unrelated starts a group of their own");
  // No two cards overlap within a row.
  const rows = new Map();
  for (const p of at.values()) rows.set(p.y, [...(rows.get(p.y) ?? []), p.x].sort((x, y) => x - y));
  for (const xs of rows.values()) for (let i = 1; i < xs.length; i++) assert.ok(xs[i] - xs[i - 1] >= NODE_W);
});

test("people set by hand stand there; the rest find a free spot beside a relative", () => {
  const auto = new Map([
    ["a", { x: 0, y: 0 }],
    ["b", { x: 300, y: 0 }],
    ["c", { x: 600, y: 0 }],
  ]);
  assert.equal(withPlaces(auto, new Map(), new Map()), auto, "nobody set: the tree as laid out");
  const at = withPlaces(auto, new Map([["a", { x: 2, y: 3 }], ["b", { x: 12, y: 3 }]]), new Map([["c", ["a"]]]));
  assert.deepEqual(at.get("a"), { x: 2 * GRID, y: 3 * GRID });
  assert.deepEqual(at.get("b"), { x: 12 * GRID, y: 3 * GRID });
  const c = at.get("c");
  for (const other of [at.get("a"), at.get("b")])
    assert.ok(Math.abs(c.x - other.x) >= NODE_W || Math.abs(c.y - other.y) >= 54, "no card on another");
  assert.equal(c.x % GRID, 0);
});
