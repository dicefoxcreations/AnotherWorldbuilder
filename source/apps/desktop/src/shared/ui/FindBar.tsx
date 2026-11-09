import { useEffect, useRef, useState, type RefObject } from "react";
import { Icon } from "./Icons";
import { FIND_EVENT } from "./QuickOpen";
import { anchoredPart } from "../Anchors";

/**
 * Ctrl+F: finds words on the page, as a browser would — every place marked,
 * the current one stronger, Enter for the next, Shift+Enter for the one
 * before, Escape to close. Only the page counts, not the side bar.
 *
 * The code of a document is a text field, which no page mark reaches: what
 * is found there the field is told (`FIELD_FIND_EVENT`), and it marks it
 * behind its text itself.
 */
type Highlights = { set: (name: string, h: unknown) => void; delete: (name: string) => void };
const highlights = (): Highlights | null => (globalThis as { CSS?: { highlights?: Highlights } }).CSS?.highlights ?? null;
const Highlight = (globalThis as { Highlight?: new (...r: Range[]) => unknown }).Highlight;

/** Sent to a code field: what of it was found, and which of that is the current one (-1: none). */
export const FIELD_FIND_EVENT = "aw-field-find";
export type FieldFound = { ranges: { start: number; end: number }[]; current: number };

/** What the find bar found in a code field — for its marks (`MarkedField`). */
export function useFieldFind(field: RefObject<HTMLTextAreaElement | null>): FieldFound {
  const [found, setFound] = useState<FieldFound>({ ranges: [], current: -1 });
  useEffect(() => {
    const el = field.current;
    if (!el) return;
    const take = (e: Event) => setFound((e as CustomEvent<FieldFound>).detail);
    el.addEventListener(FIELD_FIND_EVENT, take);
    return () => el.removeEventListener(FIELD_FIND_EVENT, take);
  }, [field]);
  return found;
}

/** A place in a code field. */
type InField = { field: HTMLTextAreaElement; start: number; end: number };
type Hit = Range | InField;
const inField = (h: Hit | null): h is InField => !!h && "field" in h;

/** Where the words stand in the code fields shown. */
function inFields(root: Element, words: string): InField[] {
  const needle = words.toLowerCase();
  if (!needle) return [];
  const out: InField[] = [];
  for (const field of root.querySelectorAll<HTMLTextAreaElement>("textarea.template-text")) {
    if (field.getClientRects().length === 0) continue;
    const text = field.value.toLowerCase();
    for (let at = text.indexOf(needle); at >= 0; at = text.indexOf(needle, at + needle.length)) out.push({ field, start: at, end: at + needle.length });
  }
  return out;
}

/** Where the words stand in the page — text that is shown, not that of fields one types in. */
function rangesOf(root: Element, words: string): Range[] {
  const needle = words.toLowerCase();
  if (!needle) return [];
  const out: Range[] = [];
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode: (n) => {
      const el = n.parentElement;
      if (!el || el.closest("input, textarea, select, .find-bar, [hidden]")) return NodeFilter.FILTER_REJECT;
      return el.getClientRects().length > 0 ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT;
    },
  });
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const text = (n as Text).data.toLowerCase();
    for (let at = text.indexOf(needle); at >= 0; at = text.indexOf(needle, at + needle.length)) {
      const r = document.createRange();
      r.setStart(n, at);
      r.setEnd(n, at + needle.length);
      out.push(r);
    }
  }
  return out;
}

/** Scrolls a code field — and the page around it — so that a place in it stands in the middle. */
function showInField({ field, start }: InField) {
  const style = getComputedStyle(field);
  const copy = document.createElement("div");
  for (const p of ["boxSizing", "paddingTop", "paddingRight", "paddingBottom", "paddingLeft", "fontFamily", "fontSize", "fontWeight", "lineHeight", "letterSpacing", "tabSize", "whiteSpace", "overflowWrap", "wordBreak"] as const)
    copy.style[p] = style[p];
  Object.assign(copy.style, { position: "absolute", visibility: "hidden", top: "0", left: "-9999px", width: `${field.clientWidth}px` });
  copy.textContent = field.value.slice(0, start);
  const mark = copy.appendChild(document.createElement("span"));
  mark.textContent = "\u200b";
  document.body.appendChild(copy);
  const y = mark.offsetTop;
  copy.remove();
  field.scrollTop = Math.max(0, y - field.clientHeight / 2);
  const box = field.getBoundingClientRect();
  const shown = box.top + y - field.scrollTop;
  if (shown < 0 || shown > window.innerHeight) {
    // The field grows with its text: the page around it scrolls instead.
    let scroller = field.parentElement;
    while (scroller && !(scroller.scrollHeight > scroller.clientHeight && /auto|scroll/.test(getComputedStyle(scroller).overflowY))) scroller = scroller.parentElement;
    scroller?.scrollBy({ top: shown - window.innerHeight / 2, behavior: "smooth" });
  }
}

