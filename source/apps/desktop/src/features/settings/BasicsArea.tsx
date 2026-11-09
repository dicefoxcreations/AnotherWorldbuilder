import { useState } from "react";
import { api, type WorldInfo } from "../../api/client";
import { USUAL_DICE } from "../dice/DiceTray";

/** The dice that can be rolled here, one or more polyhedral, or pool systems' d6 alone. */
const OFFERED = [2, 3, 4, 6, 8, 10, 12, 20, 100];

/**
 * The world's dice — what the dice bar offers to add by a click. Any die may
 * still be rolled when written (`3d7`); this is only the bar. The GM's.
 */
export function WorldDice({ world, onChanged, onError }: { world: WorldInfo; onChanged: () => void; onError: (e: unknown) => void }) {
  const now = world.dice?.length ? world.dice : USUAL_DICE;
  const [own, setOwn] = useState("");
  const set = (sides: number[]) => api.worldDiceSet(world.id, sides.sort((a, b) => a - b)).then(onChanged).catch(onError);
  // One stays: none would mean the usual set again.
  const toggle = (n: number) => (now.includes(n) ? now.length > 1 && set(now.filter((d) => d !== n)) : set([...now, n]));
  const extra = now.filter((n) => !OFFERED.includes(n));
  const n = Number(own.trim().replace(/^[dDwW]/, ""));
  const sides = Number.isInteger(n) && n >= 2 && n <= 1000 ? n : null;
  const add = () => {
    if (!sides) return;
    setOwn("");
    void set([...now.filter((d) => d !== sides), sides]);
  };
  return (
    <div className="world-dice">
      <h3>In this world</h3>
      <small className="small">In the dice bar, for everyone.</small>
      <div className="time-chips">
        {[...OFFERED, ...extra].map((n) => (
          <button key={n} className={`chip${now.includes(n) ? " on" : ""}`} onClick={() => toggle(n)}>
            d{n}
          </button>
        ))}
        <span className="world-dice-add">
          <input className="world-dice-own" value={own} placeholder="d7" onChange={(e) => setOwn(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} />
          <button className="compact" title="Add this die" disabled={!sides} onClick={add}>
            +
          </button>
        </span>
      </div>
    </div>
  );
}

/** What holds for the whole world — the GM's. */
export function BasicsArea({ world, onChanged, onError }: { world: WorldInfo; onChanged: () => void; onError: (e: unknown) => void }) {
  const run = (step: Promise<unknown>) => step.then(onChanged).catch(onError);
  return (
    <div className="type-page">
      <label className="switch-row">
        <input
          type="number"
          className="version-limit"
          min={1}
          max={1000}
          key={world.versionLimit}
          defaultValue={world.versionLimit}
          onBlur={(e) => {
            const max = Math.round(Number(e.target.value));
            if (max >= 1 && max !== world.versionLimit) run(api.worldVersionsSet(world.id, max));
            else e.target.value = String(world.versionLimit);
          }}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
        />
        <span>
          <strong>Versions per document</strong>
          <small className="small">The oldest go once there are more.</small>
        </span>
      </label>
    </div>
  );
}
