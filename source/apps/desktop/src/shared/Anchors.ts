/**
 * Leading to a place in a document's text: `@@#^2f9e17|the inn@@` goes to the
 * line closed by the anchor `^2f9e17` — in the same document, or with a name
 * in front in another one (`aw_model::link`, `aw_model::markdown`).
 */

/** An anchor asked for in a document not drawn yet — shown once it is. */
let waiting: { doc: string; anchor: string } | null = null;

/** What an anchor stands for: a whole table, a list item, a paragraph, a heading. */
export function anchoredPart(spot: Element): Element | null {
  return spot.closest(".markdown-table") ?? spot.closest("table") ?? spot.closest("li, p, h1, h2, h3, h4, h5, h6") ?? spot.parentElement;
}


/**
 * What an anchor stands for in `root`: its part — or, where the same anchor
 * closes two blocks, every block from the first to the last.
 */
export function anchoredParts(root: ParentNode, anchor: string, skip?: (spot: Element) => boolean): Element[] {
  const spots = [...root.querySelectorAll(`[data-anchor="${CSS.escape(anchor)}"]`)].filter((s) => !skip?.(s));
  if (spots.length === 0) return [];
  const first = anchoredPart(spots[0]);
  const last = anchoredPart(spots[spots.length - 1]);
  if (!first || !last) return [];
  if (first === last) return [first];
  // The blocks of the text, as they stand, from the first one's to the last one's.
  const top = (e: Element) => {
    let at = e;
    while (at.parentElement && !at.parentElement.classList.contains("markdown")) at = at.parentElement;
    return at;
  };
  const blocks = [...root.querySelectorAll(".markdown > *")].filter((e) => !skip?.(e));
  const a = blocks.indexOf(top(first));
  const b = blocks.indexOf(top(last));
  return a < 0 || b < a ? [first] : blocks.slice(a, b + 1);
}

/** Scrolls to an anchor of the open page and lights up its part. False: not drawn. */
export function showAnchor(anchor: string): boolean {
  // On the page itself, not in a preview lying over it.
  const parts = anchoredParts(document, anchor, (s) => !s.closest(".article-page") || !!s.closest(".link-preview"));
  if (parts.length === 0) return false;
  parts[0].scrollIntoView({ block: "center", behavior: "smooth" });
  for (const part of parts) {
    part.classList.remove("anchor-lit");
    // Again from the start when it was lit already.
    void (part as HTMLElement).offsetWidth;
    part.classList.add("anchor-lit");
  }
  return true;
}

/** Goes to an anchor: here at once, elsewhere once `open` has drawn that document. */
export function leadTo(doc: string, anchor: string, current: string | null, open: () => void) {
  if (doc === current && showAnchor(anchor)) return;
  waiting = { doc, anchor };
  open();
}

/** An anchor drawn: if it was asked for, it is shown now. */
export function anchorDrawn(doc: string | null, anchor: string) {
  if (!waiting || waiting.doc !== doc || waiting.anchor !== anchor) return;
  waiting = null;
  requestAnimationFrame(() => showAnchor(anchor));
}

/** An anchor closing a line: `… planen. ^2f9e17`, in a table's last cell too. */
const ANCHOR_AT_END = /\s\^([\p{L}\p{N}-]{1,64})(\s*\|)?\s*$/u;

/** A section's name as an anchor: `Magic rewards` → `magic-rewards`. */
export function anchorName(name: string): string {
  return name
    .trim()
    .toLowerCase()
    .replace(/\s+/g, "-")
    .replace(/[^\p{L}\p{N}-]/gu, "")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "")
    .slice(0, 64);
}

/** The anchors of a text, each once, with the line that carries it first. */
export function anchorsIn(text: string): { anchor: string; line: string }[] {
  const out: { anchor: string; line: string }[] = [];
  for (const line of text.split("\n")) {
    const m = ANCHOR_AT_END.exec(line);
    if (m && !out.some((a) => a.anchor === m[1])) out.push({ anchor: m[1], line: line.slice(0, m.index) });
  }
  return out;
}

/** Where a block of Markdown ends that holds line `i`: a table, a list item, a heading, a paragraph. */
function blockEnd(lines: string[], i: number): number {
  const kind = (l: string) =>
    /^\s*\|/.test(l) ? "table" : /^\s*([-*+]|\d{1,9}[.)])\s/.test(l) ? "item" : /^#{1,6}\s/.test(l) ? "heading" : l.trim() === "" ? "empty" : /^\s*</.test(l) ? "tag" : "text";
  const first = kind(lines[i]);
  if (first === "item" || first === "heading" || first === "tag" || first === "empty") return i;
  let j = i;
  while (j + 1 < lines.length && kind(lines[j + 1]) === first) j++;
  return j;
}

