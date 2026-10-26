import { createContext, useContext } from "react";
import { linker } from "../../shared/Links";
import { isProperties, pageOwner, typeOf } from "../../shared/types";
import type { ArticleView, Template } from "../../api/client";
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
 * `fold=off` never folds it, `title=off` leaves the name out,
 * `section=Map` shows that section of it alone — one map on several pages,
 * kept in one place. What this
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
  const keys = section.trim() ? sectionKeys(doc, typeOf(data.templates, doc.articleType), section) : null;
  const parts = doc.layers.filter(
    (l) => !isProperties(l.section) && l.text.trim() !== "" && (!keys || keys.some((k) => l.section === k || l.section.startsWith(`${k}-`))),
  );
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

/** The keys of a document's sections of that title — its own, or its type's. */
function sectionKeys(doc: ArticleView, type: Template | undefined, title: string): string[] {
  const wanted = title.trim().toLowerCase();
  const own = doc.layers.filter((l) => l.title.trim().toLowerCase() === wanted).map((l) => l.section);
  const typed = (type?.blocks ?? []).filter((b) => b.title.trim().toLowerCase() === wanted).map((b) => b.key);
  return [...own, ...typed];
}
