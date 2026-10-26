/**
 * The text of a flow field, cut the way it is drawn — without the field.
 * See `FlowField.tsx`.
 */

/**
 * A stretch of text, drawn as it is read: a heading, bold, italic, code, a
 * link — or plain. A list item or a quote is a line holding stretches of
 * its own (`inner`): the line is drawn as the page draws it, its marks
 * stand where the page has its bullet.
 */
export type Run = { text: string; look?: string; inner?: Run[] };

const HEADING = /^(#{1,6}) [^\n]*(\n|$)/;
/** `- item`, `* item`, `1. item` — the marker, then the item. */
const ITEM = /^( {0,3})([-*+]|\d{1,9}[.)]) (?=[^\n])/;
/** `> quoted` */
const QUOTE = /^> ?(?=[^\n])/;
/** `---` on a line of its own: a rule. */
const RULE = /^ {0,3}(?:-{3,}|\*{3,}|_{3,})[ \t]*(\n|$)/;
/** A roll: `[[…]]`, or dice as written — `2d6 + 3`, `1W20` — as Rust cuts them out. */
const DICE = String.raw`(?<![\p{L}\p{N}_])\d{0,3}[dDwW]\d{1,4}(?:[ ]*[+-][ ]*(?:\d{0,3}[dDwW]\d{1,4}|\d{1,4})(?![\p{L}\p{N}_]))*(?![\p{L}\p{N}_])`;
const ROLL = String.raw`\[\[(?:(?!\]\])[^\n])+\]\]`;
/** `^2f9e17` closing a line — in a table row before its last bar: where a link leads to (`shared/Anchors`). */
const ANCHOR = /(\s+\^[\p{L}\p{N}-]{1,64})(\s*\|?\s*)$/u;
// A link first: what stands between its marks is its own, not bold or italic.
const INLINE = new RegExp(
  String.raw`(@@(?:(?!@@)[^\n])+?@@|${ROLL}|\*\*\*[^*\n]+\*\*\*|\*\*[^*\n]+\*\*|__[^_\n]+__|\*[^*\n]+\*|_[^_\n]+_|\x60[^\x60\n]+\x60|${DICE})`,
  "gu",
);

/**
 * Cuts a text into what is drawn alike — as the page draws it.
 *
 * The marks one writes (`**`, `*`, `` ` ``, `## `) are hidden, the way the
 * page shows none, unless the caret stands in or at their stretch (`open`,
 * a position in this text): then they are there, to be changed. A link is
 * drawn as read — its name — unless the caret stands inside it: then it is
 * the text one writes.
 */
