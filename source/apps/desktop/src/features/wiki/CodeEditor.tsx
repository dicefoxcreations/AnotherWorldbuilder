import { useEffect, useRef, useState } from "react";
import { Icon } from "../../shared/ui/Icons";
import { type Caret, type LineError } from "../../api/client";
import { withClosingTag } from "../../shared/editor/Closing";
import { useSteadyCaret } from "../../shared/editor/Merge";
import { RemoteCarets, type RemoteCaret } from "../../shared/editor/RemoteCarets";
import { MarkedField } from "../../shared/editor/MarkedField";
import { useLineHeights } from "../../shared/editor/LineHeights";
import { useFieldFind } from "../../shared/ui/FindBar";
import { type MarkKind, type Range } from "../../shared/Marks";
import { type Links, useBricks } from "./EditorTools";

/** Properties as a tree: each by itself, or a group with what lies in it. */
export type Run<T> =
  | { kind: "item"; p: T; i: number }
  | { kind: "group"; name: string; path: string; items: Run<T>[]; count: number };

/**
 * Properties in runs: those of one group together, under its name — a
 * group in a group inside it. The others each on their own.
 */
export function sections<T>(list: T[], pathOf: (x: T) => string[]): Run<T>[] {
  const root: Run<T>[] = [];
  list.forEach((p, i) => {
    let into = root;
    const path = pathOf(p);
    path.forEach((name, depth) => {
      const last = into[into.length - 1];
      const group =
        last?.kind === "group" && last.name === name
          ? last
          : (() => {
              const fresh: Run<T> = { kind: "group", name, path: path.slice(0, depth + 1).join(" › "), items: [], count: 0 };
              into.push(fresh);
              return fresh;
            })();
      if (group.kind !== "group") return;
      group.count++;
      into = group.items;
    });
    into.push({ kind: "item", p, i });
  });
  return root;
}

/** The properties on top — `--- … ---` and the blank lines after — and the rest. */
function splitProperties(text: string): [string, string] {
  const head = /^(?:[ \t]*\n)*---[ \t]*\n(?:[\s\S]*?\n)?---[ \t]*(?:\n|$)\n*/.exec(text)?.[0] ?? "";
  return [head, text.slice(head.length)];
}

