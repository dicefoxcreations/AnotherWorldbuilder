// What was typed while a save was on its way goes on top of what came back;
// a caret moves with the text around it.
import assert from "node:assert/strict";
import test from "node:test";
import { mapPosition, rebase, splice } from "../../src/shared/editor/Merge.ts";

test("the difference of two texts is one splice", () => {
  assert.deepEqual(splice("The inn is old.", "The inn is very old."), { start: 11, end: 11, insert: "very " });
  assert.deepEqual(splice("abc", "abc"), { start: 3, end: 3, insert: "" });
});

test("typing after something arrived elsewhere keeps both", () => {
  const sent = "One. Two.";
  const typed = "One. Two. Three.";
  const back = "Zero. One. Two.";
  assert.equal(rebase(sent, typed, back), "Zero. One. Two. Three.");
  assert.equal(rebase(sent, "Uno. Two.", "One. Two. Drei."), "Uno. Two. Drei.");
});

test("nothing typed or nothing arrived: the other side wins as is", () => {
  assert.equal(rebase("a", "a", "b"), "b");
  assert.equal(rebase("a", "b", "a"), "b");
});

test("in the same place nothing of theirs is lost", () => {
  const out = rebase("One.", "One!", "One?");
  assert.ok(out.includes("?") && out.includes("!"), out);
});

test("a caret moves with what was written before it", () => {
  const before = "One. Two.";
  const after = "Zero. One. Two.";
  assert.equal(mapPosition(before, after, before.indexOf("Two")), after.indexOf("Two"));
  assert.equal(mapPosition(before, "One. Two. Three.", 2), 2, "written after it: it stays");
});

test("what others wrote and what one typed meanwhile both stay, part by part", async () => {
  const { mergeTyped } = await import("../../src/shared/editor/Content.ts");
  const doc = (a, b) => ({ name: "Session", access: { view: "all", edit: "players", level: "open" }, properties: [], blocks: [{ key: "j", title: "Journal", parts: [{ key: "j", text: a, view: "all", edit: "players", level: "open" }, { key: "j-2", text: b, view: "all", edit: "players", level: "open" }] }] });
  const base = doc("First.", "Second.");
  const ours = doc("First.", "Second. Bob types on");
  const theirs = doc("First. The GM adds.", "Second.");
  const out = mergeTyped(base, ours, theirs);
  assert.deepEqual(out.blocks[0].parts.map((p) => p.text), ["First. The GM adds.", "Second. Bob types on"]);
  const added = { ...ours, blocks: [{ ...ours.blocks[0], parts: [...ours.blocks[0].parts, { key: "j-3", text: "New", view: "all", edit: "players", level: "open" }] }] };
  assert.equal(mergeTyped(base, added, theirs), null, "a part added here: the code is merged");
});
