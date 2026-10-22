// Marks in the text: where it changed, and never where it would break it.
import assert from "node:assert/strict";
import test from "node:test";
import { changes, marked, MARK } from "../../src/shared/Marks.ts";

const [a1, a2] = MARK.added;
const [r1, r2] = MARK.removed;

test("a changed word is marked on both sides", () => {
  const { removed, added } = changes("The inn is old.", "The inn is new.");
  assert.equal(marked("The inn is old.", removed, "removed"), `The inn is ${r1}old${r2}.`);
  assert.equal(marked("The inn is new.", added, "added"), `The inn is ${a1}new${a2}.`);
});

test("a heading keeps its sign, a link stays whole", () => {
  const then = "## Old title\nSee @@Place:Paris@@.";
  const now = "## New title\nSee @@Place:Lyon@@.";
  const { added } = changes(then, now);
  const out = marked(now, added, "added");
  assert.ok(out.startsWith(`## ${a1}`), out);
  assert.ok(out.includes(`${a1}@@Place:Lyon@@${a2}`), out);
});

test("a mark does not run across lines", () => {
  const out = marked("One\nTwo", [{ start: 0, end: 7 }], "added");
  assert.equal(out, `${a1}One${a2}\n${a1}Two${a2}`);
});
