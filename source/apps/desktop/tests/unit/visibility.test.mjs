// What a policy amounts to — `shared/ui/Visibility`.
import assert from "node:assert/strict";
import test from "node:test";
import { effective, samePolicy } from "../../src/shared/Policies.ts";

test("everyone and the GM exclude each other; the GM always stays", () => {
  assert.deepEqual(effective("all OR gm"), ["all"], "everyone takes in the GM");
  assert.deepEqual(effective("players OR player:roque"), ["all"], "all players are everyone: every player holds both");
  assert.ok(samePolicy("players", "all"));
  assert.deepEqual(effective("player:roque"), ["gm", "player:roque"], "the GM comes along");
  assert.deepEqual(effective(""), ["gm"]);
  assert.ok(samePolicy("all OR gm", "all"));
  assert.ok(samePolicy("gm OR doc:1", "doc:1 OR gm"));
  assert.ok(!samePolicy("gm", "all"));
});
