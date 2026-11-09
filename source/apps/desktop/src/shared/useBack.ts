import { useEffect, useRef } from "react";
import { isMobile } from "./platform";

/**
 * A phone's Back closes what was opened last — the ⋮ menu, a dialog, the
 * side bar, the options — and then leaves the world; with nothing open it
 * leaves the app. Android asks `window.__awBack` first (`MainActivity.kt`):
 * true when it closed something.
 */
const open: (() => void)[] = [];

if (isMobile && typeof window !== "undefined") {
  (window as Window & { __awBack?: () => boolean }).__awBack = () => {
    const last = open.pop();
    last?.();
    return !!last;
  };
}

/** While `shown`, Back calls `close`. */
export function useBack(shown: boolean, close: () => void) {
  const latest = useRef(close);
  latest.current = close;
  useEffect(() => {
    if (!isMobile || !shown) return;
    const step = () => latest.current();
    open.push(step);
    return () => {
      const at = open.lastIndexOf(step);
      if (at >= 0) open.splice(at, 1);
    };
  }, [shown]);
}
