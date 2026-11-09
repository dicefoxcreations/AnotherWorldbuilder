import { useEffect, useLayoutEffect, useRef, type RefObject } from "react";

/** The nearest box around `el` that scrolls up and down — the page at last. */
function scrollerAround(el: Element): Element | null {
  for (let p = el.parentElement; p; p = p.parentElement) {
    const y = getComputedStyle(p).overflowY;
    if ((y === "auto" || y === "scroll") && p.scrollHeight > p.clientHeight) return p;
  }
  return document.scrollingElement;
}

/**
 * The middle button held down drags what `box` shows, wherever it is
 * pressed: sideways inside it, up and down in what scrolls around it.
 */
export function useMiddleDrag(box: RefObject<HTMLElement | null>) {
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    let from: { x: number; y: number; left: number; top: number; scroller: Element | null } | null = null;
    const down = (e: PointerEvent) => {
      if (e.button !== 1) return;
      // Before anything inside sees it.
      e.preventDefault();
      e.stopPropagation();
      const scroller = scrollerAround(el);
      from = { x: e.clientX, y: e.clientY, left: el.scrollLeft, top: scroller?.scrollTop ?? 0, scroller };
      el.setPointerCapture(e.pointerId);
      el.classList.add("grabbing");
    };
    const move = (e: PointerEvent) => {
      if (!from) return;
      el.scrollLeft = from.left - (e.clientX - from.x);
      if (from.scroller) from.scroller.scrollTop = from.top - (e.clientY - from.y);
    };
    const up = () => {
      from = null;
      el.classList.remove("grabbing");
    };
    // Not the browser's own scrolling or pasting by the middle button.
    const mouse = (e: MouseEvent) => e.button === 1 && e.preventDefault();
    el.addEventListener("pointerdown", down, true);
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
    el.addEventListener("mousedown", mouse);
    el.addEventListener("auxclick", mouse);
    return () => {
      el.removeEventListener("pointerdown", down, true);
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
      el.removeEventListener("mousedown", mouse);
      el.removeEventListener("auxclick", mouse);
    };
  }, [box]);
}

/**
 * Ctrl and the wheel over `box` zoom by a factor, `onZoom(k)`; what is under
 * the pointer in `content` stays under it — `box` scrolled sideways, what
 * scrolls around it up and down.
 */
export function useWheelZoom(box: RefObject<HTMLElement | null>, content: RefObject<Element | null>, onZoom: (k: number) => void) {
  const zoom = useRef(onZoom);
  zoom.current = onZoom;
  // Where the pointer stood, as a share of `content`, until it is drawn anew.
  const anchor = useRef<{ fx: number; fy: number; x: number; y: number } | null>(null);
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const wheel = (e: WheelEvent) => {
      if (!e.ctrlKey) return;
      e.preventDefault();
      const r = content.current?.getBoundingClientRect();
      if (r && r.width > 0 && r.height > 0)
        anchor.current = { fx: (e.clientX - r.left) / r.width, fy: (e.clientY - r.top) / r.height, x: e.clientX, y: e.clientY };
      zoom.current(Math.exp(-e.deltaY * 0.0015));
    };
    el.addEventListener("wheel", wheel, { passive: false });
    return () => el.removeEventListener("wheel", wheel);
  }, [box, content]);
  useLayoutEffect(() => {
    const a = anchor.current;
    const el = box.current;
    const r = content.current?.getBoundingClientRect();
    if (!a || !el || !r) return;
    anchor.current = null;
    el.scrollLeft += r.left + a.fx * r.width - a.x;
    const scroller = scrollerAround(el);
    if (scroller) scroller.scrollTop += r.top + a.fy * r.height - a.y;
  });
}
