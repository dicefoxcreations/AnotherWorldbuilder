// How a new document's name comes about — `features/templates/NamePattern`.
import assert from "node:assert/strict";
import test from "node:test";
import { nameFrom, nextNumber } from "../../src/features/templates/NamePattern.ts";

test("the counter goes on from the highest number and keeps its width", () => {
  const pattern = "Session %%count%%";
  assert.equal(nextNumber(pattern, []), "1");
  assert.equal(nextNumber(pattern, ["Session 001", "Session 001b", "Session 035"]), "036");
  assert.equal(nextNumber(pattern, ["Session 9"]), "10");
  assert.equal(nextNumber(pattern, ["Session 099"]), "100");
  assert.equal(nameFrom(pattern, "", nextNumber(pattern, ["Session 034", "Session 035"])), "Session 036");
});
