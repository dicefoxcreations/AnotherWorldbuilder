// Reckoning and rolling: what `[[…]]` and dice in the text do.
import assert from "node:assert/strict";
import test from "node:test";
import { hasDice, parse as read, reckon, rollFormula, signed, written } from "../../src/shared/Dice.ts";

/** The rules of a game, as a world writes them — none is built in. */
const RULES = [
  { name: "mod", params: ["x"], formula: "floor((x - 10) / 2)" },
  { name: "check", params: ["x"], formula: "1d20 + x" },
  { name: "crit", params: [], formula: "20" },
  { name: "fumble", params: [], formula: "1" },
];
const parse = (formula) => read(formula, RULES);
const sheet = { strength: 16, dexterity: 13, "armor class": 15 };
const lookup = Object.assign((name) => sheet[name.toLowerCase()], { rules: RULES });
/** Dice that always show their highest side. */
const high = (sides) => sides;

test("numbers, names and functions reckon", () => {
  assert.equal(reckon(parse("10 + mod(Dexterity)"), lookup).total, 11);
  assert.equal(reckon(parse("Strength - 10"), lookup).total, 6, "a spaced hyphen subtracts");
  assert.equal(reckon(parse("Strength mod * 2"), lookup).total, 6, "a name runs over a single space");
  assert.equal(reckon(parse("(Strength - 10) / 4"), lookup).total, 1, "division rounds down");
  assert.equal(reckon(parse("max(1, -3)"), lookup).total, 1);
  assert.equal(reckon(parse("Armor Class"), lookup).total, 15);
});

test("a name without a number, or a stray sign, says so", () => {
  assert.throws(() => reckon(parse("Wisdom + 1"), lookup), /Wisdom/);
  assert.throws(() => parse("1 +"), /too soon/);
  assert.throws(() => parse("2 3"), /one too many/);
  assert.throws(() => parse("1 $ 2"), /\$/);
});

test("dice are dice, in both spellings", () => {
  assert.ok(hasDice(parse("2W6 + 3")));
  assert.ok(hasDice(parse("d20")));
  assert.ok(!hasDice(parse("Strength")));
  const r = reckon(parse("2d6 + 1W4 + Strength mod"), lookup, high);
  assert.equal(r.total, 6 + 6 + 4 + 3);
  assert.deepEqual(r.dice.map((d) => d.sides), [6, 6, 4]);
  assert.equal(r.detail, "[6, 6] + [4] + 3");
});

test("a check rolls what the rule check says", () => {
  const r = rollFormula("+ Strength mod", lookup, high);
  assert.equal(r.total, 23);
  assert.equal(r.formula, "1d20 + 3");
  assert.equal(r.detail, "[20] + 3");
  assert.equal(r.mark, "crit", "the dice came to the rule crit");
  assert.equal(rollFormula("-1", lookup, () => 1).total, 0);
  assert.equal(rollFormula("-1", lookup, () => 1).mark, "fumble");
  assert.equal(rollFormula("+0", lookup, () => 7).mark, undefined);
  assert.equal(rollFormula("1d20", lookup, high).mark, undefined, "only a check is marked");
  // Another game: three six-sided dice, a critical from 17 on.
  const other = Object.assign(lookup.bind(null), { rules: [{ name: "check", params: ["x"], formula: "3d6 + x" }, { name: "crit", params: [], formula: "17" }] });
  const r3 = rollFormula("+2", other, high);
  assert.equal(r3.formula, "3d6 + 2");
  assert.equal(r3.mark, "crit");
  // Without a rule for checks, +4 is a number.
  const plain = (name) => sheet[name.toLowerCase()];
  const r4 = rollFormula("+4", plain, high);
  assert.equal(r4.total, 4);
  assert.equal(r4.dice.length, 0);
});

