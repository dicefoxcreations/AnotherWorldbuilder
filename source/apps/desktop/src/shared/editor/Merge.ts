import { useLayoutEffect, useRef, type RefObject } from "react";

/**
 * Text that changes under the writer's hands — because what someone else
 * wrote arrived — without their caret jumping or their last keystrokes
 * getting lost.
 */

/** Where two texts differ: the common start and end, and what lies between. */
export function splice(before: string, after: string): { start: number; end: number; insert: string } {
  let start = 0;
  while (start < before.length && start < after.length && before[start] === after[start]) start++;
  let tail = 0;
  while (
    tail < before.length - start &&
    tail < after.length - start &&
    before[before.length - 1 - tail] === after[after.length - 1 - tail]
  )
    tail++;
  return { start, end: before.length - tail, insert: after.slice(start, after.length - tail) };
}

/** Where a position in `before` stands in `after`. */
export function mapPosition(before: string, after: string, position: number): number {
  const s = splice(before, after);
  if (position <= s.start) return position;
  if (position >= s.end) return position + after.length - before.length;
  return s.start + s.insert.length;
}

/**
 * What `ours` changed against `base`, made again on top of `theirs` — the
 * keystrokes typed while a save was on its way, on top of what came back.
 */
export function rebase(base: string, ours: string, theirs: string): string {
  if (ours === base) return theirs;
  if (theirs === base) return ours;
  const mine = splice(base, ours);
  const other = splice(base, theirs);
  // Apart: ours goes where it was, shifted by what came before it.
  if (mine.end <= other.start) return theirs.slice(0, mine.start) + mine.insert + theirs.slice(mine.end);
  if (mine.start >= other.end) {
    const shift = theirs.length - base.length;
    return theirs.slice(0, mine.start + shift) + mine.insert + theirs.slice(mine.end + shift);
  }
  // In the same place: nothing of theirs is lost, ours is added after it.
  const at = other.start + other.insert.length;
  return theirs.slice(0, at) + mine.insert + theirs.slice(at);
}

/**
 * Keeps the caret of a text field in place when its value changes from
 * outside: it moves with the text around it, as if the change had been
 * typed somewhere else.
 */
export function useSteadyCaret(field: RefObject<HTMLTextAreaElement | HTMLInputElement | null>, value: string) {
  const last = useRef({ value, start: 0, end: 0 });
  useLayoutEffect(() => {
    const el = field.current;
    const was = last.current;
    if (el && document.activeElement === el && was.value !== value && el.value === value) {
      el.setSelectionRange(mapPosition(was.value, value, was.start), mapPosition(was.value, value, was.end));
    }
    last.current = {
      value,
      start: el?.selectionStart ?? 0,
      end: el?.selectionEnd ?? 0,
    };
  });
  // What is typed here is no change from outside; and the caret moves with
  // clicking too.
  const remember = () => {
    const el = field.current;
    if (el) last.current = { value: el.value, start: el.selectionStart ?? 0, end: el.selectionEnd ?? 0 };
  };
  return { onInput: remember, onSelect: remember, onKeyUp: remember, onMouseUp: remember };
}
