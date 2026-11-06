/**
 * The world's calendar — the same rules as `aw_model::calendar` in Rust.
 *
 * A moment is stored as a day count of the world; the time of day, where
 * there is one, is its fraction. The world's time runs through eras, one
 * after another, each with a calendar of its own and its years from 1.
 * Written a moment reads `15 Hay Moon 812 AR, 18:30`. Rust reads that back
 * when a document is saved; here it is only drawn and picked.
 */

export type Month = { name: string; days: number };
export type LeapRule = { every: number; month_index: number };
export type Era = {
  name: string;
  /** After the year: `AR`. */
  abbr: string;
  /** The first era only: after a year before its year 1, counted back. */
  before: string;
  months: Month[];
  weekdays: string[];
  leap: LeapRule | null;
  /** The world's day its year 1 begins on — 0 for the first. */
  start: number;
  /** Its moons — an era may know more of them, or fewer. */
  moons: Moon[];
};
/** A moon, or anything above the world that comes back. */
export type Moon = { name: string; color: string; period: number; full: number };
export type WorldCalendar = { eras: Era[] };

/** A day of the world, as its era writes it. */
export type Ymd = { era: Era; index: number; year: number; month: number; day: number };

const DAY_MINUTES = 1440;

const isLeap = (c: Era, year: number) => !!c.leap && c.leap.every > 0 && year % c.leap.every === 0;
const baseYear = (c: Era) => c.months.reduce((n, m) => n + m.days, 0);
const yearDays = (c: Era, year: number) => baseYear(c) + (isLeap(c, year) ? 1 : 0);
const floorDiv = (a: number, b: number) => Math.floor(a / b);
const mod = (a: number, b: number) => ((a % b) + b) % b;

export function monthDays(c: Era, year: number, monthIndex: number): number {
  const base = c.months[monthIndex]?.days ?? 0;
  return c.leap && c.leap.month_index === monthIndex && isLeap(c, year) ? base + 1 : base;
}

function daysBeforeYear(c: Era, year: number): number {
  const years = year - 1;
  const leaps = c.leap && c.leap.every > 0 ? floorDiv(years, c.leap.every) : 0;
  return years * baseYear(c) + leaps;
}

/** Year, month and day (both from 1) of an era as the world's day — `null` where there is no such day. */
export function toDay(c: Era, year: number, month: number, day: number): number | null {
  const index = month - 1;
  if (index < 0 || index >= c.months.length || day < 1 || day > monthDays(c, year, index)) return null;
  let total = daysBeforeYear(c, year);
  for (let i = 0; i < index; i++) total += monthDays(c, year, i);
  return c.start + total + day - 1;
}

/** A day of the world as year, month and day of an era — both from 1. */
export function eraYmd(c: Era, date: number): [number, number, number] {
  const base = baseYear(c);
  const [cycleYears, cycleDays] = c.leap && c.leap.every > 0 ? [c.leap.every, base * c.leap.every + 1] : [1, base];
  const local = date - c.start;
  let remaining = mod(local, cycleDays);
  let year = 1 + floorDiv(local, cycleDays) * cycleYears;
  while (remaining >= yearDays(c, year)) {
    remaining -= yearDays(c, year);
    year++;
  }
  let month = 0;
  while (remaining >= monthDays(c, year, month) && month + 1 < c.months.length) {
    remaining -= monthDays(c, year, month);
    month++;
  }
  return [year, month + 1, remaining + 1];
}

/** The era a day falls into, by its place. */
export function eraIndexAt(c: WorldCalendar, day: number): number {
  for (let i = c.eras.length - 1; i > 0; i--) if (c.eras[i].start <= day) return i;
  return 0;
}

/** A day of the world, in the calendar of its era. */
export function toYmd(c: WorldCalendar, day: number): Ymd {
  const index = eraIndexAt(c, day);
  const era = c.eras[index];
  const [year, month, d] = eraYmd(era, day);
  return { era, index, year, month, day: d };
}

/** A year as written: `812 AR` — before the first era's year 1, `12 BR`, counted back. */
export function yearText(era: Era, year: number): string {
  const [n, mark] = year < 1 && era.before ? [1 - year, era.before] : [year, era.abbr];
  return mark ? `${n} ${mark}` : String(n);
}

/** The day of the week, where the era has weeks — they run on through every era. */
export function weekdayOf(c: WorldCalendar, day: number): number | null {
  const era = c.eras[eraIndexAt(c, day)];
  return era.weekdays.length ? mod(day, era.weekdays.length) : null;
}

/** A stored moment: the day, and the minutes into it. */
export function splitMoment(value: number): [number, number] {
  const day = Math.floor(value);
  const minutes = Math.round((value - day) * DAY_MINUTES);
  return minutes >= DAY_MINUTES ? [day + 1, 0] : [day, minutes];
}

