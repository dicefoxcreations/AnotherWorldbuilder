// The code editor: closing tags as one types, and what counts as a change.
import assert from "node:assert/strict";
import test from "node:test";
import { withClosingTag } from "../../src/shared/editor/Closing.ts";
import { sameContent } from "../../src/shared/editor/Content.ts";

/** Types one character at `at` and lets the editor close the tag. */
function type(text, at, char) {
  const after = text.slice(0, at) + char + text.slice(at);
  return withClosingTag(text, after, at + 1);
}

test("a block typed before </body> leaves </body> on a line of its own", () => {
  const before = "<body>\n<block</body>\n</article>";
  const r = type(before, before.indexOf("</body>"), ">");
  assert.equal(r.text, "<body>\n<block>\n\n</block>\n</body>\n</article>");
  assert.equal(r.text.slice(r.caret, r.caret + 1), "\n", "the caret stands on the empty line");
});

test("a block on a line of its own closes as before", () => {
  const before = "<body>\n<block\n</body>";
  const r = type(before, before.indexOf("\n</body>"), ">");
  assert.equal(r.text, "<body>\n<block>\n\n</block>\n</body>");
});

const doc = (blocks, properties = []) => ({
  name: "Anna",
  access: { view: "all", edit: "gm", level: "open" },
  properties,
  blocks,
});
const block = (text, key = "k1") => ({
  key,
  title: "",
  style: "text",
  parts: [{ key, text, view: "all", edit: "gm", level: "open" }],
});

test("an empty block or an empty line is no change", () => {
  assert.ok(sameContent(doc([block("")]), doc([])), "an empty block is not stored — and no change");
  assert.ok(sameContent(doc([block("Text\n")]), doc([block("Text", "other-id")])));
  assert.ok(!sameContent(doc([block("Text")]), doc([])));
  assert.ok(!sameContent(doc([], [{ name: "Relations", value: "@@Olga@@" }]), doc([], [{ name: "Relations", value: "@@npc:Olga@@" }])));
});
