/**
 * The app's colours, as this reader likes them — on this device.
 *
 * Four are chosen; everything else follows from them: the surfaces lie
 * between the background and the text, the accent's softer shades between
 * the accent and the background. Red (deleting, what only the GM sees),
 * green (everyone) and blue (players) keep their meaning and only shift
 * toward the background and the text, so they read on a light page as on a
 * dark one. The relation lines keep their own hues.
 */

export type Colors = {
  /** Behind everything; panels and fields lie a little toward the text. */
  background: string;
  text: string;
  /** Links, what is chosen, the main button, focus. */
  accent: string;
  /** Titles and headings. */
  heading: string;
};

/** The colours the app comes with — what `global.css` sets on `:root`. */
export const DEFAULT_COLORS: Colors = { background: "#0e1416", text: "#e5eeee", accent: "#2fc6b2", heading: "#e5eeee" };

/** Ready-made sets to start from. */
export const SCHEMES: { name: string; colors: Colors }[] = [
  { name: "Night", colors: DEFAULT_COLORS },
  { name: "Ember", colors: { background: "#17120f", text: "#eee3d6", accent: "#e0a14a", heading: "#f3c98b" } },
  { name: "Parchment", colors: { background: "#f2e8d5", text: "#2e241b", accent: "#9a3412", heading: "#5b2a0c" } },
  { name: "Paper", colors: { background: "#fafaf8", text: "#1f2428", accent: "#0b6e99", heading: "#1f2428" } },
];

const KEY = "aw:colors";
const WARN = "#d9534f";
const GREEN = "#6fbf78";
const BLUE = "#6f9fd9";

type Rgb = [number, number, number];

/** `#rgb` or `#rrggbb`, or nothing. */
export function parseHex(hex: string): Rgb | null {
  const m = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return null;
  const h = m[1].length === 3 ? [...m[1]].map((c) => c + c).join("") : m[1];
  return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)) as Rgb;
}

const hex = (c: Rgb) => "#" + c.map((v) => Math.round(Math.min(255, Math.max(0, v))).toString(16).padStart(2, "0")).join("");

/** `t` of the way from `a` to `b`. */
function mix(a: string, b: string, t: number): string {
  const x = parseHex(a)!;
  const y = parseHex(b)!;
  return hex(x.map((v, i) => v + (y[i] - v) * t) as Rgb);
}

/** How bright a colour looks, 0 to 1. */
export function luminance(color: string): number {
  const [r, g, b] = parseHex(color)!.map((v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** Whether all four are colours. */
export function valid(c: Colors): boolean {
  return [c.background, c.text, c.accent, c.heading].every((v) => parseHex(v) !== null);
}

/** Every colour the app draws with, from the four. */
export function derive(c: Colors): Record<string, string> {
  const { background: bg, text, accent } = c;
  const toward = (t: number) => mix(bg, text, t);
  return {
    "--base": bg,
    "--sunken": toward(0.025),
    "--card": toward(0.05),
    "--popover": toward(0.08),
    "--field": toward(0.09),
    "--raised": toward(0.125),
    "--hover": toward(0.165),
    "--border": toward(0.2),
    "--track": toward(0.25),
    "--text": text,
    "--muted": toward(0.67),
    "--heading": c.heading,
    "--accent": accent,
    "--accent-soft": mix(bg, accent, 0.16),
    "--accent-line": mix(bg, accent, 0.5),
    "--on-accent": luminance(accent) > 0.35 ? mix(accent, "#000000", 0.85) : mix(accent, "#ffffff", 0.92),
    "--danger-soft": mix(bg, WARN, 0.12),
    "--danger-line": mix(bg, WARN, 0.45),
    "--danger-text": mix(WARN, text, 0.4),
    "--ok-line": mix(bg, GREEN, 0.45),
    "--ok-text": mix(GREEN, text, 0.3),
    "--info-line": mix(bg, BLUE, 0.45),
    "--info-text": mix(BLUE, text, 0.3),
    "color-scheme": luminance(bg) > 0.4 ? "light" : "dark",
  };
}

const same = (a: Colors, b: Colors) =>
  (Object.keys(a) as (keyof Colors)[]).every((k) => a[k].toLowerCase() === b[k].toLowerCase());

/** What this device keeps — the app's own colours where it keeps none. */
export function loadColors(): Colors {
  try {
    const kept = JSON.parse(localStorage.getItem(KEY) ?? "null") as Colors | null;
    if (kept && valid(kept)) return kept;
  } catch {
    // Nothing kept, or nothing readable: the app's own.
  }
  return DEFAULT_COLORS;
}

/** Draws the app in these colours. The app's own are those of `global.css`. */
export function applyColors(c: Colors) {
  const root = document.documentElement.style;
  const vars = derive(DEFAULT_COLORS);
  for (const name of Object.keys(vars)) root.removeProperty(name);
  if (!valid(c)) return;
  // Only the headings changed: the app's own surfaces stay, with their breath of teal.
  if (same({ ...c, heading: DEFAULT_COLORS.heading }, DEFAULT_COLORS)) {
    if (c.heading.toLowerCase() !== DEFAULT_COLORS.heading) root.setProperty("--heading", c.heading);
    return;
  }
  for (const [name, value] of Object.entries(derive(c))) root.setProperty(name, value);
}

/** Keeps them on this device and draws with them — in every window. */
export function saveColors(c: Colors) {
  try {
    if (same(c, DEFAULT_COLORS)) localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, JSON.stringify(c));
  } catch {
    // Not kept — they hold until the app closes.
  }
  applyColors(c);
}

/** Draws with what is kept, and follows when another window changes it. */
export function followKeptColors() {
  applyColors(loadColors());
  window.addEventListener("storage", (e) => e.key === KEY && applyColors(loadColors()));
}
