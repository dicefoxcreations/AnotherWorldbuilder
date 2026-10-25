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

test("blocks without their own access follow the document's", async () => {
  const { withAccess } = await import("../../src/shared/Policies.ts");
  const doc = {
    access: { view: "all", edit: "gm" },
    blocks: [
      { parts: [{ key: "a", view: "all", edit: "gm" }] },
      { parts: [{ key: "b", view: "gm", edit: "gm" }, { key: "c", view: "all", edit: "gm OR player:anna" }] },
    ],
  };
  const next = withAccess(doc, { edit: "gm OR player:leo" });
  assert.equal(next.access.edit, "gm OR player:leo");
  assert.deepEqual(next.blocks.map((b) => b.parts.map((p) => `${p.view}|${p.edit}`)), [["all|gm OR player:leo"], ["gm|gm OR player:leo", "all|gm OR player:anna"]]);
  assert.equal(withAccess(doc, { view: "players" }).blocks[1].parts[0].view, "gm", "a secret stays secret");
});