test("rules call rules, take their arguments, and do not loop", () => {
  const rules = [...RULES, { name: "half", params: ["x"], formula: "x / 2" }, { name: "both", params: ["a", "b"], formula: "half(a) + mod(b)" }, { name: "self", params: [], formula: "self + 1" }];
  assert.equal(reckon(read("both(Strength, Dexterity)", rules), lookup).total, 8 + 1);
  assert.throws(() => read("both(Strength)", rules), /takes 2 values/);
  assert.throws(() => read("mod(1, 2)", rules), /takes one value/);
  assert.throws(() => reckon(read("self", rules), lookup), /needs itself/);
  // A property of the name comes first.
  const own = Object.assign((name) => ({ "strength mod": 9, strength: 16 })[name.toLowerCase()], { rules });
  assert.equal(reckon(read("Strength mod", rules), own).total, 9);
  assert.throws(() => read("mod(Strength)"), /one too many/, "without rules, mod is no function");
});

test("written fills in names and keeps the dice", () => {
  assert.equal(written(parse("1d20 + Strength mod"), lookup), "1d20 + 3");
  assert.equal(written(parse("2d6 + 1 + 2"), lookup), "2d6 + 3");
  assert.equal(written(parse("2d6 + (Strength mod - 5)"), lookup), "2d6 - 2");
  assert.equal(written(parse("(1d6 + 1) * 2"), lookup), "(1d6 + 1) * 2");
  assert.equal(written(parse("10 + mod(Dexterity)"), lookup), "11");
  assert.equal(written(parse("1d20 + Wisdom"), lookup), null);
  assert.equal(written(parse("1W8 + Strength mod"), lookup), "1W8 + 3", "the die keeps its letter");
  assert.equal(written(parse("2W20kh1 + Strength mod + 2 + 2"), lookup), "2W20kh1 + 7", "the numbers of a sum add up");
  assert.equal(written(parse("1d6 + 2 + 1d4 - 5"), lookup), "1d6 + 1d4 - 3");
  assert.equal(written(parse("1W6 + Strength - 16"), lookup), "1W6", "nothing to add, nothing written");
});

test("modifiers as the rule says, and as a stat block writes them", () => {
  assert.deepEqual([1, 8, 10, 11, 16, 30].map((x) => reckon(parse("mod(x)"), () => x).total), [-5, -1, 0, 0, 3, 10]);
  assert.equal(signed(3), "+3");
  assert.equal(signed(0), "+0");
  assert.equal(signed(-1), "−1");
});

test("count and sides of a die may be reckoned", () => {
  const hero = { stufe: 3, "klasse.trefferwürfel": 10, konstitution: 14 };
  const look = Object.assign((name) => hero[name.toLowerCase()], { rules: RULES });
  const r = rollFormula("(Stufe)W(Klasse.Trefferwürfel) + Stufe * Konstitution mod", look, high);
  assert.equal(r.formula, "3W10 + 6");
  assert.equal(r.total, 36);
  assert.equal(r.dice.length, 3);
  assert.equal(rollFormula("1d(Klasse.Trefferwürfel)", look, high).formula, "1d10");
  assert.equal(rollFormula("W(Klasse.Trefferwürfel)", look, high).total, 10, "without a count, one die");
  assert.equal(rollFormula("(Stufe)d8", look, high).formula, "3d8");
  assert.equal(written(parse("(Stufe)W(Unbekannt)"), look), null, "a side without a number says nothing");
  assert.equal(reckon(parse("Klasse.Trefferwürfel / 2 + 1"), look).total, 6, "a dot goes through a reference");
});

test("or takes the first that has a number; picks one by one", () => {
  const look = Object.assign((name) => ({ durchschnitt: 7 })[name.toLowerCase()], { all: (name) => (name === "Klassen.Trefferwürfel" ? [10, 8] : undefined) });
  assert.equal(reckon(parse("or(Gewürfelt, Durchschnitt)"), look).total, 7);
  assert.equal(reckon(parse("or(3, Durchschnitt)"), look).total, 3);
  assert.throws(() => reckon(parse("or(Gewürfelt, Nichts)"), look), /Nichts/);
  assert.equal(reckon(parse("first(Klassen.Trefferwürfel)"), look).total, 10);
  assert.equal(reckon(parse("max(Klassen.Trefferwürfel)"), look).total, 10);
  assert.equal(reckon(parse("count(Klassen.Trefferwürfel)"), look).total, 2);
});

