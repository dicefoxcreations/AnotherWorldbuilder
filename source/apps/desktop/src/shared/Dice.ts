/**
 * Reckoning and rolling — what `[[…]]` and dice in the text do.
 *
 * A formula is a sum the way it is said at the table: `1d20 + 5`,
 * `2W6 + Strength mod`, `10 + mod(Dexterity)`, `[[+4]]` for a check with
 * four on top. Rust only cuts it out of the text (`aw_model::markdown`);
 * here it is read and reckoned, because only here are the property values
 * of the document known.
 *
 * No game is built in. What a modifier is, what a check rolls, when a roll
 * is a critical: the world's rules say it, in its types and groups —
 * `mod(x) = floor((x - 10) / 2)`, `check(x) = 1d20 + x`, `crit = 20`. A
 * rule is called like a function, `mod(Strength)`; one of one argument also
 * after a name, `Strength mod`. Where a document has no `check`, `[[+4]]` is
 * just the number.
 *
 * - Dice: `2d6`, `d20`, `1W8` — German writes `W`. Count and sides may be
 *   reckoned: `(Level)W(Class.Hit die)`, `1d(Hit die)`.
 * - Numbers, `+ - * /`, brackets. Division rounds down, as the rules do.
 * - Properties by name, as everywhere: `Strength`, `Strength mod`,
 *   `Armor Class`. A name runs on over single spaces and hyphens as long as
 *   a letter follows — `Strength - 10` is a difference, `Strength-mod` a name.
 *   A dot goes through a reference: `Class.Hit die` is the hit die of the
 *   class the document picks; several picks add up.
 * - `Classes.(Level * Hit die)`: reckoned within each pick, then added up —
 *   inside, a pick's own number (`with Level`) and its properties are known,
 *   and what is not, the document's own.
 * - Over several picks: `first(Classes.Hit die)`, `last`, `sum`, `count`,
 *   `min`, `max` take the picks one by one.
 * - `or(Rolled, average)`: the first that has a number — a value someone
 *   entered, or else what the rules say.
 * - Built in: `floor`, `ceil`, `round`, `abs`, `min`, `max`.
 */

import type { ArticleView, Rule } from "../api/client";

export type Ast =
  | { kind: "number"; value: number }
  /** `letter`: `d` or `W`, as written — shown as written. Count and sides may be reckoned. */
  | { kind: "dice"; count: Ast; sides: Ast; letter: string; keep?: Keep }
  | { kind: "name"; name: string }
  | { kind: "call"; name: string; args: Ast[] }
  /** `Classes.(…)`: reckoned within each pick of a reference, added up. */
  | { kind: "each"; ref: string; body: Ast }
  | { kind: "neg"; of: Ast }
  | { kind: "op"; op: Op; left: Ast; right: Ast }
  /** `[Mut, Klugheit, Intuition]`: several values, one by one — as a number, their sum. */
  | { kind: "list"; items: Ast[] }
  /** `map(success, 5W6)`: a rule on each die, each value — a list. */
  | { kind: "map"; rule: Rule; rules: Rule[]; args: Ast[] }
  /** Dice handed to a rule: thrown once, however often the rule names them. */
  | { kind: "shared"; key: number; of: Ast };

type Op = "+" | "-" | "*" | "/" | "<" | "<=" | ">" | ">=" | "==" | "!=";
const COMPARE = new Set<string>(["<", "<=", ">", ">=", "==", "!="]);

/** `2W20kh1`, `4d6kh3`, `2W20kl1`: of the dice thrown, the highest (or lowest) `n` count. */
type Keep = { high: boolean; n: number };

export const FUNCTIONS: Record<string, (...a: number[]) => number> = {
  floor: Math.floor,
  ceil: Math.ceil,
  round: Math.round,
  abs: Math.abs,
  min: (...a) => Math.min(...a),
  max: (...a) => Math.max(...a),
  first: (...a) => a[0],
  last: (...a) => a[a.length - 1],
  sum: (...a) => a.reduce((s, x) => s + x, 0),
  count: (...a) => a.length,
  // Lazy — see `reckon`: the first argument that has a number.
  or: (...a) => a[0],
  // Lazy too: `if(Level >= 5, 3, 2)` — only the branch taken is reckoned.
  if: (...a) => a[0],
  and: (...a) => (a.every((x) => x !== 0) ? 1 : 0),
  not: (x) => (x === 0 ? 1 : 0),
  /**
   * A table by steps: `steps(Level, 1, 2, 5, 3, 9, 4)` — the value of the
   * highest step at or below the first number. A proficiency bonus, spell
   * slots, whatever rises in steps.
   */
  steps: (x, ...a) => {
    let found: number | undefined;
    for (let i = 0; i + 1 < a.length; i += 2) if (x >= a[i]) found = a[i + 1];
    if (found === undefined) throw new Error(`steps() has no step for ${x}`);
    return found;
  },
};