/** A line with an anchor at its end — in a table row before its last bar. */
function anchored(line: string, anchor: string): string {
  if (/^\s*\|.*\|\s*$/.test(line)) return line.replace(/\s*\|\s*$/, ` ^${anchor} |`);
  return `${line.replace(/\s+$/, "")} ^${anchor}`;
}

/**
 * Marks what is selected (`from`–`to` in `text`) as a section named `anchor`:
 * the anchor closes the block it begins in — and, where it reaches into
 * further blocks, the last of them too: the same anchor twice is the stretch
 * between. Returns the text, or null where nothing is there to mark.
 */
export function markSection(text: string, from: number, to: number, anchor: string): string | null {
  const lines = text.split("\n");
  const lineAt = (pos: number) => text.slice(0, pos).split("\n").length - 1;
  let a = lineAt(from);
  // A selection ending at the start of a line ends on the line before.
  let b = lineAt(Math.max(from, to > from && text[to - 1] === "\n" ? to - 1 : to));
  while (a <= b && lines[a].trim() === "") a++;
  while (b >= a && lines[b].trim() === "") b--;
  if (a > b || !anchor) return null;
  const first = blockEnd(lines, a);
  const last = blockEnd(lines, b);
  lines[first] = anchored(lines[first], anchor);
  if (last > first) lines[last] = anchored(lines[last], anchor);
  return lines.join("\n");
}

const escaped = (name: string) => name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** Where a block of Markdown begins that holds line `i` — see `blockEnd`. */
function blockStart(lines: string[], i: number): number {
  const kind = (l: string) =>
    /^\s*\|/.test(l) ? "table" : /^\s*([-*+]|\d{1,9}[.)])\s/.test(l) ? "item" : /^#{1,6}\s/.test(l) ? "heading" : l.trim() === "" ? "empty" : /^\s*</.test(l) ? "tag" : "text";
  const first = kind(lines[i]);
  if (first === "item" || first === "heading" || first === "tag" || first === "empty") return i;
  let j = i;
  while (j > 0 && kind(lines[j - 1]) === first) j--;
  return j;
}

/** Where an anchor's section stands in a text: from its first block's start to the end of its last line. */
export function anchorRange(text: string, anchor: string): { start: number; end: number } | null {
  const lines = text.split("\n");
  const at = lines.map((l, i) => (ANCHOR_AT_END.exec(l)?.[1] === anchor ? i : -1)).filter((i) => i >= 0);
  if (at.length === 0) return null;
  const offset = (line: number) => lines.slice(0, line).reduce((n, l) => n + l.length + 1, 0);
  const first = blockStart(lines, at[0]);
  const last = at[at.length - 1];
  return { start: offset(first), end: offset(last) + lines[last].length };
}

/** A text with an anchor renamed — where it closes lines, and in this document's links to it (`@@#^…`). */
export function renameInText(text: string, old: string, fresh: string): string {
  const o = escaped(old);
  return text
    .split("\n")
    .map((line) => (ANCHOR_AT_END.exec(line)?.[1] === old ? line.replace(new RegExp(`(\\s\\^)${o}(?=(\\s*\\|)?\\s*$)`, "u"), `$1${fresh}`) : line))
    .join("\n")
    .replace(new RegExp(`@@#\\^${o}(?=[|@])`, "gu"), `@@#^${fresh}`);
}

/** Of a text, the links to `anchor` of another document — by the part before `#`, which `leadsThere` checks. */
export function relinked(text: string, anchor: string, fresh: string, leadsThere: (target: string) => boolean): string {
  return text.replace(new RegExp(`@@([^@\\n#]+)#\\^${escaped(anchor)}(?=[|@])`, "gu"), (whole, target: string) =>
    leadsThere(target) ? `@@${target}#^${fresh}` : whole,
  );
}

/** A text without an anchor's marks — the lines they closed stay. */
export function withoutAnchor(text: string, anchor: string): string {
  return text
    .split("\n")
    .map((line) => (ANCHOR_AT_END.exec(line)?.[1] === anchor ? line.replace(new RegExp(`\\s\\^${escaped(anchor)}(?=(\\s*\\|)?\\s*$)`, "u"), "") : line))
    .join("\n");
}
