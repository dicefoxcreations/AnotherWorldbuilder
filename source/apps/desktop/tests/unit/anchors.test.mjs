// Sections: anchors set from a selection, and found again.
import assert from "node:assert/strict";
import test from "node:test";
import { anchorName, anchorRange, anchorsIn, markSection, relinked, renameInText } from "../../src/shared/Anchors.ts";

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

test("a section's place in its text: its block, from where it begins", () => {
  const t = "Head.\n\nThe inn is old.\nIts roof leaks. ^roof\n\nAfter.";
  const r = anchorRange(t, "roof");
  assert.equal(t.slice(r.start, r.end), "The inn is old.\nIts roof leaks. ^roof");
  const s = "- a ^x\n- b\n- c ^x";
  assert.equal(s.slice(anchorRange(s, "x").start, anchorRange(s, "x").end), s);
  assert.equal(anchorRange(t, "none"), null);
});

test("renamed: the anchor and this document's links to it, not others", () => {
  const t = "Its roof leaks. ^roof\n| a | 5 ^roof |\nSee @@#^roof|the roof@@ and @@#^roofs@@ and @@Inn#^roof@@.";
  assert.equal(
    renameInText(t, "roof", "dach"),
    "Its roof leaks. ^dach\n| a | 5 ^dach |\nSee @@#^dach|the roof@@ and @@#^roofs@@ and @@Inn#^roof@@.",
  );
  assert.equal(
    relinked("@@Place:Inn#^roof|x@@ @@Place:Mill#^roof@@", "roof", "dach", (target) => target === "Place:Inn"),
    "@@Place:Inn#^dach|x@@ @@Place:Mill#^roof@@",
  );
});
