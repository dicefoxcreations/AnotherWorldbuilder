import { useEffect, useState } from "react";
import { api, type Favorite, type WorldInfo } from "../../api/client";
import { parse } from "../../shared/Dice";
import { DEFAULT_DICE, loadDice, saveDice, type DiceSettings } from "../dice/DiceLook";
import { tryDice } from "../dice/DiceTray";
import { ColorRow } from "./Appearance";

const SHOWN: [DiceSettings["show"], string][] = [
  ["all", "Everyone's"],
  ["mine", "Mine"],
  ["off", "Off"],
];

/**
 * How dice look on this device: whose roll across the window, in which
 * colours — the die and its numbers — and whether they sound.
 */
export function DiceArea({ world, rolls }: { world?: React.ReactNode; rolls?: React.ReactNode }) {
  const [dice, setDice] = useState<DiceSettings>(loadDice);
  const change = (next: DiceSettings) => {
    setDice(next);
    saveDice(next);
  };
  return (
    <div className="type-page appearance">
      <h2>Dice</h2>
      {world}
      {rolls}
      <h3>On this device</h3>
      <p className="small">For every world.</p>
      <div className="color-rows">
        <div className="setting-row">
          <span>3D dice</span>
          <div className="segmented">
            {SHOWN.map(([v, t]) => (
              <button key={v} className={dice.show === v ? "on" : ""} onClick={() => change({ ...dice, show: v })}>
                {t}
              </button>
            ))}
          </div>
        </div>
        <div className="setting-row">
          <span>Sound</span>
          <div className="segmented">
            {([true, false] as const).map((v) => (
              <button key={String(v)} className={dice.sound === v ? "on" : ""} onClick={() => change({ ...dice, sound: v })}>
                {v ? "On" : "Off"}
              </button>
            ))}
          </div>
        </div>
        <ColorRow label="Die" value={dice.die} onChange={(die) => change({ ...dice, die })} />
        <ColorRow label="Number" value={dice.number} onChange={(number) => change({ ...dice, number })} />
      </div>
      <div className="dice-try">
        <button className="primary" onClick={tryDice}>
          Throw
        </button>
        <button onClick={() => change(DEFAULT_DICE)}>Reset</button>
      </div>
    </div>
  );
}

/** `Fireball: 8d6` — a name before the colon. */
const split = (text: string): Favorite => {
  const at = text.indexOf(":");
  return at > 0 ? { name: text.slice(0, at).trim(), formula: text.slice(at + 1).trim() } : { name: "", formula: text.trim() };
};

/** Rolls kept to roll again with a click in the dice bar: `4d6`, `Fireball: 8d6`. */
export function SavedRolls({ world, onError }: { world: WorldInfo; onError: (e: unknown) => void }) {
  const [saved, setSaved] = useState<Favorite[]>([]);
  const [line, setLine] = useState("");
  useEffect(() => {
    api
      .diceLog(world.id)
      .then((l) => setSaved(l.favorites))
      .catch(onError);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [world.id]);
  const keep = (next: Favorite[]) =>
    api
      .diceFavoritesSet(world.id, next)
      .then((l) => setSaved(l.favorites))
      .catch(onError);
  const { name, formula } = split(line);
  let problem: string | null = null;
  try {
    if (formula) parse(formula);
  } catch (e) {
    problem = String(e instanceof Error ? e.message : e);
  }
  const add = () => {
    if (!formula || problem) return;
    setLine("");
    void keep([...saved, { name: name || formula, formula }]);
  };
  return (
    <div className="world-dice">
      <h3>Saved rolls</h3>
      <small className="small">In the dice bar, on this device.</small>
      <div className="time-chips">
        {saved.map((f, i) => (
          <span key={i} className="chip dice-favorite" title={f.formula}>
            <span className="dice-favorite-name">{f.name === f.formula ? f.formula : `${f.name} · ${f.formula}`}</span>
            <button className="ghost" title="Remove" onClick={() => void keep(saved.filter((_, j) => j !== i))}>
              ×
            </button>
          </span>
        ))}
        <span className="world-dice-add">
          <input className="saved-roll-own" value={line} placeholder="Fireball: 8d6" onChange={(e) => setLine(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} />
          <button className="compact" title="Save this roll" disabled={!formula || !!problem} onClick={add}>
            +
          </button>
        </span>
      </div>
      {formula && problem && <small className="small warning">{problem}</small>}
    </div>
  );
}
