import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { api, type DiceLog, type Favorite, type SharedRoll, type WorldInfo } from "../../api/client";
import { pace } from "../../app/live";
import { rollFormula, type Lookup } from "../../shared/Dice";
import { useSideBar } from "../../shared/ui/SideBar";
import type { DiceTable } from "./Dice3D";
import { loadDice } from "./DiceLook";
import { sound } from "./DiceSound";

/**
 * The dice: whatever is rolled — from a document, or typed in the side bar
 * — tumbles across the window and lands in the log, one's own rolls and,
 * with the network on, everyone's in the session.
 *
 * A roll goes to the others unless one rolls for oneself (the switch in the
 * side bar) or rolls from a part only the GM sees: what a secret monster
 * hits with stays secret. Each device keeps its own log of a world; the GM
 * can clear everyone's.
 */

/** What a roll in the text knows of where it stands. */
export type RollScope = {
  /** The document's name — what the roll is said to be from. */
  document: string;
  /** Its numbers, with what is added to them — see `shared/Sheet`. */
  lookup: Lookup;
  /** How a number came about — `15 + 1 Athlete` — or empty. */
  explain: (name: string) => string;
  /** Its type's number properties, by name — what an ability row can show. */
  numbers: string[];
  /** A part only the GM sees: its rolls stay here. */
  secret: boolean;
};

export const RollScopeContext = createContext<RollScope | null>(null);

/** The bold words a paragraph starts with — `Scimitar.` — name its rolls. */
export const RollLabel = createContext<string>("");

type Dice = {
  roll: (r: { document: string; label: string; formula: string; lookup: Lookup; secret: boolean }) => void;
  /** The side bar with the log: open or not, and what came since it was. */
  open: boolean;
  unseen: number;
  toggle: () => void;
};

const DiceContext = createContext<Dice | null>(null);

export const useDice = () => useContext(DiceContext);

const EVERYONE_KEY = "aw.dice.everyone";
const POLL_MS = 1500;
const TOAST_MS = 4000;
/** The dice one can add by a click, where the world's GM chose none. */
export const USUAL_DICE = [4, 6, 8, 10, 12, 20, 100];

const readEveryone = () => {
  try {
    return localStorage.getItem(EVERYONE_KEY) !== "no";
  } catch {
    return true;
  }
};

const newId = () => {
  const a = new Uint8Array(8);
  crypto.getRandomValues(a);
  return Array.from(a, (b) => b.toString(16).padStart(2, "0")).join("");
};

/** A roll typed in: no document, no names — numbers and dice only. */
const plain: Lookup = Object.assign(() => undefined, { rules: [] });

let table: Promise<DiceTable> | null = null;
/** The table, loaded with the first throw — three.js is large, and not needed before. */
const theTable = () => (table ??= import("./Dice3D").then((m) => new m.DiceTable()));

/**
 * Throws the dice onto the window, as this device likes them; they sound as
 * the first one lands. Without 3D they only sound.
 */
function throwDice(dice: [number, number][], mine: boolean) {
  const look = loadDice();
  const shown = look.show === "all" || (look.show === "mine" && mine);
  if (!shown) {
    if (look.sound && mine) sound();
    return;
  }
  theTable()
    .then((t) => {
      const landing = t.throw(dice.map(([sides, value]) => ({ sides, value })), look);
      if (look.sound) sound(landing ?? 0);
    })
    // No 3D here (no WebGL): the log says what was rolled all the same.
    .catch(() => look.sound && sound());
}

/** For the options: a throw to see the colours by. */
export function tryDice() {
  throwDice([4, 6, 8, 10, 12, 20].map((sides) => [sides, sides]), true);
}

