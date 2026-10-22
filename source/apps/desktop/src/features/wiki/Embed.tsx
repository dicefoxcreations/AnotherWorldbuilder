import { createContext, useContext } from "react";
import { linker } from "../../shared/Links";
import { isProperties, pageOwner } from "../../shared/types";
import { FoldChevron, useFold } from "../../shared/ui/BlockFold";
import { CurrentDocument, useWorldData } from "../worlds/WorldData";
import { CreateButton } from "./CreateButton";
import { BlockText } from "./ArticlePage";

/** The documents being drawn into one another — one that shows itself stops. */
const Embedding = createContext<string[]>([]);

/**
 * `<embed of=Sessions>` — another document's text, drawn here under its
 * name: a page made of pages. The name leads to it, its arrow folds it; a
 * type's page brings its create button. `fold=folded` starts it folded,
 * `fold=off` never folds it, `title=off` leaves the name out. What this
 * reader cannot open is not there — not even its name, nor a hint that
 * something is missing, also not while the GM looks as a player.
 */
export function Embed({ of, fold = "", title = "" }: { of: string; fold?: string; title?: string }) {
  const data = useWorldData();
  const chain = useContext(Embedding);
  const current = useContext(CurrentDocument);
  const wanted = of.trim().toLowerCase();
  const doc = data?.articles.find((a) => a.id === of.trim()) ?? data?.articles.find((a) => a.name.toLowerCase() === wanted);
  const [foldedNow, toggle] = useFold(current ?? "", `embed:${doc?.id ?? of}`, fold === "folded");
  const headless = title === "off";
  const folds = fold !== "off" && !headless;
  const folded = folds && foldedNow;
  if (!data) return <p className="listing-placeholder small">{of}</p>;
  if (!doc)
    return data.world.isGm && !data.previewing ? (
      <p className="dead-link">{of.startsWith("art:") ? "This page is gone." : `“${of}” is not here.`}</p>
    ) : null;
  if (doc.id === current || chain.includes(doc.id)) return null;
  const owner = pageOwner(data.templates, data.world.home, doc);
  const resolve = linker(data.articles, data.templates);
  const parts = doc.layers.filter((l) => !isProperties(l.section) && l.text.trim() !== "");
  return (
    <Embedding.Provider value={[...chain, doc.id]}>
      <CurrentDocument.Provider value={doc.id}>
        <section className={`embedded${folded ? " folded" : ""}`}>
          {!headless && (
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
          {/* Its headings fold here as on its own page — remembered for this page. */}
          {!folded && <BlockText article={current ?? ""} block={`embed:${doc.id}`} layers={parts} base={doc.visibility} resolve={resolve} onPlace={data.onPlace} />}
        </section>
      </CurrentDocument.Provider>
    </Embedding.Provider>
  );
}
