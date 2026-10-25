// Folding a text's headings — `shared/Sections`.
import assert from "node:assert/strict";
import test from "node:test";
import { sectioned } from "../../src/shared/Sections.ts";

const h = (level, text) => ({ heading: { level, text } });
const p = {};
const hidden = (placed) => placed.map((part) => part.map((n) => (n.hidden ? "-" : n.head ?? "p")));

test("nothing folded: everything shows, every heading is named", () => {
  assert.deepEqual(hidden(sectioned([[h(1, "Areas"), p, h(1, "Timeline"), p]], new Set())), [["1:areas", "p", "1:timeline", "p"]]);
});

test("a folded heading hides up to the next one as high, across parts", () => {
  const parts = [[h(1, "Timeline"), p, h(2, "Early"), p], [p], [h(1, "Group"), p]];
  assert.deepEqual(hidden(sectioned(parts, new Set(["1:timeline"]))), [["1:timeline", "-", "-", "-"], ["-"], ["1:group", "p"]]);
});

test("a lower heading folds only its own section", () => {
  const parts = [[h(1, "Timeline"), h(2, "Early"), p, h(2, "Late"), p, h(1, "Group"), p]];
  assert.deepEqual(hidden(sectioned(parts, new Set(["2:early"]))), [["1:timeline", "2:early", "-", "2:late", "p", "1:group", "p"]]);
});

test("the same heading twice gets a number", () => {
  const parts = [[h(2, "Notes"), p, h(2, "notes "), p]];
  assert.deepEqual(hidden(sectioned(parts, new Set(["2:notes~2"]))), [["2:notes", "p", "2:notes~2", "-"]]);
});

test("an embedded document ends the section above it", () => {
  const embed = { ends: 3 };
  const placed = sectioned([[h(3, "Harbour"), p, embed, p]], new Set(["3:harbour"]));
  assert.deepEqual(placed[0].map((x) => x.hidden), [false, true, false, false]);
  const deeper = sectioned([[h(2, "City"), p, embed]], new Set(["2:city"]));
  assert.deepEqual(deeper[0].map((x) => x.hidden), [false, true, true], "under a higher heading it folds along");
});
