/**
 * Where a brick goes.
 *
 * Everything except the skeleton belongs between `<body>` and `</body>`. If
 * the cursor was elsewhere — in the bracket, in the head, below
 * `</article>` — the brick still lands there: a tag outside the body would
 * only be an error one has to tidy away afterwards.
 */

export type Spot = {
  /** Character position after which the new line is inserted. */
  end: number;
  /** Indent every inserted line gets. */
  indent: string;
};

export function insertionSpot(text: string, cursor: number, head = false): Spot {
  const lineBreak = text.indexOf("\n", cursor);
  const atCursor = lineBreak === -1 ? text.length : lineBreak;
  if (head) return { end: atCursor, indent: "" };

  const bodyOpen = text.indexOf("<body>");
  const bodyClose = text.indexOf("</body>");
  if (bodyOpen < 0 || bodyClose <= bodyOpen) return { end: atCursor, indent: "" };

  // The end of the <body> line and the end of the line before the one with
  // </body>: the body lies in between.
  const afterOpen = text.indexOf("\n", bodyOpen);
  const beforeClose = text.lastIndexOf("\n", bodyClose);
  if (afterOpen < 0) return { end: atCursor, indent: "" };

  const inside = atCursor >= afterOpen && atCursor <= beforeClose;
  const depth = /^ */.exec(text.slice(beforeClose + 1))?.[0].length ?? 4;
  return {
    end: inside ? atCursor : Math.max(afterOpen, beforeClose),
    indent: " ".repeat(depth + 2),
  };
}
