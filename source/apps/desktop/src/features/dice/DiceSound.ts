import rolling from "./dice-roll.mp3";

/**
 * The sound of dice on a table — a recording (CC0, see
 * docs/development/licenses.md), played on this device only; a shared roll
 * sounds on each device that shows it, nothing goes over the network.
 */
export function sound(after = 0) {
  setTimeout(() => {
    const a = new Audio(rolling);
    a.volume = 0.7;
    // A webview that will not play without a click first: silent, the dice still roll.
    a.play().catch(() => {});
  }, Math.max(0, after));
}
