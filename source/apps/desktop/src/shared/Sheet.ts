import type { ArticleView, Template, TemplateField } from "../api/client";
import { idsIn, isA, picksFor, propertyNamed, propertyValues, typeOf } from "./types";
import { parse, reckon, type Lookup, type Scope } from "./Dice";
import { perLists } from "./Cache";

/**
 * Numbers that others change — a character's sheet.
 *
 * A number says what it does by its operator:
 *
 * - `Strength: 15` — the document's own value, a score as rolled;
 * - `Strength: +1` — raises it: a feat;
 * - `Strength: -1` — lowers it: a curse;
 * - `Strength: =19` — sets it, whatever it was;
 * - `Strength: >=19` — sets it to at least 19: gauntlets of ogre power;
 * - `Strength: <=3` — to at most 3.
 *
 * Operators act where a reference marked `adds` (`Feats: @@Feat@@ multiple
 * adds`) picks the document: on the character's Strength, wherever it shows —
 * the infobox, `%%Strength%%`, the ability row, every formula. Matched by
 * name, as text names properties. The score is own + what raises and
 * lowers it — or what it is set to, by the last pick that sets it — then
 * held to the highest “at least” and the lowest “at most”. What a pick
 * picks in turn counts too: a belt that grants a feat. What a thing does is
 * the world's to say: the gauntlets are `>=19` because that is what they
 * do, not because the app knows them.
 *
 * A number may be reckoned: a default in `[[…]]` — `Hit points: number =
 * [[Class.Hit die + (Level - 1) * (Class.Hit die / 2 + 1)]]` — is no value a
 * new document copies but a formula. A document without a value of its own
 * has what it comes to, from its own numbers and, over a dot, from what it
 * picks. Feats add to it as to any own value. A value of its own — rolled
 * hit points — stands instead.
 */

/** Where a share of a number comes from. */
export type Share = { from: ArticleView; amount: number };

export type Sum = {
  /** What the document itself says — `undefined` when it says nothing. */
  own: number | undefined;
  /** Whether `own` was reckoned from the property's formula. */
  reckoned: boolean;
  /** What picks raise or lower it by. */
  shares: Share[];
  /** What picks set it to — the last one counts. */
  sets: Share[];
  /** What picks set it to at least. */
  floors: Share[];
  /** What picks set it to at most. */
  ceilings: Share[];
  /** `undefined` only when nothing at all gives a number. */
  total: number | undefined;
};

/** A stored number, read: its operator — none for a bare value — and its amount. */
export type Op = "" | "+" | "-" | "=" | ">=" | "<=";

const OPS: [string, Op][] = [[">=", ">="], ["≥", ">="], ["<=", "<="], ["≤", "<="], ["+", "+"], ["-", "-"], ["=", "="]];

export function readNumber(v: unknown): { op: Op; amount: number } | undefined {
  if (typeof v === "number") return Number.isFinite(v) ? { op: "", amount: v } : undefined;
  if (typeof v !== "string" || !v.trim()) return undefined;
  const t = v.trim();
  const [sign, op] = OPS.find(([sign]) => t.startsWith(sign)) ?? ["", ""];
  const amount = Number(t.slice(sign.length).trim());
  return Number.isFinite(amount) ? { op, amount } : undefined;
}

/** A number read on its own: `-5` is −5, `=19` is 19. */
const plainly = (n: ReturnType<typeof readNumber>) => (n ? (n.op === "-" ? -n.amount : n.amount) : undefined);

/** The references of a type whose picks change its numbers. */
const adding = (type: Template | undefined) =>
  (type?.properties ?? []).filter((f): f is Extract<TemplateField, { kind: "reference" }> => f.kind === "reference" && !!f.adds);

/** A number property of a document, read with its operator. */
function numberIn(templates: Template[], article: ArticleView, name: string) {
  const field = propertyNamed(typeOf(templates, article.articleType), name);
  return field?.kind === "number" ? readNumber(propertyValues(article)[field.key]) : undefined;
}

/** What stands in `[[…]]` — a formula — or `undefined`. */
export const formulaIn = (v: unknown) => (typeof v === "string" ? /^\s*\[\[([\s\S]*)\]\]\s*$/.exec(v)?.[1] : undefined);