export function FindBar() {
  const [open, setOpen] = useState(false);
  const [words, setWords] = useState("");
  const [found, setFound] = useState<Hit[]>([]);
  const [at, setAt] = useState(0);
  const field = useRef<HTMLInputElement>(null);
  const scrolled = useRef("");
  /**
   * Words asked for at a place — a section found in a book: the first time
   * they stand there or after, in that document; `scroll: false` where an
   * anchor brought the page there already. `range`: that one, once found.
   */
  const asked = useRef<{ doc?: string; anchor?: string; scroll: boolean; range?: Range } | null>(null);
  const [asking, setAsking] = useState(0);

  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "f") {
        e.preventDefault();
        asked.current = null;
        setAsking((n) => n + 1);
        setOpen(true);
        requestAnimationFrame(() => field.current?.select());
      }
    };
    // Words searched in every document: the page opens with them found —
    // just words, or `{ words, doc, anchor, scroll }`.
    const ask = (e: Event) => {
      const detail = (e as CustomEvent).detail;
      const at = typeof detail === "object" && detail ? detail : null;
      asked.current = at && { doc: at.doc, anchor: at.anchor, scroll: at.scroll !== false };
      scrolled.current = "";
      setWords(String((at ? at.words : detail) ?? ""));
      setAt(0);
      setAsking((n) => n + 1);
      setOpen(true);
    };
    document.addEventListener("keydown", key, true);
    document.addEventListener(FIND_EVENT, ask);
    return () => {
      document.removeEventListener("keydown", key, true);
      document.removeEventListener(FIND_EVENT, ask);
    };
  }, []);

  // Found anew as one types — and when the page changes under the bar.
  useEffect(() => {
    if (!open) return;
    // The page of the world — where there is none, everything.
    const root = document.querySelector(".world-body") ?? document.body;
    const find = () => {
      // Asked for in a document: only once it is drawn — not in a preview.
      const want = asked.current;
      const scope = want?.doc
        ? [...root.querySelectorAll(".article-page")].find((p) => p.getAttribute("data-document") === want.doc && !p.closest(".link-preview"))
        : root;
      const ranges = scope ? rangesOf(scope, words.trim()) : [];
      setFound(want?.doc || !scope ? ranges : [...ranges, ...inFields(scope, words.trim())]);
      if (!want) return;
      const spot = want.anchor ? scope?.querySelector(`[data-anchor="${CSS.escape(want.anchor)}"]`) : null;
      const part = spot && (anchoredPart(spot) ?? spot);
      const after = (r: Range) => !!part && (part.contains(r.startContainer) || !!(part.compareDocumentPosition(r.startContainer) & Node.DOCUMENT_POSITION_FOLLOWING));
      const index = want.anchor ? ranges.findIndex(after) : ranges.length ? 0 : -1;
      if (index >= 0) {
        want.range = ranges[index];
        setAt(index);
      }
    };
    find();
    // A burst of changes — each key typed in the page — is looked through
    // once, at the next frame, not once for every change.
    let frame = 0;
    const soon = () => {
      if (!frame) frame = requestAnimationFrame(() => ((frame = 0), find()));
    };
    const watch = new MutationObserver(soon);
    watch.observe(root, { childList: true, subtree: true, characterData: true });
    return () => {
      watch.disconnect();
      cancelAnimationFrame(frame);
    };
  }, [open, words, asking]);

  const current = found.length ? found[Math.min(at, found.length - 1)] : null;
  // Each code field marks what was found in it — and forgets it once the bar closes.
  useEffect(() => {
    const fields = new Set<HTMLTextAreaElement>(document.querySelectorAll("textarea.template-text"));
    for (const h of found) if (inField(h)) fields.add(h.field);
    for (const field of fields) {
      const mine = open ? found.filter((h): h is InField => inField(h) && h.field === field) : [];
      const detail: FieldFound = { ranges: mine.map(({ start, end }) => ({ start, end })), current: mine.indexOf(current as InField) };
      field.dispatchEvent(new CustomEvent(FIELD_FIND_EVENT, { detail }));
    }
  }, [open, found, current]);
  useEffect(() => {
    const h = highlights();
    if (!open) {
      h?.delete("find");
      h?.delete("find-current");
      return;
    }
    if (h && Highlight) {
      h.set("find", new Highlight(...found.filter((f): f is Range => !inField(f))));
      if (current && !inField(current)) h.set("find-current", new Highlight(current));
      else h.delete("find-current");
    }
    // Only when one steps or types — not each time the page changes under
    // the bar; asked for at a place, once it is found there.
    const key = `${words}\u0000${at}`;
    const want = asked.current;
    if (current && (!want || want.range === current) && scrolled.current !== key) {
      scrolled.current = key;
      asked.current = null;
      if (inField(current)) showInField(current);
      else if (want?.scroll !== false) current.startContainer.parentElement?.scrollIntoView({ block: "center", behavior: "smooth" });
    }
    // A browser paints no marks in a button: a link gets a frame.
    const framed = inField(current) ? null : current?.startContainer.parentElement?.closest<HTMLElement>("button, a, [data-raw]");
    framed?.classList.add("find-here");
    return () => framed?.classList.remove("find-here");
  }, [open, found, current, words, at]);
  useEffect(
    () => () => {
      highlights()?.delete("find");
      highlights()?.delete("find-current");
    },
    [],
  );

  if (!open) return null;
  const step = (by: number) => {
    asked.current = null;
    if (found.length) setAt((i) => (i + by + found.length) % found.length);
  };
  return (
    <div className="find-bar" role="search">
      <input
        ref={field}
        autoFocus
        value={words}
        placeholder="Find on the page"
        onChange={(e) => {
          asked.current = null;
          setAt(0);
          setWords(e.target.value);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") step(e.shiftKey ? -1 : 1);
          if (e.key === "Escape") setOpen(false);
        }}
      />
      <span className="small muted find-count">{words.trim() ? (found.length ? `${Math.min(at, found.length - 1) + 1}/${found.length}` : "0") : ""}</span>
      <button className="ghost compact" title="Before (Shift+Enter)" onClick={() => step(-1)} disabled={!found.length}>
        <Icon name="up" />
      </button>
      <button className="ghost compact" title="Next (Enter)" onClick={() => step(1)} disabled={!found.length}>
        <Icon name="down" />
      </button>
      <button className="ghost compact" title="Close (Escape)" onClick={() => setOpen(false)}>
        <Icon name="close" />
      </button>
    </div>
  );
}
