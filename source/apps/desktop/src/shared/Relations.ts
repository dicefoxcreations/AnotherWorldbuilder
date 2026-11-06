import type { ArticleView, Template, TemplateField } from "../api/client";
import { linkText, scanLinks, type Link } from "./Links";
import { isA, isProperties, typeOf } from "./types";

/**
 * Relations between documents: who someone is to whom.
 *
 * The same notation as `aw_model::relation` in Rust. A property
 * `Relations: @@Character@@ as @@Relation@@` holds entries, each naming a
 * document and two roles — what it is to this one, and what this one is to
 * it: `@@Olga@@ Mother/Daughter, @@Ada@@ Friend, @@Kurt@@`.
 *
 * Every entry stands in both documents, each the right way round; Rust
 * writes the other side when one is saved.
 */

export type RelationField = Extract<TemplateField, { kind: "relation" }>;

/** One entry as stored: ids. An open role is empty. */
export type RelationEntry = { with: string; role: string; ownRole: string };

/** One entry as written: the link, the roles by name. */
export type WrittenRelation = { link: Link; role: string; ownRole: string };

export const isRelation = (f: TemplateField | undefined): f is RelationField => f?.kind === "relation";

/** The entries of a stored value. */
export function entriesOf(value: unknown): RelationEntry[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((v) =>
    v && typeof v === "object" && typeof (v as Record<string, unknown>).with === "string"
      ? [
          {
            with: String((v as Record<string, unknown>).with),
            role: String((v as Record<string, unknown>).role ?? ""),
            ownRole: String((v as Record<string, unknown>).own_role ?? ""),
          },
        ]
      : [],
  );
}

/** What a document's relation property holds — from whichever layer this reader can open. */
export function relationsOf(article: ArticleView, key: string): RelationEntry[] {
  for (const layer of article.layers) {
    if (!isProperties(layer.section)) continue;
    const value = (layer.fields ?? {})[key];
    if (value !== undefined) return entriesOf(value);
  }
  return [];
}

/**
 * Whether this reader can see a document's relation property at all —
 * otherwise “it does not name me” means nothing.
 */
export function holdsRelations(article: ArticleView, field: RelationField): boolean {
  return article.layers.some(
    (l) =>
      (l.section === `properties:${field.key}` && !!field.access) ||
      (l.section === "properties" && !field.access) ||
      (isProperties(l.section) && (l.fields ?? {})[field.key] !== undefined),
  );
}

/** The relation properties of a document's type. */
export function relationFields(templates: Template[], articleType: string): RelationField[] {
  return (typeOf(templates, articleType)?.properties ?? []).filter(isRelation);
}

/** Writes one entry: `@@Olga@@ Mother/Daughter`. */
export function writeRelation(link: string, role: string, ownRole: string): string {
  if (!ownRole) return role ? `${link} ${role}` : link;
  return `${link} ${role}/${ownRole}`;
}

export const writeRelations = (list: WrittenRelation[]) =>
  list.map((w) => writeRelation(linkText(w.link.of, w.link.name), w.role, w.ownRole)).join(", ");

/**
 * Reads what a relation property says. Forgiving, unlike Rust: while typing
 * a stray word is the normal case, and the visual side keeps up anyway.
 */
export function readRelations(text: string): WrittenRelation[] {
  const out: WrittenRelation[] = [];
  for (const piece of scanLinks(text)) {
    if (typeof piece !== "string") {
      out.push({ link: piece, role: "", ownRole: "" });
      continue;
    }
    const roles = piece.split(",")[0].trim();
    const last = out[out.length - 1];
    if (!roles || !last || last.role || last.ownRole) continue;
    const slash = roles.indexOf("/");
    last.role = (slash >= 0 ? roles.slice(0, slash) : roles).trim();
    last.ownRole = slash >= 0 ? roles.slice(slash + 1).trim() : "";
  }
  return out;
}

/** The documents a role may be: those of the relation's role type. */
export const roleChoices = (templates: Template[], all: ArticleView[], field: RelationField) =>
  all.filter((a) => isA(templates, a.articleType, field.roleType)).sort((a, b) => a.name.localeCompare(b.name));

/** A role by id or by name, among the role choices. */
export function findRole(roles: ArticleView[], idOrName: string): ArticleView | undefined {
  if (!idOrName) return undefined;
  const lower = idOrName.toLowerCase();
  return roles.find((r) => r.id === idOrName) ?? roles.find((r) => r.name.toLowerCase() === lower);
}

// --- The graph --------------------------------------------------------------

/** A person in the graph. `article` is missing for one this reader cannot open. */
/** `hidden`: for `???`, the document behind it — never shown, only where its card was put. */
export type GraphNode = { id: string; article?: ArticleView; hidden?: string };

/**
 * A line between two people — every entry about them merged, from both
 * sides. `atA` is what A is to B (roles by id), `atB` the other way.
 */
