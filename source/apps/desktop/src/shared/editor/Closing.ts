/**
 * Closing tags as one types: `<block …>` brings its `</block>` along, and so
 * do `<infobox …>` and `<graph …>`.
 *
 * A block stands between its tags, and one without the second is an error.
 * So whoever finishes the opening tag gets the closing one at once, and the
 * caret stands on the empty line between them.
 */

const PAIRED = /^<(block|infobox|graph)\b[^<>]*>$/;

/**
 * After a keystroke: if it finished an opening `<block …>` or `<infobox …>`
 * that is not closed yet, the text with its closing tag — otherwise `null`.
 *
 * `before` is the text before the keystroke, `after` the one after it,
 * `caret` where the caret stands now.
 */
export function withClosingTag(
  before: string,
  after: string,
  caret: number,
): { text: string; caret: number } | null {
  // Only a single typed `>` — not a paste, not a deletion.
  if (after.length !== before.length + 1 || after[caret - 1] !== ">") return null;
  const open = after.lastIndexOf("<", caret - 1);
  const tag = open < 0 ? null : PAIRED.exec(after.slice(open, caret));
  if (!tag) return null;
  const name = tag[1];

  // Already closed further down — someone is only fixing the tag.
  let depth = 0;
  for (const m of after.slice(caret).matchAll(new RegExp(`<(/?)${name}\\b[^<>]*>`, "g"))) {
    if (m[1]) {
      if (depth === 0) return null;
      depth -= 1;
    } else depth += 1;
  }

  const lineStart = after.lastIndexOf("\n", open - 1) + 1;
  const indent = /^[ \t]*/.exec(after.slice(lineStart, open))?.[0] ?? "";
  // Indented code indents what stands inside; flat code stays flat.
  const inner = indent ? `${indent}  ` : "";
  // What stood after the caret on that line goes below the closing tag —
  // `</body>` must keep a line of its own.
  const rest = after.slice(caret);
  const lineRest = rest.slice(0, rest.indexOf("\n") < 0 ? rest.length : rest.indexOf("\n"));
  const tail = lineRest.trim() ? `\n${indent}${rest.trimStart()}` : rest;
  const text = `${after.slice(0, caret)}\n${inner}\n${indent}</${name}>${tail}`;
  return { text, caret: caret + 1 + inner.length };
}
