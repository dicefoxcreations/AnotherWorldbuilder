/**
 * Sections of a text, as its headings make them.
 *
 * A heading folds what follows it, up to the next heading as high or higher —
 * text, a list, a timeline alike. A block may be made of several parts (each
 * its own layer with its own access); a section runs on across them, as the
 * reader sees one text.
 */

/** What a section needs to know of a node at the top of a part. */
export type Top = { heading?: { level: number; text: string } };

/** Where a node stands: hidden under a folded heading, or a heading itself. */
export type Placed = { hidden: boolean; head?: string };

/**
 * Names the headings and says what their folding hides. A heading is named
 * by its level and its text — the same text twice gets a number — so what
 * a reader folded stays folded when the text around it changes.
 */
export function sectioned(parts: Top[][], folded: ReadonlySet<string>): Placed[][] {
  const seen = new Map<string, number>();
  // The level of the folded heading whose section this is.
  let under: number | null = null;
  return parts.map((nodes) =>
    nodes.map(({ heading }) => {
      if (!heading) return { hidden: under !== null };
      if (under !== null && heading.level > under) return { hidden: true };
      const base = `${heading.level}:${heading.text.trim().toLowerCase()}`;
      const n = (seen.get(base) ?? 0) + 1;
      seen.set(base, n);
      const head = n === 1 ? base : `${base}~${n}`;
      under = folded.has(head) ? heading.level : null;
      return { hidden: false, head };
    }),
  );
}

/** The level of a heading line, `## Places` — 0 where it is none. */
export function headingLevel(line: string): number {
  const m = /^(#{1,6}) \S/.exec(line);
  return m ? m[1].length : 0;
}

/**
 * A text cut at its headings of one level: what stands before the first,
 * then each heading's title with what follows it — up to the next as high.
 * Headings in a code fence are none.
 */
export function cutAtHeadings(text: string, level: number): { before: string; sections: { title: string; text: string }[] } {
  const trim = (lines: string[]) => lines.join("\n").replace(/^\n+|\n+$/g, "");
  const before: string[] = [];
  const sections: { title: string; lines: string[] }[] = [];
  let fence = false;
  for (const line of text.split("\n")) {
    if (line.startsWith("```")) fence = !fence;
    if (!fence && headingLevel(line) === level) sections.push({ title: line.slice(level + 1).trim(), lines: [] });
    else if (sections.length > 0) sections[sections.length - 1].lines.push(line);
    else before.push(line);
  }
  return { before: trim(before), sections: sections.map((s) => ({ title: s.title, text: trim(s.lines) })) };
}
