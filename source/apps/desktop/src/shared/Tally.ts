/**
 * A number as marks — how well someone has a skill: `●` once, `●●` twice
 * (expertise), `◐` for a half (half proficiency), a faint `●` for none.
 * Other signs repeat as they are; only `●` has a half.
 */
export function tally(sign: string, n: number): { text: string; none: boolean } {
  const whole = Math.max(0, Math.min(5, Math.floor(n)));
  const half = sign === "●" && n - Math.floor(n) >= 0.5 && whole < 5;
  if (whole === 0 && !half) return { text: sign, none: true };
  return { text: sign.repeat(whole) + (half ? "◐" : ""), none: false };
}
