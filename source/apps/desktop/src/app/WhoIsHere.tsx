import { useEffect, useState } from "react";
import { api, type ArticleView, type Here } from "../api/client";
import { colourOf, initialsOf } from "../shared/editor/RemoteCarets";
import { pace, SLOW_MS } from "./live";

/**
 * Who else is in the world right now — each in their colour, the one their
 * caret has in a text too. Tells the others on which page one is; a click
 * on someone goes where they are.
 */
export function WhoIsHere({
  world,
  article,
  articles,
  onOpen,
}: {
  world: string;
  /** The page open here — empty where it is no document. */
  article: string;
  articles: ArticleView[];
  onOpen: (article: string) => void;
}) {
  const [here, setHere] = useState<Here[]>([]);

  useEffect(() => {
    let gone = false;
    const tell = () => {
      if (!pace.network) {
        setHere([]);
        return;
      }
      api
        .worldPresence(world, article)
        .then((h) => {
          if (!gone) setHere((old) => (JSON.stringify(old) === JSON.stringify(h) ? old : h));
        })
        .catch(() => {});
    };
    tell();
    const timer = setInterval(tell, SLOW_MS);
    return () => {
      gone = true;
      clearInterval(timer);
    };
  }, [world, article]);

  if (here.length === 0) return null;
  const nameOf = (id: string) => articles.find((a) => a.id === id)?.name;
  return (
    <span className="who-is-here">
      {here.map((h) => {
        const page = nameOf(h.article);
        return (
          <button
            key={h.id}
            className={h.editing ? "here-one editing" : "here-one"}
            style={{ background: colourOf(h.id) }}
            title={page ? `${h.name} · ${h.editing ? "writing in" : "on"} ${page}` : h.name}
            disabled={!page}
            onClick={() => page && onOpen(h.article)}
          >
            {initialsOf(h.name)}
          </button>
        );
      })}
    </span>
  );
}
