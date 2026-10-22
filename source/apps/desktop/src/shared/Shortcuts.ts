/**
 * Keys that do something: the fixed ones every world has, and those a
 * member sets for a world — Ctrl+L makes the selected words a link to a
 * location, the location made where there is none yet.
 *
 * Kept on this device, per world: the types are the world's, the hands that
 * press the keys are this device's.
 */

/** A key with Ctrl (⌘ on a Mac) and maybe Shift: `ctrl+l`, `ctrl+shift+l`. */
export type Combo = string;

/** Ctrl+L → a link to a document of this type. */
export type TypeShortcut = { combo: Combo; type: string };

/** What the fixed keys do — shown in the options, not changed there. */
export const FIXED: { combo: Combo; what: string }[] = [
  { combo: "ctrl+p", what: "Open a document" },
  { combo: "ctrl+shift+f", what: "Search all documents" },
  { combo: "alt+left", what: "Back" },
  { combo: "alt+right", what: "Forward" },
  { combo: "ctrl+f", what: "Find on the page" },
  { combo: "ctrl+b", what: "Bold" },
  { combo: "ctrl+i", what: "Italic" },
  { combo: "ctrl+k", what: "Link the selected words" },
  { combo: "ctrl+e", what: "Edit or read" },
  { combo: "ctrl+z", what: "Undo" },
  { combo: "ctrl+y", what: "Redo" },
  { combo: "?", what: "These keys" },
];

/** Keys the app or the system already uses — not for a type. */
const TAKEN = new Set([...FIXED.map((f) => f.combo), "ctrl+c", "ctrl+v", "ctrl+x", "ctrl+a", "ctrl+shift+z", "ctrl+shift+m", "ctrl+q", "ctrl+w", "ctrl+r"]);

/** The combination a key press makes — `null` without Ctrl or ⌘, or for Ctrl alone. */
export function comboOf(e: { key: string; ctrlKey: boolean; metaKey: boolean; shiftKey: boolean; altKey: boolean }): Combo | null {
  if (!(e.ctrlKey || e.metaKey) || e.altKey) return null;
  const key = e.key.toLowerCase();
  if (["control", "meta", "shift", "alt"].includes(key) || key.length !== 1) return null;
  return `ctrl+${e.shiftKey ? "shift+" : ""}${key}`;
}

/** How a combination is read: `Ctrl+Shift+L`. */
export const shownCombo = (c: Combo) =>
  c
    .split("+")
    .map((p) => ({ ctrl: "Ctrl", shift: "Shift", alt: "Alt", left: "←", right: "→" })[p] ?? p.toUpperCase())
    .join("+");

/** Whether a combination is free for a type. */
export const freeFor = (c: Combo) => !TAKEN.has(c);

const keyOf = (world: string) => `aw.shortcuts.${world}`;

export function loadShortcuts(world: string): TypeShortcut[] {
  try {
    const kept = JSON.parse(localStorage.getItem(keyOf(world)) ?? "[]") as TypeShortcut[];
    return Array.isArray(kept) ? kept.filter((s) => typeof s?.combo === "string" && typeof s?.type === "string") : [];
  } catch {
    return [];
  }
}

export function saveShortcuts(world: string, all: TypeShortcut[]) {
  try {
    localStorage.setItem(keyOf(world), JSON.stringify(all));
  } catch {
    // Not kept — they hold until the app closes.
  }
}
