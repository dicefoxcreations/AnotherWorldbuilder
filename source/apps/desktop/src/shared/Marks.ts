import { sideBySide } from "./Diff";

/**
 * Where two versions of a text differ, marked in the text itself — so the
 * page and the editor show it where it stands.
 *
 * The marks are characters of the private use area; nobody types them. The
 * page draws them as red and green (see `MarkdownText`), the editor as a
 * background behind the text field (see `MarkedField`).
 */

export type Range = { start: number; end: number };
export type MarkKind = "added" | "removed";

export const MARK = {
  added: ["", ""],
  removed: ["", ""],
} as const;

/** What is gone from `then`, and what is new in `now` — as ranges of each. */
export function changes(then: string, now: string): { removed: Range[]; added: Range[] } {
  const { left, right } = sideBySide(then, now);
  const ranges = (segments: { text: string; kind: string }[], kind: string) => {
    const out: Range[] = [];
    let at = 0;
    for (const s of segments) {
      if (s.kind === kind && s.text.trim()) out.push({ start: at, end: at + s.text.length });
      at += s.text.length;
    }
    return out;
  };
  return { removed: ranges(left, "gone"), added: ranges(right, "new") };
}

/** A link, a value, a tag: marked whole or not at all. */
const ATOM = /@@[^@\n]*@@|%%[^%\n]*%%|<[^>\n]*>/g;
/** What makes a line a heading, a list item, a quote — stays before a mark. */
const BLOCK_START = /^[ \t]*(?:#{1,6}[ \t]+|[-*+][ \t]+|\d+[.)][ \t]+|>[ \t]?)*/;

/**
 * `text` with `ranges` marked, ready for the page: never inside a link or a
 * tag, never across a line, never before a heading's or list item's sign.
 */
export function marked(text: string, ranges: Range[], kind: MarkKind): string {
  if (ranges.length === 0) return text;
  const atoms = [...text.matchAll(ATOM)].map((m) => ({ start: m.index!, end: m.index! + m[0].length }));
  const pieces: Range[] = [];
  for (const r of ranges) {
    let { start, end } = r;
    for (const a of atoms)
      if (start < a.end && a.start < end) {
        start = Math.min(start, a.start);
        end = Math.max(end, a.end);
      }
    // Line by line — a mark stays within its line.
    let lineStart = text.lastIndexOf("\n", start - 1) + 1;
    while (lineStart < end) {
      const lineEnd = text.indexOf("\n", lineStart) === -1 ? text.length : text.indexOf("\n", lineStart);
      const prefix = BLOCK_START.exec(text.slice(lineStart, lineEnd))?.[0].length ?? 0;
      const s = Math.max(start, lineStart + prefix);
      const e = Math.min(end, lineEnd);
      if (e > s && text.slice(s, e).trim()) pieces.push({ start: s, end: e });
      lineStart = lineEnd + 1;
    }
  }
  pieces.sort((a, b) => a.start - b.start);
  const [open, close] = MARK[kind];
  let out = "";
  let at = 0;
  for (const p of pieces) {
    if (p.start < at) continue;
    out += text.slice(at, p.start) + open + text.slice(p.start, p.end) + close;
    at = p.end;
  }
  return out + text.slice(at);
}
