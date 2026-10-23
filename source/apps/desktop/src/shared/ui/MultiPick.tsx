import { useEffect, useRef, useState } from "react";
import type { ArticleView } from "../../api/client";

/**
 * Several documents picked: each one with a × to take it out, and one
 * picker for the rest — a click either way. Where each pick carries a number
 * of its own (`with Level`), it stands beside the name to be changed.
 */
export function MultiPick({
  chosen,
  open,
  onRemove,
  onAdd,
  count,
  onCount,
}: {
  chosen: { key: string; name: string; count?: string }[];
  open: ArticleView[];
  onRemove: (i: number) => void;
  onAdd: (a: ArticleView) => void;
  /** The name of the number each pick carries — `Level`. */
  count?: string;
  onCount?: (i: number, n: string) => void;
}) {
  return (
    <span className="multi-pick">
      {chosen.map((c, i) => (
        <span key={c.key} className="pick">
          {c.name}
          {count && onCount && (
            <input
              className="pick-count"
              inputMode="decimal"
              size={Math.max(1, (c.count ?? "").length)}
              value={c.count ?? ""}
              placeholder="–"
              title={`${count} — ${c.name}`}
              onChange={(e) => onCount(i, e.target.value)}
            />
          )}
          <button className="pick-x" title={`Remove ${c.name}`} onClick={() => onRemove(i)}>
            ×
          </button>
        </span>
      ))}
      {open.length > 0 && <AddPick open={open} onAdd={onAdd} />}
    </span>
  );
}


/**
 * ＋ and what may be added: a search over the names, the matches below it —
 * Enter takes the first, Escape closes. One after another, it stays open.
 */
function AddPick({ open, onAdd }: { open: ArticleView[]; onAdd: (a: ArticleView) => void }) {
  const [shown, setShown] = useState(false);
  const [text, setText] = useState("");
  const box = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (!shown) return;
    const away = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setShown(false);
    };
    document.addEventListener("mousedown", away);
    return () => document.removeEventListener("mousedown", away);
  }, [shown]);
  const needle = text.trim().toLowerCase();
  const matches = open.filter((c) => !needle || c.name.toLowerCase().includes(needle)).slice(0, 50);
  const take = (c: ArticleView) => {
    onAdd(c);
    setText("");
  };
  return (
    <span className="pick-add" ref={box}>
      <button className="compact" title="Add" onClick={() => setShown((s) => !s)}>
        ＋
      </button>
      {shown && (
        <span className="pick-menu">
          <input
            autoFocus
            value={text}
            placeholder="Search"
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && matches[0]) take(matches[0]);
              if (e.key === "Escape") setShown(false);
            }}
          />
          <span className="pick-options">
            {matches.map((c) => (
              <button key={c.id} className="ghost" onClick={() => take(c)}>
                {c.name}
              </button>
            ))}
            {matches.length === 0 && <small className="small muted">nothing</small>}
          </span>
        </span>
      )}
    </span>
  );
}

/**
 * Every document of a type, each with its number — skills and how well.
 * Nothing to add or take out: the type says which there are.
 */
export function EveryPick({
  items,
  count,
  onCount,
}: {
  items: { key: string; name: string; count: string }[];
  /** The name of the number — `Proficiency`. */
  count: string;
  onCount: (i: number, n: string) => void;
}) {
  return (
    <span className="every-pick">
      {items.map((c, i) => (
        <span key={c.key} className={`pick${c.count && Number(c.count) !== 0 ? " set" : ""}`}>
          {c.name}
          <input
            className="pick-count"
            inputMode="decimal"
            size={Math.max(1, c.count.length)}
            value={c.count}
            placeholder="0"
            title={`${count} — ${c.name}`}
            onChange={(e) => onCount(i, e.target.value)}
          />
        </span>
      ))}
    </span>
  );
}
