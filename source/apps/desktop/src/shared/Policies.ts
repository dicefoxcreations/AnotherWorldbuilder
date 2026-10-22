/**
 * Policies as people pick them: an OR over terms, `players OR class:rogue`.
 *
 * The game master holds every key: they see and change everything anyway.
 * So *Everyone*, *All players* and *GM only* exclude each other, and whoever
 * is added by name comes to the GM, who always stays.
 */

export const OR = " OR ";

export const terms = (policy: string): string[] =>
  (policy ?? "")
    .split(/\s+OR\s+/i)
    .map((t) => t.trim())
    .filter(Boolean);

export const BASICS = ["all", "players", "gm"];

/**
 * What a policy amounts to: everyone takes in all players and the GM, all
 * players any one of them, and the GM is always there. `all OR gm` is
 * everyone; `gm OR player:roque` the GM and Roque; nothing at all the GM.
 */
export function effective(policy: string): string[] {
  const all = terms(policy);
  const has = (t: string) => all.some((x) => x.toLowerCase() === t);
  if (has("all")) return ["all"];
  if (has("players")) return ["players"];
  const named = all.filter((x) => x.toLowerCase() !== "gm");
  return ["gm", ...named.filter((x, i) => named.indexOf(x) === i)];
}

/** Whether two policies amount to the same. */
export const samePolicy = (a: string, b: string) =>
  effective(a).map((t) => t.toLowerCase()).sort().join(OR) === effective(b).map((t) => t.toLowerCase()).sort().join(OR);
