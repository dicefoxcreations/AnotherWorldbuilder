import { PROPERTIES, type ArticleView, type Template, type TemplateField } from "../api/client";
import { perList, perLists } from "./Cache";

/**
 * Comparing names as the language does — made once: `localeCompare` with
 * options builds a collator on every call, and sorting thousands of names
 * calls it hundreds of thousands of times.
 */
const plain = new Intl.Collator().compare;
/** `Session 9` before `Session 10`. */
const numeric = new Intl.Collator(undefined, { numeric: true }).compare;

/**
 * Types among each other: supertypes, inheritance and the parent axis.
 *
 * The same rules as `aw_model::types` in Rust — there they are checked
 * before saving, here they only steer the display. Walking up always stops
 * on a cycle or a missing type: a damaged file must not hang the interface.
 */

export const findType = (all: Template[], id: string) => all.find((t) => t.id === id);

/** The supertypes above `start`, the direct one first. */
export function ancestors(all: Template[], start: Template): Template[] {
  const out: Template[] = [];
  let next = start.supertype;
  while (next) {
    const t = findType(all, next);
    if (!t || t.id === start.id || out.some((o) => o.id === t.id)) break;
    out.push(t);
    next = t.supertype;
  }
  return out;
}

/** Whether the type `id` is `ancestor` or lies below it. */
export function isA(all: Template[], id: string, ancestor: string): boolean {
  if (id === ancestor) return true;
  const known = isAKnown(all);
  const key = `${id} ${ancestor}`;
  let answer = known.get(key);
  if (answer === undefined) {
    const t = findType(all, id);
    answer = !!t && ancestors(all, t).some((a) => a.id === ancestor);
    known.set(key, answer);
  }
  return answer;
}
const isAKnown = perList((_: Template[]) => new Map<string, boolean>());

/**
 * The documents of a type and those below it, by name — what a field that
 * picks one offers. Worked out once per list, see `shared/Cache`.
 */
export function documentsOf(all: ArticleView[], templates: Template[], type: string): ArticleView[] {
  const known = documentsKnown(all, templates);
  let found = known.get(type);
  if (!found) {
    found = all.filter((a) => isA(templates, a.articleType, type)).sort((a, b) => plain(a.name, b.name));
    known.set(type, found);
  }
  return found;
}
const documentsKnown = perLists((_a: ArticleView[], _t: Template[]) => new Map<string, ArticleView[]>());

/** The direct subtypes of a type. */
export const subtypes = (all: Template[], id: string) => all.filter((t) => t.supertype === id);

/** The id of the topmost type above `start` — or its own. */
export function rootId(all: Template[], start: Template): string {
  const chain = ancestors(all, start);
  return chain.length > 0 ? chain[chain.length - 1].id : start.id;
}

/**
 * Fields with their groups unfolded: in place of `Stats: group Abilities`
 * stand the abilities, each marked with the path of groups it lies in —
 * `["Stats"]`, or `["Values", "Stats"]` for a group in a group. One that
 * contains itself, or is gone, gives nothing.
 */
export function unfold(all: Template[], fields: TemplateField[]): TemplateField[] {
  const out: TemplateField[] = [];
  const push = (field: TemplateField, path: string[], seen: string[]) => {
    if (field.kind !== "group") {
      out.push(path.length ? { ...field, inGroup: path } : field);
      return;
    }
    const g = findType(all, field.group);
    if (!g?.group || seen.includes(g.id)) return;
    for (const member of g.properties) push(member, [...path, field.label], [...seen, g.id]);
  };
  for (const f of fields) push(f, [], []);
  return out;
}

/** The types, without the property groups — what documents can be of. */
export const realTypes = (all: Template[]) => all.filter((t) => !t.group);

/** The types that take a group in, directly or through other groups. */
export function usersOf(all: Template[], group: string): Template[] {
  return all.filter((t) => t.id !== group && unfoldsTo(all, t, group, []));
}

/**
 * Whether documents of a type have a group's properties — taken in by the
 * type, a supertype, or a group they take in.
 */