export function DiceTray({ world, children }: { world: WorldInfo; children: ReactNode }) {
  const [log, setLog] = useState<DiceLog>({ rolls: [], favorites: [] });
  const [everyone, setEveryone] = useState(readEveryone);
  const [problem, setProblem] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const [unseen, setUnseen] = useState(0);
  const [toast, setToast] = useState<SharedRoll | null>(null);
  const toastTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  // Rolls already taken in, by id — the poll brings them again.
  const known = useRef(new Set<string>());
  const openRef = useRef(open);
  openRef.current = open;
  const me = useRef<string | undefined>(undefined);
  useSideBar("dice", open, () => setOpen(false));

  useEffect(() => {
    api
      .diceLog(world.id)
      .then((l) => {
        l.rolls.forEach((r) => known.current.add(r.id));
        setLog(l);
      })
      .catch(() => {});
  }, [world.id]);

  /** One roll arrived — one's own or someone's — into the log. */
  const take = useCallback(
    (r: SharedRoll, mine: boolean) => {
      if (known.current.has(r.id)) return;
      known.current.add(r.id);
      api.diceLogAdd(world.id, r).then(setLog).catch(() => {});
      if (r.mark === "clear") return;
      if (!mine) throwDice(r.dice, false);
      if (!openRef.current) {
        if (!mine) setUnseen((n) => n + 1);
        setToast(r);
        clearTimeout(toastTimer.current);
        toastTimer.current = setTimeout(() => setToast(null), TOAST_MS);
      }
    },
    [world.id],
  );

  const roll: Dice["roll"] = useCallback(
    ({ document, label, formula, lookup, secret }) => {
      let r: ReturnType<typeof rollFormula>;
      try {
        r = rollFormula(formula, lookup);
      } catch (e) {
        setProblem(e instanceof Error ? e.message : String(e));
        return;
      }
      setProblem(null);
      const hidden = secret || !everyone;
      const entry: SharedRoll = {
        id: newId(),
        member: me.current,
        document,
        label,
        formula: r.formula,
        detail: r.detail,
        total: r.total,
        dice: r.dice.map((d) => [d.sides, d.value]),
        ...(r.mark ? { mark: r.mark } : {}),
        at: Date.now(),
        ...(hidden ? { secret: true } : {}),
      };
      throwDice(entry.dice, true);
      take(entry, true);
      if (!hidden) api.diceRoll(world.id, entry).catch(() => {});
    },
    [everyone, world.id, take],
  );

  // The others' rolls, while the network is on.
  useEffect(() => {
    let gone = false;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      if (pace.network) {
        try {
          for (const r of await api.diceRolls(world.id)) if (r.member !== me.current || r.mark === "clear") take(r, false);
        } catch {
          // The next round tries again.
        }
      }
      if (!gone) timer = setTimeout(() => void poll(), POLL_MS);
    };
    void poll();
    return () => {
      gone = true;
      clearTimeout(timer);
    };
  }, [world.id, take]);

  // Who one is: one's own rolls come back from the network too.
  useEffect(() => {
    api
      .identityStatus()
      .then((i) => (me.current = i?.id))
      .catch(() => {});
  }, []);

  const toggleEveryone = () => {
    setEveryone((v) => {
      try {
        localStorage.setItem(EVERYONE_KEY, v ? "no" : "yes");
      } catch {
        // Only a convenience.
      }
      return !v;
    });
  };
  const toggle = () =>
    setOpen((o) => {
      if (!o) {
        setUnseen(0);
        setToast(null);
      }
      return !o;
    });

  return (
    <DiceContext.Provider value={{ roll, open, unseen, toggle }}>
      {children}
      {open && (
        <DicePanel
          world={world}
          log={log}
          everyone={everyone}
          problem={problem}
          onEveryone={toggleEveryone}
          onRoll={(formula, label) => roll({ document: "", label, formula, lookup: plain, secret: false })}
          onFavorites={(favorites) => api.diceFavoritesSet(world.id, favorites).then(setLog).catch(() => {})}
          onClear={() => api.diceLogClear(world.id).then(setLog).catch((e) => setProblem(String(e)))}
          onClose={() => setOpen(false)}
          mine={(r) => r.member === undefined || r.member === me.current}
        />
      )}
      {!open && (toast || problem) && (
        <aside className="dice-toast" aria-live="polite" onClick={toggle}>
          {toast && (
            <ol className="dice-log">
              <RollLine entry={toast} mine={toast.member === undefined || toast.member === me.current} />
            </ol>
          )}
          {problem && <p className="dice-problem">{problem}</p>}
        </aside>
      )}
    </DiceContext.Provider>
  );
}

/** The 🎲 in the header bar: opens the side bar, counts what came meanwhile. */
export function DiceButton() {
  const dice = useDice();
  if (!dice) return null;
  return (
    <button className={dice.open ? "ghost icon round on" : "ghost icon round"} onClick={dice.toggle} title="Dice">
      🎲
      {dice.unseen > 0 && <span className="badge">{dice.unseen}</span>}
    </button>
  );
}

/**
 * The side bar: a line to type a roll in — `1d20 + 3`, or `Attack: 1d20 + 5`
 * — the dice to add by a click, the favourites to roll with one, and the log.
 */
