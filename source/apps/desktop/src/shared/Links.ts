import type { ArticleView, Template } from "../api/client";
import { perLists } from "./Cache";
import { ancestors, findType } from "./types";

/**
 * Links between documents: `@@Species:Elves@@`.
 *
 * The same notation as `aw_model::link` in Rust. `@@` opens a link, a second
 * `@@` closes it. The type stands in front, divided by a colon; after a `#`
 * a property of the target, whose value is then shown — or after `#^` an
 * anchor in its text (`shared/Anchors`); after a `|` the text shown in its
 * place: `@@Species:Elves|Elf@@`, `@@Character:Tim#Status@@`, `@@#^2f9e17|the inn@@`.
 */

export type Link = { of: string; name: string; property: string; text: string };

/** What opens and closes a link. */
export const MARK = "@@";

/**
 * Writes a link — with a property and text of its own only where given; the
 * text only where it differs from the name.
 */
export function linkText(of: string, name: string, text = "", property = ""): string {
  // A name holding a colon would otherwise read as type and name.
  const target = of ? `${of}:${name}` : name.includes(":") ? `:${name}` : name;
  const prop = property ? `#${property}` : "";
  const shown = text.trim() && text.trim() !== name ? `|${text.trim()}` : "";
  return `${MARK}${target}${prop}${shown}${MARK}`;
}

/** What stands between the marks, if it is a link. */
export function linkIn(inner: string): Link | null {
  if (inner.includes("\n")) return null;
  const bar = inner.indexOf("|");
  const whole = bar >= 0 ? inner.slice(0, bar) : inner;
  const text = bar >= 0 ? inner.slice(bar + 1).trim() : "";
  const hash = whole.lastIndexOf("#");
  const target = hash >= 0 ? whole.slice(0, hash) : whole;
  const property = hash >= 0 ? whole.slice(hash + 1).trim() : "";
  const colon = target.indexOf(":");
  const of = colon >= 0 ? target.slice(0, colon).trim() : "";
  const name = (colon >= 0 ? target.slice(colon + 1) : target).trim();
  // Without a name only an anchor in the same document: `@@#^2f9e17|the inn@@`.
  if (!name) return !of && property.length > 1 && property.startsWith("^") ? { of, name, property, text: text || property.slice(1) } : null;
  return { of, name, property, text: text || name };
}

/** Splits a piece of text into plain text and links. */
export function scanLinks(text: string): (string | Link)[] {
  const out: (string | Link)[] = [];
  let plain = 0;
  let from = 0;
  for (;;) {
    const open = text.indexOf(MARK, from);
    if (open < 0) break;
    const start = open + MARK.length;
    const close = text.indexOf(MARK, start);
    if (close < 0) break;
    const link = linkIn(text.slice(start, close));
    if (link) {
      if (plain < open) out.push(text.slice(plain, open));
      out.push(link);
      plain = close + MARK.length;
      from = plain;
    } else {
      // Not a link — the second mark may open the next one.
      from = close;
    }
  }
  if (plain < text.length) out.push(text.slice(plain));
  return out;
}

export const linksIn = (text: string): Link[] =>
  scanLinks(text).filter((p): p is Link => typeof p !== "string");

/** The name of a document's type, then those above it. */
export function typeNames(templates: Template[], articleType: string): string[] {
  const t = findType(templates, articleType);
  return t ? [t, ...ancestors(templates, t)].map((x) => x.name) : [];
}

/** How a link to this document is written. */
export const linkTo = (templates: Template[], a: ArticleView, text = "", property = "") =>
  linkText(typeNames(templates, a.articleType)[0] ?? "", a.name, text, property);

/**
 * The id a link points at, if it is stored as one: `@@art:01HX…@@`.
 *
 * Text stores ids and shows names — see `aw_model::names`. Only the code
 * editor sees the names; the display resolves the id.
 */
export function idOf(link: Pick<Link, "of" | "name">): string | null {
  return link.of === "art" && /^[0-9A-Za-z]{26}$/.test(link.name) ? `art:${link.name}` : null;
}

/**
 * Finds what a link points at — among the documents this reader can open.
 *
 * `@@Place:Paris@@` finds Paris when it is a city too: the type may be one
 * above the document's own.
 */
/** Finds what a link names — one index per list of documents, see `shared/Cache`. */
export const linker = perLists(makeLinker);

function makeLinker(all: ArticleView[], templates: Template[]) {
  const byId = new Map(all.map((a) => [a.id, a] as const));
  const byName = new Map<string, ArticleView[]>();
  for (const a of all) {
    const key = a.name.toLowerCase();
    byName.set(key, [...(byName.get(key) ?? []), a]);
  }
  return (link: Pick<Link, "of" | "name">): ArticleView | undefined => {
    const id = idOf(link);
    if (id) return byId.get(id);
    const named = byName.get(link.name.toLowerCase()) ?? [];
    if (!link.of) return named[0];
    const of = link.of.toLowerCase();
    return named.find((a) => typeNames(templates, a.articleType).some((n) => n.toLowerCase() === of));
  };
}

export type Resolve = ReturnType<typeof makeLinker>;