export function takesIn(all: Template[], typeId: string, group: string): boolean {
  const t = findType(all, typeId);
  if (!t) return false;
  return [t, ...ancestors(all, t)].some((x) => x.id === group || unfoldsTo(all, x, group, []));
}

function unfoldsTo(all: Template[], t: Template, group: string, seen: string[]): boolean {
  return t.properties.some((f) => {
    if (f.kind !== "group" || seen.includes(f.group)) return false;
    if (f.group === group) return true;
    const g = findType(all, f.group);
    return !!g && unfoldsTo(all, g, group, [...seen, f.group]);
  });
}

/** The groups a field list takes in, directly or through other groups. */
function groupsIn(all: Template[], fields: TemplateField[], seen: string[]) {
  for (const f of fields) {
    if (f.kind !== "group" || seen.includes(f.group)) continue;
    seen.push(f.group);
    const g = findType(all, f.group);
    if (g) groupsIn(all, g.properties, seen);
  }
}

/** The type with everything it inherits — the general before the specific. */
export function effective(all: Template[], template: Template): Template {
  const chain = [...ancestors(all, template).reverse(), template];
  // The infobox is laid out as a whole: the nearest type that shows
  // anything decides.
  const shaper = [...chain].reverse().find((t) => t.infobox.show.length > 0);
  // The rules: the supertypes', the type's own, and those of every group
  // it takes in.
  const groups: string[] = [];
  for (const t of chain) groupsIn(all, t.properties, groups);
  const rulers = [...chain, ...groups.flatMap((g) => findType(all, g) ?? [])];
  return {
    ...template,
    properties: unfold(all, chain.flatMap((t) => t.properties)),
    rules: rulers.flatMap((t) => t.rules ?? []),
    infobox: shaper?.infobox ?? template.infobox,
    blocks: inOrder(
      chain.flatMap((t) => t.blocks),
      [...chain].reverse().find((t) => t.order?.length)?.order ?? [],
    ),
  };
}

/** One step of an order: a property's key — or `name` — and which way. */
export type SortKey = { key: string; desc?: boolean };

/** The order a type sets for its documents — the nearest type that sets one. */
export function typeSort(all: Template[], typeId: string | undefined): SortKey[] {
  const type = typeId ? findType(all, typeId) : undefined;
  const keys = type ? ([...ancestors(all, type), type].reverse().find((t) => t.sort?.length)?.sort ?? []) : [];
  return keys.map((key) => ({ key }));
}

/**
 * `sort="Role desc, name"` as keys of the listed type — what it does not
 * have is left out.
 */
export function sortWritten(type: Template | undefined, written: string[]): SortKey[] {
  return written.flatMap((w) => {
    const desc = /\s+desc$/i.test(w);
    const name = w.replace(/\s+(desc|asc)$/i, "").trim();
    if (name.toLowerCase() === "name") return [{ key: "name", desc }];
    // `Type`: by the name of each document's own type — a subtype's.
    if (name.toLowerCase() === "type" && !propertyNamed(type, name)) return [{ key: "type", desc }];
    const field = propertyNamed(type, name);
    return field ? [{ key: field.key, desc }] : [];
  });
}

/**
 * How documents of a type line up: by the properties its sort names — the
 * nearest type that sets one — then by name, numbers as numbers. A pick
 * goes by where the picked document stands among its own kind: events by
 * their session. A value one lacks puts it after those that have it.
 */
export function documentOrder(
  all: Template[],
  typeId: string | undefined,
  articles: ArticleView[] = [],
  depth = 0,
): (a: ArticleView, b: ArticleView) => number {
  return orderBy(all, articles, typeSort(all, typeId), depth);
}

/**
 * Documents in the order the keys give, one after the other, then by name.
 * `desc` turns a key round; one who lacks the value still comes last.
 */
