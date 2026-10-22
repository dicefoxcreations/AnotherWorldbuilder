// Formatting what is selected — `shared/editor/Format`.
import assert from "node:assert/strict";
import test from "node:test";
import { formatted } from "../../src/shared/editor/Format.ts";

test("marks around a selection, and off again", () => {
  const text = "The old lighthouse";
  const [bold, a, b] = formatted(text, 4, 7, { wrap: "**" });
  assert.equal(bold, "The **old** lighthouse");
  assert.equal(bold.slice(a, b), "old");
  assert.deepEqual(formatted(bold, a, b, { wrap: "**" })[0], text, "a second time takes them off");
  assert.equal(formatted("a word here", 1, 7, { wrap: "*" })[0], "a *word* here", "space at the edges stays outside");
});

test("each line touched becomes a heading, a quote, an item — or plain again", () => {
  const text = "first\nsecond\nthird";
  assert.equal(formatted(text, 2, 9, { line: "- " })[0], "- first\n- second\nthird");
  assert.equal(formatted("## Old\nx", 3, 4, { line: "# " })[0], "# Old\nx", "another kind replaces the one there");
  assert.equal(formatted("> said\nx", 2, 4, { line: "> " })[0], "said\nx", "the same kind again: plain");
});

test("a text cut at its headings of one level", async () => {
  const { cutAtHeadings, headingLevel } = await import("../../src/shared/Sections.ts");
  const text = "Welcome.\n\n## Places\n<list type=Place>\n### Small\nx\n\n## Timeline\n<timeline>\n```\n## not one\n```";
  assert.deepEqual(cutAtHeadings(text, 2), {
    before: "Welcome.",
    sections: [
      { title: "Places", text: "<list type=Place>\n### Small\nx" },
      { title: "Timeline", text: "<timeline>\n```\n## not one\n```" },
    ],
  });
  assert.equal(headingLevel("### Small"), 3);
  assert.equal(headingLevel("#hashtag"), 0);
});
