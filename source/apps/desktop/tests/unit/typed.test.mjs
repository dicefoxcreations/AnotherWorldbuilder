// What one typed stays when the saved reading comes back — `shared/editor/Content`.
import assert from "node:assert/strict";
import test from "node:test";
import { keepTyped } from "../../src/shared/editor/Content.ts";

const doc = (...texts) => ({
  name: "Live",
  access: { view: "all", edit: "all" },
  properties: [],
  blocks: [{ title: "", style: "", parts: texts.map((text) => ({ text, view: "all", edit: "all", level: "open" })) }],
});

test("a space and a new line at the end stay while one types on", () => {
  const saved = doc("Durnan", "Yen");
  const kept = keepTyped(saved, doc("Durnan ", "Yen\n"));
  assert.deepEqual(kept.blocks[0].parts.map((p) => p.text), ["Durnan ", "Yen\n"]);
});

test("what someone else changed comes in", () => {
  const kept = keepTyped(doc("Durnan nickt"), doc("Durnan "));
  assert.equal(kept.blocks[0].parts[0].text, "Durnan nickt");
  assert.equal(keepTyped(doc("a", "b"), doc("a ")).blocks[0].parts.length, 2);
});