/** Functions whose arguments may be many: a path over several picks gives each. */
const OVER_PICKS = new Set(["first", "last", "sum", "count", "min", "max"]);

type Token =
  | { t: "num"; v: number }
  | { t: "dice"; count: number; sides: number; letter: string; keep?: Keep }
  | { t: "name"; v: string }
  | { t: "sym"; v: string };

const LETTER = /[\p{L}_]/u;
const WORD = /[\p{L}\p{N}_]/u;
const DIE = /^(\d{0,3})([dDwW])(\d{1,4})(?:k([hlHL])(\d{1,3}))?(?![\p{L}\p{N}_])/u;
/** What follows a reckoned die: `kh1`. */
const KEEP = /^k([hl])(\d{1,3})$/i;

function tokens(formula: string): Token[] {
  const out: Token[] = [];
  let i = 0;
  while (i < formula.length) {
    const c = formula[i];
    if (c === " " || c === "\t") {
      i++;
      continue;
    }
    // A die whose count or sides are reckoned: `(Level)W(Hit die)`, `1d(…)`,
    // `W(…)` — the letter stands right before a bracket, or right after one.
    const last = out[out.length - 1];
    const glued = i > 0 && formula[i - 1] !== " " && formula[i - 1] !== "\t";
    if (/[dDwW]/.test(c) && (formula[i + 1] === "(" || (glued && last?.t === "sym" && last.v === ")" && /[\d(]/.test(formula[i + 1] ?? "")))) {
      out.push({ t: "sym", v: c.toLowerCase() === "w" ? "W" : "d" });
      i++;
      continue;
    }
    const die = DIE.exec(formula.slice(i));
    if (die) {
      out.push({
        t: "dice",
        count: die[1] ? Number(die[1]) : 1,
        sides: Number(die[3]),
        letter: die[2].toLowerCase() === "w" ? "W" : "d",
        ...(die[4] ? { keep: { high: die[4].toLowerCase() === "h", n: Number(die[5]) } } : {}),
      });
      i += die[0].length;
      continue;
    }
    // A count glued to a reckoned die: the `1` of `1W(…)`.
    const counted = /^(\d{1,3})(?=[dDwW]\()/.exec(formula.slice(i));
    if (counted) {
      out.push({ t: "num", v: Number(counted[1]) });
      i += counted[0].length;
      continue;
    }
    if (/\d/.test(c)) {
      const m = /^\d+(?:[.,]\d+)?/.exec(formula.slice(i))!;
      out.push({ t: "num", v: Number(m[0].replace(",", ".")) });
      i += m[0].length;
      continue;
    }
    if (LETTER.test(c)) {
      let j = i;
      for (;;) {
        while (j < formula.length && WORD.test(formula[j])) j++;
        // `Classes[Class]`: which picks — part of the name.
        if (formula[j] === "[") {
          const close = formula.indexOf("]", j);
          if (close < 0) throw new Error("“]” is missing");
          j = close + 1;
          if (formula[j] === "." && LETTER.test(formula[j + 1] ?? "")) {
            j++;
            continue;
          }
          break;
        }
        // A single space, hyphen or dot joins the next word, if a letter follows.
        if ((formula[j] === " " || formula[j] === "-" || formula[j] === ".") && j + 1 < formula.length && LETTER.test(formula[j + 1])) {
          j++;
          continue;
        }
        break;
      }
      out.push({ t: "name", v: formula.slice(i, j) });
      i = j;
      continue;
    }
    // `Classes.(…)`: within each pick.
    if (c === "." && formula[i + 1] === "(") {
      out.push({ t: "sym", v: "." });
      i++;
      continue;
    }
    // Comparisons: `Level >= 5` is 1 or 0. A single `=` is `==`.
    const cmp = /^(<=|>=|==|!=|<|>|=)/.exec(formula.slice(i));
    if (cmp) {
      out.push({ t: "sym", v: cmp[1] === "=" ? "==" : cmp[1] });
      i += cmp[1].length;
      continue;
    }
    if ("+-*/(),[]".includes(c)) {
      out.push({ t: "sym", v: c });
      i++;
      continue;
    }
    // `×` and `÷` as one writes them by hand.
    if (c === "×" || c === "÷") {
      out.push({ t: "sym", v: c === "×" ? "*" : "/" });
      i++;
      continue;
    }
    throw new Error(`“${c}” is no part of a formula`);
  }
  return out;
}

const ruleNamed = (rules: Rule[], name: string) => rules.find((r) => r.name.toLowerCase() === name.toLowerCase());

/** A rule's formula with its arguments put in for its parameters. */
function applied(rule: Rule, args: Ast[], rules: Rule[], inside: string[]): Ast {
  if (args.length !== rule.params.length)
    throw new Error(`${rule.name} takes ${rule.params.length === 1 ? "one value" : `${rule.params.length} values`}`);
  if (inside.includes(rule.name.toLowerCase())) throw new Error(`the rule ${rule.name} needs itself`);
  const body = parse(rule.formula, rules, [...inside, rule.name.toLowerCase()]);
  // Dice go in once: `qs(3W20)` throws three dice, however often `qs` names them.
  const once = args.map((a): Ast => (hasDice(a) && a.kind !== "shared" ? { kind: "shared", key: ++shared, of: a } : a));
  const given = new Map(rule.params.map((p, i) => [p.toLowerCase(), once[i]]));
  const put = (a: Ast): Ast => {
    switch (a.kind) {
      case "name":
        return given.get(a.name.toLowerCase()) ?? a;
      case "dice":
        return { ...a, count: put(a.count), sides: put(a.sides) };
      case "call":
        return { ...a, args: a.args.map(put) };
      case "each":
        return { ...a, body: put(a.body) };
      case "neg":
        return { ...a, of: put(a.of) };
      case "op":
        return { ...a, left: put(a.left), right: put(a.right) };
      case "list":
        return { ...a, items: a.items.map(put) };
      case "map":
        return { ...a, args: a.args.map(put) };
      default:
        return a;
    }
  };
  return put(body);
}

/** Keys of dice handed to rules — one throw each. */
let shared = 0;

/**
 * Reads a formula — or throws, saying what does not read. The world's
 * `rules` are put in where they are called.
 */
export function parse(formula: string, rules: Rule[] = [], inside: string[] = []): Ast {
  const list = tokens(formula);
  let at = 0;
  const peek = () => list[at];
  const isSym = (v: string) => peek()?.t === "sym" && (peek() as { v: string }).v === v;
  const expect = (v: string) => {
    if (!isSym(v)) throw new Error(`“${v}” is missing`);
    at++;
  };
  /** A comparison, the loosest of all: `Ab Stufe <= Stufe`. */
  const compare = (): Ast => {
    const left = sum();
    const t = peek();
    if (t?.t === "sym" && COMPARE.has(t.v)) {
      at++;
      return { kind: "op", op: t.v as Op, left, right: sum() };
    }
    return left;
  };
  const sum = (): Ast => {
    let left = product();
    while (isSym("+") || isSym("-")) {
      const op = (list[at++] as { v: "+" | "-" }).v;
      left = { kind: "op", op, left, right: product() };
    }
    return left;
  };
  const product = (): Ast => {
    let left = unary();
    while (isSym("*") || isSym("/")) {
      const op = (list[at++] as { v: "*" | "/" }).v;
      left = { kind: "op", op, left, right: unary() };
    }
    return left;
  };
  const unary = (): Ast => {
    if (isSym("-")) {
      at++;
      return { kind: "neg", of: unary() };
    }
    if (isSym("+")) {
      at++;
      return unary();
    }
    // `W(…)` alone is one die.
    if (isSym("d") || isSym("W")) return die({ kind: "number", value: 1 });
    const left = atom();
    return isSym("d") || isSym("W") ? die(left) : left;
  };
  /** A die thrown `count` times, its sides after the letter. */
  const die = (count: Ast): Ast => {
    const letter = (list[at++] as { v: string }).v;
    const sides = atom();
    const next = peek();
    const keep = next?.t === "name" ? KEEP.exec(next.v) : null;
    if (keep) at++;
    return { kind: "dice", count, sides, letter, ...(keep ? { keep: { high: keep[1].toLowerCase() === "h", n: Number(keep[2]) } } : {}) };
  };
  const atom = (): Ast => {
    const t = list[at++];
    if (!t) throw new Error("the formula ends too soon");
    if (t.t === "num") return { kind: "number", value: t.v };
    if (t.t === "dice")
      return {
        kind: "dice",
        count: { kind: "number", value: t.count },
        sides: { kind: "number", value: t.sides },
        letter: t.letter,
        ...(t.keep ? { keep: t.keep } : {}),
      };
    if (t.t === "name") {
      if (Object.prototype.hasOwnProperty.call(FUNCTIONS, t.v.toLowerCase()) && isSym("(")) {
        at++;
        const args = [compare()];
        while (isSym(",")) {
          at++;
          args.push(compare());
        }
        expect(")");
        return { kind: "call", name: t.v.toLowerCase(), args };
      }
      // `map(rule, 5W6, …)`: the rule on each die, each value.
      if (t.v.toLowerCase() === "map" && isSym("(")) {
        at++;
        const named = peek();
        const each = named?.t === "name" ? ruleNamed(rules, named.v) : undefined;
        if (!each) throw new Error("map() takes a rule first, then what to take it to");
        at++;
        const args: Ast[] = [];
        while (isSym(",")) {
          at++;
          args.push(compare());
        }
        expect(")");
        if (args.length !== each.params.length) throw new Error(`${each.name} takes ${each.params.length} values`);
        return { kind: "map", rule: each, rules, args };
      }
      const rule = ruleNamed(rules, t.v);
      if (rule && isSym("(")) {
        at++;
        const args = isSym(")") ? [] : [compare()];
        while (isSym(",")) {
          at++;
          args.push(compare());
        }
        expect(")");
        return applied(rule, args, rules, inside);
      }
      // A property of that name comes first; otherwise the rule — `crit`,
      // or `Strength mod` for `mod(Strength)`.
      if (rule && rule.params.length === 0) return or({ kind: "name", name: t.v }, applied(rule, [], rules, inside));
      const after = /^(.+?)[\s-]+([\p{L}_][\p{L}\p{N}_]*)$/u.exec(t.v);
      const suffix = after && ruleNamed(rules, after[2]);
      if (after && suffix?.params.length === 1)
        return or({ kind: "name", name: t.v }, applied(suffix, [{ kind: "name", name: after[1] }], rules, inside));
      if (isSym(".")) {
        at++;
        expect("(");
        const body = sum();
        expect(")");
        return { kind: "each", ref: t.v, body };
      }
      return { kind: "name", name: t.v };
    }
    if (t.v === "(") {
      const inner = sum();
      expect(")");
      return inner;
    }
    if (t.v === "[") {
      const items: Ast[] = isSym("]") ? [] : [compare()];
      while (isSym(",")) {
        at++;
        items.push(compare());
      }
      expect("]");
      return { kind: "list", items };
    }
    throw new Error(`“${t.v}” stands where a value belongs`);
  };
  if (list.length === 0) throw new Error("the formula is empty");
  const ast = compare();
  if (at < list.length) throw new Error(`“${(list[at] as { v?: unknown }).v ?? "?"}” is one too many`);
  return ast;
}

/** The first that has a number: a property, or else a rule. */
const or = (first: Ast, then: Ast): Ast => ({ kind: "call", name: "or", args: [first, then] });

/** Whether a formula rolls at all — or only reckons. */
export function hasDice(ast: Ast): boolean {
  switch (ast.kind) {
    case "dice":
      return true;
    case "neg":
      return hasDice(ast.of);
    case "op":
      return hasDice(ast.left) || hasDice(ast.right);
    case "call":
      return ast.args.some(hasDice);
    case "each":
      return hasDice(ast.body);
    case "list":
      return ast.items.some(hasDice);
    case "map":
      return ast.args.some(hasDice);
    case "shared":
      return hasDice(ast.of);
    default:
      return false;
  }
}

/**
 * `[[+4]]`, `[[-1]]`, `[[+ Strength mod]]`: a check, as a stat block writes
 * “+4 to hit” — what it rolls, the rule `check` says.
 */
export const isCheck = (formula: string) => /^\s*[+-]/.test(formula);

/** Whether the rules say what a check rolls. */
export const checks = (rules: Rule[] = []) => ruleNamed(rules, "check")?.params.length === 1;

/** A formula read as it rolls: a check through the rule `check`. */
export const parseRoll = (formula: string, rules: Rule[] = []): Ast =>
  parse(isCheck(formula) && checks(rules) ? `check(${formula})` : formula, rules);

/**
 * What a property name is worth here — `undefined` when the document has
 * none. `all` gives a path over several picks pick by pick; `within` the
 * lookups inside each pick of a reference.
 */
export type Lookup = ((name: string) => number | undefined) & {
  all?: (name: string) => number[] | undefined;
  within?: (ref: string) => Lookup[] | undefined;
  /** The picks of a reference with their lookups. */
  scopes?: (ref: string) => Scope[] | undefined;
  /** The ids a reference picks — here, or further out. */
  ids?: (name: string) => string[] | undefined;
  /** The rules of the document's type — and inside a pick, the picker's after them. */
  rules?: Rule[];
  /** Inside a pick: its own number (`with Proficiency`). */
  count?: number;
};

/** A picked document and what names are worth inside it. */
export type Scope = { pick: ArticleView; look: Lookup };

/** One die thrown: its sides and what it showed. */
export type Thrown = { sides: number; value: number; dropped?: true };

export type Rolled = {
  total: number;
  /** Every die, in the order thrown. */
  dice: Thrown[];
  /** The formula with names filled in and dice shown: `[14] + 3`. */
  detail: string;
};

const random = (sides: number): number => {
  const a = new Uint32Array(1);
  // Rejection keeps every side equally likely.
  const limit = Math.floor(0x100000000 / sides) * sides;
  do crypto.getRandomValues(a);
  while (a[0] >= limit);
  return (a[0] % sides) + 1;
};

const MAX_DICE = 200;

/**
 * Reckons a formula. With `roll` the dice are thrown; without, a formula
 * with dice is not a number and throws.
 */
export function reckon(ast: Ast, lookup: Lookup, roll?: (sides: number) => number): Rolled {
  const dice: Thrown[] = [];
  /** Dice handed to rules, thrown once. */
  const memo = new Map<number, W>();
  /** A value — its number, how it came about, and where it is several, each. */
  type W = [number, string, number[]?];
  const walk = (a: Ast): W => {
    switch (a.kind) {
      case "number":
        return [a.value, String(a.value)];
      case "dice": {
        if (!roll) throw new Error("dice are no number");
        const count = Math.floor(walk(a.count)[0]);
        const sides = Math.floor(walk(a.sides)[0]);
        if (sides < 1) throw new Error(`a die has at least one side`);
        if (count < 0) throw new Error(`no die is thrown less than never`);
        if (dice.length + count > MAX_DICE) throw new Error(`more than ${MAX_DICE} dice`);
        const thrown = Array.from({ length: count }, () => roll(sides));
        // Which count: all, or the highest (lowest) `n` — the others are dropped.
        const order = thrown.map((v, i) => [v, i]).sort((x, y) => (a.keep?.high === false ? x[0] - y[0] : y[0] - x[0]));
        const kept = new Set(a.keep ? order.slice(0, a.keep.n).map(([, i]) => i) : thrown.map((_, i) => i));
        thrown.forEach((v, i) => dice.push(kept.has(i) ? { sides, value: v } : { sides, value: v, dropped: true }));
        const each = thrown.filter((_, i) => kept.has(i));
        // A dropped die stands struck through: `[17, 4̶]`.
        const shown = thrown.map((v, i) => (kept.has(i) ? String(v) : [...String(v)].map((c) => c + "\u0336").join("")));
        return [each.reduce((x, y) => x + y, 0), `[${shown.join(", ")}]`, each];
      }
      case "name": {
        const v = lookup(a.name);
        if (v === undefined) throw new Error(`“${a.name}” has no number here`);
        return [v, String(v)];
      }
      case "neg": {
        const [v, d] = walk(a.of);
        return [-v, `-${d}`];
      }
      case "list": {
        const items = a.items.map(walk);
        const each = items.flatMap((w) => w[2] ?? [w[0]]);
        return [each.reduce((x, y) => x + y, 0), `[${items.map((w) => w[1]).join(", ")}]`, each];
      }
      case "shared": {
        const known = memo.get(a.key);
        if (known) return known;
        const w = walk(a.of);
        memo.set(a.key, w);
        return w;
      }
      case "map": {
        // Each value of each list in turn; a single number goes with every one.
        const args = a.args.map(walk);
        const size = Math.max(...args.map((w) => w[2]?.length ?? 1));
        const at = (w: W, i: number) => (w[2] ? w[2][i] : w[0]);
        if (args.some((w) => w[2] && w[2].length !== size)) throw new Error(`${a.rule.name}: the lists are not equally long`);
        const each = Array.from({ length: size }, (_, i) => {
          const r = reckon(applied(a.rule, args.map((w) => ({ kind: "number", value: at(w, i) })), a.rules, []), lookup, roll);
          dice.push(...r.dice);
          return r.total;
        });
        const total = each.reduce((x, y) => x + y, 0);
        return [total, `${args.filter((w) => w[2]).map((w) => w[1]).join(" ")} → ${total}`, each];
      }
      case "call": {
        // `or`: the first that has a number — the rest is not even asked.
        if (a.name === "or") {
          let problem: unknown = new Error("or() needs something to choose from");
          for (const arg of a.args) {
            try {
              return walk(arg);
            } catch (e) {
              problem = e;
            }
          }
          throw problem;
        }
        // `if`: only the branch taken is reckoned — the other may lack a number.
        if (a.name === "if") {
          if (a.args.length !== 3) throw new Error("if() takes a condition, then two values");
          const [c] = walk(a.args[0]);
          return walk(a.args[c !== 0 ? 1 : 2]);
        }
        // Over several picks, or several dice, each is an argument of its own.
        const whole: W[] = [];
        const parts = a.args.flatMap((arg): [number, string][] => {
          if (OVER_PICKS.has(a.name) && arg.kind === "name") {
            const many = lookup.all?.(arg.name);
            if (many) return many.map((v) => [v, String(v)]);
          }
          if (OVER_PICKS.has(a.name) && arg.kind === "each") return within(arg).map((r) => [r.total, r.detail]);
          const w = walk(arg);
          whole.push(w);
          if (OVER_PICKS.has(a.name) && w[2]) return w[2].map((v) => [v, String(v)]);
          return [[w[0], w[1]]];
        });
        if (parts.length === 0 && a.name !== "count" && a.name !== "sum") throw new Error(`${a.name}() has nothing to take`);
        const v = FUNCTIONS[a.name](...parts.map((p) => p[0]));
        // Without dice in it, what a function comes to is all one needs to see;
        // over one throw, the throw.
        if (!hasDice(a)) return [v, String(v)];
        if (whole.length === 1 && a.args.length === 1) return [v, `${whole[0][1]}${whole[0][1].includes("→") ? "" : ` → ${v}`}`];
        return [v, `${a.name}(${parts.map((p) => p[1]).join(", ")})`];
      }
      case "each": {
        const each = within(a);
        return [each.reduce((s, r) => s + r.total, 0), `(${each.map((r) => r.detail).join(" + ") || "0"})`];
      }
      case "op": {
        const [l, ld] = walk(a.left);
        const [r, rd] = walk(a.right);
        const v = arithmetic(a.op, l, r);
        // `[15] + -1` reads `[15] - 1`.
        if (a.op === "+" && rd.startsWith("-")) return [v, `${ld} - ${rd.slice(1)}`];
        return [v, `${ld} ${a.op} ${rd}`];
      }
    }
  };
  /** `Classes.(…)` within each pick — its dice thrown with the rest. */
  const within = (a: Extract<Ast, { kind: "each" }>): Rolled[] => {
    const scopes = lookup.within?.(a.ref);
    if (!scopes) throw new Error(`“${a.ref}” picks nothing here`);
    return scopes.map((scope) => {
      const r = reckon(a.body, scope, roll);
      dice.push(...r.dice);
      return r;
    });
  };
  const [total, detail] = walk(ast);
  return { total, dice, detail };
}

/** What an operation comes to: comparisons are 1 or 0. Division rounds down. */
function arithmetic(op: Op, l: number, r: number): number {
  switch (op) {
    case "+":
      return l + r;
    case "-":
      return l - r;
    case "*":
      return l * r;
    case "/":
      return r === 0 ? 0 : Math.floor(l / r);
    case "<":
      return l < r ? 1 : 0;
    case "<=":
      return l <= r ? 1 : 0;
    case ">":
      return l > r ? 1 : 0;
    case ">=":
      return l >= r ? 1 : 0;
    case "==":
      return l === r ? 1 : 0;
    case "!=":
      return l !== r ? 1 : 0;
  }
}

/**
 * How a formula reads with the names filled in: `1d20 + Strength mod` →
 * `1d20 + 3`. The dice stay dice. `null` when a name has no number.
 */
export function written(ast: Ast, lookup: Lookup): string | null {
  const rank = (op: string) => (COMPARE.has(op) ? 0 : op === "+" || op === "-" ? 1 : 2);
  const number = (a: Ast) => {
    try {
      return String(reckon(a, lookup).total);
    } catch {
      return null;
    }
  };
  /** `outer`: the operation it stands in, and whether on its right. */
  const walk = (a: Ast, outer?: { op: string; right: boolean }): string | null => {
    // What has no dice in it is reckoned — `10 + 3` reads `13`.
    if (!hasDice(a) && a.kind !== "number") return number(a);
    switch (a.kind) {
      case "number":
        return String(a.value);
      case "dice": {
        const count = number(a.count);
        const sides = number(a.sides);
        const keep = a.keep ? `k${a.keep.high ? "h" : "l"}${a.keep.n}` : "";
        return count === null || sides === null ? null : `${count}${a.letter}${sides}${keep}`;
      }
      case "name":
        return number(a);
      case "neg": {
        const inner = walk(a.of, { op: "*", right: true });
        return inner === null ? null : `-${inner}`;
      }
      case "list": {
        const items = a.items.map((x) => walk(x));
        return items.includes(null) ? null : `[${items.join(", ")}]`;
      }
      case "shared":
        return walk(a.of, outer);
      // A rule on each die reads as the dice: `5W6`.
      case "map": {
        const thrown = a.args.filter(hasDice).map((x) => walk(x));
        return thrown.includes(null) ? null : thrown.join(", ");
      }
      case "call": {
        // Over one throw — `sum(map(hit, 5W6))` — the throw.
        if (OVER_PICKS.has(a.name) && a.args.length === 1) return walk(a.args[0], outer);
        // `if` and `or` read as the branch they take.
        if (a.name === "if" && a.args.length === 3) {
          const c = number(a.args[0]);
          return c === null ? null : walk(a.args[Number(c) !== 0 ? 1 : 2], outer);
        }
        if (a.name === "or") {
          for (const arg of a.args) {
            const w = walk(arg, outer);
            if (w !== null) return w;
          }
          return null;
        }
        const args = a.args.map((x) => walk(x));
        return args.includes(null) ? null : `${a.name}(${args.join(", ")})`;
      }
      // Within each pick, written as each pick has it: `3W10 + 2W8`.
      case "each": {
        const parts = (lookup.within?.(a.ref) ?? []).map((scope) => written(a.body, scope));
        if (parts.length === 0 || parts.includes(null)) return null;
        const text = parts.join(" + ");
        // A sum: in brackets only where a sum needs them.
        return outer && parts.length > 1 && (rank(outer.op) > 1 || (outer.right && outer.op === "-")) ? `(${text})` : text;
      }
      case "op": {
        // A sum with dice: the dice as written, the numbers added up into
        // one — `2W20kh1 + 3 + 2 + 2` reads `2W20kh1 + 7`.
        if (a.op === "+" || a.op === "-") {
          const terms: [number, Ast][] = [];
          const flat = (x: Ast, sign: number) => {
            if (x.kind === "op" && (x.op === "+" || x.op === "-")) {
              flat(x.left, sign);
              flat(x.right, x.op === "-" ? -sign : sign);
            } else terms.push([sign, x]);
          };
          flat(a, 1);
          const loose = terms.filter(([, x]) => !hasDice(x));
          if (loose.length > 0 && loose.length < terms.length) {
            let sum = 0;
            for (const [sign, x] of loose) {
              const n = number(x);
              if (n === null) return null;
              sum += sign * Number(n);
            }
            const parts: string[] = [];
            for (const [sign, x] of terms.filter(([, x]) => hasDice(x))) {
              const w = walk(x, { op: "+", right: parts.length > 0 });
              if (w === null) return null;
              parts.push(parts.length === 0 ? (sign < 0 ? `-${w}` : w) : `${sign < 0 ? "-" : "+"} ${w}`);
            }
            if (sum !== 0) parts.push(`${sum < 0 ? "-" : "+"} ${Math.abs(sum)}`);
            const text = parts.join(" ");
            return outer && (rank(outer.op) > 1 || (outer.right && outer.op === "-")) ? `(${text})` : text;
          }
        }
        const l = walk(a.left, { op: a.op, right: false });
        const r = walk(a.right, { op: a.op, right: true });
        if (l === null || r === null) return null;
        // `2d6 + -1` reads `2d6 - 1`.
        const text = a.op === "+" && r.startsWith("-") ? `${l} - ${r.slice(1)}` : `${l} ${a.op} ${r}`;
        const bracket =
          outer && (rank(a.op) < rank(outer.op) || (outer.right && rank(a.op) === rank(outer.op) && (outer.op === "-" || outer.op === "/")));
        return bracket ? `(${text})` : text;
      }
    }
  };
  return walk(ast);
}

/** A modifier as a stat block writes it: `+3`, `-1`, `+0`. */
export const signed = (n: number) => (n < 0 ? `−${-n}` : `+${n}`);

/** What the rules make of a check: a critical, a fumble, a success or a failure. */
export type Mark = "crit" | "fumble" | "success" | "failure";

/** `check(Mut, Klugheit, Intuition, 7)`: a check of several values, called by name. */
const calledCheck = (formula: string) => /^\s*check\s*\(/i.test(formula);

/** Whether a roll is a check: `[[+4]]` where the rules have `check`, or `[[check(…)]]`. */
export const isJudged = (formula: string, rules: Rule[] = []) =>
  (isCheck(formula) && checks(rules)) || (calledCheck(formula) && !!ruleNamed(rules, "check"));

/**
 * What a check shows before it is rolled: its value, signed — `+4` — or
 * what the rule `shown` makes of the same values, unsigned: `shown(x) = x`
 * for a skill rolled under, `shown(a, b, c, fw) = fw` for three dice
 * against three values. `undefined`: nothing to show here.
 */
export function checkShown(formula: string, rules: Rule[], lookup: Lookup): string | undefined {
  const shownRule = ruleNamed(rules, "shown");
  try {
    if (calledCheck(formula)) {
      if (!shownRule) return undefined;
      return String(reckon(parse(formula.replace(/^\s*check/i, "shown"), rules), lookup).total);
    }
    const x = reckon(parse(formula, rules), lookup).total;
    return shownRule?.params.length === 1 ? String(reckon(parse(`shown(${formula})`, rules), lookup).total) : signed(x);
  } catch {
    return undefined;
  }
}

/**
 * Rolls what a roll in the text asks for: a check (`+4` → `check(4)`, say
 * `1d20 + 4`) or the formula itself.
 *
 * A check is then judged by the rules, each on `d` — the dice it threw, the
 * dropped ones left out: as a number their sum, to `map` and `count` each
 * die — `x`, the value checked, and `r`, what the check came to:
 *
 * - `crit = 20` / `fumble = 1`: the dice at that or beyond; `success = 10`:
 *   the result at least that;
 * - `crit(d, x) = d <= x / 5`, `fumble(d) = d >= 96`,
 *   `success(d, x) = d <= x`: rolling under a skill;
 * - `success(d, x, r) = r >= 0`, `crit(d) = count(map(one, d)) >= 2`: three
 *   dice against three values, pools that count what shows six.
 */
export function rollFormula(
  formula: string,
  lookup: Lookup,
  roller: (sides: number) => number = random,
): Rolled & { formula: string; mark?: Mark } {
  const rules = lookup.rules ?? [];
  const ast = parseRoll(formula, rules);
  const shown = written(ast, lookup) ?? formula;
  const rolled = reckon(ast, lookup, roller);
  if (!isJudged(formula, rules) || rolled.dice.length === 0) return { ...rolled, formula: shown };
  const kept = rolled.dice.filter((t) => !t.dropped).map((t) => t.value);
  const d = kept.reduce((s, v) => s + v, 0);
  let x = 0;
  if (isCheck(formula)) {
    try {
      x = reckon(parse(formula, rules), lookup).total;
    } catch {
      // A check on a name without a number has rolled anyway.
    }
  }
  const r = rolled.total;
  /** Whether the rule says yes — `undefined` where the world has no such rule. */
  const says = (name: string, threshold: (limit: number) => boolean): boolean | undefined => {
    const rule = ruleNamed(rules, name);
    if (!rule || rule.params.length > 3) return undefined;
    try {
      if (rule.params.length === 0) return threshold(reckon(parse(rule.formula, rules), lookup).total);
      const values: Ast[] = [
        { kind: "list", items: kept.map((value) => ({ kind: "number", value })) },
        { kind: "number", value: x },
        { kind: "number", value: r },
      ];
      return reckon(applied(rule, values.slice(0, rule.params.length), rules, []), lookup).total !== 0;
    } catch {
      return undefined;
    }
  };
  const success = says("success", (limit) => r >= limit);
  const mark: Mark | undefined = says("crit", (limit) => d >= limit)
    ? "crit"
    : says("fumble", (limit) => d <= limit)
      ? "fumble"
      : success === undefined
        ? undefined
        : success
          ? "success"
          : "failure";
  return { ...rolled, formula: shown, ...(mark ? { mark } : {}) };
}
