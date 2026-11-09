import type { ArticleView, Template } from "../api/client";
import { parentIdOf } from "./types";

/**
 * The documents in tree order, each with its depth — siblings in the order
 * they come in, which is the list's sort: a book's chapters by their number.
 *
 * A document whose parent is among them hangs below it. One whose parent
 * lies outside — another type, or unreadable here — starts a tree of its own.
 */
export function asTree(templates: Template[], articles: ArticleView[]): { article: ArticleView; depth: number }[] {
  const inList = new Set(articles.map((a) => a.id));
  const children = new Map<string, ArticleView[]>();
  const roots: ArticleView[] = [];
  for (const a of articles) {
    const parent = parentIdOf(templates, a);
    if (parent && inList.has(parent) && parent !== a.id) {
      children.set(parent, [...(children.get(parent) ?? []), a]);
    } else {
      roots.push(a);
    }
  }
  const out: { article: ArticleView; depth: number }[] = [];
  const seen = new Set<string>();
  const walk = (a: ArticleView, depth: number) => {
    if (seen.has(a.id)) return;
    seen.add(a.id);
    out.push({ article: a, depth });
    for (const c of children.get(a.id) ?? []) walk(c, depth + 1);
  };
  roots.forEach((r) => walk(r, 0));
  // A cycle among parents leaves documents unvisited — they still belong in
  // the list.
  for (const a of articles) if (!seen.has(a.id)) walk(a, 0);
  return out;
}
