import { createContext, useContext, type ReactNode } from "react";
import { createPortal } from "react-dom";

/** The place in the header bar where a page puts its own switches. */
export const HeaderSlot = createContext<HTMLElement | null>(null);

/**
 * A page's switches, shown in the header bar.
 *
 * The page keeps their state; only the buttons move up. They stand left of
 * the world's and the app's icons, which never move. Without a header bar —
 * on the test bench — they stand where they are written.
 */
export function HeaderTools({ children }: { children: ReactNode }) {
  const slot = useContext(HeaderSlot);
  if (!slot) return <div className="header-tools">{children}</div>;
  return createPortal(children, slot);
}