/** The formula of a reckoned number property — its default in `[[…]]`. */
export function formulaOf(field: TemplateField | undefined): string | undefined {
  if (field?.kind !== "number") return undefined;
  return formulaIn(field.default ?? "");
}

/**
 * What a document's formula for a number comes to — `undefined` when it has
 * none, or a name in it has no number here. The formula is the document's
 * own value where that is one (`Base: [[Dexterity mod]]`), else its type's
 * default. `busy` holds what is being reckoned already: a formula that needs
 * itself gives nothing.
 *
 * Inside a pick it reckons for whoever picked it: a skill's
 * `[[Dexterity mod]]` is the dexterity of the character that has the skill.
 */
function reckoned(templates: Template[], articles: ArticleView[], article: ArticleView, name: string, busy: Set<string>, inside?: Inside) {
  const field = propertyNamed(typeOf(templates, article.articleType), name);
  if (field?.kind !== "number") return undefined;
  const formula = formulaIn(propertyValues(article)[field.key]) ?? formulaOf(field);
  const key = `${article.id}\u0000${name.toLowerCase()}`;
  if (formula === undefined || busy.has(key)) return undefined;
  try {
    const look = lookupIn(templates, articles, article, new Set(busy).add(key), inside);
    return reckon(parse(formula, look.rules), look).total;
  } catch {
    return undefined;
  }
}

/**
 * What a document's picks do to one of its numbers — and, through theirs,
 * what their picks do. `seen` guards against picks that pick each other.
 */
function effects(templates: Template[], articles: ArticleView[], article: ArticleView, name: string, seen: Set<string>) {
  const shares: Share[] = [];
  const sets: Share[] = [];
  const floors: Share[] = [];
  const ceilings: Share[] = [];
  seen.add(article.id);
  for (const ref of adding(typeOf(templates, article.articleType))) {
    for (const id of idsIn(article, ref.key)) {
      if (seen.has(id)) continue;
      const pick = articles.find((a) => a.id === id);
      if (!pick) continue;
      const n = numberIn(templates, pick, name);
      const inner = effects(templates, articles, pick, name, new Set(seen));
      // A bare number is the pick's own and changes nothing here.
      const amount = (n?.op === "+" ? n.amount : n?.op === "-" ? -n.amount : 0) + inner.shares.reduce((s, x) => s + x.amount, 0);
      if (amount !== 0) shares.push({ from: pick, amount });
      if (n?.op === "=") sets.push({ from: pick, amount: n.amount });
      if (n?.op === ">=") floors.push({ from: pick, amount: n.amount });
      if (n?.op === "<=") ceilings.push({ from: pick, amount: n.amount });
      sets.push(...inner.sets);
      floors.push(...inner.floors);
      ceilings.push(...inner.ceilings);
    }
  }
  return { shares, sets, floors, ceilings };
}

/** A number property of a document, by name, with all its picks do to it. */
export function sumOf(templates: Template[], articles: ArticleView[], article: ArticleView, name: string, busy = new Set<string>(), inside?: Inside): Sum {
  const written = plainly(numberIn(templates, article, name));
  const formula = written === undefined ? reckoned(templates, articles, article, name, busy, inside) : undefined;
  const own = written ?? formula;
  const { shares, sets, floors, ceilings } = effects(templates, articles, article, name, new Set());
  // Raising needs something to raise: an NPC without Strength does not get
  // one from being human. What sets it stands on its own.
  const added = own === undefined ? undefined : own + shares.reduce((s, x) => s + x.amount, 0);
  let total = sets.length ? sets[sets.length - 1].amount : added;
  if (floors.length) total = Math.max(total ?? -Infinity, ...floors.map((f) => f.amount));
  if (ceilings.length) total = Math.min(total ?? Infinity, ...ceilings.map((f) => f.amount));
  return { own, reckoned: formula !== undefined, shares, sets, floors, ceilings, total };
}

