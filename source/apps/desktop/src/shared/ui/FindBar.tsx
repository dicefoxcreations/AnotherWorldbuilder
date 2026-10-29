import { useEffect, useRef, useState } from "react";
import { FIND_EVENT } from "./QuickOpen";
import { anchoredPart } from "../Anchors";

/**
 * Ctrl+F: finds words on the page, as a browser would — every place marked,
 * the current one stronger, Enter for the next, Shift+Enter for the one
 * before, Escape to close. Only the page counts, not the side bar.
 */
type Highlights = { set: (name: string, h: unknown) => void; delete: (name: string) => void };
const highlights = (): Highlights | null => (globalThis as { CSS?: { highlights?: Highlights } }).CSS?.highlights ?? null;
const Highlight = (globalThis as { Highlight?: new (...r: Range[]) => unknown }).Highlight;

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

export function FindBar() {
  const [open, setOpen] = useState(false);
  const [words, setWords] = useState("");
  const [found, setFound] = useState<Range[]>([]);
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
    const find = () => {
      const root = document.querySelector(".world-body") ?? document.body;
      // Asked for in a document: only once it is drawn — not in a preview.
      const want = asked.current;
      const scope = want?.doc
        ? [...root.querySelectorAll(".article-page")].find((p) => p.getAttribute("data-document") === want.doc && !p.closest(".link-preview"))
        : root;
      const ranges = scope ? rangesOf(scope, words.trim()) : [];
      setFound(ranges);
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
    const page = document.querySelector(".world-body");
    const watch = new MutationObserver(() => find());
    if (page) watch.observe(page, { childList: true, subtree: true, characterData: true });
    return () => watch.disconnect();
  }, [open, words, asking]);

  const current = found.length ? found[Math.min(at, found.length - 1)] : null;
  useEffect(() => {
    const h = highlights();
    if (!open) {
      h?.delete("find");
      h?.delete("find-current");
      return;
    }
    if (h && Highlight) {
      h.set("find", new Highlight(...found));
      if (current) h.set("find-current", new Highlight(current));
      else h.delete("find-current");
    }
    // Only when one steps or types — not each time the page changes under
    // the bar; asked for at a place, once it is found there.
    const key = `${words}\u0000${at}`;
    const want = asked.current;
    if (current && (!want || want.range === current) && scrolled.current !== key) {
      scrolled.current = key;
      asked.current = null;
      if (want?.scroll !== false) current.startContainer.parentElement?.scrollIntoView({ block: "center", behavior: "smooth" });
    }
    // A browser paints no marks in a button: a link gets a frame.
    const framed = current?.startContainer.parentElement?.closest<HTMLElement>("button, a, [data-raw]");
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
        ↑
      </button>
      <button className="ghost compact" title="Next (Enter)" onClick={() => step(1)} disabled={!found.length}>
        ↓
      </button>
      <button className="ghost compact" title="Close (Escape)" onClick={() => setOpen(false)}>
        ×
      </button>
    </div>
  );
}
