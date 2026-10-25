import { useState } from "react";
import { linker } from "../../shared/Links";
import { readTable, writeTable, type Align, type TableModel } from "../../shared/TableText";
import { useWorldData } from "../worlds/WorldData";
import { InspectorSection, Surface, TagSection } from "./Inspector";
import { MarkdownText } from "./MarkdownText";

/** Where the caret is: a row (-1 the head) and a column. */
type Cell = { row: number; col: number };

/**
 * A table in the visual editor — drawn as it is read. A click in a cell
 * writes in it; Tab and Enter go on. The side bar adds and takes rows and
 * columns and aligns one.
 */
export function TableCard({ text, onChange }: { text: string; onChange: (next: string | null) => void }) {
  const data = useWorldData();
  const table = readTable(text);
  const [at, setAt] = useState<Cell | null>(null);
  const [draft, setDraft] = useState("");
  const width = table.head.length;
  const valueAt = (c: Cell) => (c.row < 0 ? table.head[c.col] : table.rows[c.row]?.[c.col]) ?? "";
  const write = (next: TableModel) => onChange(writeTable(next));
  const withCell = (c: Cell, value: string): TableModel => ({
    ...table,
    head: c.row < 0 ? table.head.map((h, i) => (i === c.col ? value : h)) : table.head,
    rows: table.rows.map((r, j) => (j === c.row ? r.map((x, i) => (i === c.col ? value : x)) : r)),
  });
  const open = (c: Cell) => {
    setAt(c);
    setDraft(valueAt(c));
  };
  /** The cell written goes in; then on to the next, or out. */
  const commit = (next?: Cell) => {
    if (at && draft !== valueAt(at)) write(withCell(at, draft));
    if (next) open(next);
    else setAt(null);
  };
  const after = (c: Cell, back: boolean): Cell | undefined => {
    const flat = (c.row + 1) * width + c.col + (back ? -1 : 1);
    if (flat < 0 || flat >= (table.rows.length + 1) * width) return undefined;
    return { row: Math.floor(flat / width) - 1, col: flat % width };
  };
  const resolve = linker(data?.articles ?? [], data?.templates ?? []);
  const onPlace = data?.onPlace ?? (() => {});
  /** A click in a cell: the one written in stays as it is. */
  const choose = (c: Cell) => {
    if (at && at.row === c.row && at.col === c.col) return;
    if (at) commit(c);
    else open(c);
  };
  const cell = (c: Cell) =>
    at && at.row === c.row && at.col === c.col ? (
      <input
        className="table-cell-input"
        autoFocus
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => commit()}
        onKeyDown={(e) => {
          if (e.key === "Escape") setAt(null);
          else if (e.key === "Tab" || e.key === "Enter") {
            e.preventDefault();
            commit(e.key === "Enter" ? (c.row + 1 < table.rows.length ? { row: c.row + 1, col: c.col } : undefined) : after(c, e.shiftKey));
          }
        }}
      />
    ) : (
      <MarkdownText text={valueAt(c)} resolve={resolve} onPlace={onPlace} />
    );
  const col = at?.col ?? width - 1;
  const row = at && at.row >= 0 ? at.row : table.rows.length - 1;
  const align = (a: Align) => write({ ...table, align: table.align.map((x, i) => (i === col ? a : x)) });
  const style = (i: number) => (table.align[i] ? { textAlign: table.align[i] as "left" } : undefined);
  return (
    <Surface
      label="Table"
      inspector={
        <>
          <InspectorSection title="Rows and columns">
            <div className="segmented">
              <button onClick={() => write({ ...table, rows: [...table.rows.slice(0, row + 1), Array(width).fill(""), ...table.rows.slice(row + 1)] })}>＋ Row</button>
              <button
                onClick={() =>
                  write({
                    align: [...table.align.slice(0, col + 1), "", ...table.align.slice(col + 1)],
                    head: [...table.head.slice(0, col + 1), "", ...table.head.slice(col + 1)],
                    rows: table.rows.map((r) => [...r.slice(0, col + 1), "", ...r.slice(col + 1)]),
                  })
                }
              >
                ＋ Column
              </button>
            </div>
            <div className="segmented">
              <button disabled={table.rows.length === 0} onClick={() => (setAt(null), write({ ...table, rows: table.rows.filter((_, j) => j !== row) }))}>
                − Row
              </button>
              <button
                disabled={width < 2}
                onClick={() => {
                  setAt(null);
                  write({ align: table.align.filter((_, i) => i !== col), head: table.head.filter((_, i) => i !== col), rows: table.rows.map((r) => r.filter((_, i) => i !== col)) });
                }}
              >
                − Column
              </button>
            </div>
          </InspectorSection>
          <InspectorSection title="Column">
            <div className="segmented">
              {(
                [
                  ["", "Auto"],
                  ["left", "Left"],
                  ["center", "Centre"],
                  ["right", "Right"],
                ] as const
              ).map(([v, t]) => (
                <button key={v} className={(table.align[col] ?? "") === v ? "on" : ""} onClick={() => align(v)}>
                  {t}
                </button>
              ))}
            </div>
          </InspectorSection>
          <TagSection code={text.replace(/\n+$/, "")} onCode={onChange} />
          <button className="danger compact inspector-remove" onClick={() => onChange(null)}>
            Remove the table
          </button>
        </>
      }
    >
      <div className="markdown-table table-card">
        <table>
          <thead>
            <tr>
              {table.head.map((_, i) => (
                <th key={i} style={style(i)} onClick={() => choose({ row: -1, col: i })}>
                  {cell({ row: -1, col: i })}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {table.rows.map((r, j) => (
              <tr key={j}>
                {r.map((_, i) => (
                  <td key={i} style={style(i)} onClick={() => choose({ row: j, col: i })}>
                    {cell({ row: j, col: i })}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </Surface>
  );
}
