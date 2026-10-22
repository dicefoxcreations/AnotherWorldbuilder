import { useEffect, useRef, useState } from "react";
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

type Card = { id: string; box: DOMRect };

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
      clearTimeout(showTimer.current);
      clearTimeout(hideTimer.current);
      if (link && id) {
        // The card this link opened already: it stays, and those above it.
        if (current.current[depth + 1]?.id === id) return;
        showTimer.current = setTimeout(() => {
          setStack((s) => [...s.slice(0, depth + 1), { id, box: link.getBoundingClientRect() }]);
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
            className="link-preview"
            style={style}
            role="tooltip"
          >
            <div className="link-preview-page">
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
            </div>
          </div>
        );
      })}
    </>
  );
}
