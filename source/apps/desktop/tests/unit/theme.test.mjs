// The reader's colours — `shared/Theme`.
import assert from "node:assert/strict";
import test from "node:test";
import { derive, luminance, parseHex, SCHEMES, valid } from "../../src/shared/Theme.ts";

test("a colour is written #rgb or #rrggbb", () => {
  assert.deepEqual(parseHex("#2fc6b2"), [47, 198, 178]);
  assert.deepEqual(parseHex("abc"), [170, 187, 204]);
  assert.equal(parseHex("#12345"), null);
  assert.equal(valid({ background: "#000", text: "#fff", accent: "red", heading: "#fff" }), false);
});

test("the surfaces lie between the background and the text, rising toward it", () => {
  for (const { colors } of SCHEMES) {
    const v = derive(colors);
    const steps = ["--base", "--sunken", "--card", "--popover", "--raised", "--border", "--muted", "--text"].map((n) => luminance(v[n]));
    const rising = luminance(colors.text) > luminance(colors.background);
    steps.slice(1).forEach((l, i) => assert.ok(rising ? l >= steps[i] : l <= steps[i], `${colors.background}: step ${i + 1}`));
  }
});

test("a light background is drawn light; on the accent stands what reads on it", () => {
  const parchment = derive(SCHEMES.find((s) => s.name === "Parchment").colors);
  assert.equal(parchment["color-scheme"], "light");
  assert.ok(luminance(parchment["--on-accent"]) > 0.7, "dark accent: light writing");
  const night = derive(SCHEMES[0].colors);
  assert.equal(night["color-scheme"], "dark");
  assert.ok(luminance(night["--on-accent"]) < 0.05, "bright accent: dark writing");
});
