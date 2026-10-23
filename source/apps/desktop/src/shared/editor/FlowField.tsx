import { useEffect, useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";
import { pieces, type Piece } from "../GraphText";
import { mapPosition } from "./Merge";
import { runs, shownName, type Run } from "./FlowText";

/**
 * One field for a part of a document — its text with the images and graphs
 * standing in it, the way they stand when it is read.
 *
 * A `<textarea>` is a box: an image could only stand above, below or beside
 * it, and two images cut the text into three fields. Here the text is one
 * editable flow; an image is an element in it that floats left or right,
 * and the text runs past it and on below it at full width — as on the page.
 *
 * The text is the truth, as in a field: what is shown is built from it, and
 * what is typed is read back into it. Surfaces (images, graphs) stand on
 * lines of their own in the text; the field keeps it that way.
 */

/** Zero-width space: a place to put the caret where no text is — read back as nothing. */
const ROOM = "​";

const strip = (s: string) => s.split(ROOM).join("");

/** Plain text in a row counts as one stretch; empty ones not at all. */
function joined(parts: string[]): string {
  const out: string[] = [];
  for (const p of parts) {
    if (p === "t0") continue;
    const last = out[out.length - 1];
    if (/^t\d+$/.test(p) && last && /^t\d+$/.test(last)) out[out.length - 1] = `t${Number(last.slice(1)) + Number(p.slice(1))}`;
    else out.push(p);
  }
  return out.join("|");
}

/** The runs as they stand in a container of the field — to see whether they still fit the text. */
function shownIn(box: Node): string[] {
  return [...box.childNodes].flatMap((c) => {
    const el = c as HTMLElement;
    if (c.nodeType === Node.TEXT_NODE) return [`t${strip((c as Text).data).length}`];
    if (el.dataset?.raw !== undefined) return [`${el.dataset.look}:${el.dataset.raw.length}`];
    if (el.dataset?.line !== undefined) return [`${el.dataset.line}(`, ...shownIn(el), ")"];
    if (el.dataset?.look !== undefined) return [`${el.dataset.look}:${strip(el.textContent ?? "").length}`];
    return [el.dataset?.surface !== undefined ? "s" : "t0"];
  });
}

const shownRuns = (root: HTMLElement) => joined(shownIn(root));

/** The same, for runs as they would be built. */
const runTokens = (all: Run[]): string[] =>
  all.flatMap((r) => (r.inner ? [`${r.look}(`, ...runTokens(r.inner), ")"] : [r.look ? `${r.look}:${r.text.length}` : `t${r.text.length}`]));

/** The same, for a text as it would be built — with the caret at `caret`. */
function builtRuns(all: Piece[], caret: number): string {
  let at = 0;
  return joined(
    all.flatMap((p) => {
      const from = at;
      at += p.text.length;
      return p.kind !== "text" ? ["s"] : runTokens(runs(p.text, caret - from));
    }),
  );
}

/**
 * Where the page around an element stands — and a way back there. WebKit
 * moves a page when a field takes the focus or a selection is set: to the
 * field's start, before a click has placed the caret where it was clicked.
 * Nobody scrolls while clicking, so what moved meanwhile goes back.
 */
export function scrollOf(el: Element): () => void {
  const boxes: Element[] = [];
  for (let e: Element | null = el.parentElement; e; e = e.parentElement) {
    if (e.scrollHeight > e.clientHeight && /auto|scroll/.test(getComputedStyle(e).overflowY)) boxes.push(e);
  }
  if (document.scrollingElement) boxes.push(document.scrollingElement);
  const tops = boxes.map((b) => b.scrollTop);
  return () =>
    boxes.forEach((b, i) => {
      if (b.scrollTop !== tops[i]) b.scrollTop = tops[i];
    });
}

/**
 * The field as the helpers of a text field see it: value, selection, focus
 * — and where a position stands on screen, which a text field cannot say.
 */
export class FlowHandle {
  root: HTMLDivElement | null = null;
  /** The text as it stands in the field. */
  text = "";
  /** The surfaces standing in the field, by the index of their piece. */
  shown: Piece[] = [];

  get value() {
    return this.text;
  }
  get focused() {
    return this.root !== null && document.activeElement === this.root;
  }
  get selectionStart() {
    return this.selection()?.[0] ?? 0;
  }
  get selectionEnd() {
    return this.selection()?.[1] ?? 0;
  }
  focus() {
    this.root?.focus();
  }
  /** The selection as positions in the text — `null` where it is not in here. */
  selection(): [number, number] | null {
    const sel = window.getSelection();
    if (!this.root || !sel || sel.rangeCount === 0) return null;
    const r = sel.getRangeAt(0);
    if (!this.root.contains(r.startContainer) || !this.root.contains(r.endContainer)) return null;
    const a = this.offsetOf(r.startContainer, r.startOffset);
    const b = this.offsetOf(r.endContainer, r.endOffset);
    return [Math.min(a, b), Math.max(a, b)];
  }
  setSelectionRange(start: number, end: number) {
    const sel = window.getSelection();
    if (!this.root || !sel) return;
    // Setting the selection by hand moves no page: WebKit would show it.
    const keep = scrollOf(this.root);
    const r = document.createRange();
    const [an, ao] = this.pointAt(start);
    const [bn, bo] = this.pointAt(end);
    r.setStart(an, ao);
    r.setEnd(bn, bo);
    sel.removeAllRanges();
    sel.addRange(r);
    keep();
  }
  /** Where a position stands on screen — a thin box at the caret. */
  rectAt(offset: number): DOMRect | null {
    if (!this.root) return null;
    const [n, o] = this.pointAt(offset);
    const r = document.createRange();
    r.setStart(n, o);
    r.collapse(true);
    const rects = r.getClientRects();
    const rect = rects.length > 0 ? rects[0] : r.getBoundingClientRect();
    if (rect.height > 0) return rect;
    // An empty place: its line, at the start.
    const box = (n.nodeType === Node.TEXT_NODE ? n.parentElement : (n as Element)) ?? this.root;
    const b = box.getBoundingClientRect();
    return new DOMRect(b.left, b.top, 0, parseFloat(getComputedStyle(this.root).lineHeight) || 20);
  }
  /** The boxes a stretch of text covers on screen. */
  rectsOf(start: number, end: number): DOMRect[] {
    if (!this.root || end <= start) return [];
    const r = document.createRange();
    const [an, ao] = this.pointAt(start);
    const [bn, bo] = this.pointAt(end);
    r.setStart(an, ao);
    r.setEnd(bn, bo);
    return [...r.getClientRects()].filter((x) => x.width > 0);
  }

  /** How much of the text a node of the field stands for. */
  private lengthOf(node: ChildNode): number {
    if (node.nodeType === Node.TEXT_NODE) return strip((node as Text).data).length;
    const el = node as HTMLElement;
    if (el.dataset.surface !== undefined) return this.shown[Number(el.dataset.surface)]?.text.length ?? 0;
    if (el.dataset.raw !== undefined) return el.dataset.raw.length;
    if (el.dataset.room !== undefined) return 0;
    if (el.tagName === "BR") return 1;
    if (el.dataset.line !== undefined) return [...el.childNodes].reduce((n, c) => n + this.lengthOf(c), 0);
    return strip(el.textContent ?? "").length;
  }

  /** A point in the field, as a position in the text. */
  offsetOf(node: Node, offset: number): number {
    return this.root ? this.offsetIn(this.root, node, offset) : 0;
  }

  /** A point inside `box`, as a position counted from its start. */
  private offsetIn(box: Node, node: Node, offset: number): number {
    if (node === box) return [...box.childNodes].slice(0, offset).reduce((n, c) => n + this.lengthOf(c), 0);
    let total = 0;
    for (const child of box.childNodes) {
      if (child === node || child.contains(node)) {
        if (child.nodeType === Node.TEXT_NODE) return total + strip((child as Text).data.slice(0, offset)).length;
        const el = child as HTMLElement;
        // Inside a link drawn as it is read: before it — or after it, from its end.
        if (el.dataset.raw !== undefined) return node === child && offset > 0 ? total + this.lengthOf(child) : total;
        // Inside a drawn stretch or a line: count through it.
        if (el.dataset.look !== undefined || el.dataset.line !== undefined) return total + this.offsetIn(child, node, offset);
        // Inside a surface: before it.
        return total;
      }
      total += this.lengthOf(child);
    }
    return total;
  }

  /** A position in the text, as a point in the field. */
  pointAt(offset: number): [Node, number] {
    return this.pointIn(this.root!, Math.max(0, offset));
  }

  private pointIn(box: Node, at: number): [Node, number] {
    let left = at;
    const children = [...box.childNodes];
    for (let i = 0; i < children.length; i++) {
      const child = children[i];
      const length = this.lengthOf(child);
      const el = child as HTMLElement;
      if (child.nodeType === Node.TEXT_NODE) {
        if (left <= length) {
          // Count the characters, passing over the empty rooms.
          const data = (child as Text).data;
          let pos = 0;
          let seen = 0;
          while (pos < data.length && seen < left) {
            if (data[pos] !== ROOM) seen++;
            pos++;
          }
          return [child, pos];
        }
      } else if (el.dataset.raw !== undefined) {
        if (left < length) return [box, i];
      } else if (el.dataset.look !== undefined || el.dataset.line !== undefined) {
        // The end of a line is after its line break — that is the next line.
        const breaks = (child.textContent ?? "").endsWith("\n") && i + 1 < children.length;
        if (left < length || (left === length && !breaks)) {
          if (el.dataset.line !== undefined) {
            // A hidden mark holds no caret: its edge is before or after the stretch.
            const hidden = (n: ChildNode | null) => (n as HTMLElement | null)?.dataset?.look === "mark-hidden";
            if (left === 0 && hidden(child.firstChild)) return [box, i];
            if (left === length && hidden(child.lastChild)) return [box, i + 1];
            return this.pointIn(child, left);
          }
          const t = child.firstChild;
          if (!t) return [child, 0];
          return [t, Math.min(left, (t as Text).data.length)];
        }
      } else if (left < length || (left === 0 && length === 0)) {
        return [box, i];
      }
      left -= length;
    }
    return [box, children.length];
  }
}

/** The text a field holds: its text nodes, and the surfaces as their lines. */
function readBack(root: HTMLElement, shown: Piece[]): string {
  let out = "";
  const add = (s: string) => (out += s);
  const walk = (node: ChildNode) => {
    if (node.nodeType === Node.TEXT_NODE) return add(strip((node as Text).data));
    const el = node as HTMLElement;
    if (el.dataset.room !== undefined) return;
    if (el.dataset.surface !== undefined) {
      const piece = shown[Number(el.dataset.surface)];
      if (!piece) return;
      // A surface stands on a line of its own.
      if (out && !out.endsWith("\n")) add("\n");
      add(piece.text.endsWith("\n") ? piece.text : `${piece.text}\n`);
      return;
    }
    if (el.tagName === "BR") return add("\n");
    if (el.dataset.raw !== undefined) return add(el.dataset.raw);
    if (el.dataset.look !== undefined || el.dataset.line !== undefined) return el.childNodes.forEach(walk);
    // What a browser wraps in a line of its own.
    if (out && !out.endsWith("\n")) add("\n");
    el.childNodes.forEach(walk);
  };
  root.childNodes.forEach(walk);
  return out;
}

/** Whether the field holds nothing but what it was built of. */
function flat(root: HTMLElement): boolean {
  return [...root.childNodes].every(
    (c) =>
      c.nodeType === Node.TEXT_NODE ||
      (c as HTMLElement).dataset.surface !== undefined ||
      (c as HTMLElement).dataset.room !== undefined ||
      ((c as HTMLElement).dataset.line !== undefined && flat(c as HTMLElement)) ||
      ((c as HTMLElement).dataset.look !== undefined && [...c.childNodes].every((t) => t.nodeType === Node.TEXT_NODE)),
  );
}

type Snapshot = { text: string; caret: number };

/** Whether two cuttings differ in their surfaces alone — the text between them the same. */
function sameText(a: Piece[], b: Piece[]): boolean {
  return a.length === b.length && a.every((p, i) => p.kind === b[i].kind && (p.kind !== "text" || p.text === b[i].text));
}

/** Whether two cuttings of a text have the same surfaces in the same places. */
function sameSurfaces(a: Piece[], b: Piece[]): boolean {
  return a.length === b.length && a.every((p, i) => p.kind === b[i].kind && (p.kind === "text" || p.text === b[i].text));
}

export function FlowField({
  handle,
  text,
  className,
  placeholder,
  surface,
  surfaceClass,
  linkLook,
  onAnchor,
  onText,
  onCaret,
  handlers,
  children,
}: {
  handle: RefObject<FlowHandle>;
  text: string;
  className?: string;
  placeholder?: string;
  /** Draws the surface of a piece — an image, a graph. */
  surface: (piece: Piece, index: number) => ReactNode;
  /** How its place in the flow looks: `image right`, `graph`. */
  surfaceClass: (piece: Piece) => string;
  /** How a link is read: what it shows, and whether it leads anywhere. */
  linkLook?: (written: string) => { text: string; dead: boolean; id?: string; icon?: string; anchor?: string };
  /** A click on an anchor (`^name`): it is chosen — its name shown to change. */
  onAnchor?: (name: string) => void;
  onText: (text: string) => void;
  onCaret?: (start: number, end: number) => void;
  /** What the helpers listen to: key presses, selection, the context menu. */
  handlers?: Partial<React.DOMAttributes<HTMLDivElement>>;
  /** What is drawn over the field: marks, the carets of others. */
  children?: ReactNode;
}) {
  const root = useRef<HTMLDivElement>(null);
  const [hosts, setHosts] = useState<Map<number, HTMLElement>>(new Map());
  const undo = useRef<{ back: Snapshot[]; forward: Snapshot[]; at: number }>({ back: [], forward: [], at: 0 });
  const composing = useRef(false);
  const h = handle.current;

  /** Puts runs into a container of the field — lines as containers of their own. */
  const draw = (into: HTMLElement, all: Run[]) => {
    for (const r of all) {
      if (!r.look) {
        into.appendChild(document.createTextNode(r.text));
        continue;
      }
      if (r.inner) {
        const line = document.createElement("span");
        line.dataset.line = r.look;
        line.className = `flow-${r.look.split(" ").join(" flow-")}`;
        draw(line, r.inner);
        into.appendChild(line);
        continue;
      }
      if (r.look === "anchor") {
        // An anchor (`^2f9e17`) as one small sign; its name where it has one to read.
        const name = r.text.trim().slice(1);
        const atom = document.createElement("span");
        atom.contentEditable = "false";
        atom.dataset.look = "anchor";
        atom.dataset.raw = r.text;
        atom.className = "flow-anchor";
        atom.title = `Anchor ^${name}`;
        atom.textContent = /^[0-9a-f]{6}$/.test(name) ? "⚓" : `⚓ ${name}`;
        into.appendChild(atom);
        continue;
      }
      if (r.look === "link") {
        // One piece, not letters: the caret passes over it, a click opens it.
        const look = latest.current.linkLook?.(r.text) ?? { text: shownName(r.text), dead: false };
        const atom = document.createElement("span");
        atom.contentEditable = "false";
        atom.dataset.look = "link";
        atom.dataset.raw = r.text;
        atom.className = look.dead ? "flow-link dead-link" : "flow-link wikilink";
        if (look.id) atom.dataset.preview = look.id;
        if (look.anchor) atom.dataset.previewAnchor = look.anchor;
        if (look.icon) atom.dataset.icon = look.icon;
        atom.textContent = look.text;
        into.appendChild(atom);
        continue;
      }
      const span = document.createElement("span");
      span.dataset.look = r.look;
      span.className = `flow-${r.look.split(" ").join(" flow-")}`;
      span.textContent = r.text;
      into.appendChild(span);
    }
  };

  /** Builds the field anew from the text — a link with the caret in it as text, the others as read. */
  const build = (value: string, caret = -1) => {
    const el = root.current;
    if (!el) return;
    const all = pieces(value);
    el.textContent = "";
    const made = new Map<number, HTMLElement>();
    let at = 0;
    all.forEach((p, i) => {
      const from = at;
      at += p.text.length;
      if (p.kind === "text") {
        draw(el, runs(p.text, caret - from));
        return;
      }
      // Two surfaces in a row, or one at the end: a place for the caret
      // between them and after them.
      const host = document.createElement("div");
      host.contentEditable = "false";
      host.dataset.surface = String(i);
      host.className = `flow-surface ${surfaceClass(p)}`;
      el.appendChild(host);
      made.set(i, host);
      const next = all[i + 1];
      if (!next || next.kind !== "text") el.appendChild(document.createTextNode(ROOM));
    });
    // A line break at the very end makes a line — one can stand in it.
    if (value === "" || value.endsWith("\n")) el.appendChild(document.createTextNode(ROOM));
    // A browser swallows a line break right after a block — an image, a
    // heading. The empty line is there in the text, so it is shown.
    for (const n of el.childNodes) {
      const prev = n.previousSibling as HTMLElement | null;
      if (n.nodeType === Node.TEXT_NODE && prev?.nodeType === Node.ELEMENT_NODE && (n as Text).data.startsWith("\n")) {
        const block = prev.dataset.line === "item" || prev.dataset.line === "quote" || !!prev.dataset.line?.startsWith("heading");
        if (prev.dataset.surface !== undefined || block) (n as Text).data = ROOM + (n as Text).data;
      }
    }
    h.root = el;
    h.text = value;
    h.shown = all;
    setHosts(made);
  };

  // From outside: the text changed — rebuilt, the caret where it belongs.
  // Where only a surface changed — its width, its caption — the field stays
  // as it is: what one types into the surface keeps the focus.
  useLayoutEffect(() => {
    if (h.text === text && root.current?.childNodes.length) return;
    const cut = pieces(text);
    if (root.current?.childNodes.length && sameText(cut, h.shown)) {
      h.text = text;
      h.shown = cut;
      for (const [i, host] of hosts) host.className = `flow-surface ${surfaceClass(cut[i])}`;
      setHosts(new Map(hosts));
      return;
    }
    const caret = h.focused ? h.selection() : null;
    const before = h.text;
    const moved = caret ? [mapPosition(before, text, caret[0]), mapPosition(before, text, caret[1])] : null;
    build(text, moved && moved[0] === moved[1] ? moved[0] : -1);
    if (moved) h.setSelectionRange(moved[0], moved[1]);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text]);

  // A floating surface that leaves no room beside it stands on its own: the
  // caret would have only a sliver next to it. The page does the same.
  useLayoutEffect(() => {
    const el = root.current;
    if (!el) return;
    const fit = () => {
      const room = el.clientWidth;
      for (const host of hosts.values()) {
        if (host.classList.contains("nowrap") || (!host.classList.contains("left") && !host.classList.contains("right"))) continue;
        host.classList.remove("full");
        const side = parseFloat(getComputedStyle(host).marginLeft) + parseFloat(getComputedStyle(host).marginRight);
        if (host.getBoundingClientRect().width + side > room - 120) host.classList.add("full");
      }
    };
    fit();
    const watch = new ResizeObserver(fit);
    watch.observe(el);
    hosts.forEach((host) => watch.observe(host));
    return () => watch.disconnect();
  }, [hosts]);

  /** What was typed — into the text, and the field tidied where it has to be. */
  const typed = (next: string, caret?: number) => {
    const was = h.text;
    if (next === was) return;
    const u = undo.current;
    const now = Date.now();
    // A pause, a new line or more than a letter starts a new step back.
    const lines = (t: string) => t.split("\n").length;
    if (now - u.at > 600 || Math.abs(next.length - was.length) !== 1 || lines(next) !== lines(was)) {
      u.back.push({ text: was, caret: h.selection()?.[0] ?? 0 });
    }
    if (u.back.length > 200) u.back.shift();
    u.forward = [];
    u.at = now;
    const at = caret ?? h.selection()?.[0];
    const cut = pieces(next);
    // A new image typed, a line break given, a link closed: the field is laid out anew.
    if (caret !== undefined || !flat(root.current!) || !sameSurfaces(cut, h.shown) || shownRuns(root.current!) !== builtRuns(cut, at ?? -1)) {
      build(next, at ?? -1);
      if (at !== undefined) h.setSelectionRange(at, at);
    } else {
      h.text = next;
      h.shown = cut;
    }
    latest.current.onText(next);
  };

  /** Puts `insert` where the selection is. */
  const replaceSelection = (insert: string) => {
    const [a, b] = h.selection() ?? [h.text.length, h.text.length];
    typed(h.text.slice(0, a) + insert + h.text.slice(b), a + insert.length);
  };

  const step = (from: Snapshot[], to: Snapshot[]) => {
    const snap = from.pop();
    if (!snap) return;
    to.push({ text: h.text, caret: h.selection()?.[0] ?? 0 });
    build(snap.text, snap.caret);
    h.setSelectionRange(snap.caret, snap.caret);
    latest.current.onText(snap.text);
  };

  // The listeners hang on the field once; they reach what is current here.
  const latest = useRef({ typed, replaceSelection, step, onText, linkLook });
  latest.current = { typed, replaceSelection, step, onText, linkLook };

  // A link drawn before its document was here — made from the right-click
  // menu a moment ago — looks as it now resolves. Only what changed is touched.
  useEffect(() => {
    if (!linkLook || !root.current) return;
    for (const atom of root.current.querySelectorAll<HTMLElement>(":scope [data-look=link]")) {
      if (atom.closest("[data-surface]")) continue;
      const look = linkLook(atom.dataset.raw ?? "");
      const className = look.dead ? "flow-link dead-link" : "flow-link wikilink";
      if (atom.className !== className) atom.className = className;
      if (atom.textContent !== look.text) atom.textContent = look.text;
      if ((atom.dataset.preview ?? "") !== (look.id ?? "")) {
        if (look.id) atom.dataset.preview = look.id;
        else delete atom.dataset.preview;
      }
      if ((atom.dataset.previewAnchor ?? "") !== (look.anchor ?? "")) {
        if (look.anchor) atom.dataset.previewAnchor = look.anchor;
        else delete atom.dataset.previewAnchor;
      }
      if ((atom.dataset.icon ?? "") !== (look.icon ?? "")) {
        if (look.icon) atom.dataset.icon = look.icon;
        else delete atom.dataset.icon;
      }
    }
  });

  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const inSurface = (t: EventTarget | null) => t instanceof Node && t !== el && !!(t instanceof Element ? t : t.parentElement)?.closest("[data-surface]");
    const now = () => latest.current;
    const input = (e: Event) => {
      if (inSurface(e.target) || composing.current) return;
      now().typed(readBack(el, h.shown));
    };
    const before = (e: InputEvent) => {
      if (inSurface(e.target)) return;
      switch (e.inputType) {
        case "insertParagraph":
        case "insertLineBreak":
          e.preventDefault();
          now().replaceSelection("\n");
          break;
        case "insertFromPaste":
        case "insertFromDrop": {
          e.preventDefault();
          const pasted = e.dataTransfer?.getData("text/plain") ?? "";
          if (pasted) now().replaceSelection(pasted.replace(/\r\n?/g, "\n"));
          break;
        }
        case "historyUndo":
          e.preventDefault();
          now().step(undo.current.back, undo.current.forward);
          break;
        case "historyRedo":
          e.preventDefault();
          now().step(undo.current.forward, undo.current.back);
          break;
        case "formatBold":
        case "formatItalic":
        case "formatUnderline":
          e.preventDefault();
          break;
      }
    };
    const start = () => (composing.current = true);
    const end = () => {
      composing.current = false;
      now().typed(readBack(el, h.shown));
    };
    el.addEventListener("input", input);
    el.addEventListener("beforeinput", before as EventListener);
    el.addEventListener("compositionstart", start);
    el.addEventListener("compositionend", end);
    return () => {
      el.removeEventListener("input", input);
      el.removeEventListener("beforeinput", before as EventListener);
      el.removeEventListener("compositionstart", start);
      el.removeEventListener("compositionend", end);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** Whether an event comes from inside a surface — its caption, its width: none of the field's business. */
  const fromSurface = (e: React.SyntheticEvent) => e.target instanceof Element && e.target !== root.current && !!e.target.closest("[data-surface]");
  // What the helpers listen to, only for the text — not for what is typed into a surface.
  const own = Object.fromEntries(
    Object.entries(handlers ?? {}).map(([name, f]) => [
      name,
      (e: React.SyntheticEvent) => {
        if (!fromSurface(e)) (f as (e: React.SyntheticEvent) => void)(e);
      },
    ]),
  ) as Partial<React.DOMAttributes<HTMLDivElement>>;

  const pressed = useRef<(() => void) | null>(null);
  const caretMoved = () => {
    const sel = h.selection();
    if (!sel) return;
    // Into a link or a stretch, out of one: it is drawn anew — as text to
    // write, or as read.
    if (sel[0] === sel[1] && root.current && shownRuns(root.current) !== builtRuns(h.shown, sel[0])) {
      build(h.text, sel[0]);
      h.setSelectionRange(sel[0], sel[0]);
    }
    if (onCaret) onCaret(sel[0], sel[1]);
  };

  /** A click on a link opens it: its text, the caret before its closing marks. */
  const openAtom = (e: React.MouseEvent) => {
    const atom = e.target instanceof Element ? e.target.closest<HTMLElement>("[data-raw]") : null;
    if (!atom || !root.current?.contains(atom)) return false;
    e.preventDefault();
    // An anchor is chosen, not opened: its name is set in the side bar.
    if (atom.dataset.look === "anchor" && onAnchor) {
      onAnchor((atom.dataset.raw ?? "").trim().slice(1));
      return true;
    }
    // In a list item, a heading or bold text too: counted from its own container.
    const box = atom.parentNode!;
    const start = h.offsetOf(box, [...box.childNodes].indexOf(atom));
    // A link opens before its closing marks, an anchor after itself.
    const end = start + (atom.dataset.raw?.length ?? 0) - (atom.dataset.look === "link" ? 2 : 0);
    root.current.focus();
    build(h.text, end);
    h.setSelectionRange(end, end);
    onCaret?.(end, end);
    return true;
  };

  return (
    <div className="flow-box">
      <div
        ref={(el) => {
          root.current = el;
          h.root = el;
        }}
        className={`flow-field ${className ?? ""}${strip(text).trim() === "" && hosts.size === 0 ? " empty" : ""}`}
        contentEditable="plaintext-only"
        suppressContentEditableWarning
        spellCheck
        data-placeholder={placeholder}
        {...own}
        onKeyDown={(e) => {
          if (fromSurface(e)) return;
          handlers?.onKeyDown?.(e);
          if (e.defaultPrevented) return;
          const mod = e.ctrlKey || e.metaKey;
          if (mod && e.key.toLowerCase() === "z") {
            e.preventDefault();
            if (e.shiftKey) step(undo.current.forward, undo.current.back);
            else step(undo.current.back, undo.current.forward);
          } else if (mod && e.key.toLowerCase() === "y") {
            e.preventDefault();
            step(undo.current.forward, undo.current.back);
          }
        }}
        onKeyUp={(e) => {
          if (fromSurface(e)) return;
          handlers?.onKeyUp?.(e);
          caretMoved();
        }}
        onMouseDown={(e) => {
          if (fromSurface(e)) return;
          // Where the page stood when the button went down — it stays there.
          pressed.current = scrollOf(e.currentTarget);
          if (openAtom(e)) return;
          handlers?.onMouseDown?.(e);
        }}
        onMouseUp={(e) => {
          if (fromSurface(e)) return;
          handlers?.onMouseUp?.(e);
          caretMoved();
          const back = pressed.current;
          pressed.current = null;
          if (back) {
            back();
            requestAnimationFrame(back);
          }
        }}
        onSelect={(e) => {
          if (fromSurface(e)) return;
          handlers?.onSelect?.(e);
          caretMoved();
        }}
      />
      {[...hosts.entries()].map(([i, host]) => {
        const piece = h.shown[i];
        return piece ? createPortal(surface(piece, i), host, `s${i}`) : null;
      })}
      {children}
    </div>
  );
}

/** The marks of a version comparison over a flow field — boxes behind the text. */
export function FlowMarks({
  handle,
  text,
  ranges,
  kind,
}: {
  handle: RefObject<FlowHandle>;
  text: string;
  ranges: { start: number; end: number }[];
  kind: string;
}) {
  const [boxes, setBoxes] = useState<{ top: number; left: number; width: number; height: number }[]>([]);
  useLayoutEffect(() => {
    const h = handle.current;
    const box = h.root?.parentElement;
    if (!h.root || !box || ranges.length === 0) {
      setBoxes([]);
      return;
    }
    const place = () => {
      const origin = box.getBoundingClientRect();
      setBoxes(
        ranges.flatMap((r) =>
          h.rectsOf(r.start, r.end).map((x) => ({ top: x.top - origin.top, left: x.left - origin.left, width: x.width, height: x.height })),
        ),
      );
    };
    place();
    const observer = new ResizeObserver(place);
    observer.observe(h.root);
    return () => observer.disconnect();
  }, [handle, text, ranges]);
  return (
    <>
      {boxes.map((b, i) => (
        <span key={i} className={`flow-diff mark-${kind}`} style={b} aria-hidden="true" />
      ))}
    </>
  );
}
