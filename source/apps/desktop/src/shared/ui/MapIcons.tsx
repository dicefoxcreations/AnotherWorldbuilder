import { MARKERS } from "../GraphText";
import { DiceFoxIcon, type DiceFoxIconName } from "./dicefox-icons/DiceFoxIcon";

/**
 * The markers a pin on a map may be (`MARKERS` in `GraphText`) — DiceFox
 * "Feine Kontur", in the colour of the text or the one the pin is given.
 * `pin` is the plain pin.
 */
export { MARKERS } from "../GraphText";

/** A marker drawn: `door`, `ladder` … — an unknown name is the pin. */
export function MarkerIcon({ name, size = 16 }: { name: string; size?: number }) {
  const known = (MARKERS as readonly string[]).includes(name) ? (name as DiceFoxIconName) : "pin";
  return <DiceFoxIcon className="marker-icon" name={known} size={size} mode="mono" />;
}