export function orderBy(
  all: Template[],
  articles: ArticleView[],
  keys: SortKey[],
  depth = 0,
): (a: ArticleView, b: ArticleView) => number {
  const byName = (a: ArticleView, b: ArticleView) => numeric(a.name, b.name);
  if (keys.length === 0 || depth > 2) return byName;
  const byId = new Map(articles.map((a) => [a.id, a]));
  const picked = (v: unknown) => (typeof v === "string" ? byId.get(v) : Array.isArray(v) && typeof v[0] === "string" ? byId.get(v[0]) : undefined);
  const valueOf = (a: ArticleView, key: string): unknown =>
    key === "name" ? a.name : key === "type" ? (findType(all, a.articleType)?.name ?? "") : propertyValues(a)[key];
  const has = (v: unknown) => typeof v === "number" || (typeof v === "string" && v !== "") || (Array.isArray(v) && v.length > 0);
  const nested = new Map<string, (a: ArticleView, b: ArticleView) => number>();
  const orderOf = (typeId: string) => {
    if (!nested.has(typeId)) nested.set(typeId, documentOrder(all, typeId, articles, depth + 1));
    return nested.get(typeId)!;
  };
  return (a, b) => {
    for (const { key, desc } of keys) {
      const x = valueOf(a, key);
      const y = valueOf(b, key);
      if (!has(x) || !has(y)) {
        if (has(x) !== has(y)) return has(x) ? -1 : 1;
        continue;
      }
      const [p, q] = [picked(x), picked(y)];
      const d =
        p && q
          ? p.articleType === q.articleType
            ? orderOf(p.articleType)(p, q)
            : byName(p, q)
          : typeof x === "number" && typeof y === "number"
            ? x - y
            : numeric(String(x), String(y));
      if (d !== 0) return desc ? -d : d;
    }
    return byName(a, b);
  };
}

/** Blocks in an order: those it names as it names them, the rest after them as they came. */
export function inOrder<B extends { key: string }>(blocks: B[], order: string[]): B[] {
  const rank = (b: B) => {
    const i = order.indexOf(b.key);
    return i < 0 ? order.length : i;
  };
  return blocks
    .map((b, i) => [b, i] as const)
    .sort(([a, i], [b, j]) => rank(a) - rank(b) || i - j)
    .map(([b]) => b);
}

/** The effective type of an article, if its template exists. */
export function typeOf(all: Template[], articleType: string): Template | undefined {
  const known = typesKnown(all);
  if (!known.has(articleType)) {
    const t = findType(all, articleType);
    known.set(articleType, t && effective(all, t));
  }
  return known.get(articleType);
}
const typesKnown = perList((_: Template[]) => new Map<string, Template | undefined>());

/** The field that is the axis of the tree. */
export const parentField = (effectiveType: Template | undefined) =>
  effectiveType?.properties.find(
    (f): f is Extract<TemplateField, { kind: "reference" }> => f.kind === "reference" && f.parent,
  );

/**
 * The type a reference field points at.
 *
 * A bare `parent` names no type: it points at the root of the owning type's
 * tree — a city lies in a place.
 */
export function targetOf(all: Template[], ownerType: string, field: TemplateField): string {
  if (field.kind === "relation") return field.articleType;
  if (field.kind !== "reference") return "";
  if (field.articleType) return field.articleType;
  const owner = findType(all, ownerType);
  return owner ? rootId(all, owner) : "";
}

/**
 * Whether a layer carries property values: the common one, or a property's
 * own (`properties:<key>`) when it has access of its own.
 */
export const isProperties = (section: string) => section === PROPERTIES || section.startsWith(`${PROPERTIES}:`);

/** The property values this reader has, by key — from whichever layers hold them. */
export function propertyValues(article: ArticleView): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const layer of article.layers) if (isProperties(layer.section)) Object.assign(out, layer.fields ?? {});
  return out;
}

/**
 * A property of a type by its name — as text is written — or by its key, as
 * text is stored.
 */
export const propertyNamed = (type: Template | undefined, name: string) =>
  type?.properties.find((p) => p.key === name.trim() || p.label.toLowerCase() === name.trim().toLowerCase());

/**
 * Whose page an article is: the world's, a type's — or nobody's, then it is
 * an article like any other.
 *
 * Every page is an article too; this only says where it belongs, so it can
 * wear the owner's icon and stand where the owner stands.
 */
