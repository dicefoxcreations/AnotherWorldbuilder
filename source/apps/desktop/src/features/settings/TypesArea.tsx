import { useEffect, useState } from "react";
import { Icon } from "../../shared/ui/Icons";
import { DeleteDialog, DeleteButton } from "../../shared/ui/Delete";
import { api, type ArticleView, type Template, type WorldInfo } from "../../api/client";
import { TemplateEditor } from "../templates/TemplateEditor";
import { findType, isA, realTypes, subtypes, usersOf } from "../../shared/types";

export function TypesArea({
  world,
  templates,
  articles,
  onChanged,
  onArticlesChanged,
  onOpen,
  onError,
}: {
  world: WorldInfo;
  templates: Template[];
  articles: ArticleView[];
  onChanged: (t: Template[]) => void;
  onArticlesChanged: () => void;
  onOpen: (id: string) => void;
  onError: (e: unknown) => void;
}) {
  const [open, setOpen] = useState<string | null>(null);
  const [draft, setDraft] = useState<Template | null>(null);
  const [saved, setSaved] = useState(false);

  // Opening a type edits a copy of its own.
  useEffect(() => {
    if (open === null) return;
    const found = templates.find((t) => t.id === open);
    if (found) {
      setDraft(structuredClone(found));
      setSaved(false);
    }
  }, [open, templates]);

  /**
   * A new type comes into being as a draft, not as a file.
   *
   * It used to be saved immediately on creation, and the name was then asked
   * for again in a second field — two fields for the same thing, and the upper
   * one without effect. Now there is only the field in the editor, and it is
   * saved only at the press of a button.
   */
  const create = (group: boolean) => {
    setDraft({
      id: "",
      name: "",
      icon: group ? "🧩" : "📄",
      supertype: "",
      group,
      rules: [],
      description: "",
      properties: [],
      infobox: { align: "none", width: "auto", show: [] },
      auth: null,
      namePattern: "%%input%%",
      page: "",
      blocks: [],
    });
    setOpen(null);
    setSaved(false);
  };

  const save = () => {
    if (!draft) return;
    const name = draft.name.trim();
    if (!name) {
      onError(draft.group ? "The group needs a name." : "The type needs a name.");
      return;
    }
    // The name may occur only once: references in the text point at it
    // (`type=Place`), and two types of the same name could not be told apart
    // there. The id behind it is untouched by this — it never changes, not
    // even on a rename.
    const duplicate = templates.some(
      (t) => t.id !== draft.id && t.name.trim().toLowerCase() === name.toLowerCase(),
    );
    if (duplicate) {
      onError(`“${name}” already exists — references could not tell the two apart.`);
      return;
    }

    api
      .templateSave(world.id, { ...draft, name })
      .then((all) => {
        onChanged(all);
        // A new type has a new page, a renamed one a renamed page.
        onArticlesChanged();
        // The id comes from Rust: a new type gets it there.
        const stored = all.find((t) => t.name === name);
        if (stored) {
          setDraft(stored);
          setOpen(stored.id);
        }
        setSaved(true);
        setTimeout(() => setSaved(false), 2000);
      })
      .catch(onError);
  };

  const [remove, setRemove] = useState<Template | null>(null);
  const [unfolded, setUnfolded] = useState<Set<string>>(new Set());

  const back = () => {
    setDraft(null);
    setOpen(null);
  };

  if (draft)
    return (
      <TemplateEditor
        world={world}
        draft={draft}
        templates={templates}
        articles={articles}
        onChange={setDraft}
        onSave={save}
        onBack={back}
        onArticlesChanged={onArticlesChanged}
        onOpen={onOpen}
        onError={onError}
        saved={saved}
      />
    );

  // A subtype sits inside its supertype, folded away until opened — a
  // character opens to show the NPC. A type whose supertype is gone stands
  // at the top. By name within each level, so things can be found again.
  const byName = (list: Template[]) => [...list].sort((a, b) => a.name.localeCompare(b.name));
  const types = realTypes(templates);
  const groups = byName(templates.filter((t) => t.group));
  const roots = byName(types.filter((t) => !t.supertype || !findType(templates, t.supertype)));
  const toggle = (id: string) =>
    setUnfolded((all) => {
      const next = new Set(all);
      if (!next.delete(id)) next.add(id);
      return next;
    });

  const row = (t: Template, seen: Set<string>): React.ReactNode => {
    // A supertype chain that runs in a circle ends here, not in the stack.
    if (seen.has(t.id)) return null;
    const inner = new Set(seen).add(t.id);
    const children = byName(subtypes(templates, t.id));
    const isUnfolded = unfolded.has(t.id);
    const count = articles.filter((a) => isA(templates, a.articleType, t.id)).length;
    return (
      <li key={t.id}>
        <div className="type-row">
          <button
            className="type-fold ghost icon compact"
            disabled={children.length === 0}
            aria-label={isUnfolded ? `Fold ${t.name}` : `Unfold ${t.name}`}
            onClick={() => toggle(t.id)}
          >
            {children.length === 0 ? "" : isUnfolded ? <Icon name="chevron-down" /> : <Icon name="chevron-right" />}
          </button>
          <button className="type-open" onClick={() => setOpen(t.id)} title={t.description || undefined}>
            <span className="type-icon">{t.icon || "📄"}</span>
            <strong>{t.name}</strong>
            <small className="small type-counts">
              {count} {count === 1 ? "document" : "documents"} · {t.properties.length}{" "}
              {t.properties.length === 1 ? "property" : "properties"} · {t.blocks.length}{" "}
              {t.blocks.length === 1 ? "block" : "blocks"}
            </small>
          </button>
          <DeleteButton what={t.name} onClick={() => setRemove(t)} />
        </div>
        {isUnfolded && children.length > 0 && (
          <ul className="type-tree">{children.map((c) => row(c, inner))}</ul>
        )}
      </li>
    );
  };

  return (
    <div className="type-page">
      <div className="line between">
        <h2>Types</h2>
        <button className="primary" onClick={() => create(false)}>
          <Icon name="add" /> New
        </button>
      </div>

      {types.length === 0 ? (
        <p className="small empty">No type yet.</p>
      ) : (
        <ul className="type-tree top">{roots.map((t) => row(t, new Set()))}</ul>
      )}

      {/* Property groups: properties several types take in — the abilities
          of characters, monsters and feats alike. */}
      <div className="line between groups-head">
        <h3>Property groups</h3>
        <button onClick={() => create(true)}><Icon name="add" /> Group</button>
      </div>
      {groups.length === 0 ? (
        <p className="small empty">No group yet.</p>
      ) : (
        <ul className="type-tree top">
          {groups.map((g) => {
            const users = usersOf(templates, g.id).filter((t) => !t.group);
            return (
              <li key={g.id}>
                <div className="type-row">
                  <button className="type-fold ghost icon compact" disabled aria-hidden="true" tabIndex={-1} />
                  <button className="type-open" onClick={() => setOpen(g.id)} title={g.description || undefined}>
                    <span className="type-icon">{g.icon || "🧩"}</span>
                    <strong>{g.name}</strong>
                    <small className="small type-counts">
                      {g.properties.length} {g.properties.length === 1 ? "property" : "properties"}
                      {users.length > 0 && ` · in ${users.map((u) => u.name).join(", ")}`}
                    </small>
                  </button>
                  <DeleteButton what={g.name} onClick={() => setRemove(g)} />
                </div>
              </li>
            );
          })}
        </ul>
      )}

      {remove && (
        <DeleteDialog
          title={`Delete ${remove.icon || "📄"} ${remove.name}`}
          name={remove.name}
          warning={
            remove.group
              ? "The group goes with its properties."
              : "The template goes — with every field and block you put together in it. Documents of this type stay, but have no template any more, and references to it offer nothing."
          }
          onCancel={() => setRemove(null)}
          onDelete={() => {
            api
              .templateDelete(world.id, remove.id)
              .then((remaining) => {
                setRemove(null);
                onChanged(remaining);
              })
              .catch(onError);
          }}
        />
      )}

    </div>
  );
}
