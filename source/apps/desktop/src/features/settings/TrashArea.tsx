import { useEffect, useState } from "react";
import { api, type Template, type TrashedView, type WorldInfo } from "../../api/client";
import { findType } from "../../shared/types";
import { Icon } from "../../shared/ui/Icons";

/**
 * The world's trash, the GM's: what whoever may edit a document deleted,
 * newest first — brought back whole, or deleted for good.
 */
export function TrashArea({
  world,
  templates,
  onChanged,
  onError,
}: {
  world: WorldInfo;
  templates: Template[];
  /** The documents changed: read what changed. */
  onChanged: () => void;
  onError: (e: unknown) => void;
}) {
  const [items, setItems] = useState<TrashedView[] | null>(null);
  // The one asked about being deleted for good.
  const [asking, setAsking] = useState<string | null>(null);
  const load = () => api.trashList(world.id).then(setItems).catch(onError);
  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [world.id]);
  const act = (step: Promise<void>) =>
    step
      .then(() => {
        setAsking(null);
        onChanged();
        return load();
      })
      .catch(onError);
  const when = (at: number) => new Date(at).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });

  return (
    <section className="type-page trash">
      <h3>Trash</h3>
      {items === null ? (
        <p className="small muted">Loading</p>
      ) : items.length === 0 ? (
        <p className="small muted">Empty.</p>
      ) : (
        <ul className="trash-list">
          {items.map((t) => {
            const type = findType(templates, t.articleType);
            return (
              <li key={t.id}>
                <span className="trash-name">
                  {type?.icon ? `${type.icon} ` : ""}
                  {t.name}
                </span>
                <span className="small muted">
                  {type?.name ?? t.articleType} · {t.by} · {when(t.at)}
                </span>
                <span className="trash-actions">
                  <button className="compact" onClick={() => act(api.trashRestore(world.id, t.id))}>
                    <Icon name="undo" /> Restore
                  </button>
                  {asking === t.id ? (
                    <button className="danger compact" onClick={() => act(api.trashPurge(world.id, t.id))}>
                      Delete for good?
                    </button>
                  ) : (
                    <button className="ghost compact" title="Delete for good" onClick={() => setAsking(t.id)}>
                      <Icon name="delete" />
                    </button>
                  )}
                </span>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
