import { useEffect, useRef, useState } from "react";
import { api } from "../../api/client";

/**
 * An image of the world, drawn from its encrypted parts.
 *
 * An image never changes under its id, so what was opened once is kept for
 * the rest of the run. What could not be opened — not here yet, or not for
 * this reader — is asked again next time: the parts may have come by then.
 *
 * Opening takes the device's work: a gallery of hundreds asks for them a
 * few at a time, and only for those on screen. What is asked for and gone
 * from the screen before its turn is not opened at all — whoever moves on
 * quickly does not wait for what they left.
 */
const opened = new Map<string, Promise<string>>();

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
  if (!found) {
    found = api.imageData(world, id, preview);
    opened.set(key, found);
    found.catch(() => opened.delete(key));
  }
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
        🖼
      </span>
    );
  return <img src={url} alt={alt} className={className} style={style} draggable={false} onClick={onClick} />;
}