export const momentOf = (day: number, minutes: number | null) => day + (minutes ?? 0) / DAY_MINUTES;

/** A day as written: `15 Hay Moon 812 AR`. */
export function formatDay(c: WorldCalendar, day: number): string {
  const { era, year, month, day: d } = toYmd(c, day);
  return `${d} ${era.months[month - 1]?.name ?? "?"} ${yearText(era, year)}`;
}

/** A moment as written: its day, and its time where it has one. */
export function formatMoment(c: WorldCalendar, value: number): string {
  const [day, minutes] = splitMoment(value);
  const date = formatDay(c, day);
  if (minutes === 0 && value % 1 === 0) return date;
  return `${date}, ${String(Math.floor(minutes / 60)).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`;
}

/** A date and time of ours, as the reader's own system writes it. */
export function formatOurs(stored: string): string {
  const hasTime = stored.includes("T");
  const date = new Date(hasTime ? stored : `${stored}T00:00`);
  if (Number.isNaN(date.getTime())) return stored;
  return date.toLocaleString(undefined, {
    weekday: "short",
    day: "numeric",
    month: "long",
    year: "numeric",
    ...(hasTime ? { hour: "2-digit", minute: "2-digit" } : {}),
  });
}

export const DEFAULT_ERA: Era = {
  name: "Standard calendar",
  abbr: "AR",
  before: "",
  months: ["First Moon", "Thaw Moon", "Seed Moon", "Blossom Moon", "Pasture Moon", "Sun Moon", "Hay Moon", "Harvest Moon", "Mist Moon", "Fall Moon", "Frost Moon", "Dark Moon"].map(
    (name) => ({ name, days: 30 }),
  ),
  weekdays: ["Firstday", "Secondday", "Thirdday", "Fourthday", "Fifthday", "Restday"],
  leap: null,
  start: 0,
  moons: [],
};

export const DEFAULT_CALENDAR: WorldCalendar = { eras: [DEFAULT_ERA] };

/**
 * Reads a moment as written — `15 Hay Moon 812 AR`, `15. Hay Moon 812`,
 * with `, 18:30` after it — or a bare day count. `null` where it is none.
 * Without an era's mark, the date is of the last era.
 */
export function parseMoment(c: WorldCalendar, text: string): number | null {
  const t = text.trim();
  if (t !== "" && Number.isFinite(Number(t))) return Number(t);
  if (c.eras.length === 0) return null;
  const time = /^(.*?)[,\s]\s*(\d{1,2}):(\d{2})$/.exec(t);
  const [date, minutes] = time && Number(time[2]) < 24 && Number(time[3]) < 60 ? [time[1], Number(time[2]) * 60 + Number(time[3])] : [t, null];
  // Which era: the longest mark the date ends with.
  const marks: [Era, string, boolean][] = [...c.eras.map((e): [Era, string, boolean] => [e, e.abbr.trim(), false]), [c.eras[0], c.eras[0].before.trim(), true]];
  const found = marks
    .filter(([, m]) => m)
    .sort((a, b) => b[1].length - a[1].length)
    .map(([era, mark, back]) => {
      const trimmed = date.trim();
      if (!trimmed.endsWith(mark)) return null;
      const rest = trimmed.slice(0, -mark.length);
      return /[\s\d]$/.test(rest) ? ([era, rest.trimEnd(), back] as const) : null;
    })
    .find((x) => x !== null);
  const [era, written, back] = found ?? [c.eras[c.eras.length - 1], date.trim(), false];

  const m = /^(\d+)\.?\s+(.*)$/.exec(written);
  if (!m) return null;
  const rest = m[2];
  const month = [...era.months.entries()]
    .sort((a, b) => b[1].name.length - a[1].name.length)
    .find(([, mo]) => rest.toLowerCase().startsWith(mo.name.toLowerCase()) && /^\s/.test(rest.slice(mo.name.length)));
  if (!month) return null;
  const year = rest.slice(month[1].name.length).trim();
  if (!/^-?\d+$/.test(year)) return null;
  const day = toDay(era, back ? 1 - Number(year) : Number(year), month[0] + 1, Number(m[1]));
  return day === null ? null : momentOf(day, minutes);
}

/**
 * How far a moon is through its round on a day: 0 new, ½ full. Its round
 * starts half a period before a day it stood full.
 */
/** The moons over a day: those of its era. */
export const moonsAt = (c: WorldCalendar, day: number): Moon[] => c.eras[eraIndexAt(c, day)]?.moons ?? [];

export const moonAge = (moon: Moon, day: number) => mod((day - moon.full) / moon.period + 0.5, 1);

/** The latest of these moments — where the story stands. */
export const latest = (moments: number[]) => (moments.length ? Math.max(...moments) : null);