/** `15 + 1 Athlete, at least 19 Gauntlets` — how a number came about, for a tooltip. */
export function explain(sum: Sum): string {
  const others = sum.sets.length + sum.floors.length + sum.ceilings.length;
  if (sum.shares.length === 0 && others === 0) return sum.reckoned ? `${sum.own} by formula` : "";
  const parts = [`${sum.own ?? 0}${sum.reckoned ? " by formula" : ""}`, ...sum.shares.map((s) => `${s.amount < 0 ? "−" : "+"} ${Math.abs(s.amount)} ${s.from.name}`)];
  return [
    parts.join(" "),
    ...sum.sets.map((f) => `= ${f.amount} ${f.from.name}`),
    ...sum.floors.map((f) => `at least ${f.amount} ${f.from.name}`),
    ...sum.ceilings.map((f) => `at most ${f.amount} ${f.from.name}`),
  ].join(", ");
}

/**
 * What a name in a formula is worth for this document: a number property
 * with what is added to it. The lookup carries the rules of its type.
 */
export function lookupFor(templates: Template[], articles: ArticleView[], article: ArticleView) {
  return lookupIn(templates, articles, article, new Set());
}

/** Inside a pick: its own number, by its name, and the document that picked it. */
type Inside = { count?: [string, number]; outer: Lookup };

/** The first dot outside brackets: `Classes[Class].Level` → `Classes[Class]`, `Level`. */
function splitPath(name: string): [string, string] | undefined {
  let depth = 0;
  for (let i = 0; i < name.length; i++) {
    if (name[i] === "[") depth++;
    else if (name[i] === "]") depth--;
    else if (name[i] === "." && depth === 0 && i > 0) return [name.slice(0, i), name.slice(i + 1)];
  }
  return undefined;
}

/**
 * The lookup of one document. Inside a pick it knows the pick's number, the
 * pick's properties, and — what it does not know — the picker's, and so on
 * out. Its rules are its type's, then the picker's.
 *
 * A path may choose among the picks: `Classes[Class].Level` is the level in
 * the class that `Class` picks — a reference, here or further out — and
 * `Skills[Perception].Bonus` the bonus of the skill so named.
 */
function lookupIn(templates: Template[], articles: ArticleView[], article: ArticleView, busy: Set<string>, inside?: Inside): Lookup {
  const type = typeOf(templates, article.articleType);
  const ownRules = type?.rules ?? [];
  const rules = [...ownRules, ...(inside?.outer.rules ?? []).filter((r) => !ownRules.some((o) => o.name.toLowerCase() === r.name.toLowerCase()))];
  /** The documents a reference of ours picks — `undefined` where we have none so named. */
  const ownIds = (name: string): string[] | undefined => {
    const field = propertyNamed(type, name);
    if (field?.kind !== "reference" && field?.kind !== "relation") return undefined;
    return picksFor(templates, articles, article, field).map((p) => p.id);
  };
  const ids = (name: string): string[] | undefined => ownIds(name) ?? inside?.outer.ids?.(name);
  /**
   * `[Class]` → `[=id]`: what a choice names is settled where the formula
   * stands, before the path is followed outward.
   */
  const pin = (name: string) =>
    name.replace(/\[([^\]=][^\]]*)\]/g, (_, sel: string) => {
      const chosen = ids(sel.trim()) ?? articles.filter((a) => a.name.toLowerCase() === sel.trim().toLowerCase()).map((a) => a.id);
      return `[=${chosen.join(",")}]`;
    });
  /** The picks of a reference of ours, each with its lookup — `[=ids]` keeps those. */
  const ownScopes = (ref: string): Scope[] | undefined => {
    const m = /^(.*?)\s*\[=([^\]]*)\]$/.exec(ref.trim());
    const name = m ? m[1].trim() : ref.trim();
    const only = m ? m[2].split(",").filter(Boolean) : undefined;
    const field = propertyNamed(type, name);
    if (field?.kind !== "reference" && field?.kind !== "relation") return undefined;
    const counted = field.kind === "reference" ? (field.count ?? "").trim().toLowerCase() : "";
    return picksFor(templates, articles, article, field)
      .filter((p) => !only || only.includes(p.id))
      .flatMap((p) => {
        const pick = articles.find((a) => a.id === p.id);
        if (!pick) return [];
        const count: [string, number] | undefined = counted ? [counted, p.count ?? 0] : undefined;
        return [{ pick, look: lookupIn(templates, articles, pick, busy, { count, outer: look }) }];
      });
  };
  const scopes = (ref: string): Scope[] | undefined => ownScopes(ref) ?? inside?.outer.scopes?.(ref);
  // `Class.Hit die`: through a reference, pick by pick.
  const each = (name: string): number[] | undefined => {
    const path = splitPath(name);
    if (!path) return undefined;
    const found = ownScopes(path[0]);
    if (!found) return undefined;
    return found.map((x) => x.look(path[1])).filter((v): v is number => v !== undefined);
  };
  const value = (name: string): number | undefined => {
    if (inside?.count && name.toLowerCase() === inside.count[0]) return inside.count[1];
    const many = each(name);
    if (many) return many.length ? many.reduce((a, b) => a + b, 0) : undefined;
    return sumOf(templates, articles, article, name, busy, inside).total;
  };
  const look: Lookup = Object.assign(
    (name: string) => {
      const n = pin(name.trim());
      return value(n) ?? inside?.outer(n);
    },
    {
      all: (name: string) => {
        const n = pin(name.trim());
        const many = each(n);
        if (many) return many;
        const one = value(n);
        if (one !== undefined) return [one];
        return inside?.outer.all?.(n);
      },
      within: (ref: string) => scopes(pin(ref))?.map((x) => x.look),
      scopes: (ref: string) => scopes(pin(ref)),
      ids,
      rules,
      count: inside?.count?.[1],
    },
  );
  return look;
}

