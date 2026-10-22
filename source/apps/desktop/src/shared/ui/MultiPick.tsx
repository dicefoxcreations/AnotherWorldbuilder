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
      {open.length > 0 && (
        <select
          className="pick-add"
          value=""
          title="Add"
          onChange={(e) => {
            const picked = open.find((c) => c.id === e.target.value);
            if (picked) onAdd(picked);
          }}
        >
          <option value="">＋</option>
          {open.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
        </select>
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
