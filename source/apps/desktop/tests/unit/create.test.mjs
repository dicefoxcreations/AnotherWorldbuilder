// Who may make documents of a type — `shared/types`.
import assert from "node:assert/strict";
import test from "node:test";
import { mayCreate, permits } from "../../src/shared/types.ts";

test("a view as written: OR between choices, AND within one; every player holds players and all", () => {
  assert.equal(permits("players", []), true);
  assert.equal(permits("class:rogue OR gm", ["class:rogue"]), true);
  assert.equal(permits("class:rogue OR gm", []), false);
  assert.equal(permits("players AND doc:ilva", ["doc:ilva"]), true);
  assert.equal(permits("players AND doc:ilva", ["doc:fennick"]), false);
  assert.equal(permits("", []), false);
});

test("the GM makes every type; a player those that say so", () => {
  const type = (create) => ({ id: "t", name: "Log", group: false, create });
  const player = { isGm: false, attributes: [] };
  assert.equal(mayCreate({ isGm: true, attributes: [] }, type("")), true);
  assert.equal(mayCreate(player, type("")), false, "without create=: the GM alone");
  assert.equal(mayCreate(player, type("players")), true);
  assert.equal(mayCreate(player, type("class:rogue")), false);
  assert.equal(mayCreate(player, { ...type("players"), group: true }), false, "a group is no document type");
});