export function pageOwner(
  templates: Template[],
  home: string,
  article: ArticleView,
): { kind: "world" } | { kind: "type"; type: Template } | null {
  if (article.id === home) return { kind: "world" };
  const type = templates.find((t) => t.page === article.id);
  return type ? { kind: "type", type } : null;
}

/** The ids a field holds in an article — whichever readable section carries it. */
export function idsIn(article: ArticleView, key: string): string[] {
  return picksIn(article, key).map((p) => p.id);
}

/** One pick of a reference: the id, and its own number where the field has one (`with Level`). */
export type Pick = { id: string; count?: number };

/** A reference's value as picks — stored as ids, or as `{with, count}` where each carries a number. */
export function picksOf(value: unknown): Pick[] {
  const one = (v: unknown): Pick => {
    if (v && typeof v === "object" && "with" in v) {
      const o = v as { with: unknown; count?: unknown };
      const n = Number(o.count);
      return o.count === undefined || o.count === null || !Number.isFinite(n) ? { id: String(o.with) } : { id: String(o.with), count: n };
    }
    return { id: String(v) };
  };
  if (Array.isArray(value)) return value.map(one);
  if (value === undefined || value === null || value === "") return [];
  return [one(value)];
}

/** The picks a field holds in an article — whichever readable section carries it. */
export function picksIn(article: ArticleView, key: string): Pick[] {
  for (const layer of article.layers) {
    const value = (layer.fields ?? {})[key];
    if (value !== undefined && value !== null && value !== "") return picksOf(value);
  }
  return [];
}

/**
 * The picks of a reference as they count: for one that holds `every`
 * document of its type, each of them — in name order, with its number, or 0
 * where none is stored. Otherwise what is stored.
 */
export function picksFor(all: Template[], articles: ArticleView[], article: ArticleView, field: TemplateField): Pick[] {
  const stored = picksIn(article, field.key);
  if (field.kind !== "reference" || !field.every) return stored;
  return articles
    .filter((a) => isA(all, a.articleType, field.articleType))
    .sort((a, b) => plain(a.name, b.name))
    .map((a) => ({ id: a.id, count: stored.find((p) => p.id === a.id)?.count ?? 0 }));
}

/** The parent an article names — the id, whether readable or not. */
export function parentIdOf(all: Template[], article: ArticleView): string | undefined {
  const field = parentField(typeOf(all, article.articleType));
  return field ? idsIn(article, field.key)[0] : undefined;
}

/**
 * The chain above an article, the direct parent first.
 *
 * A parent this reader cannot decrypt ends the chain as `null`: it exists,
 * but what lies above it cannot be known here.
 */
export function parentChain(
  all: Template[],
  articles: ArticleView[],
  article: ArticleView,
): (ArticleView | null)[] {
  const out: (ArticleView | null)[] = [];
  const seen = new Set([article.id]);
  let current = article;
  for (;;) {
    const id = parentIdOf(all, current);
    if (!id || seen.has(id)) break;
    const parent = articles.find((a) => a.id === id);
    if (!parent) {
      out.push(null);
      break;
    }
    out.push(parent);
    seen.add(id);
    current = parent;
  }
  return out;
}

/** The infobox layer's key — re-exported so callers need only this module. */
export { PROPERTIES };

/**
 * Whether someone holding these attributes passes a view as written —
 * `players`, `class:rogue OR gm`, `players AND doc:…`. Every player holds
 * `players` and `all`.
 */
export function permits(policy: string, held: string[]): boolean {
  const have = new Set(["players", "all", ...held].map((a) => a.trim().toLowerCase()));
  return policy
    .replace(/[()]/g, " ")
    .split(/\s+OR\s+/i)
    .some((choice) => choice.trim() !== "" && choice.split(/\s+AND\s+/i).every((a) => have.has(a.trim().toLowerCase())));
}

/** Whether this member may make documents of a type: the GM always, a player where the type says (`create=`). */
export function mayCreate(world: { isGm: boolean; attributes: string[] }, type: Template | undefined): boolean {
  if (!type || type.group) return false;
  return world.isGm || (!!type.create?.trim() && permits(type.create, world.attributes));
}