export function runs(text: string, open = -1, around: { after?: boolean; before?: boolean } = {}): Run[] {
  const out: Run[] = [];
  // What the last line was: an empty line after a heading, a list, a quote,
  // a rule or a surface is the room the page leaves there, no line of its own.
  let prev = around.after ? "surface" : "start";
  const plain = (into: Run[], t: string) => {
    if (!t) return;
    const last = into[into.length - 1];
    if (last && !last.look) last.text += t;
    else into.push({ text: t });
  };
  const mark = (t: string, shown: boolean): Run => ({ text: t, look: shown ? "mark" : "mark-hidden" });
  /** The stretches of a piece of a line that starts at `from` in the text. */
  const inline = (into: Run[], piece: string, from: number) => {
    // Drawn faint, at the line's end: the page shows nothing there.
    const anchor = ANCHOR.exec(piece);
    const t = anchor ? piece.slice(0, anchor.index) : piece;
    let at = 0;
    for (const m of t.matchAll(INLINE)) {
      plain(into, t.slice(at, m.index));
      const whole = m[0];
      const start = from + m.index!;
      const end = start + whole.length;
      if (whole.startsWith("@@")) {
        into.push({ text: whole, look: open > start && open < end ? "link-open" : "link" });
      } else if (whole.startsWith("[[") || /^\d{0,3}[dDwW]\d/.test(whole)) {
        into.push({ text: whole, look: "roll" });
      } else {
        const width = whole.startsWith("***") ? 3 : whole.startsWith("**") || whole.startsWith("__") ? 2 : 1;
        const look = whole.startsWith("`") ? "code" : width === 3 ? "bold italic" : width === 2 ? "bold" : "italic";
        const shown = open >= start && open <= end;
        // Bold or italic around a link: the link inside is one too. Code is code.
        const middle: Run[] = [];
        if (look === "code") middle.push({ text: whole.slice(width, -width) });
        else inline(middle, whole.slice(width, -width), start + width);
        into.push({
          text: whole,
          look,
          inner: [mark(whole.slice(0, width), shown), ...(middle.length ? middle : [{ text: "" }]), mark(whole.slice(-width), shown)],
        });
      }
      at = m.index! + whole.length;
    }
    plain(into, t.slice(at));
    if (anchor) {
      into.push({ text: anchor[1], look: "anchor" });
      plain(into, anchor[2]);
    }
  };
  let rest = text;
  // Where the line being cut starts in the text.
  let base = 0;
  while (rest) {
    const nl = rest.indexOf("\n");
    const line = nl < 0 ? rest : rest.slice(0, nl + 1);
    if (line === "\n") {
      const last = rest.length === 1;
      const next = rest.slice(1, rest.indexOf("\n", 1) + 1 || undefined);
      const toSurface = last && !!around.before;
      const toRule = prev === "text" && RULE.test(next) && !HEADING.test(next);
      if (["heading", "item", "quote", "rule", "surface"].includes(prev) || (toSurface && prev !== "blank") || toRule) {
        out.push({ text: line, look: ["gap", `after-${prev}`, toSurface ? "before-surface" : toRule ? "before-rule" : ""].join(" ").trim() });
        prev = "gap";
      } else {
        plain(out, line);
        prev = "blank";
      }
      base += 1;
      rest = rest.slice(1);
      continue;
    }
    const h = HEADING.exec(line);
    if (!h && RULE.test(line)) {
      // As the page draws it — the dashes while the caret is on the line.
      const shown = open >= base && open <= base + line.replace(/\n$/, "").length;
      out.push({ text: line, look: shown ? "rule-open" : "rule" });
      prev = "rule";
      base += line.length;
      rest = rest.slice(line.length);
      continue;
    }
    const item = h ? null : ITEM.exec(line);
    const quote = h || item ? null : QUOTE.exec(line);
    if (h) {
      // A heading takes its line break with it: it stands on a line of its own.
      const hashes = h[1].length + 1;
      const shown = open >= base && open <= base + line.replace(/\n$/, "").length;
      const inner: Run[] = [mark(line.slice(0, hashes), shown)];
      inline(inner, line.slice(hashes), base + hashes);
      out.push({ text: line, look: `heading h${Math.min(7, h[1].length + 1)}`, inner });
      prev = "heading";
    } else if (item || quote) {
      const marker = (item ?? quote)![0];
      const inner: Run[] = [{ text: marker, look: item ? (/\d/.test(item[2]) ? "number" : "bullet") : "quote-mark" }];
      inline(inner, line.slice(marker.length), base + marker.length);
      out.push({ text: line, look: item ? "item" : "quote", inner });
      prev = item ? "item" : "quote";
    } else {
      inline(out, line, base);
      prev = "text";
    }
    base += line.length;
    rest = rest.slice(line.length);
  }
  return out;
}

/** What a link shows without knowing the world: its own text, or the name it names. */
export function shownName(written: string): string {
  const inner = written.slice(2, -2);
  const [target, text] = inner.split("|");
  if (text?.trim()) return text.trim();
  const name = target.split("#")[0];
  const colon = name.indexOf(":");
  return (colon < 0 ? name : name.slice(colon + 1)).trim();
}

/** The link the caret stands inside, as positions in the text — `null` where it is in none. */
export function openLink(text: string, caret: number): [number, number] | null {
  for (const m of text.matchAll(INLINE)) {
    if (!m[0].startsWith("@@")) continue;
    const from = m.index!;
    if (caret > from && caret < from + m[0].length) return [from, from + m[0].length];
  }
  return null;
}

