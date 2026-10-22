// Two versions side by side: what is gone on the left, what is new on the right.
import assert from "node:assert/strict";
import test from "node:test";
import { sideBySide } from "../../src/shared/Diff.ts";

const marked = (segments, kind) => segments.filter((s) => s.kind === kind).map((s) => s.text.trim()).filter(Boolean);

test("a changed word stands out, not its whole line", () => {
  const { left, right } = sideBySide("The inn is old.\nIt rains.\n", "The inn is new.\nIt rains.\n");
  assert.deepEqual(marked(left, "gone"), ["old"]);
  assert.deepEqual(marked(right, "new"), ["new"]);
  assert.equal(left.map((s) => s.text).join(""), "The inn is old.\nIt rains.\n");
  assert.equal(right.map((s) => s.text).join(""), "The inn is new.\nIt rains.\n");
});

test("added and removed lines", () => {
  const { left, right } = sideBySide("One.\nTwo.\n", "One.\nTwo.\nThree.\n");
  assert.deepEqual(marked(left, "gone"), []);
  assert.deepEqual(marked(right, "new"), ["Three."]);
  const back = sideBySide("One.\nTwo.\n", "Two.\n");
  assert.deepEqual(marked(back.left, "gone"), ["One."]);
});

test("the same text has nothing marked", () => {
  const { left, right } = sideBySide("Same.", "Same.");
  assert.deepEqual([...marked(left, "gone"), ...marked(right, "new")], []);
});

test("a word changed into a similar one: only the letters that changed", () => {
  const { left, right } = sideBySide("aaaa", "aaabbb");
  assert.deepEqual(left, [
    { text: "aaa", kind: "same" },
    { text: "a", kind: "gone" },
  ]);
  assert.deepEqual(right, [
    { text: "aaa", kind: "same" },
    { text: "bbb", kind: "new" },
  ]);
  const house = sideBySide("The house.", "The mouse.");
  assert.deepEqual(marked(house.left, "gone"), ["h"]);
  assert.deepEqual(marked(house.right, "new"), ["m"]);
});

test("words with little in common stay whole — no confetti", () => {
  const { left, right } = sideBySide("It is beautiful.", "It is terrible.");
  assert.deepEqual(marked(left, "gone"), ["beautiful"]);
  assert.deepEqual(marked(right, "new"), ["terrible"]);
});
