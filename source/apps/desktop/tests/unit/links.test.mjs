// Links: anchors in the same document and in another one.
import assert from "node:assert/strict";
import test from "node:test";
import { linksIn, scanLinks } from "../../src/shared/Links.ts";

test("a link may lead to an anchor here or in another document", () => {
  assert.deepEqual(linksIn("as @@#^2f9e17|the inn@@ said"), [{ of: "", name: "", property: "^2f9e17", text: "the inn" }]);
  assert.deepEqual(linksIn("@@Session log:Session 1#^c70ff9@@")[0].property, "^c70ff9");
  // Nothing but a property, or nothing at all, is no link — the text stays.
  assert.deepEqual(scanLinks("@@#Status@@"), ["@@#Status@@"]);
  assert.deepEqual(scanLinks("@@|x@@"), ["@@|x@@"]);
});
