import { useEffect } from "react";
import { createPortal } from "react-dom";

/** One choice of a menu. */
export type MenuItem = { label: string; danger?: boolean; disabled?: boolean; title?: string; onClick: () => void };

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
  return createPortal(
    <div
      className="brick-menu popup-menu"
      style={{ left: Math.min(x, window.innerWidth - 240), top: Math.min(y, window.innerHeight - 40 - items.length * 34) }}
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
