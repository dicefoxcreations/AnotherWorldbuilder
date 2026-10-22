/**
 * Formats what is selected in `text`: marks around it — taken off again
 * where they stand around it already — or each line it touches made a
 * heading, a quote, an item, or back to plain. Returns the new text and the
 * selection in it.
 */
export function formatted(text: string, start: number, end: number, f: { wrap?: string; line?: string }): [string, number, number] {
  if (f.wrap) {
    const m = f.wrap;
    if (text.slice(start - m.length, start) === m && text.slice(end, end + m.length) === m)
      return [text.slice(0, start - m.length) + text.slice(start, end) + text.slice(end + m.length), start - m.length, end - m.length];
    // Space at the edges stays outside the marks: `** word**` would be no mark.
    const inner = text.slice(start, end);
    const lead = inner.length - inner.trimStart().length;
    const trail = inner.length - inner.trimEnd().length;
    const [a, b] = [start + lead, end - trail];
    return [text.slice(0, a) + m + text.slice(a, b) + m + text.slice(b), a + m.length, b + m.length];
  }
  const kind = f.line ?? "";
  const from = text.lastIndexOf("\n", start - 1) + 1;
  const nl = text.indexOf("\n", Math.max(end - 1, start));
  const to = nl < 0 ? text.length : nl;
  const lines = text.slice(from, to).split("\n");
  const bare = (l: string) => l.replace(/^(#{1,6} |> |[-*+] )/, "");
  // Already all of this kind: back to plain.
  const off = lines.every((l) => l.startsWith(kind));
  const out = lines.map((l) => (off ? bare(l) : kind + bare(l))).join("\n");
  return [text.slice(0, from) + out + text.slice(to), from, from + out.length];
}
