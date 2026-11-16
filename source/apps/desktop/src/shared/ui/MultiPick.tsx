import { useEffect, useReducer, useRef, useState, type CSSProperties } from "react";
import { Icon } from "./Icons";
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
            <Icon name="close" />
          </button>
        </span>
      ))}
      {open.length > 0 && <AddPick open={open} onAdd={onAdd} />}
    </span>
  );
}

/** Closes a menu when a click lands outside its box. */
function useClickAway(box: React.RefObject<HTMLSpanElement | null>, shown: boolean, close: () => void) {
  useEffect(() => {
    if (!shown) return;
    const away = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) close();
    };
    document.addEventListener("mousedown", away);
    return () => document.removeEventListener("mousedown", away);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [shown]);
}

/**
 * Where a menu opens: over the page, not in what holds its button — the page
 * or a side bar would cut it off. Below the button, or above where there is
 * more room; as high as the window lets it, and it scrolls itself.
 */
function useMenuPlace(box: React.RefObject<HTMLSpanElement | null>, shown: boolean): CSSProperties {
  const [, again] = useReducer((n: number) => n + 1, 0);
  useEffect(() => {
    if (!shown) return;
    window.addEventListener("resize", again);
    window.addEventListener("scroll", again, true);
    return () => {
      window.removeEventListener("resize", again);
      window.removeEventListener("scroll", again, true);
    };
  }, [shown]);
  const r = box.current?.getBoundingClientRect();
  if (!shown || !r) return {};
  const below = window.innerHeight - r.bottom - 12;
  const above = r.top - 12;
  const width = Math.min(280, window.innerWidth - 16);
  const left = Math.max(8, Math.min(r.left, window.innerWidth - width - 8));
  return below >= 240 || below >= above
    ? { top: r.bottom + 4, left, width, maxHeight: Math.min(520, below) }
    : { bottom: window.innerHeight - r.top + 4, left, width, maxHeight: Math.min(520, above) };
}

/**
 * ＋ and what may be added: a search over the names, the matches below it —
 * Enter takes the first, Escape closes. One after another, it stays open.
 */
export function AddPick({
  open,
  onAdd,
  title = "Add",
}: {
  /** What may be added — or how to find it, asked only once the menu opens. */
  open: ArticleView[] | (() => ArticleView[]);
  onAdd: (a: ArticleView) => void;
  title?: string;
}) {
  const [shown, setShown] = useState(false);
  const [text, setText] = useState("");
  const box = useRef<HTMLSpanElement>(null);
  useClickAway(box, shown, () => setShown(false));
  const place = useMenuPlace(box, shown);
  const needle = text.trim().toLowerCase();
  const matches = !shown ? [] : (typeof open === "function" ? open() : open).filter((c) => !needle || c.name.toLowerCase().includes(needle)).slice(0, 50);
  const take = (c: ArticleView) => {
    onAdd(c);
    setText("");
  };
  return (
    <span className="pick-add" ref={box}>
      <button className="compact" title={title} onClick={() => setShown((s) => !s)}>
        <Icon name="add" />
      </button>
      {shown && (
        <span className="pick-menu" style={place}>
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
 * Several short texts — names someone also goes by: each with ✕ to take it
 * out, ＋ for one more, written and taken with Enter. It stays open for the next.
 */
export function TextPicks({ items, onItems }: { items: string[]; onItems: (items: string[]) => void }) {
  const [shown, setShown] = useState(false);
  const [text, setText] = useState("");
  const box = useRef<HTMLSpanElement>(null);
  useClickAway(box, shown, () => setShown(false));
  const place = useMenuPlace(box, shown);
  // A comma divides them where they are written: none in one of them.
  const take = () => {
    const fresh = text.replace(/,/g, " ").trim();
    if (fresh && !items.includes(fresh)) onItems([...items, fresh]);
    setText("");
  };
  return (
    <span className="multi-pick">
      {items.map((item, i) => (
        <span key={`${i}-${item}`} className="pick">
          {item}
          <button className="pick-x" title={`Remove ${item}`} onClick={() => onItems(items.filter((_, j) => j !== i))}>
            <Icon name="close" />
          </button>
        </span>
      ))}
      <span className="pick-add" ref={box}>
        <button className="compact" title="Add" onClick={() => setShown((s) => !s)}>
          <Icon name="add" />
        </button>
        {shown && (
          <span className="pick-menu" style={place}>
            <input
              autoFocus
              value={text}
              placeholder="Name"
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") take();
                if (e.key === "Escape") setShown(false);
              }}
            />
          </span>
        )}
      </span>
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
