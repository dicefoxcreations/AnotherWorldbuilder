import { useState } from "react";
import { api, type ArticleView, type Template, type WorldInfo } from "../../api/client";
import { defaults, layersFor, slugify, startTexts } from "../wiki/Article";
import { DeleteDialog, DeleteButton } from "../../shared/ui/Delete";
import { typeOf } from "../../shared/types";

/**
 * The documents of a type, quick to add by name.
 *
 * Some types are mostly names: gender with male, female, diverse. Damage
 * type with fire, cold, acid. They exist so a property like `@@Gender@@` can
 * reach them, and
 * nobody wants to open a full article page for each one. Here a name is
 * enough — whoever wants to write more about “male” later opens it like any
 * other document.
 *
 * They are ordinary documents, made exactly as “New” makes them: with the
 * type's defaults and its blocks. A click on one opens it.
 */
export function ValueList({
  world,
  template,
  templates,
  articles,
  onArticlesChanged,
  onOpen,
  onError,
}: {
  world: WorldInfo;
  template: Template;
  templates: Template[];
  articles: ArticleView[];
  onArticlesChanged: () => void;
  /** Opens a document — leaving the options. */
  onOpen: (id: string) => void;
  onError: (e: unknown) => void;
}) {
  const [fresh, setFresh] = useState("");
  const [remove, setRemove] = useState<ArticleView | null>(null);
  // Alphabetical, always: a new value falls into line instead of being
  // appended at the bottom. Otherwise the list stands in the order in which
  // things occurred to someone.
  const values = articles
    .filter((a) => a.articleType === template.id)
    .sort((a, b) => a.name.localeCompare(b.name));

  const add = () => {
    const name = fresh.trim();
    if (!name || !template.id) return;
    if (values.some((w) => w.name.trim().toLowerCase() === name.toLowerCase())) {
      onError(`“${name}” is already on the list.`);
      return;
    }
    // The saved type with what it inherits — the draft may not be saved yet.
    const type = typeOf(templates, template.id) ?? template;
    const visibility = type.auth?.view ?? "all";
    const layers = layersFor(type, defaults(type, articles, templates), startTexts(type), {}, visibility);
    api
      .articleCreate(world.id, template.id, slugify(name), name, visibility, layers)
      .then(() => {
        setFresh("");
        onArticlesChanged();
      })
      .catch(onError);
  };

  if (!template.id)
    return (
      <p className="note">
        Save first — then you can add documents. They hang on the type's id, and that comes
        into being when you save.
      </p>
    );

  return (
    <div className="value-list">
      <h3>
        Documents <small className="small">{values.length}</small>
      </h3>
      <ul className="data-values">
        {values.map((w) => (
          <li key={w.id}>
            <button className="link-button" onClick={() => onOpen(w.id)} title={`Open ${w.name}`}>
              {w.name}
            </button>
            <DeleteButton what={w.name} onClick={() => setRemove(w)} />
          </li>
        ))}
        {values.length === 0 && <li className="note">None yet.</li>}
      </ul>

      <div className="line">
        <input
          value={fresh}
          onChange={(e) => setFresh(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
          placeholder="New document by name — “female”, say"
        />
        <button onClick={add} disabled={!fresh.trim()}>
          ＋ Add
        </button>
      </div>

      {/* The same path as everywhere: the name is typed, and it is also the
          confirmation Rust wants to see. */}
      {remove && (
        <DeleteDialog
          title={`Delete ${remove.name}`}
          name={remove.name}
          warning="The document is gone. Fields that picked it will sit empty."
          onCancel={() => setRemove(null)}
          onDelete={(confirmation) => {
            api
              .articleDelete(world.id, remove.id, confirmation)
              .then(() => {
                setRemove(null);
                onArticlesChanged();
              })
              .catch(onError);
          }}
        />
      )}
    </div>
  );
}
