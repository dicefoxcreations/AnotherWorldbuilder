import { useState } from "react";
import { findType, mayCreate } from "../../shared/types";
import { PopupMenu } from "../../shared/ui/PopupMenu";
import { useWorldData } from "../worlds/WorldData";

/**
 * `<create type="Event, Session log" label="New event">`: a button that
 * makes a new document of its type — of several, a click asks which. Only
 * the types this member may make (`mayCreate`): the GM all, a player those
 * whose `create=` lets them — their documents the GM's device makes. Not
 * while previewing. While writing (`still`) it is shown, not pressed.
 */
export function CreateButton({ of, label, still, small }: { of: string[]; label: string; still?: boolean; small?: boolean }) {
  const data = useWorldData();
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  if (!data) return null;
  // By id as stored, by name as written.
  const named = (t: string) => findType(data.templates, t) ?? data.templates.find((x) => x.name.toLowerCase() === t.toLowerCase());
  const types = of.map((t) => named(t.replace(/[«»]/g, "").trim())).filter((t) => !!t && (still || mayCreate(data.world, t)));
  const may = !data.previewing && !!data.onNew;
  if (!still && (!may || types.length === 0)) return null;
  const text = label || (types.length === 1 ? `＋ New ${types[0]!.name}` : "＋ New");
  return (
    <span className={small ? "create-button small" : "create-button"}>
      <button
        className={small ? "ghost compact" : "compact"}
        disabled={types.length === 0}
        title={types.length === 0 ? `No such type: ${of.join(", ") || "none chosen"}` : `New ${types.map((t) => t!.name).join(" or ")}`}
        onClick={(e) => {
          if (still) return;
          if (types.length === 1) data.onNew!(types[0]!.id);
          else setMenu({ x: e.clientX, y: e.clientY });
        }}
      >
        {text}
      </button>
      {menu && (
        <PopupMenu
          x={menu.x}
          y={menu.y}
          title="New"
          items={types.map((t) => ({ label: `${t!.icon ?? ""} ${t!.name}`.trim(), onClick: () => data.onNew!(t!.id) }))}
          onClose={() => setMenu(null)}
        />
      )}
    </span>
  );
}