export type GraphEdge = {
  id: string;
  a: string;
  b: string;
  atA: string[];
  atB: string[];
  /** The relation property it stands in, by key. */
  property: string;
  /** Who holds an entry of it. */
  holders: string[];
};

export type Graph = { nodes: GraphNode[]; edges: GraphEdge[] };

/** What a graph shows — `<graph …>`, with ids where stored. */
export type GraphQuery = {
  /** The listed type's id — empty: only who was put on the graph, or who is around the middle. */
  of: string;
  /** Relation properties, by key or name — empty for all. */
  via: string[];
  /** The document in the middle, by id — empty for everyone. */
  around: string;
  depth: number;
};

/**
 * Builds the graph from what this reader can open.
 *
 * `overrides` stand in for stored entries: the document being edited has
 * them in its text, not saved yet.
 */
export function buildGraph(
  templates: Template[],
  articles: ArticleView[],
  query: GraphQuery,
  overrides: Map<string, Map<string, RelationEntry[]>> = new Map(),
  /** People set on the graph by hand — drawn even without a relation. */
  extra: string[] = [],
): Graph {
  const byId = new Map(articles.map((a) => [a.id, a] as const));
  const wanted = (f: RelationField) =>
    query.via.length === 0 ||
    query.via.some((v) => v === f.key || v.toLowerCase() === f.label.toLowerCase());

  const edges = new Map<string, GraphEdge>();
  const unknown: GraphNode[] = [];
  const add = (from: string, field: RelationField, entry: RelationEntry, index: number) => {
    // Every entry to someone unreadable is someone of its own: two of them
    // would otherwise say they are the same person.
    let other = entry.with;
    if (!byId.has(other)) {
      other = `unknown:${from}:${field.key}:${index}`;
      unknown.push({ id: other, hidden: entry.with });
    }
    const [a, b] = from < other ? [from, other] : [other, from];
    const id = `${a}|${b}`;
    const edge = edges.get(id) ?? { id, a, b, atA: [], atB: [], property: field.key, holders: [] };
    // The entry's role is what the other is to `from`.
    const atFrom = from === a ? edge.atA : edge.atB;
    const atOther = from === a ? edge.atB : edge.atA;
    if (entry.ownRole && !atFrom.includes(entry.ownRole)) atFrom.push(entry.ownRole);
    if (entry.role && !atOther.includes(entry.role)) atOther.push(entry.role);
    if (!edge.holders.includes(from)) edge.holders.push(from);
    edges.set(id, edge);
  };

  for (const article of articles) {
    for (const field of relationFields(templates, article.articleType).filter(wanted)) {
      const entries = overrides.get(article.id)?.get(field.key) ?? relationsOf(article, field.key);
      entries.forEach((e, i) => e.with !== article.id && add(article.id, field, e, i));
    }
  }

  const listed = (a: ArticleView) => !query.of || isA(templates, a.articleType, query.of);
  let ids: Set<string>;
  if (query.around) {
    // Who is reachable from the middle, step by step.
    ids = new Set([query.around]);
    let frontier = [query.around];
    for (let step = 0; step < Math.max(1, query.depth) && frontier.length > 0; step++) {
      const next: string[] = [];
      for (const e of edges.values()) {
        for (const [x, y] of [
          [e.a, e.b],
          [e.b, e.a],
        ]) {
          if (frontier.includes(x) && !ids.has(y)) {
            const target = byId.get(y);
            if (target && !listed(target)) continue;
            ids.add(y);
            next.push(y);
          }
        }
      }
      frontier = next;
    }
  } else if (query.of) {
    ids = new Set(articles.filter(listed).map((a) => a.id));
    for (const e of edges.values()) {
      const [x, y] = [byId.get(e.a), byId.get(e.b)];
      if ((x && listed(x)) || (y && listed(y))) {
        ids.add(e.a);
        ids.add(e.b);
      }
    }
  } else {
    // Neither a middle nor a type: only who was put on the graph — and
    // whom of them nobody here can open, so “Mother: ???” still shows.
    ids = new Set(extra.filter((id) => byId.has(id)));
    for (const e of edges.values()) {
      if (ids.has(e.a) && !byId.has(e.b)) ids.add(e.b);
      if (ids.has(e.b) && !byId.has(e.a)) ids.add(e.a);
    }
  }

  for (const id of extra) if (byId.has(id)) ids.add(id);

  const nodes: GraphNode[] = [
    ...[...ids].flatMap((id) => (byId.has(id) ? [{ id, article: byId.get(id) }] : [])),
    ...unknown.filter((u) => ids.has(u.id)),
  ].sort((x, y) => (x.article?.name ?? "￿").localeCompare(y.article?.name ?? "￿"));
  return {
    nodes,
    edges: [...edges.values()].filter((e) => ids.has(e.a) && ids.has(e.b)),
  };
}