function DicePanel({
  world,
  log,
  everyone,
  problem,
  onEveryone,
  onRoll,
  onFavorites,
  onClear,
  onClose,
  mine,
}: {
  world: WorldInfo;
  log: DiceLog;
  everyone: boolean;
  problem: string | null;
  onEveryone: () => void;
  onRoll: (formula: string, label: string) => void;
  onFavorites: (f: Favorite[]) => void;
  onClear: () => void;
  onClose: () => void;
  mine: (r: SharedRoll) => boolean;
}) {
  const [line, setLine] = useState("");
  const list = useRef<HTMLOListElement>(null);
  useEffect(() => {
    list.current?.scrollTo({ top: list.current.scrollHeight });
  }, [log.rolls.length]);
  // `Attack: 1d20 + 5` — a name before the colon.
  const split = (text: string): Favorite => {
    const at = text.indexOf(":");
    return at > 0 ? { name: text.slice(0, at).trim(), formula: text.slice(at + 1).trim() } : { name: "", formula: text.trim() };
  };
  const go = () => {
    const { name, formula } = split(line);
    if (!formula) return;
    onRoll(formula, name);
  };
  // A die more: `1d20` becomes `2d20`, another kind is added.
  const add = (sides: number) =>
    setLine((now) => {
      const die = new RegExp(`(\\d*)d${sides}(?!\\d)`, "i");
      const m = die.exec(now);
      if (m) return now.replace(die, `${(Number(m[1]) || 1) + 1}d${sides}`);
      return now.trim() ? `${now.trim()} + 1d${sides}` : `1d${sides}`;
    });
  const favored = split(line);
  return (
    <aside className="side-panel dice-panel" onMouseDown={(e) => e.stopPropagation()}>
      <div className="side-panel-head">
        <h3>Dice</h3>
        <button className="ghost icon round" title="Close" onClick={onClose}>
          ×
        </button>
      </div>
      <div className="dice-input">
        <input
          value={line}
          placeholder="1d20 + 3"
          onChange={(e) => setLine(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && go()}
          spellCheck={false}
        />
        <button className="primary" disabled={!favored.formula} onClick={go}>
          Roll
        </button>
        <button
          className="ghost compact"
          title="Keep as a favourite — “Name: 1d20 + 5”"
          disabled={!favored.formula}
          onClick={() => onFavorites([...log.favorites, { name: favored.name || favored.formula, formula: favored.formula }])}
        >
          ☆
        </button>
      </div>
      <div className="dice-pick">
        {(world.dice?.length ? world.dice : USUAL_DICE).map((d) => (
          <button key={d} className="compact" onClick={() => add(d)} title={`Add a d${d}`}>
            d{d}
          </button>
        ))}
        {line && (
          <button className="ghost compact" onClick={() => setLine("")} title="Empty the line">
            ⌫
          </button>
        )}
      </div>
      {log.favorites.length > 0 && (
        <div className="dice-favorites">
          {log.favorites.map((f, i) => (
            <span key={i} className="chip dice-favorite">
              <button className="ghost" title={f.formula} onClick={() => onRoll(f.formula, f.name === f.formula ? "" : f.name)}>
                {f.name}
              </button>
              <button className="ghost" title="Remove" onClick={() => onFavorites(log.favorites.filter((_, j) => j !== i))}>
                ×
              </button>
            </span>
          ))}
        </div>
      )}
      <div className="dice-head">
        <button className={`chip${everyone ? " on" : ""}`} onClick={onEveryone} title="Who sees your rolls">
          {everyone ? "Everyone" : "Only me"}
        </button>
        {world.isGm && log.rolls.length > 0 && (
          <button className="danger compact" onClick={onClear} title="Clears the log here and for everyone in the session">
            Clear log
          </button>
        )}
      </div>
      {problem && <p className="dice-problem">{problem}</p>}
      <ol className="dice-log" ref={list}>
        {log.rolls.length === 0 && <li className="small muted">Nothing rolled yet.</li>}
        {log.rolls.map((e) => (
          <RollLine key={e.id} entry={e} mine={mine(e)} />
        ))}
      </ol>
    </aside>
  );
}

function RollLine({ entry: e, mine }: { entry: SharedRoll; mine: boolean }) {
  // A critical, a fumble, a success or a failure — as the rules of the
  // document said when it was rolled.
  const mark = e.mark && ["crit", "fumble", "success", "failure"].includes(e.mark) ? ` ${e.mark}` : "";
  return (
    <li className={`dice-entry${mine ? " mine" : ""}${mark}`}>
      <span className="dice-what">
        <span className="dice-who">
          {mine ? "You" : e.name || "?"}
          {e.secret && " 🔒"}
          {e.document && ` · ${e.document}`}
        </span>
        {e.label && <span className="dice-label">{e.label}</span>}
        <span className="dice-detail" title={e.formula}>
          {e.detail}
        </span>
      </span>
      <span className="dice-total">
        {e.total}
        {e.mark === "success" && <span className="dice-judged" title="Success">✓</span>}
        {e.mark === "failure" && <span className="dice-judged" title="Failure">✗</span>}
      </span>
    </li>
  );
}
