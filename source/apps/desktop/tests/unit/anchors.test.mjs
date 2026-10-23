// Sections: anchors set from a selection, and found again.
import assert from "node:assert/strict";
import test from "node:test";
import { anchorName, anchorsIn, markSection } from "../../src/shared/Anchors.ts";

const text = "Intro line.\n\nThe inn is old.\nIts roof leaks.\n\n| Rarity | Reward |\n| --- | --- |\n| Rare | 500 gold |\n\n- one\n- two\n\nAfter.";

test("a section's name becomes its anchor", () => {
  assert.equal(anchorName("  Magic Rewards! "), "magic-rewards");
  assert.equal(anchorName("Faire Preise – Obaya"), "faire-preise-obaya");
});

test("one paragraph marked: the anchor closes it", () => {
  const at = text.indexOf("roof");
  assert.equal(markSection(text, at, at + 4, "roof"), text.replace("Its roof leaks.", "Its roof leaks. ^roof"));
});

test("a table marked: the anchor goes into its last cell", () => {
  const at = text.indexOf("Rarity");
  assert.equal(markSection(text, at, at + 3, "rewards"), text.replace("| Rare | 500 gold |", "| Rare | 500 gold ^rewards |"));
});

test("several blocks marked: the same anchor closes the first and the last", () => {
  const from = text.indexOf("The inn");
  const to = text.indexOf("two") + 3;
  const out = markSection(text, from, to, "story");
  assert.ok(out.includes("Its roof leaks. ^story"));
  assert.ok(out.includes("- two ^story"));
  assert.deepEqual(anchorsIn(out), [{ anchor: "story", line: "Its roof leaks." }]);
});

test("anchors are found with their line", () => {
  assert.deepEqual(anchorsIn("A. ^2f9e17\n| x | 5 pp ^dbbde3 |\nx^2 is not one"), [
    { anchor: "2f9e17", line: "A." },
    { anchor: "dbbde3", line: "| x | 5 pp" },
  ]);
});
