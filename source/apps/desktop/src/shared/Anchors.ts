/**
 * Leading to a place in a document's text: `@@#^2f9e17|the inn@@` goes to the
 * line closed by the anchor `^2f9e17` — in the same document, or with a name
 * in front in another one (`aw_model::link`, `aw_model::markdown`).
 */

/** An anchor asked for in a document not drawn yet — shown once it is. */
let waiting: { doc: string; anchor: string } | null = null;

/** Scrolls to an anchor of the open page and lights up its line. False: not drawn. */
export function showAnchor(anchor: string): boolean {
  const spot = document.querySelector(`.article-page [data-anchor="${CSS.escape(anchor)}"]`);
  if (!spot) return false;
  const line = spot.closest("li, tr, p, h1, h2, h3, h4, h5, h6") ?? spot.parentElement;
  if (!line) return false;
  line.scrollIntoView({ block: "center", behavior: "smooth" });
  line.classList.remove("anchor-lit");
  // Again from the start when it was lit already.
  void (line as HTMLElement).offsetWidth;
  line.classList.add("anchor-lit");
  return true;
}

/** Goes to an anchor: here at once, elsewhere once `open` has drawn that document. */
export function leadTo(doc: string, anchor: string, current: string | null, open: () => void) {
  if (doc === current && showAnchor(anchor)) return;
  waiting = { doc, anchor };
  open();
}

/** An anchor drawn: if it was asked for, it is shown now. */
export function anchorDrawn(doc: string | null, anchor: string) {
  if (!waiting || waiting.doc !== doc || waiting.anchor !== anchor) return;
  waiting = null;
  requestAnimationFrame(() => showAnchor(anchor));
}
