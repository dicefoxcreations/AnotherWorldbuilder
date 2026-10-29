import type { ArticleView } from "../api/client";
import { isProperties } from "./types";

/**
 * A document's text as a reader goes through it: its headings that one can
 * jump to, and its words to search — what the reader may see, nothing else.
 * The layers a reader may not see are not in their `ArticleView` at all.
 */

/** A heading with an anchor, and the ones below it. */
export type OutlineEntry = { anchor: string; label: string; level: number; children: OutlineEntry[] };
/** Where words were found: the section, the headings above it, a bit of the text. */
export type TextHit = { article: ArticleView; section?: string; path?: string[]; anchor?: string; snippet: string };

/** The text from one heading to the next. */
type Segment = { label?: string; path?: string[]; anchor?: string; level: number; text: string };

/** Worked out once per drawn document — a new view of it is a new object. */
const cached = new WeakMap<ArticleView, { segments: Segment[]; outline: OutlineEntry[] }>();

const ANCHOR = "[\\p{L}\\p{N}-]{1,64}";
const HEADING = new RegExp(`^(#{1,6})\\s+(.+?)(?:\\s+\\^(${ANCHOR}))?\\s*$`, "u");
/** An anchor on a line of its own, right below a heading: the heading's. */
const ANCHOR_LINE = new RegExp(`^\\s*\\u200b?\\s*\\^(${ANCHOR})\\s*$`, "u");

/** The words of a text: no link or picture ids, no anchors, no marks. */
export function readableText(text: string): string {
  return text
    .replace(/@@[^@|]*\|([^@]*)@@/g, "$1")
    .replace(/@@art:[^@]*@@/g, "")
    .replace(/@@(?:[^@:]*:)?([^@]*)@@/g, "$1")
    .replace(/<[^>]*>/g, " ")
    .replace(new RegExp(`\\s\\^${ANCHOR}(?=\\s|$)`, "gu"), "")
    .replace(/[#*_>`​]/g, "")
    .replace(/\\\|/g, "|")
    .replace(/\s+/g, " ")
    .trim();
}

function read(article: ArticleView) {
  const kept = cached.get(article);
  if (kept) return kept;
  const segments: Segment[] = [];
  for (const layer of article.layers) {
    if (isProperties(layer.section) || !layer.text) continue;
    let current: Segment = { level: 0, text: "" };
    segments.push(current);
    // What looks like a heading in a code block is none.
    let fence: string | null = null;
    for (const line of layer.text.split("\n")) {
      const fenced = /^\s*(`{3,}|~{3,})/.exec(line);
      if (fenced) {
        if (!fence) fence = fenced[1][0];
        else if (fence === fenced[1][0]) fence = null;
        current.text += "\n" + line;
        continue;
      }
      const head = fence ? null : HEADING.exec(line);
      if (head) {
        current = { level: head[1].length, label: readableText(head[2]), anchor: head[3], text: line };
        segments.push(current);
        continue;
      }
      const belowHeading = !!current.label && !current.anchor && current.text.split("\n").slice(1).every((l) => !l.trim());
      current.text += "\n" + line;
      if (belowHeading) current.anchor = ANCHOR_LINE.exec(line)?.[1];
    }
  }
  // The headings above each one, and the outline: those with an anchor, each once.
  const outline: OutlineEntry[] = [];
  const above: Segment[] = [];
  const open: OutlineEntry[] = [];
  const anchors = new Set<string>();
  for (const section of segments) {
    section.text = readableText(section.text);
    if (!section.label) {
      above.length = 0;
      open.length = 0;
      continue;
    }
    while (above.length && above[above.length - 1].level >= section.level) above.pop();
    section.path = [...above.map((h) => h.label!), section.label];
    above.push(section);
    while (open.length && open[open.length - 1].level >= section.level) open.pop();
    if (!section.anchor || anchors.has(section.anchor)) continue;
    anchors.add(section.anchor);
    const entry: OutlineEntry = { anchor: section.anchor, label: section.label, level: section.level, children: [] };
    (open[open.length - 1]?.children ?? outline).push(entry);
    open.push(entry);
  }
  const result = { segments, outline };
  cached.set(article, result);
  return result;
}

/** The headings one can jump to, as they nest. */
export const documentOutline = (article: ArticleView): OutlineEntry[] => read(article).outline;

/** Every section the words stand in, in order — the name first, if they are in it. */
export function searchDocument(article: ArticleView, words: string): TextHit[] {
  const query = words.trim().toLowerCase();
  if (!query) return [];
  const { segments } = read(article);
  const hits: TextHit[] = [];
  if (article.name.toLowerCase().includes(query)) {
    hits.push({ article, snippet: segments.map((s) => s.text).join(" ").slice(0, 150) });
  }
  const seen = new Set<string>();
  for (const segment of segments) {
    const at = segment.text.toLowerCase().indexOf(query);
    if (at < 0 || (segment.anchor && seen.has(segment.anchor))) continue;
    if (segment.anchor) seen.add(segment.anchor);
    const from = Math.max(0, at - 55);
    const to = at + query.length + 95;
    const snippet = (from > 0 ? "…" : "") + segment.text.slice(from, to) + (to < segment.text.length ? "…" : "");
    hits.push({ article, section: segment.label, path: segment.path, anchor: segment.anchor, snippet });
  }
  return hits;
}
