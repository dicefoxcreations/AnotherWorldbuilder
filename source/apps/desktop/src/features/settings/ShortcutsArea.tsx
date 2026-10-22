import { useState } from "react";
import type { Template, WorldInfo } from "../../api/client";
import { comboOf, FIXED, freeFor, loadShortcuts, saveShortcuts, shownCombo, type Combo, type TypeShortcut } from "../../shared/Shortcuts";

/**
 * The keys: those every world has, and a key per type — the selected words
 * become a link to a document of it, made where there is none.
 */
export function ShortcutsArea({ world, templates }: { world: WorldInfo; templates: Template[] }) {
  const [mine, setMine] = useState<TypeShortcut[]>(() => loadShortcuts(world.id));
  const [combo, setCombo] = useState<Combo | null>(null);
  const [type, setType] = useState("");
  const types = templates.filter((t) => !t.group).sort((a, b) => a.name.localeCompare(b.name));
  const change = (next: TypeShortcut[]) => {
    setMine(next);
    saveShortcuts(world.id, next);
  };
  const used = combo !== null && mine.some((s) => s.combo === combo);
  const blocked = combo !== null && !freeFor(combo);
  return (
    <div className="type-page appearance shortcuts">
      <h2>Shortcuts</h2>
      <p className="small">On this device, for this world.</p>
      <table className="shortcut-table">
        <tbody>
          {FIXED.map((f) => (
            <tr key={f.combo}>
              <td>
                <kbd>{shownCombo(f.combo)}</kbd>
              </td>
              <td>{f.what}</td>
              <td />
            </tr>
          ))}
          {mine.map((s) => (
            <tr key={s.combo}>
              <td>
                <kbd>{shownCombo(s.combo)}</kbd>
              </td>
              <td>
                Link to a {templates.find((t) => t.id === s.type)?.name ?? "missing type"}
              </td>
              <td>
                <button className="ghost compact" title="Remove" onClick={() => change(mine.filter((x) => x.combo !== s.combo))}>
                  ×
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <h3>New</h3>
      <div className="line shortcut-new">
        <input
          className="shortcut-key"
          readOnly
          value={combo ? shownCombo(combo) : ""}
          placeholder="Press keys"
          onKeyDown={(e) => {
            if (e.key === "Tab") return;
            e.preventDefault();
            const c = comboOf(e.nativeEvent);
            if (c) setCombo(c);
          }}
        />
        <select value={type} onChange={(e) => setType(e.target.value)}>
          <option value="">Type …</option>
          {types.map((t) => (
            <option key={t.id} value={t.id}>
              {t.icon} {t.name}
            </option>
          ))}
        </select>
        <button
          className="primary compact"
          disabled={!combo || !type || used || blocked}
          onClick={() => {
            if (!combo || !type) return;
            change([...mine, { combo, type }].sort((a, b) => a.combo.localeCompare(b.combo)));
            setCombo(null);
            setType("");
          }}
        >
          Add
        </button>
      </div>
      {(used || blocked) && <p className="small warning">{shownCombo(combo!)} is taken.</p>}
      <p className="small muted">Select words, press the keys: a link to the document of that name — made where there is none.</p>
    </div>
  );
}
