import { createContext, useContext } from "react";
import { pageOwner } from "../../shared/types";
import { FoldChevron, useFold } from "../../shared/ui/BlockFold";
import { CurrentDocument, useWorldData } from "../worlds/WorldData";
import { CreateButton } from "./CreateButton";
import { ArticlePage } from "./ArticlePage";

/** The documents being drawn into one another — one that shows itself stops. */
const Embedding = createContext<string[]>([]);

/**
 * `<embed of=Sessions>` — another document drawn here under its name, as
 * its own page reads: its infobox — a creature's stat block — and its text.
 * A page made of pages. The name leads to it, its arrow folds it; a
 * type's page brings its create button. `fold=folded` starts it folded,
 * `fold=off` never folds it, `title=off` leaves the name out,
 * `section=Map` shows that section of it alone, as it looks there and with
 * its values — one map on several pages kept in one place, a creature's
 * stat block in a book. What this
 * reader cannot open is not there — not even its name, nor a hint that
 * something is missing, also not while the GM looks as a player.
 */
export function Embed({ of, fold = "", title = "", section = "" }: { of: string; fold?: string; title?: string; section?: string }) {
  const data = useWorldData();
  const chain = useContext(Embedding);
  const current = useContext(CurrentDocument);
  const wanted = of.trim().toLowerCase();
  const doc = data?.articles.find((a) => a.id === of.trim()) ?? data?.articles.find((a) => a.name.toLowerCase() === wanted);
  const [foldedNow, toggle] = useFold(current ?? "", `embed:${doc?.id ?? of}${section ? `:${section}` : ""}`, fold === "folded");
  const headless = title === "off";
  // One section names its document inside, as its own first line.
  const head = !headless && !section.trim();
  const folds = fold !== "off" && head;
  const folded = folds && foldedNow;
  if (!data) return <p className="listing-placeholder small">{of}</p>;
  if (!doc)
    return data.world.isGm && !data.previewing ? (
      <p className="dead-link">{of.startsWith("art:") ? "This page is gone." : `“${of}” is not here.`}</p>
    ) : null;
  if (doc.id === current || chain.includes(doc.id)) return null;
  const owner = pageOwner(data.templates, data.world.home, doc);
  return (
    <Embedding.Provider value={[...chain, doc.id]}>
      <CurrentDocument.Provider value={doc.id}>
        <section className={`embedded${folded ? " folded" : ""}`}>
          {head && (
            <h3 className="markdown-head embed-head">
              {folds && (
                <button className="head-fold" aria-expanded={!folded} title={folded ? "Open" : "Fold"} onClick={toggle}>
                  <FoldChevron open={!folded} />
                </button>
              )}
              <button className="embed-name" data-preview={doc.id} onClick={() => data.onPlace({ kind: "article", id: doc.id })}>
                {doc.name}
              </button>
              {owner?.kind === "type" && <CreateButton of={[owner.type.id]} label="＋" small />}
            </h3>
          )}
          {!folded && (
            <div className="embedded-page">
              <ArticlePage world={data.world} article={doc} templates={data.templates} all={data.articles} onPlace={data.onPlace} glance headless={section.trim() ? headless : true} section={section} />
            </div>
          )}
        </section>
      </CurrentDocument.Provider>
    </Embedding.Provider>
  );
}
