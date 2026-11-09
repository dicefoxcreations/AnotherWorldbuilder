import { useLayoutEffect, useState, type RefObject } from "react";
import { FlowHandle } from "./FlowField";

/** Someone else's caret in a text field: where, and whose. */
export type RemoteCaret = { id: string; name: string; start: number; end: number };

/** Colours far apart, so two people are not told apart by a shade. */
const COLOURS = ["#f08b35", "#b48cf2", "#7cc96b", "#f27ca8", "#f2d14c", "#5aa9f2", "#f25f5c", "#4fd1c5"];

/** Everyone keeps their colour — derived from their id, the same on every device. */
export function colourOf(id: string): string {
  let hash = 0;
  for (const c of id) hash = (hash * 31 + c.charCodeAt(0)) | 0;
  return COLOURS[Math.abs(hash) % COLOURS.length];
}

/** Up to two letters for a name: `Test-Spieler` → TS, `Yen` → Y. */
export function initialsOf(name: string): string {
  const words = name.split(/[\s\-_.]+/).filter(Boolean);
  return (words.length > 1 ? words[0][0] + words[1][0] : (words[0] ?? "?")[0]).toUpperCase();
}

const COPIED = [
  "boxSizing",
  "width",
  "fontFamily",
  "fontSize",
  "fontWeight",
  "fontStyle",
  "letterSpacing",
  "lineHeight",
  "textTransform",
  "wordSpacing",
  "tabSize",
  "paddingTop",
  "paddingRight",
  "paddingBottom",
  "paddingLeft",
  "borderTopWidth",
  "borderRightWidth",
  "borderBottomWidth",
  "borderLeftWidth",
] as const;

/**
 * Where a position of a text field's value stands on screen, relative to
 * the field. A text field says nothing about that; an invisible copy of it
 * with the same font and width does.
 */
function measure(field: HTMLTextAreaElement, position: number): { top: number; left: number; height: number } {
  const style = getComputedStyle(field);
  const copy = document.createElement("div");
  for (const key of COPIED) copy.style[key] = style[key];
  copy.style.position = "absolute";
  copy.style.visibility = "hidden";
  // A code field does not wrap its lines; a text field does.
  copy.style.whiteSpace = style.whiteSpace === "pre" ? "pre" : "pre-wrap";
  copy.style.overflowWrap = "break-word";
  copy.style.top = "0";
  copy.style.left = "-9999px";
  copy.textContent = field.value.slice(0, position);
  const mark = document.createElement("span");
  mark.textContent = field.value.slice(position) || ".";
  copy.appendChild(mark);
  document.body.appendChild(copy);
  const height = parseFloat(style.lineHeight) || parseFloat(style.fontSize) * 1.4;
  const at = { top: mark.offsetTop - field.scrollTop, left: mark.offsetLeft - field.scrollLeft, height };
  copy.remove();
  return at;
}

/**
 * The carets of others, drawn over a text field — each with its colour and
 * name. Place the field and this in a common box with `position: relative`.
 */
export function RemoteCarets({ field, carets }: { field: RefObject<HTMLTextAreaElement | FlowHandle | null>; carets: RemoteCaret[] }) {
  const [shown, setShown] = useState<(RemoteCaret & { top: number; left: number; height: number })[]>([]);
  const [tick, setTick] = useState(0);

  // Scrolling and resizing move the text under the carets.
  useLayoutEffect(() => {
    const f = field.current;
    const el = f instanceof FlowHandle ? f.root : f;
    if (!el) return;
    const again = () => setTick((t) => t + 1);
    el.addEventListener("scroll", again);
    const observer = new ResizeObserver(again);
    observer.observe(el);
    return () => {
      el.removeEventListener("scroll", again);
      observer.disconnect();
    };
  }, [field]);

  useLayoutEffect(() => {
    const el = field.current;
    if (!el || carets.length === 0) {
      setShown([]);
      return;
    }
    if (el instanceof FlowHandle) {
      // In a flow field the place is asked of the text itself.
      const box = el.root?.parentElement?.getBoundingClientRect();
      setShown(
        carets.flatMap((c) => {
          const r = el.rectAt(Math.min(Math.max(c.end, 0), el.value.length));
          return r && box ? [{ ...c, top: r.top - box.top, left: r.left - box.left, height: r.height }] : [];
        }),
      );
      return;
    }
    const length = el.value.length;
    setShown(
      carets.map((c) => {
        const at = measure(el, Math.min(Math.max(c.end, 0), length));
        return { ...c, top: at.top + el.offsetTop, left: at.left + el.offsetLeft, height: at.height };
      }),
    );
  }, [field, carets, tick]);

  return (
    <>
      {shown
        .filter((c) => {
          const f = field.current;
          if (!f || f instanceof FlowHandle) return true;
          return c.top >= f.offsetTop - 4 && c.top <= f.offsetTop + f.clientHeight;
        })
        .map((c) => (
          <span
            key={c.id}
            className="remote-caret"
            style={{ top: c.top, left: c.left, height: c.height, borderColor: colourOf(c.id) }}
          >
            <span className="remote-name" style={{ background: colourOf(c.id) }}>
              {initialsOf(c.name)}
            </span>
          </span>
        ))}
    </>
  );
}
