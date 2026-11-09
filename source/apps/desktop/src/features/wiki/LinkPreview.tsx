import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { anchoredParts } from "../../shared/Anchors";
import { Unfolded } from "../../shared/ui/BlockFold";
import { useWorldData } from "../worlds/WorldData";
import { ArticlePage } from "./ArticlePage";

/**
 * A look at a document without opening it: resting the pointer on a link
 * shows the whole page as it would be read — smaller, to look at, not to
 * change. What this reader cannot decrypt is not there, as on the page; a
 * document with nothing readable shows only its name.
 *
 * Any element with `data-preview=<id>` is such a link — in text, in the
 * infobox, in lists, in the editor and in a preview itself: resting on a
 * link in a preview opens the next one above it, a stack like nested menus.
 * A card stays while the pointer is on it, on a card above it or on the link
 * that opened it; going back to a card closes those above it. One listener
 * for the whole page, so no link has to know about it.
 */

const SHOW_MS = 450;
const HIDE_MS = 300;
const WIDTH = 760;

/** `anchor`: a link to a line of the document — the card shows that part alone. */
type Card = { id: string; anchor?: string; box: DOMRect };

export function LinkPreview() {
  const data = useWorldData();
  const [stack, setStack] = useState<Card[]>([]);
  const cards = useRef<(HTMLDivElement | null)[]>([]);
  const current = useRef<Card[]>([]);
  current.current = stack;
  const showTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const hideTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  useEffect(() => {
    /** Which card a node lies in — -1 for the page. */
    const depthOf = (t: EventTarget | null) => {
      if (!(t instanceof Node)) return -1;
      for (let i = current.current.length - 1; i >= 0; i--) if (cards.current[i]?.contains(t)) return i;
      return -1;
    };
    const keep = (n: number) => {
      clearTimeout(showTimer.current);
      clearTimeout(hideTimer.current);
      setStack((s) => (s.length > n ? s.slice(0, n) : s));
    };
    const over = (e: MouseEvent) => {
      const depth = depthOf(e.target);
      const link = (e.target as Element | null)?.closest?.("[data-preview]");
      const id = link ? link.getAttribute("data-preview") : null;
      const anchor = link?.getAttribute("data-preview-anchor") ?? undefined;
      clearTimeout(showTimer.current);
      clearTimeout(hideTimer.current);
      if (link && id) {
        // The card this link opened already: it stays, and those above it.
        const open = current.current[depth + 1];
        if (open?.id === id && open.anchor === anchor) return;
        showTimer.current = setTimeout(() => {
          setStack((s) => [...s.slice(0, depth + 1), { id, anchor, box: link.getBoundingClientRect() }]);
        }, SHOW_MS);
        return;
      }
      // On a card, off its links: the cards above it go — after a moment, to
      // get from a link to the card it opened.
      hideTimer.current = setTimeout(() => keep(depth + 1), HIDE_MS);
    };
    const down = (e: MouseEvent) => {
      if (depthOf(e.target) < 0) keep(0);
    };
    // A click in a card does what it does — and then the cards go.
    const clicked = (e: MouseEvent) => {
      if (depthOf(e.target) >= 0) setTimeout(() => keep(0), 0);
    };
    const scrolled = (e: Event) => {
      if (depthOf(e.target) < 0) keep(0);
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") keep(Math.max(0, current.current.length - 1));
    };
    document.addEventListener("mouseover", over);
    document.addEventListener("mousedown", down, true);
    document.addEventListener("click", clicked);
    document.addEventListener("scroll", scrolled, true);
    document.addEventListener("keydown", key, true);
    return () => {
      clearTimeout(showTimer.current);
      clearTimeout(hideTimer.current);
      document.removeEventListener("mouseover", over);
      document.removeEventListener("mousedown", down, true);
      document.removeEventListener("click", clicked);
      document.removeEventListener("scroll", scrolled, true);
      document.removeEventListener("keydown", key, true);
    };
  }, []);

  if (!data || stack.length === 0) return null;
  return (
    <>
      {stack.map((card, i) => {
        const article = data.articles.find((a) => a.id === card.id);
        if (!article) return null;
        // Where there is more room: below the link or above it.
        const below = window.innerHeight - card.box.bottom - 16;
        const above = card.box.top - 16;
        const left = Math.max(8, Math.min(card.box.left, window.innerWidth - WIDTH - 8));
        const style =
          below >= above
            ? { left, top: card.box.bottom + 8, maxHeight: below, zIndex: 60 + i }
            : { left, bottom: window.innerHeight - card.box.top + 8, maxHeight: above, zIndex: 60 + i };
        return (
          <div
            key={`${i}-${card.id}`}
            ref={(el) => {
              cards.current[i] = el;
            }}
            className={card.anchor ? "link-preview anchored" : "link-preview"}
            style={style}
            role="tooltip"
          >
            {(() => {
              const page = (
                <ArticlePage
                  world={data.world}
                  article={article}
                  templates={data.templates}
                  all={data.articles}
                  onPlace={data.onPlace}
                  onChanged={data.onChanged}
                  onError={() => {}}
                  glance
                />
              );
              return card.anchor ? <PartOnly anchor={card.anchor}>{page}</PartOnly> : <div className="link-preview-page">{page}</div>;
            })()}
          </div>
        );
      })}
    </>
  );
}

/**
 * Of a page, the part an anchor stands for — a paragraph, a list item, a
 * table. The page is drawn out of sight; once the anchor is in it, a copy of
 * its part is shown. Not there (closed to this reader, or gone): the page.
 */
function PartOnly({ anchor, children }: { anchor: string; children: ReactNode }) {
  const hidden = useRef<HTMLDivElement>(null);
  const shown = useRef<HTMLDivElement>(null);
  const [missing, setMissing] = useState(false);
  useLayoutEffect(() => {
    const page = hidden.current;
    if (!page) return;
    const take = () => {
      const parts = anchoredParts(page, anchor);
      if (parts.length === 0 || !shown.current) return false;
      // A table stands with its frame; a lone item with the list around it.
      const lone = parts.length === 1 && parts[0].tagName === "LI";
      const holder = document.createElement(lone ? (parts[0].parentElement?.tagName === "OL" ? "ol" : "ul") : "div");
      holder.className = "markdown";
      for (const part of parts) holder.appendChild(part.cloneNode(true));
      shown.current.replaceChildren(holder);
      return true;
    };
    if (take()) return;
    // The text is read after the page is drawn — wait for it; found, the
    // wait ends, and the page stays out of sight.
    const watch = new MutationObserver(() => {
      if (!take()) return;
      watch.disconnect();
      clearTimeout(give);
    });
    watch.observe(page, { childList: true, subtree: true });
    const give = setTimeout(() => (watch.disconnect(), setMissing(true)), 2000);
    return () => (watch.disconnect(), clearTimeout(give));
  }, [anchor]);
  return (
    <>
      <div ref={shown} className="link-preview-part" />
      <div ref={hidden} className={missing ? "link-preview-page" : "link-preview-page out-of-sight"}>
        {/* Looked for with nothing folded — a part under a folded heading is found too. */}
        <Unfolded.Provider value={!missing}>{children}</Unfolded.Provider>
      </div>
    </>
  );
}

