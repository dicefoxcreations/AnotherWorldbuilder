import test from "node:test";
import assert from "node:assert/strict";
import { defaultNavigation } from "../../src/shared/navigation.ts";
import { parentIdOf, targetOf, parentField, typeOf } from "../../src/shared/types.ts";

test("the default outline keeps books and the home page; helper types and chapters remain usable below books", () => {
  const types = [
    { id: "book", name: "Book", properties: [], blocks: [] },
    {
      id: "book-chapter",
      name: "Book chapter",
      navigationHidden: true,
      properties: [{ key: "book", label: "Book", kind: "reference", articleType: "book", parent: true }],
      blocks: [],
    },
    { id: "gender", name: "Gender", navigationHidden: true, properties: [], blocks: [] },
  ];
  assert.deepEqual(
    defaultNavigation({ home: "art:home" }, types).map((e) => e.target),
    ["art:home", "book"],
  );
  for (const t of types) {
    t.group = false;
    t.rules = [];
    t.infobox = { align: "none", width: "auto", show: [] };
  }
  const chapter = { id: "art:chapter", articleType: "book-chapter", layers: [{ section: "properties", fields: { book: "art:book" } }] };
  assert.equal(parentIdOf(types, chapter), "art:book");
  assert.equal(targetOf(types, "book-chapter", parentField(typeOf(types, "book-chapter"))), "book");
});
