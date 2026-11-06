import type { ArticleView, FolderNode, GalleryImage, NavEntry, Template } from "../../api/client";
import { ancestors, findType, isA, isProperties } from "../../shared/types";

/**
 * Folders after the navigation: an image lies where its document stands in
 * the side bar on the left — `Welt › Charaktere › NPCs`. The type entry that
 * fits a document most closely holds it, as the side bar does: a homebrew
 * monster lies under `Monster › Homebrew`, not under `Monster`.
 */

/** The folder for images no document shows. */
export const UNUSED = "Unused";

export type Places = {
  /** Where a document stands in the navigation, as folder names. */
  of: (article: ArticleView) => string[] | null;
  /** Every place the navigation has for documents of a type — each a folder, empty or not. */
  all: string[][];
};

export function navigationPlaces(nav: NavEntry[], articles: ArticleView[], templates: Template[]): Places {
  const name = (e: NavEntry) =>
    e.label?.trim() ||
    (e.kind === "type" ? findType(templates, e.target ?? "")?.name : e.kind === "document" ? articles.find((a) => a.id === e.target)?.name : "") ||
    "";
  const types: { type: string; path: string[]; depth: number }[] = [];
  const documents = new Map<string, string[]>();
  const walk = (entries: NavEntry[], above: string[]) => {
    for (const e of entries) {
      const path = [...above, name(e)].filter(Boolean);
      if (e.kind === "type" && e.target) {
        const t = findType(templates, e.target);
        types.push({ type: e.target, path, depth: t ? ancestors(templates, t).length : 0 });
      }
      // A page stands with what is around it — the home page by its own name.
      if (e.kind === "document" && e.target) documents.set(e.target, above.length ? above : path);
      if (e.children) walk(e.children, path);
    }
  };
  walk(nav, []);
  return {
    of: (article) => {
      const own = documents.get(article.id);
      if (own) return own;
      let best: (typeof types)[number] | null = null;
      for (const t of types) {
        if (!isA(templates, article.articleType, t.type)) continue;
        if (!best || t.depth > best.depth || (t.depth === best.depth && t.path.length > best.path.length)) best = t;
      }
      return best?.path ?? null;
    },
    all: types.map((t) => t.path),
  };
}

/** The folder at a path of names, if there is one. */
export function folderAt(folders: FolderNode[], path: string[]): string | null {
  let parent = "";
  for (const part of path) {
    const next = folders.find((f) => f.parent === parent && f.name === part);
    if (!next) return null;
    parent = next.id;
  }
  return parent || null;
}

/** The words of a name: `Map-OldHarbour` → map, old, harbour. */
const words = (name: string) =>
  new Set(
    name
      .replace(/([a-z])([A-Z])/g, "$1 $2")
      .toLowerCase()
      .split(/[^a-z0-9äöüß]+/)
      .filter((w) => w.length > 2),
  );

/** Whether a document shows the image as its own picture — in an image property. */
const asPicture = (article: ArticleView, image: string) =>
  article.layers.some((l) => isProperties(l.section) && Object.values(l.fields ?? {}).some((v) => v === image));

/**
 * Whose image it is, of those that show it: one that has it as its picture
 * before one that only shows it in its text, then the one whose name the
 * image's name shares most words with — the location's picture, which a
 * session borrows, lies with the location.
 */
export function ownerOf(image: GalleryImage, articles: Map<string, ArticleView>): ArticleView | null {
  const mine = words(image.name);
  let best: { article: ArticleView; score: number } | null = null;
  for (const u of image.usedIn) {
    const article = articles.get(u.id);
    if (!article) continue;
    const shared = [...words(article.name)].filter((w) => mine.has(w)).length;
    const score = (asPicture(article, image.id) ? 100 : 0) + shared;
    if (!best || score > best.score) best = { article, score };
  }
  return best?.article ?? null;
}

/**
 * The images in no folder, each into the folder of where its document
 * stands — what no document shows into “Unused”. Every place of the
 * navigation gets its folder, so the tree is there before it fills. What
 * someone sorted already stays where it is.
 */
export function sortIntoFolders(
  images: GalleryImage[],
  folders: FolderNode[],
  places: Places,
  articles: ArticleView[],
  newId: () => string,
): { folders: FolderNode[]; moves: [string, string][] } {
  const known = new Set(folders.map((f) => f.id));
  const all = [...folders];
  const byId = new Map(articles.map((a) => [a.id, a]));
  const ensure = (path: string[]) => {
    let parent = "";
    for (const part of path) {
      let next = all.find((f) => f.parent === parent && f.name === part);
      if (!next) {
        next = { id: newId(), name: part, parent };
        all.push(next);
      }
      parent = next.id;
    }
    return parent;
  };
  for (const path of places.all) ensure(path);
  const moves: [string, string][] = [];
  for (const image of images) {
    if (known.has(image.folder)) continue;
    if (image.usedIn.length === 0) {
      moves.push([image.id, ensure([UNUSED])]);
      continue;
    }
    const owner = ownerOf(image, byId);
    const path = owner && places.of(owner);
    if (path && path.length > 0) moves.push([image.id, ensure(path)]);
  }
  return { folders: all, moves };
}
