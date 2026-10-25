import type { ArticleView, Place, Template, WorldInfo } from "../../api/client";
import { DocumentPage } from "../wiki/Document";
import { findType, isA, pageOwner, subtypes, mayCreate } from "../../shared/types";

export type { Place };

/**
 * Where one stands inside a world — always an article.
 *
 * The world's page, a type's page, a document: they are all articles and are
 * shown the same way. A type's page only adds what belongs to the type —
 * its subtypes and the button for a new document.
 */
export function WorldOverview({
  world,
  templates,
  articles,
  loading,
  place,
  onPlace,
  onNew,
  onChanged,
  onError,
}: {
  world: WorldInfo;
  templates: Template[];
  articles: ArticleView[];
  /** The documents are still being read. */
  loading: boolean;
  place: Place;
  onPlace: (o: Place) => void;
  onNew: (articleType: string) => void;
  onChanged: () => void;
  onError: (e: unknown) => void;
}) {
  const id =
    place.kind === "article"
      ? place.id
      : place.kind === "list"
        ? findType(templates, place.articleType)?.page
        : world.home;
  const article = id ? articles.find((a) => a.id === id) : undefined;
  if (!article && loading) return <p className="note">Loading …</p>;
  if (!article) {
    return (
      <p className="note">
        {place.kind === "article"
          ? "This article does not exist here — perhaps it is not readable in this view."
          : "Nothing released to you here yet."}
      </p>
    );
  }

  const owner = pageOwner(templates, world.home, article);
  return (
    <DocumentPage
      key={article.id}
      world={world}
      article={article}
      templates={templates}
      all={articles}
      actions={
        owner?.kind === "type" ? (
          <TypeActions world={world} templates={templates} articles={articles} type={owner.type} onPlace={onPlace} onNew={onNew} />
        ) : undefined
      }
      onPlace={onPlace}
      onChanged={onChanged}
      onError={onError}
    />
  );
}

/** What a type's page offers beyond its text: the subtypes, and a new one. */
function TypeActions({
  world,
  templates,
  articles,
  type,
  onPlace,
  onNew,
}: {
  world: WorldInfo;
  templates: Template[];
  articles: ArticleView[];
  type: Template;
  onPlace: (o: Place) => void;
  onNew: (articleType: string) => void;
}) {
  return (
    <>
      {/* The subtypes are collections of their own — and part of this one. */}
      {subtypes(templates, type.id).map((b) => (
        <button key={b.id} className="chip" onClick={() => onPlace({ kind: "list", articleType: b.id })}>
          {b.icon} {b.name}
          <small className="small">{articles.filter((a) => isA(templates, a.articleType, b.id)).length}</small>
        </button>
      ))}
      {mayCreate(world, type) && (
        <button className="primary" onClick={() => onNew(type.id)}>
          ＋ New {type.name}
        </button>
      )}
    </>
  );
}

