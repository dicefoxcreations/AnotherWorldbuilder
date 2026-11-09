import type { ArticleView, Template } from "../api/client";
import { perLists } from "./Cache";
import { linker, linksIn } from "./Links";
import { idsIn, typeOf } from "./types";

/**
 * Everything that points at an article, grouped by the field it points with.
 *
 * Every reference field has a reverse direction: whoever names Chicago as
 * “Home” shows up at Chicago under “Home of” — a session naming Tim among
 * its characters under “Characters of”, not under “Characters”, which would
 * promise characters. The parent field is the tree, so its reverse is called
 * what it is — what lies below. Only what this reader may see takes part.
 */
export function incoming(
  templates: Template[],
  all: ArticleView[],
  article: ArticleView,
): { title: string; articles: ArticleView[] }[] {
  const groups = backlinks(all, templates).get(article.id) ?? new Map<string, ArticleView[]>();
  return [...groups.entries()]
    .sort(([x], [y]) => (x === "Below" ? -1 : y === "Below" ? 1 : x.localeCompare(y)))
    .map(([title, articles]) => ({
      title,
      articles: [...articles].sort((x, y) => x.name.localeCompare(y.name)),
    }));
}

/**
 * What points at each document, by the title of its group — every text and
 * reference of the world read once per list of documents, not again for
 * each page opened.
 */
const backlinks = perLists((all: ArticleView[], templates: Template[]) => {
  const index = new Map<string, Map<string, ArticleView[]>>();
  const resolve = linker(all, templates);
  const add = (target: string | undefined, title: string, a: ArticleView) => {
    if (!target || target === a.id) return;
    let groups = index.get(target);
    if (!groups) index.set(target, (groups = new Map()));
    const list = groups.get(title);
    if (!list) groups.set(title, [a]);
    // Each document's links are read together: had it pointed here already, it is the last one.
    else if (list[list.length - 1] !== a) list.push(a);
  };
  for (const a of all) {
    const type = typeOf(templates, a.articleType);
    for (const field of type?.properties ?? []) {
      if (field.kind !== "reference") continue;
      for (const id of idsIn(a, field.key)) add(id, field.parent ? "Below" : field.label ? `${field.label} of` : "Referenced by", a);
    }
    // Free properties name documents by `@@Type:Name@@`, like the text does.
    const known = new Set((type?.properties ?? []).map((f) => f.key));
    for (const layer of a.layers) {
      for (const [name, value] of Object.entries(layer.fields ?? {})) {
        if (!known.has(name) && typeof value === "string") for (const l of linksIn(value)) add(resolve(l)?.id, `${name} of`, a);
      }
      for (const l of linksIn(layer.text)) add(resolve(l)?.id, "Mentioned in", a);
    }
  }
  return index;
});