export function CodeEditor({
  text,
  errors,
  gm,
  links,
  propsOpen,
  marks,
  carets,
  onPropsOpen,
  onText,
  onCaret,
  movable,
  onMove,
}: {
  text: string;
  errors: LineError[];
  gm: boolean;
  links: Links;
  propsOpen: boolean;
  /** What differs from a version, in the code. */
  marks: { kind: MarkKind; ranges: Range[] };
  /** Where the others write, in the code. */
  carets: RemoteCaret[];
  onPropsOpen: (open: boolean) => void;
  onText: (t: string) => void;
  onCaret: (c: Caret) => void;
  /** Whether the `i`-th block may go one up (-1) or down (1) — and the move. */
  movable: (i: number, by: -1 | 1) => boolean;
  onMove: (i: number, by: -1 | 1) => void;
}) {
  const numbers = useRef<HTMLDivElement>(null);
  // A change that would take a block away is not taken: blocks move by
  // their arrows, they do not go.
  const [kept, setKept] = useState(false);
  useEffect(() => {
    if (!kept) return;
    const t = setTimeout(() => setKept(false), 4000);
    return () => clearTimeout(t);
  }, [kept]);
  const [head, rest] = splitProperties(text);
  // Folded, the field holds only what comes after the properties; the line
  // numbers still count from the top, so errors keep their line.
  const folded = !propsOpen && head !== "";
  const shown = folded ? rest : text;
  const onShown = (t: string) => onText(folded ? head + t : t);
  const offset = folded ? head.split("\n").length - 1 : 0;
  const count = head.split("\n").filter((l) => l.trim() && l.trim() !== "---").length;

  // Whoever types the properties in by hand should not see them vanish the
  // moment the closing --- stands there.
  const hadHead = useRef(head !== "");
  useEffect(() => {
    if (head !== "" && !hadHead.current) onPropsOpen(true);
    hadHead.current = head !== "";
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [head !== ""]);

  const bricks = useBricks(gm, shown, onShown, links);
  const caret = useSteadyCaret(bricks.props.ref, shown);
  const found = useFieldFind(bricks.props.ref);
  const lines = useLineHeights(bricks.props.ref, shown);
  const errorLines = new Set(errors.map((e) => e.line));
  const blockLines = linesOfBlocks(shown);
  // Folded, the field starts after the properties.
  const skip = folded ? head.length : 0;
  const report = (e: React.SyntheticEvent<HTMLTextAreaElement>) =>
    onCaret({ code: text, start: e.currentTarget.selectionStart + skip, end: e.currentTarget.selectionEnd + skip });
  const theirs = carets
    .map((c) => ({ ...c, start: c.start - skip, end: c.end - skip }))
    .filter((c) => c.end >= 0);
  const shownMarks = marks.ranges
    .map((m) => ({ start: m.start - skip, end: m.end - skip }))
    .filter((m) => m.end > 0)
    .map((m) => ({ start: Math.max(0, m.start), end: m.end }));
  return (
    <div className="text-side">
      {head !== "" && (
        <button className="fold code-fold ghost compact" onClick={() => onPropsOpen(!propsOpen)} aria-expanded={propsOpen}>
          {propsOpen ? <Icon name="chevron-down" /> : <Icon name="chevron-right" />} Properties <small className="small">{count}</small>
        </button>
      )}
      <div className="text-field">
        <div className="line-numbers" ref={numbers} aria-hidden="true">
          {shown.split("\n").map((_, i) => {
            const b = blockLines[i];
            const cls = [errorLines.has(i + 1 + offset) && "stuck", b && "in-block", b?.first && "block-first", b?.last && "block-last"].filter(Boolean).join(" ");
            return (
              <span key={i} className={cls} style={lines[i] ? { height: lines[i] } : undefined}>
                {/* Alone where it stands — in the text, at the side — nothing to move. */}
                {b?.first && (movable(b.index, -1) || movable(b.index, 1)) && (
                  <b className="block-moves">
                    <button tabIndex={-1} title="Up" disabled={!movable(b.index, -1)} onClick={() => onMove(b.index, -1)}>
                      <Icon name="up" size={14} />
                    </button>
                    <button tabIndex={-1} title="Down" disabled={!movable(b.index, 1)} onClick={() => onMove(b.index, 1)}>
                      <Icon name="down" size={14} />
                    </button>
                  </b>
                )}
                {i + 1 + offset}
              </span>
            );
          })}
        </div>
        <div className="caret-box">
          <textarea
            {...bricks.props}
            {...caret}
            onSelect={(e) => {
              caret.onSelect();
              report(e);
            }}
            onKeyUp={(e) => {
              caret.onKeyUp();
              report(e);
            }}
            className="template-text"
            value={shown}
            onChange={(e) => {
              const field = e.currentTarget;
              if (blocksIn(field.value) < blockLines.filter((b) => b?.last).length) {
                const at = Math.min(field.selectionStart, shown.length);
                setKept(true);
                queueMicrotask(() => field.setSelectionRange(at, at));
                return;
              }
              const closed = withClosingTag(shown, field.value, field.selectionStart);
              onShown(closed?.text ?? field.value);
              if (closed) queueMicrotask(() => field.setSelectionRange(closed.caret, closed.caret));
            }}
            onScroll={(e) => {
              if (numbers.current) numbers.current.scrollTop = e.currentTarget.scrollTop;
            }}
            spellCheck={false}
            placeholder="Write here. @@ links a document, right-click inserts lists, images …"
          />
          <MarkedField field={bricks.props.ref} text={shown} ranges={shownMarks} kind={marks.kind} found={found} />
          <RemoteCarets field={bricks.props.ref} carets={theirs} />
        </div>
      </div>
      {kept && <p className="small muted block-kept">Sections stay — move them with the arrows.</p>}
      {errors.length > 0 && (
        <ul className="error-list">
          {errors.map((e, i) => (
            <li key={i}>
              <span>{e.line > 0 ? `Line ${e.line}` : "Document"}</span>
              {e.text}
            </li>
          ))}
        </ul>
      )}
      {bricks.element}
    </div>
  );
}

/** How many blocks a text holds — whole ones, opened and closed; the parts inside a block do not count. */
export function blocksIn(text: string): number {
  return linesOfBlocks(text).filter((b) => b?.last).length;
}

/**
 * Per line of a document's code: which block it lies in — the `index`-th —
 * and whether it opens or closes it. A `<block>` inside a block is a part
 * of it, not a block of its own.
 */
export function linesOfBlocks(text: string): ({ index: number; first: boolean; last: boolean } | null)[] {
  let index = -1;
  let depth = 0;
  return text.split("\n").map((line) => {
    if (/^\s*<block\b/.test(line)) {
      depth++;
      if (depth === 1) {
        index++;
        return { index, first: true, last: false };
      }
      return { index, first: false, last: false };
    }
    if (/^\s*<\/block>/.test(line) && depth > 0) {
      depth--;
      return { index, first: false, last: depth === 0 };
    }
    return depth > 0 ? { index, first: false, last: false } : null;
  });
}
