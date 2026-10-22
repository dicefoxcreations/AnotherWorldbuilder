import { useEffect, useRef, useState, type CSSProperties } from "react";

export type Pickable = { id: string; name: string; icon?: string; hint?: string };

/**
 * Picks from many by typing: what is chosen stands as chips with ×, a field
 * beside them finds the rest — by name, as one types — and Enter or a click
 * adds it. With `single`, a new choice takes the old one's place. However
 * many there are to pick from, only the chosen ones take room.
 */
export function SearchPick({
  items,
  chosen,
  onAdd,
  onRemove,
  placeholder,
  none,
  single = false,
}: {
  items: Pickable[];
  chosen: string[];
  onAdd: (id: string) => void;
  onRemove: (id: string) => void;
  placeholder: string;
  /** What it means when nothing is chosen: “all”. */
  none?: string;
  single?: boolean;
}) {
  const [text, setText] = useState("");
  const [open, setOpen] = useState(false);
  const [at, setAt] = useState(0);
  const field = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLUListElement>(null);
  // The one the arrow keys stand on stays in sight.
  useEffect(() => list.current?.querySelector(".on")?.scrollIntoView({ block: "nearest" }), [at]);
  const query = text.trim().toLowerCase();
  const found = items
    .filter((i) => !chosen.includes(i.id) && (!query || i.name.toLowerCase().includes(query)))
    // What begins with the text first, then what holds it.
    .sort((a, b) => Number(!a.name.toLowerCase().startsWith(query)) - Number(!b.name.toLowerCase().startsWith(query)));
  // Over the page, not in what holds the field: a side bar would cut it off.
  // Below the field, or above where there is more room; it scrolls itself.
  const place = (): CSSProperties => {
    const r = field.current?.getBoundingClientRect();
    if (!r) return {};
    const below = window.innerHeight - r.bottom - 12;
    const above = r.top - 12;
    const width = Math.min(320, window.innerWidth - 16);
    const left = Math.max(8, Math.min(r.left, window.innerWidth - width - 8));
    return below >= 200 || below >= above
      ? { top: r.bottom + 4, left, maxWidth: width, maxHeight: Math.min(360, below) }
      : { bottom: window.innerHeight - r.top + 4, left, maxWidth: width, maxHeight: Math.min(360, above) };
  };
  const add = (id: string) => {
    if (single) chosen.forEach(onRemove);
    onAdd(id);
    setText("");
    setAt(0);
  };
  return (
    <div className="search-pick">
      {chosen.map((id) => {
        const item = items.find((i) => i.id === id);
        return (
          <span key={id} className="chip on search-pick-chip" title={item?.hint}>
            {item?.icon && <span>{item.icon}</span>}
            {item?.name ?? id}
            <button className="pick-x" title={`Remove ${item?.name ?? id}`} onClick={() => onRemove(id)}>
              ×
            </button>
          </span>
        );
      })}
      {chosen.length === 0 && none && <span className="small muted">{none}</span>}
      <span className="search-pick-field">
        <input
          ref={field}
          value={text}
          placeholder={placeholder}
          onChange={(e) => {
            setText(e.target.value);
            setOpen(true);
            setAt(0);
          }}
          onFocus={() => setOpen(true)}
          onBlur={() => setTimeout(() => setOpen(false), 150)}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") setAt((n) => Math.min(n + 1, found.length - 1));
            else if (e.key === "ArrowUp") setAt((n) => Math.max(n - 1, 0));
            else if (e.key === "Enter" && found[at]) add(found[at].id);
            else if (e.key === "Escape") setOpen(false);
            else return;
            e.preventDefault();
          }}
        />
        {open && found.length > 0 && (
          <ul ref={list} className="search-pick-list" role="listbox" style={place()}>
            {found.map((i, n) => (
              <li key={i.id}>
                <button className={n === at ? "on" : ""} onMouseDown={(e) => e.preventDefault()} onClick={() => add(i.id)}>
                  {i.icon && <span>{i.icon}</span>}
                  {i.name}
                  {i.hint && <small>{i.hint}</small>}
                </button>
              </li>
            ))}
          </ul>
        )}
      </span>
    </div>
  );
}
