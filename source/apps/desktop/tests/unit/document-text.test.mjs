import test from "node:test";
import assert from "node:assert/strict";
import { documentOutline, searchDocument, readableText } from "../../src/shared/documentText.ts";

const matchDocument = (article, words) => searchDocument(article, words)[0] ?? null;

const chapter = (name, text) => ({ id: name, articleType: "chapter", name, layers: [{ section: "content", text }] });
const crafting = chapter(
  "Crafting",
  `Intro.
## Tools of the Trade ^made
Crafting needs tools.
### Work ^work
Make a simple tool.
## Equipment ^equipment
Rare moonsilver components are valuable.
## Old Recipes ^recipes
Recipes are collected here.
## Cooking ^cooking
### Meals ^meals
A fictional meal has a special ingredient.`,
);

test("chapter outlines retain all nested source headings and their destinations in source order", () => {
  const outline = documentOutline(crafting);
  assert.deepEqual(
    outline.map((h) => h.label),
    ["Tools of the Trade", "Equipment", "Old Recipes", "Cooking"],
  );
  assert.deepEqual(
    outline.map((h) => h.anchor),
    ["made", "equipment", "recipes", "cooking"],
  );
  assert.equal(outline[0].children[0].label, "Work");
  assert.equal(outline[3].children[0].anchor, "meals");
});
test("book terms match readable body text and return the closest section, anchor and excerpt", () => {
  const hit = matchDocument(crafting, "MOONSILVER");
  assert.equal(hit.article.id, "Crafting");
  assert.equal(hit.section, "Equipment");
  assert.equal(hit.anchor, "equipment");
  assert.match(hit.snippet, /moonsilver components/);
  const nested = matchDocument(crafting, "special ingredient");
  assert.equal(nested.section, "Meals");
  assert.equal(nested.anchor, "meals");
  assert.equal(matchDocument(crafting, "absent word"), null);
  assert.equal(matchDocument(crafting, "   "), null);
});
test("search does not index property layers, hidden anchor IDs or image IDs", () => {
  const doc = chapter("Visible", "## Intro ^internal-secret-anchor\nVisible text.\n<image src=img:secret-image>");
  doc.layers.push({ section: "properties:secret", text: "Not a prose search result", fields: { password: "private" } });
  for (const query of ["internal-secret-anchor", "secret-image", "password", "Not a prose search result"]) assert.equal(matchDocument(doc, query), null, query);
  assert.equal(readableText("See @@art:01234567890123456789012345|**Moss**@@. ^target"), "See Moss.");
});
test("code fences and unrelated paragraph anchors do not invent subchapter entries", () => {
  const doc = chapter("Code", "## One ^one\nText.\n```md\n## Fake ^fake\n```\n## Unlinked\nParagraph.\n\u200b ^paragraph\n## Real ^real\nText.");
  assert.deepEqual(
    documentOutline(doc).map((h) => h.label),
    ["One", "Real"],
  );
});
test("an immediately following destination belongs to the heading; repeated destinations stay unique", () => {
  const doc = chapter("Other", "## First\n\n\u200b ^first\nBody.\n## Duplicate ^first\nIgnored.\n## Second ^second\nBody.");
  assert.deepEqual(
    documentOutline(doc).map((h) => h.anchor),
    ["first", "second"],
  );
});
test("all matching sections are returned, including repeated section names under Cooking", () => {
  const doc = chapter(
    "Crafting",
    `## Tools of the Trade ^made
### Forging ^forging
A quirk can be a boon or a flaw. More quirks, boons and flaws.
### Quirks ^item-quirks
A quirk grants a boon or a flaw.
## Cooking ^cooking
### Quirks ^cooking-quirks
A quirk in a meal can be a boon or a flaw.
#### Cooking Flaws ^cooking-flaws
A flaw makes the meal worse.
#### Cooking Boons ^cooking-boons
A boon improves the meal.`,
  );
  for (const term of ["quirk", "boon", "flaw"]) {
    const hits = searchDocument(doc, term.toUpperCase());
    assert.equal(hits[0].anchor, "forging");
    assert.ok(hits.some((h) => h.anchor === "item-quirks"));
    const cooking = hits.find((h) => h.anchor === "cooking-quirks");
    assert.ok(cooking, `${term} must not stop after the first match`);
    assert.deepEqual(cooking.path, ["Cooking", "Quirks"]);
    assert.equal(hits.filter((h) => h.anchor === "forging").length, 1, "Repeated words in one section produce one section result");
  }
  assert.ok(searchDocument(doc, "flaw").some((h) => h.anchor === "cooking-flaws"));
  assert.ok(searchDocument(doc, "boon").some((h) => h.anchor === "cooking-boons"));
  assert.deepEqual(searchDocument(doc, "absent"), []);
  assert.deepEqual(searchDocument(doc, " "), []);
});
test("a matching document name does not suppress matching body sections", () => {
  const doc = chapter("Quirks", "## Cooking ^cooking\nA meal has quirks.\n### Effects ^effects\nMore quirks.");
  const hits = searchDocument(doc, "quirk");
  assert.equal(hits.length, 3);
  assert.equal(hits[0].section, undefined);
  assert.deepEqual(
    hits.slice(1).map((h) => h.path),
    [["Cooking"], ["Cooking", "Effects"]],
  );
});
