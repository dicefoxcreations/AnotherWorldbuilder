import { useLayoutEffect, useRef, type RefObject } from "react";
import type { MarkKind, Range } from "../Marks";

const COPIED = [
  "fontFamily",
  "fontSize",
  "fontWeight",
  "fontStyle",
  "letterSpacing",
  "lineHeight",
  "wordSpacing",
  "tabSize",
  "whiteSpace",
  "overflowWrap",
  "wordBreak",
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
 * Marks behind a text field: what changed against another version, in
 * colour — while the field stays a field one types in.
 *
 * A text field cannot colour parts of its text. So behind it lies a copy of
 * its text with the same font, size and wrapping, where the changed pieces
 * have a background; the field itself becomes transparent. Place both in a
 * box with `position: relative` — and this *after* the field: only then is
 * the field there when this is placed.
 */
export function MarkedField({
  field,
  text,
  ranges,
  kind,
}: {
  field: RefObject<HTMLTextAreaElement | null>;
  text: string;
  ranges: Range[];
  kind: MarkKind;
}) {
  const back = useRef<HTMLDivElement>(null);
  const background = useRef<string | null>(null);
  const on = ranges.length > 0;

  useLayoutEffect(() => {
    const el = field.current;
    const b = back.current;
    if (!el || !b) return;
    if (background.current === null) background.current = getComputedStyle(el).backgroundColor;
    const place = () => {
      const style = getComputedStyle(el);
      for (const key of COPIED) b.style[key] = style[key];
      const borders = parseFloat(style.borderLeftWidth) + parseFloat(style.borderRightWidth);
      b.style.top = `${el.offsetTop}px`;
      b.style.left = `${el.offsetLeft}px`;
      // Without the field's scrollbar — so the lines wrap alike.
      b.style.width = `${el.clientWidth + borders}px`;
      b.style.height = `${el.offsetHeight}px`;
      b.style.background = background.current ?? "";
      b.scrollTop = el.scrollTop;
      b.scrollLeft = el.scrollLeft;
    };
    place();
    el.style.background = on ? "transparent" : "";
    el.addEventListener("scroll", place);
    const observer = new ResizeObserver(place);
    observer.observe(el);
    return () => {
      el.removeEventListener("scroll", place);
      observer.disconnect();
      el.style.background = "";
    };
  }, [field, on, text]);

  if (!on) return null;
  const pieces: { text: string; marked: boolean }[] = [];
  let at = 0;
  for (const r of [...ranges].sort((a, b) => a.start - b.start)) {
    if (r.start < at) continue;
    pieces.push({ text: text.slice(at, r.start), marked: false }, { text: text.slice(r.start, r.end), marked: true });
    at = r.end;
  }
  pieces.push({ text: text.slice(at), marked: false });
  return (
    <div ref={back} className="field-marks" aria-hidden="true">
      {pieces.map((p, i) => (p.marked ? <mark key={i} className={`mark-${kind}`}>{p.text}</mark> : p.text))}
      {"\n"}
    </div>
  );
}