/**
 * What a document picks along a path — `Classes`, or `Classes.Features` —
 * each with its lookup: inside it the pick's number and properties count,
 * then those of every pick on the way, then the document's.
 */
export function picksAlong(templates: Template[], articles: ArticleView[], article: ArticleView, path: string): Scope[] | undefined {
  let found: Scope[] = [{ pick: article, look: lookupFor(templates, articles, article) }];
  let rest: string | undefined = path.trim();
  while (rest) {
    const split = splitPath(rest);
    const step: string = split ? split[0] : rest;
    rest = split?.[1];
    const next: Scope[] = [];
    for (const s of found) {
      const inner = s.look.scopes?.(step);
      if (!inner) return undefined;
      next.push(...inner);
    }
    found = next;
  }
  return found;
}

/**
 * The property values of a document with every number summed up — what
 * reading shows. Keys as stored.
 */
export function summedValues(templates: Template[], articles: ArticleView[], article: ArticleView): Record<string, unknown> {
  const values = { ...propertyValues(article) };
  const type = typeOf(templates, article.articleType);
  // `Next: @@Session log@@ from Previous` — what points here, never stored.
  for (const f of type?.properties ?? []) {
    if (f.kind !== "reference" || !f.from) continue;
    const ids = pointingAt(articles, templates)(f.articleType, f.from).get(article.id) ?? [];
    values[f.key] = f.multiple ? ids : ids[0];
  }
  if (!type || (!adding(type).length && !type.properties.some((f) => formulaOf(f) !== undefined))) return values;
  for (const f of type.properties) {
    if (f.kind !== "number") continue;
    // What a value does to others stays as written: gauntlets set Strength
    // to at least 19 — they do not have 19.
    if (readNumber(values[f.key])?.op) continue;
    const total = sumOf(templates, articles, article, f.label).total;
    if (total !== undefined) values[f.key] = total;
  }
  return values;
}

/**
 * Which documents of a type point at which, by a property of theirs — an
 * index made once per list: `(Session log, Previous)` gives each session the
 * one whose Previous it is.
 */
const pointingAt = perLists((articles: ArticleView[], templates: Template[]) => {
  const made = new Map<string, Map<string, string[]>>();
  return (of: string, property: string): Map<string, string[]> => {
    const key = `${of}\u0000${property.toLowerCase()}`;
    let index = made.get(key);
    if (index) return index;
    index = new Map();
    for (const a of [...articles].sort((x, y) => x.name.localeCompare(y.name, undefined, { numeric: true }))) {
      if (of && !isA(templates, a.articleType, of)) continue;
      const field = propertyNamed(typeOf(templates, a.articleType), property);
      if (!field) continue;
      for (const target of idsIn(a, field.key)) {
        const list = index.get(target) ?? [];
        list.push(a.id);
        index.set(target, list);
      }
    }
    made.set(key, index);
    return index;
  };
});
