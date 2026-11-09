import { useEffect, useLayoutEffect, useRef } from "react";
import { createPortal } from "react-dom";

/** One choice of a menu. */
export type MenuItem = { label: string; danger?: boolean; disabled?: boolean; title?: string; onClick: () => void };

/**
 * Keeps a menu opened at x, y wholly in the window: moved left and up as far
 * as it is wide and tall. Measured after each render — what it holds may change.
 */
export function useInWindow(x: number, y: number) {
  const box = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    el.style.left = `${Math.max(8, Math.min(x, window.innerWidth - el.offsetWidth - 8))}px`;
    el.style.top = `${Math.max(8, Math.min(y, window.innerHeight - el.offsetHeight - 8))}px`;
  });
  return box;
}

/**
 * A right-click menu: a few choices where the pointer was. A click outside
 * or Escape closes it; a choice does what it says, then closes it.
 */
export function PopupMenu({ x, y, title, items, onClose }: { x: number; y: number; title?: string; items: MenuItem[]; onClose: () => void }) {
  useEffect(() => {
    const close = () => onClose();
    const key = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", key);
    };
  }, [onClose]);
  const box = useInWindow(x, y);
  return createPortal(
    <div
      ref={box}
      className="brick-menu popup-menu"
      style={{ left: x, top: y }}
      onMouseDown={(e) => e.stopPropagation()}
      onPointerDown={(e) => e.stopPropagation()}
      onContextMenu={(e) => e.preventDefault()}
    >
      <div className="brick-group">
        {title && <h5>{title}</h5>}
        {items.map((item) => (
          <button
            key={item.label}
            className={`ghost${item.danger ? " danger" : ""}`}
            disabled={item.disabled}
            title={item.title}
            onClick={() => {
              item.onClick();
              onClose();
            }}
          >
            {item.label}
          </button>
        ))}
      </div>
    </div>,
    document.body,
  );
}
