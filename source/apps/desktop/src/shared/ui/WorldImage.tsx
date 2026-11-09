import { useEffect, useRef, useState } from "react";
import { Icon } from "./Icons";
import { api } from "../../api/client";

/**
 * An image of the world, as its holder gives it to this reader.
 *
 * An image never changes under its id, so what was opened is kept — up to
 * a budget, the longest unseen let go first: a map's original is many
 * megabytes. What could not be opened — not here yet, or not for this
 * reader — is asked again next time: it may be there by then.
 *
 * Opening takes the device's work: a gallery of hundreds asks for them a
 * few at a time, and only for those on screen. What is asked for and gone
 * from the screen before its turn is not opened at all — whoever moves on
 * quickly does not wait for what they left.
 */
const opened = new Map<string, Promise<string>>();
/** How large each kept image is, as text — and all of them together. */
const sizes = new Map<string, number>();
let kept = 0;
const BUDGET = 256 * 1024 * 1024;

/** Kept as wanted just now: last to be let go. */
function touch(key: string, found: Promise<string>) {
  opened.delete(key);
  opened.set(key, found);
}

/** Lets go of the longest unseen images until the rest fits the budget. */
function fit() {
  for (const key of opened.keys()) {
    if (kept <= BUDGET) return;
    const size = sizes.get(key);
    // One still on its way weighs nothing yet.
    if (size === undefined) continue;
    opened.delete(key);
    sizes.delete(key);
    kept -= size;
  }
}

/** How many images are opened at once. */
const AT_ONCE = 4;
let running = 0;
type Waiting = { key: string; start: () => void; wanted: number; listeners: ((url: string) => void)[] };
const waiting: Waiting[] = [];

function next() {
  while (running < AT_ONCE) {
    const turn = waiting.shift();
    if (!turn) return;
    if (turn.wanted <= 0) continue;
    running++;
    turn.start();
  }
}

/** Opens an image — at once, without waiting for a turn. */
export function imageUrl(world: string, id: string, preview: boolean): Promise<string> {
  const key = `${world} ${id} ${preview}`;
  let found = opened.get(key);
  if (found) {
    touch(key, found);
    return found;
  }
  found = api.imageData(world, id, preview);
  opened.set(key, found);
  found.then(
    (url) => {
      if (opened.get(key) !== found) return;
      sizes.set(key, url.length);
      kept += url.length;
      fit();
    },
    () => opened.delete(key),
  );
  return found;
}

/**
 * Opens an image when its turn comes. The returned function says the image
 * is no longer wanted; if its turn has not come, it is not opened.
 */
function queued(world: string, id: string, preview: boolean, done: (url: string) => void): () => void {
  const key = `${world} ${id} ${preview}`;
  const ready = opened.get(key);
  if (ready) {
    touch(key, ready);
    ready.then(done).catch(() => {});
    return () => {};
  }
  let turn = waiting.find((w) => w.key === key);
  if (!turn) {
    const mine: Waiting = {
      key,
      wanted: 0,
      listeners: [],
      start: () =>
        imageUrl(world, id, preview)
          .then((url) => mine.listeners.forEach((l) => l(url)))
          .catch(() => {})
          .finally(() => {
            running--;
            next();
          }),
    };
    turn = mine;
    waiting.push(mine);
  }
  let gone = false;
  turn.wanted++;
  turn.listeners.push((url) => !gone && done(url));
  const mine = turn;
  next();
  return () => {
    gone = true;
    mine.wanted--;
  };
}

export function WorldImage({
  world,
  id,
  preview = false,
  alt = "",
  className,
  style,
  onClick,
}: {
  world: string;
  id: string;
  /** The small gallery picture instead of the original. */
  preview?: boolean;
  alt?: string;
  className?: string;
  style?: React.CSSProperties;
  onClick?: () => void;
}) {
  const [url, setUrl] = useState<string | null>(null);
  const [seen, setSeen] = useState(false);
  const spot = useRef<HTMLSpanElement>(null);

  // Opened once it comes near the screen.
  useEffect(() => {
    setSeen(false);
    const el = spot.current;
    if (!el || typeof IntersectionObserver === "undefined") {
      setSeen(true);
      return;
    }
    const watch = new IntersectionObserver((entries) => entries.some((e) => e.isIntersecting) && setSeen(true), { rootMargin: "300px" });
    watch.observe(el);
    return () => watch.disconnect();
  }, [world, id, preview]);

  useEffect(() => {
    setUrl(null);
    if (!seen) return;
    return queued(world, id, preview, setUrl);
  }, [world, id, preview, seen]);

  if (!url)
    return (
      <span ref={spot} className={`image-placeholder ${className ?? ""}`} style={style}>
        <Icon name="image" />
      </span>
    );
  return <img src={url} alt={alt} className={className} style={style} draggable={false} onClick={onClick} />;
}
