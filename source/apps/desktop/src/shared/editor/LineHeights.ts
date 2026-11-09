import { useLayoutEffect, useState, type RefObject } from "react";

const COPIED = ["fontFamily", "fontSize", "fontWeight", "fontStyle", "letterSpacing", "wordSpacing", "lineHeight", "tabSize", "whiteSpace", "overflowWrap", "wordBreak", "paddingLeft", "paddingRight"] as const;

/**
 * How high each line of a code field stands, wrapped as the field wraps it
 * — so that the line numbers beside it stay with their lines. Measured on an
 * invisible copy, again when the text or the field's width changes.
 */
export function useLineHeights(field: RefObject<HTMLTextAreaElement | null>, text: string): number[] {
  const [heights, setHeights] = useState<number[]>([]);
  useLayoutEffect(() => {
    const el = field.current;
    if (!el) return;
    const measure = () => {
      const style = getComputedStyle(el);
      const copy = document.createElement("div");
      for (const p of COPIED) copy.style[p] = style[p];
      Object.assign(copy.style, { position: "absolute", visibility: "hidden", top: "0", left: "-9999px", boxSizing: "border-box", width: `${el.clientWidth}px` });
      for (const line of text.split("\n")) copy.appendChild(document.createElement("div")).textContent = line || "​";
      document.body.appendChild(copy);
      const next = [...copy.children].map((c) => c.getBoundingClientRect().height);
      copy.remove();
      setHeights((was) => (was.length === next.length && was.every((h, i) => h === next[i]) ? was : next));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [field, text]);
  return heights;
}
