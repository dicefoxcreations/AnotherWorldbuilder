import { useEffect, useSyncExternalStore } from "react";

/**
 * The side bar at the window's right edge holds one thing at a time — the
 * versions of a document, the roles of what was clicked in a graph. Opening
 * one closes whatever stood there before.
 *
 * Whoever opens something there claims the bar with a name of its own; when
 * someone else claims it, the one before is told to close.
 */

let owner: string | null = null;
const listeners = new Set<() => void>();

const tell = () => listeners.forEach((l) => l());

/** Who holds the bar now — `null` when nobody does. */
export const sideBarOwner = () => owner;

export function claimSideBar(id: string) {
  if (owner === id) return;
  owner = id;
  tell();
}

export function releaseSideBar(id: string) {
  if (owner !== id) return;
  owner = null;
  tell();
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};

/**
 * Holds the side bar while `open` — and calls `onClose` once someone else
 * takes it.
 */
export function useSideBar(id: string, open: boolean, onClose: () => void) {
  const now = useSyncExternalStore(subscribe, () => owner);
  useEffect(() => {
    if (open) claimSideBar(id);
    else releaseSideBar(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id, open]);
  useEffect(() => {
    // The live owner, not the one this render saw: claiming above may just
    // have made it this one.
    if (open && owner !== null && owner !== id) onClose();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [now]);
  // Leaving gives the bar back.
  useEffect(() => () => releaseSideBar(id), [id]);
}
