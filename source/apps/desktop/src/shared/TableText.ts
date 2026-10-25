/**
 * A Markdown table as cells — to change one cell without writing bars:
 *
 * ```text
 * | Item Rarity | Reward |
 * | ----------- | -----: |
 * | Rare        | 500 pp |
 * ```
 *
 * A bar inside a cell is written `\|`. Written back, the columns line up.
 */

export type Align = "" | "left" | "center" | "right";
export type TableModel = { align: Align[]; head: string[]; rows: string[][] };

/** The row under the head: `| --- | :-: |`. */
const DELIMITER = /^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?\s*$/;

const isRow = (line: string) => /^\s*\|/.test(line);

/** How many lines from `i` on make a table — 0 where none begins. */
export function tableAt(lines: string[], i: number): number {
  if (!isRow(lines[i] ?? "") || !DELIMITER.test(lines[i + 1] ?? "") || !(lines[i + 1] ?? "").includes("-")) return 0;
  let end = i + 2;
  while (end < lines.length && isRow(lines[end])) end++;
  return end - i;
}

/** A row's cells, a bar escaped in one kept as a bar. */
export function cells(line: string): string[] {
  let s = line.trim();
  if (s.startsWith("|")) s = s.slice(1);
  if (s.endsWith("|") && !s.endsWith("\\|")) s = s.slice(0, -1);
  const out: string[] = [];
  let cell = "";
  for (let i = 0; i < s.length; i++) {
    if (s[i] === "\\" && s[i + 1] === "|") {
      cell += "|";
      i++;
    } else if (s[i] === "|") {
      out.push(cell.trim());
      cell = "";
    } else cell += s[i];
  }
  out.push(cell.trim());
  return out;
}

export function readTable(text: string): TableModel {
  const lines = text.split("\n").filter((l) => l.trim());
  const head = cells(lines[0] ?? "");
  const align = cells(lines[1] ?? "").map((d): Align => {
    const l = d.startsWith(":");
    const r = d.endsWith(":");
    return l && r ? "center" : r ? "right" : l ? "left" : "";
  });
  const width = head.length;
  const fit = (row: string[]) => Array.from({ length: width }, (_, i) => row[i] ?? "");
  // A row's mark after its last bar belongs to its last cell (`aw_model::markdown`).
  const rows = lines.slice(2).map((l) => {
    const row = cells(l);
    if (row.length === width + 1 && /^\^[\p{L}\p{N}-]{1,64}$/u.test(row[width])) row[width - 1] = `${row[width - 1]} ${row.pop()}`.trim();
    return fit(row);
  });
  return { align: fit(align) as Align[], head, rows };
}

const escapeCell = (c: string) => c.replace(/\n/g, " ").replace(/\|/g, "\\|");

export function writeTable(t: TableModel): string {
  const all = [t.head, ...t.rows].map((r) => r.map(escapeCell));
  const width = t.head.length;
  const size = Array.from({ length: width }, (_, i) => Math.max(3, ...all.map((r) => [...(r[i] ?? "")].length)));
  const pad = (c: string, i: number) => c + " ".repeat(size[i] - [...c].length);
  const line = (r: string[]) => `| ${r.map(pad).join(" | ")} |`;
  const rule = (a: Align, i: number) => {
    const dashes = "-".repeat(size[i] - (a === "center" ? 2 : a ? 1 : 0));
    return a === "center" ? `:${dashes}:` : a === "right" ? `${dashes}:` : a === "left" ? `:${dashes}` : dashes;
  };
  return [line(all[0]), `| ${Array.from({ length: width }, (_, i) => rule(t.align[i] ?? "", i)).join(" | ")} |`, ...all.slice(1).map(line)].join("\n");
}