test("comparisons are 1 or 0; if takes one branch only", () => {
  assert.equal(reckon(parse("Strength >= 16"), lookup).total, 1);
  assert.equal(reckon(parse("Strength < 10 + 2"), lookup).total, 0, "a comparison binds loosest");
  assert.equal(reckon(parse("Dexterity = 13"), lookup).total, 1, "a single = compares");
  assert.equal(reckon(parse("if(Strength > 15, 2, Nothing)"), lookup).total, 2, "the other branch is not asked");
  assert.equal(reckon(parse("and(1, 0) + not(0)"), lookup).total, 1);
  assert.equal(written(parse("if(Strength > 15, 1d8, 1d6) + 1"), lookup), "1d8 + 1");
});

test("steps: a table that rises — a proficiency bonus by level", () => {
  const bonus = (level) => reckon(parse("steps(Level, 1, 2, 5, 3, 9, 4, 13, 5, 17, 6)"), () => level).total;
  assert.deepEqual([1, 4, 5, 8, 9, 13, 16, 17, 20].map(bonus), [2, 2, 3, 3, 4, 5, 5, 6, 6]);
  assert.equal(reckon(parse("2 + floor((Level - 1) / 4)"), () => 9).total, 4, "or as a formula");
  assert.throws(() => reckon(parse("steps(0, 1, 2)"), lookup), /no step/);
});

test("keep the highest or lowest: advantage, disadvantage, 4d6 drop the lowest", () => {
  const seq = (...v) => { let i = 0; return () => v[i++]; };
  const adv = rollFormula("2W20kh1 + 3", lookup, seq(4, 17));
  assert.equal(adv.total, 20);
  assert.equal(adv.formula, "2W20kh1 + 3");
  assert.equal(adv.detail, "[4̶, 17] + 3");
  assert.equal(rollFormula("2d20kl1", lookup, seq(4, 17)).total, 4);
  assert.equal(rollFormula("4d6kh3", lookup, seq(1, 5, 3, 6)).total, 14);
  assert.equal(rollFormula("(2)W(20)kh1", lookup, seq(9, 3)).total, 9, "a reckoned die keeps too");
  // A check by advantage: the rule says what that is.
  const rules = [...RULES, { name: "vorteil", params: ["x"], formula: "2W20kh1 + x" }];
  const r = rollFormula("vorteil(Strength mod)", Object.assign(lookup.bind(null), { rules }), seq(20, 2));
  assert.equal(r.total, 23);
});

test("a check is judged by the rules: rolling under a skill", () => {
  const seq = (...v) => { let i = 0; return () => v[i++]; };
  const rules = [
    { name: "check", params: ["x"], formula: "1W100" },
    { name: "success", params: ["d", "x"], formula: "d <= x" },
    { name: "crit", params: ["d", "x"], formula: "d <= x / 5" },
    { name: "fumble", params: ["d"], formula: "d >= 96" },
  ];
  const look = Object.assign(() => 60, { rules });
  assert.equal(rollFormula("+ Klettern", look, seq(45)).mark, "success");
  assert.equal(rollFormula("+ Klettern", look, seq(61)).mark, "failure");
  assert.equal(rollFormula("+ Klettern", look, seq(12)).mark, "crit");
  assert.equal(rollFormula("+ Klettern", look, seq(99)).mark, "fumble");
  // A fixed target: 2d6 + x, ten or more succeeds.
  const target = Object.assign(() => 1, { rules: [{ name: "check", params: ["x"], formula: "2d6 + x" }, { name: "success", params: [], formula: "10" }] });
  assert.equal(rollFormula("+1", target, seq(4, 5)).mark, "success");
  assert.equal(rollFormula("+1", target, seq(4, 4)).mark, "failure");
  // Advantage on a check: the dropped die is no critical.
  const adv = Object.assign(lookup.bind(null), { rules: [...RULES.filter((r) => r.name !== "check"), { name: "check", params: ["x"], formula: "2W20kh1 + x" }] });
  assert.equal(rollFormula("+0", adv, seq(20, 3)).mark, "crit");
  assert.equal(rollFormula("+0", adv, seq(1, 3)).mark, undefined);
});

