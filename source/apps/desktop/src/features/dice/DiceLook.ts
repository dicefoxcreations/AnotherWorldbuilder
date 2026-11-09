import type { DiceLook } from "./Dice3D";

/**
 * How dice look on this device, and whose tumble across the window: none,
 * one's own, or everyone's in the session — and whether they sound. Kept with the device's colours
 * (`shared/Theme`) — a reader's taste, for every world.
 */
export type DiceSettings = DiceLook & { show: "off" | "mine" | "all"; sound: boolean };

export const DEFAULT_DICE: DiceSettings = { die: "#ece4d2", number: "#1f1b16", show: "all", sound: true };

const KEY = "aw:dice";

export function loadDice(): DiceSettings {
  try {
    const kept = JSON.parse(localStorage.getItem(KEY) ?? "null") as Partial<DiceSettings> | null;
    return { ...DEFAULT_DICE, ...(kept ?? {}) };
  } catch {
    return DEFAULT_DICE;
  }
}

export function saveDice(d: DiceSettings) {
  try {
    localStorage.setItem(KEY, JSON.stringify(d));
  } catch {
    // Not kept — they hold until the app closes.
  }
}
