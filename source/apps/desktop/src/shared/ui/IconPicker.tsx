import { useEffect, useRef, useState } from "react";

/**
 * Picking an emoji from a curated collection.
 *
 * Deliberately emoji and no shipped image files: they are sharp at every
 * size, need no storage, travel in the world bundle as two characters and
 * look equally good in breadcrumbs and on tiles. An image of its own per
 * article type can be layered on top later without this having to go.
 */

type Group = { name: string; chars: string[] };

export const COLLECTION: Group[] = [
  {
    name: "Worlds",
    chars: ["🌍", "🌎", "🌏", "🗺️", "🧭", "🌌", "🪐", "☀️", "🌙", "⭐", "🌋", "🏔️"],
  },
  {
    name: "Beings",
    chars: ["🧑", "🧝", "🧙", "🧛", "🧟", "🧞", "🧜", "🧚", "👑", "💀", "👹", "🐉"],
  },
  {
    name: "Places",
    chars: ["🏰", "🏯", "🏛️", "⛪", "🕌", "🗼", "🏚️", "⛺", "🏘️", "🌲", "🏜️", "🏝️"],
  },
  {
    name: "Groups",
    chars: ["⚔️", "🛡️", "🏴", "🚩", "⚜️", "🔱", "🤝", "👥", "⛓️", "🗝️", "⚖️", "🎭"],
  },
  {
    name: "Things",
    chars: ["🗡️", "🏹", "🪓", "🔮", "📜", "📕", "🧪", "💍", "💎", "🪙", "🍺", "🕯️"],
  },
  {
    name: "Nature",
    chars: ["🐺", "🐍", "🕷️", "🦅", "🐴", "🌿", "🍄", "🔥", "💧", "❄️", "⚡", "🌊"],
  },
  {
    name: "Chronicle",
    chars: ["📜", "📖", "🗓️", "◷", "⏳", "📌", "🧵", "🗂️", "📝", "🔎", "💬", "🎲"],
  },
];

export function IconPicker({
  value,
  onChange,
  large,
}: {
  value: string;
  onChange: (icon: string) => void;
  large?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const click = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    const key = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", click);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("mousedown", click);
      document.removeEventListener("keydown", key);
    };
  }, [open]);

  return (
    <div className="icon-picker" ref={ref}>
      <button
        className={large ? "icon icon-button large" : "icon icon-button"}
        onClick={() => setOpen((o) => !o)}
        title="Pick an icon"
      >
        {value || "📄"}
      </button>

      {open && (
        <div className="icon-menu">
          {COLLECTION.map((g) => (
            <div key={g.name}>
              <h5>{g.name}</h5>
              <div className="icon-grid">
                {g.chars.map((z) => (
                  <button
                    key={`${g.name}-${z}`}
                    className={z === value ? "ghost on" : "ghost"}
                    onClick={() => {
                      onChange(z);
                      setOpen(false);
                    }}
                    title={z}
                  >
                    {z}
                  </button>
                ))}
              </div>
            </div>
          ))}

          <div className="icon-own">
            <label>
              Your own character
              <input
                value={value}
                onChange={(e) => onChange([...e.target.value].slice(0, 2).join(""))}
                placeholder="⚗️"
              />
            </label>
            <p className="small">
              Any emoji works — even one that is not listed here.
            </p>
          </div>
        </div>
      )}
    </div>
  );
}
