// The text of the visual editor, cut the way it is drawn.
import assert from "node:assert/strict";
import test from "node:test";
import { openLink, runs, shownName } from "../../src/shared/editor/FlowText.ts";
import { pieces } from "../../src/shared/GraphText.ts";

const flat = (all) => all.map((r) => (r.inner ? `${r.look}(${flat(r.inner)})` : r.look ? `${r.look}:${r.text}` : r.text)).join("|");
const length = (all) => all.reduce((n, r) => n + r.text.length, 0);

test("a link is drawn as read — and as text while the caret stands in it", () => {
  const text = "To @@Place:Paris@@ and back.";
  assert.equal(flat(runs(text)), "To |link:@@Place:Paris@@| and back.");
  assert.equal(flat(runs(text, 8)), "To |link-open:@@Place:Paris@@| and back.");
  // At its edges the caret is outside.
  assert.equal(flat(runs(text, 3)), "To |link:@@Place:Paris@@| and back.");
  assert.equal(flat(runs(text, 18)), "To |link:@@Place:Paris@@| and back.");
  assert.deepEqual(openLink(text, 8), [3, 18]);
  assert.equal(openLink(text, 18), null);
});

test("the marks of bold, italic, code and headings show only with the caret at them", () => {
  const text = "## Title\nA **bold** word.";
  assert.equal(flat(runs(text)), "heading h3(mark-hidden:## |Title\n)|A |bold(mark-hidden:**|bold|mark-hidden:**)| word.");
  // In the heading, at the bold word's edge: their marks are there.
  assert.equal(flat(runs(text, 3)), "heading h3(mark:## |Title\n)|A |bold(mark-hidden:**|bold|mark-hidden:**)| word.");
  assert.equal(flat(runs(text, 11)), "heading h3(mark-hidden:## |Title\n)|A |bold(mark:**|bold|mark:**)| word.");
  assert.equal(length(runs(text)), text.length);
});

test("what a link shows without the world", () => {
  assert.equal(shownName("@@Place:Paris@@"), "Paris");
  assert.equal(shownName("@@Paris|the city@@"), "the city");
  assert.equal(shownName("@@Character:Tim#Status@@"), "Tim");
});

test("list items and quotes are lines with their marks in front", () => {
  const text = "- one @@Tim@@\n- two\n1. first\n> said\nplain\n";
  const r = runs(text);
  assert.equal(flat(r), "item(bullet:- |one |link:@@Tim@@|\n)|item(bullet:- |two\n)|item(number:1. |first\n)|quote(quote-mark:> |said\n)|plain\n");
  // Nothing is lost: the runs are the text.
  assert.equal(length(r), text.length);
});

test("a link inside an item opens by its place in the whole text", () => {
  const text = "x\n- @@Tim@@\n";
  assert.equal(flat(runs(text, 6)), "x\n|item(bullet:- |link-open:@@Tim@@|\n)");
});

test("a list tag on a line of its own is a surface; a link in it stays text", () => {
  const p = pieces('Before.\n<list from=Equipment style=table show="Art, Wert">\nAfter.\n');
  assert.deepEqual(p.map((x) => x.kind), ["text", "list", "text"]);
  assert.deepEqual(p[1].list, [["from", "Equipment"], ["style", "table"], ["show", "Art, Wert"]]);
});

test("rolls and rules are drawn as read", () => {
  const text = "Hit: 7 (2d6 + 3) and [[+4]], not 2d6x.\n---\nAfter.";
  assert.equal(flat(runs(text)), "Hit: 7 (|roll:2d6 + 3|) and |roll:[[+4]]|, not 2d6x.\n|rule:---\n|After.");
  assert.equal(flat(runs(text, 40)), "Hit: 7 (|roll:2d6 + 3|) and |roll:[[+4]]|, not 2d6x.\n|rule-open:---\n|After.");
  assert.equal(length(runs(text)), text.length);
});

test("bold italic, as a stat block names an action", () => {
  assert.equal(flat(runs("***Scimitar.*** *Melee:* hit")), "bold italic(mark-hidden:***|Scimitar.|mark-hidden:***)| |italic(mark-hidden:*|Melee:|mark-hidden:*)| hit");
});
