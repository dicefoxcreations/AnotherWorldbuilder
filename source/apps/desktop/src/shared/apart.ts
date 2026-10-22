/**
 * Whether a click means “open this apart”, in a window of its own: with
 * Ctrl (⌘ on a Mac) held, or with the middle button — as in a browser.
 *
 * The app goes to a place in many spots: links in text, lists, cards,
 * chips, timelines. None of them needs to know about windows: they all go
 * through the one navigation of the app, which asks here.
 */
let apart = false;

const settle = () => setTimeout(() => (apart = false), 0);

export function watchApartClicks() {
  // On pressing already: a link in the editor opens on mousedown.
  document.addEventListener(
    "mousedown",
    (e) => {
      if (e.button === 0 && (e.ctrlKey || e.metaKey)) apart = true;
      // No scrolling by the middle button over what opens.
      if (e.button === 1 && (e.target as Element).closest("button, a")) e.preventDefault();
    },
    true,
  );
  document.addEventListener("click", (e) => (e.ctrlKey || e.metaKey ? ((apart = true), settle()) : settle()), true);
  // The middle button clicks no button by itself: it clicks for it.
  document.addEventListener(
    "auxclick",
    (e) => {
      if (e.button !== 1) return;
      const target = (e.target as Element).closest("button, a");
      if (!target) return;
      e.preventDefault();
      apart = true;
      (target as HTMLElement).click();
      apart = false;
    },
    true,
  );
}

export const wantsApart = () => apart;
