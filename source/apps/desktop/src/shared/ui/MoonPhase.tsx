import { moonAge, type Moon } from "../Calendar";

const PHASES = ["new", "waxing crescent", "first quarter", "waxing gibbous", "full", "waning gibbous", "last quarter", "waning crescent"];

/** A moon's phase in words: `full`. */
export const phaseName = (age: number) => PHASES[Math.round(age * 8) % 8];

/**
 * A moon as it stands on a day, in its own colour: the lit part drawn, the
 * dark part faint. Waxing it is lit from the right.
 */
export function MoonPhase({ moon, day, size = 12 }: { moon: Moon; day: number; size?: number }) {
  const age = moonAge(moon, day);
  const c = size / 2;
  const r = c - 0.75;
  // 1 at new, -1 at full: how wide the shadow's edge bulges.
  const k = Math.cos(2 * Math.PI * age);
  const rx = r * Math.abs(k);
  const waxing = age < 0.5;
  const outer = waxing ? 1 : 0;
  const inner = waxing === k > 0 ? 0 : 1;
  const lit = `M ${c} ${c - r} A ${r} ${r} 0 0 ${outer} ${c} ${c + r} A ${rx} ${r} 0 0 ${inner} ${c} ${c - r} Z`;
  return (
    <svg className="moon-phase" width={size} height={size} viewBox={`0 0 ${size} ${size}`} style={{ color: moon.color }}>
      <title>{`${moon.name}: ${phaseName(age)}`}</title>
      <circle cx={c} cy={c} r={r} fill="currentColor" fillOpacity={0.14} stroke="currentColor" strokeOpacity={0.45} strokeWidth={0.75} />
      {Math.abs(age - 0) > 0.01 && Math.abs(age - 1) > 0.01 && <path d={lit} fill="currentColor" />}
    </svg>
  );
}

/** Every moon on a day — as many as the world has. */
export function MoonsOf({ moons, day, size }: { moons: Moon[]; day: number; size?: number }) {
  if (moons.length === 0) return null;
  return (
    <span className="moons">
      {moons.map((m, i) => (
        <MoonPhase key={i} moon={m} day={day} size={size} />
      ))}
    </span>
  );
}
