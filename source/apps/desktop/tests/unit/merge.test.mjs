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