test("dice handed to a rule are thrown once, however often it names them", () => {
  let thrown = 0;
  const rules = [{ name: "twice", params: ["w"], formula: "w + w" }];
  const r = reckon(read("twice(1d6)", rules), lookup, () => (thrown++, 4));
  assert.equal(r.total, 8);
  assert.equal(thrown, 1);
});

test("a pool: dice counted by what they show", () => {
  const seq = (...v) => { let i = 0; return () => v[i++]; };
  const rules = [
    { name: "treffer", params: ["w"], formula: "w >= 5" },
    { name: "eins", params: ["w"], formula: "w == 1" },
    { name: "check", params: ["x"], formula: "sum(map(treffer, (x)W6))" },
    { name: "success", params: [], formula: "1" },
    { name: "fumble", params: ["d"], formula: "sum(map(eins, d)) * 2 > count(d)" },
    { name: "shown", params: ["x"], formula: "x" },
  ];
  const look = Object.assign((name) => ({ pool: 5 })[name.toLowerCase()], { rules });
  const r = rollFormula("+ Pool", look, seq(6, 2, 5, 1, 5));
  assert.equal(r.total, 3, "three hits");
  assert.equal(r.formula, "5W6");
  assert.equal(r.detail, "[6, 2, 5, 1, 5] → 3");
  assert.equal(r.mark, "success");
  assert.equal(rollFormula("+ Pool", look, seq(1, 1, 1, 2, 3)).mark, "fumble", "more than half show one");
  assert.equal(rollFormula("+ Pool", look, seq(2, 2, 3, 4, 3)).mark, "failure");
});

test("three dice against three values: a three-ability check", async () => {
  const seq = (...v) => { let i = 0; return () => v[i++]; };
  const rules = [
    { name: "over", params: ["w", "e"], formula: "max(0, w - e)" },
    { name: "check", params: ["a", "b", "c", "fw"], formula: "fw - sum(map(over, 3W20, [a, b, c]))" },
    { name: "success", params: ["d", "x", "r"], formula: "r >= 0" },
    { name: "eins", params: ["w"], formula: "w == 1" },
    { name: "zwanzig", params: ["w"], formula: "w == 20" },
    { name: "crit", params: ["d"], formula: "sum(map(eins, d)) >= 2" },
    { name: "fumble", params: ["d"], formula: "sum(map(zwanzig, d)) >= 2" },
    { name: "shown", params: ["a", "b", "c", "fw"], formula: "fw" },
  ];
  const look = Object.assign((name) => ({ mut: 12, klugheit: 13, intuition: 14, klettern: 6 })[name.toLowerCase()], { rules });
  const probe = "check(Mut, Klugheit, Intuition, Klettern)";
  const r = rollFormula(probe, look, seq(15, 10, 16));
  assert.equal(r.total, 1, "3 over Mut, 2 over Intuition: 6 - 5 left");
  assert.equal(r.mark, "success");
  assert.equal(rollFormula(probe, look, seq(20, 18, 19)).mark, "failure");
  assert.equal(rollFormula(probe, look, seq(1, 1, 19)).mark, "crit");
  assert.equal(rollFormula(probe, look, seq(20, 20, 3)).mark, "fumble");
  const { checkShown } = await import("../../src/shared/Dice.ts");
  assert.equal(checkShown(probe, rules, look), "6", "the button shows the skill value");
});
