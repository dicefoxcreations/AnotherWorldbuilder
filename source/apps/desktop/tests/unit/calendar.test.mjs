// The world's calendar: the same days as in Rust (`aw_model::calendar`).
import assert from "node:assert/strict";
import test from "node:test";
import { DEFAULT_ERA, eraYmd, formatDay, formatMoment, moonAge, moonsAt, parseMoment, toDay, toYmd } from "../../src/shared/Calendar.ts";

const leap = { ...DEFAULT_ERA, leap: { every: 4, month_index: 1 } };
const world = (era) => ({ eras: [era] });

// The coast's count before the Mist, and after it a new one — as in Rust's tests.
const old = { ...DEFAULT_ERA, name: "Old count", abbr: "OC", before: "BOC" };
const mist = {
  ...DEFAULT_ERA,
  name: "Mist count",
  abbr: "AM",
  months: [{ name: "Ice", days: 40 }, { name: "Thaw", days: 50 }],
  start: toDay(old, 301, 1, 1),
};
const twoEras = { eras: [old, mist] };

test("there and back, with and without a leap year, before the epoch too", () => {
  for (const c of [DEFAULT_ERA, leap]) {
    for (const [y, m, d] of [[1, 1, 1], [4, 2, 30], [812, 7, 15], [0, 12, 30], [-17, 6, 12]]) {
      assert.deepEqual(eraYmd(c, toDay(c, y, m, d)), [y, m, d], `${y}-${m}-${d}`);
    }
  }
  assert.equal(toDay(leap, 4, 2, 31) !== null, true);
  assert.equal(toDay(leap, 3, 2, 31), null);
  assert.equal(toDay(DEFAULT_ERA, 1, 1, 1), 0);
});

test("a moment reads as in Rust", () => {
  const day = toDay(leap, 812, 7, 15);
  assert.equal(formatMoment(world(leap), day), "15 Hay Moon 812 AR");
  assert.equal(formatMoment(world(leap), day + 0.75), "15 Hay Moon 812 AR, 18:00");
});

test("the day count has no gaps", () => {
  const start = toDay(leap, -2, 1, 1);
  for (let step = 0; step < 6 * 360; step++) {
    const [y, m, d] = eraYmd(leap, start + step);
    assert.equal(toDay(leap, y, m, d), start + step);
  }
});

test("a moment is read back as Rust reads it", () => {
  const c = world(leap);
  const day = toDay(leap, 812, 7, 15);
  for (const w of ["15 Hay Moon 812 AR", "15. Hay Moon 812", "15 hay moon 812 AR"]) assert.equal(parseMoment(c, w), day, w);
  assert.equal(parseMoment(c, "15 Hay Moon 812 AR, 18:00"), day + 0.75);
  assert.equal(parseMoment(c, "448804"), 448804);
  for (const w of ["", "Hay Moon 812", "15 Wet Moon 812", "31 Hay Moon 812", "15 Hay Moon"]) assert.equal(parseMoment(c, w), null, w);
});

test("each era writes and reads its own days", () => {
  assert.equal(formatDay(twoEras, mist.start - 1), "30 Dark Moon 300 OC");
  assert.equal(formatDay(twoEras, mist.start), "1 Ice 1 AM");
  assert.equal(formatDay(twoEras, mist.start + 95), "6 Ice 2 AM");
  assert.equal(formatDay(twoEras, -1), "30 Dark Moon 1 BOC");
  assert.equal(toYmd(twoEras, mist.start).index, 1);
  for (const day of [-800, -361, -1, 0, 5000, mist.start - 1, mist.start, mist.start + 95, mist.start + 4000]) {
    assert.equal(parseMoment(twoEras, formatDay(twoEras, day)), day, formatDay(twoEras, day));
  }
  assert.equal(parseMoment(twoEras, "1 Ice 1"), mist.start);
  assert.equal(parseMoment(twoEras, "1 Ice 12 OC"), null);
});

test("a moon is full on its day and new half a round later", () => {
  const moon = { name: "Silver", color: "#ddd", period: 30, full: 100 };
  assert.equal(moonAge(moon, 100), 0.5);
  assert.equal(moonAge(moon, 115), 0);
  assert.equal(moonAge(moon, 85), 0);
  assert.equal(moonAge(moon, 130), 0.5);
});

test("each era has its own moons", () => {
  const blood = { name: "Blood", color: "#e06", period: 71, full: mist.start };
  const c = { eras: [old, { ...mist, moons: [blood] }] };
  assert.deepEqual(moonsAt(c, mist.start - 1), []);
  assert.deepEqual(moonsAt(c, mist.start + 10), [blood]);
});
