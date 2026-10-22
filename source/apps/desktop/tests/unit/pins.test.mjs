// Pins set apart, and how a pin's name stands — `shared/GraphText`.
import assert from "node:assert/strict";
import test from "node:test";
import { joinPins, mapText, readMap } from "../../src/shared/GraphText.ts";

const map = (text) => {
  const lines = text.split("\n");
  return readMap(lines[0], lines.slice(1, -1));
};

test("a pin's own access and where its name stands are read and written back", () => {
  const text = '<map src=img:chart>\n<pin at=30,40 label="Smugglers\' cave" label-at="top hover" view=gm edit=gm>\n<pin at=1,2 label=Harbour label-at=left>\n<pin at=3,3 label-at=hover>\n<area at="1,1 5,1 5,5" label=Bog view="doc:ilva OR gm" edit=gm>\n</map>';
  const m = map(text);
  assert.deepEqual(
    m.pins.map((p) => [p.label_at, p.label_hover, p.view]),
    [["top", true, "gm"], ["left", undefined, undefined], [undefined, true, undefined]],
    "the side and the pointer apart; hover alone stands right",
  );
  assert.equal(m.areas[0].view, "doc:ilva OR gm");
  assert.equal(mapText(m), text);
});

test("pins set apart go back onto the map with their picture, marked with who sees them", () => {
  const texts = joinPins([
    { text: "The coast.\n<map src=img:chart>\n<pin at=10,20 label=Harbour>\n</map>\nAfter.", policy: "players" },
    { text: "<pins for=img:chart>\n<pin at=30,40 label=Cave>\n</pins>", policy: "gm" },
    { text: "<pins for=img:chart>\n<pin at=50,50 label=Wreck>\n</pins>", policy: "doc:ilva OR gm" },
  ]);
  assert.deepEqual(texts, [
    'The coast.\n<map src=img:chart>\n<pin at=10,20 label=Harbour>\n<pin at=30,40 label=Cave view=gm>\n<pin at=50,50 label=Wreck view="doc:ilva OR gm">\n</map>\nAfter.',
    "",
    "",
  ]);
});

test("without their map, pins set apart show nowhere — and other text is left alone", () => {
  assert.deepEqual(joinPins([{ text: "<pins for=img:chart>\n<pin at=1,1>\n</pins>", policy: "gm" }]), [""]);
  assert.deepEqual(joinPins([{ text: "<pins for=img:x>\nnot only pins\n</pins>", policy: "gm" }]), ["<pins for=img:x>\nnot only pins\n</pins>"]);
});
