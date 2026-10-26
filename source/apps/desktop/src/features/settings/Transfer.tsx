import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { api, type ArticleView, type PackageSummary, type Template, type WorldInfo } from "../../api/client";
import { ancestors, isA, realTypes } from "../../shared/types";

/**
 * Types and documents from one world into another — the GM's. A package is
 * a folder, as plain text and files; see `transfer.rs`.
 */
export function TransferArea({
  world,
  templates,
  articles,
  onImported,
  onError,
}: {
  world: WorldInfo;
  templates: Template[];
  articles: ArticleView[];
  onImported: () => void;
  onError: (e: unknown) => void;
}) {
  return (
    <div className="type-page transfer">
      <h2>Transfer</h2>
      <ExportPart world={world} templates={templates} articles={articles} onError={onError} />
      <ImportPart world={world} onImported={onImported} onError={onError} />
    </div>
  );
}

/** Its types in tree order, each with its depth. */
function inTree(templates: Template[]): [Template, number][] {
  const kinds = [...realTypes(templates), ...templates.filter((t) => t.group)];
  const out: [Template, number][] = [];
  const add = (parent: string, depth: number) => {
    for (const t of kinds.filter((k) => k.supertype === parent).sort((a, b) => a.name.localeCompare(b.name))) {
      out.push([t, depth]);
      add(t.id, depth + 1);
    }
  };
  add("", 0);
  // What hangs on a type that is gone still shows.
  for (const t of kinds) if (!out.some(([o]) => o.id === t.id)) out.push([t, ancestors(templates, t).length]);
  return out;
}

function ExportPart({ world, templates, articles, onError }: { world: WorldInfo; templates: Template[]; articles: ArticleView[]; onError: (e: unknown) => void }) {
  const [what, setWhat] = useState<"types" | "content">("content");
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const [name, setName] = useState(world.name);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<PackageSummary | null>(null);
  const toggle = (id: string) =>
    setChosen((was) => {
      const now = new Set(was);
      if (!now.delete(id)) now.add(id);
      return now;
    });
  const tree = inTree(templates);
  const count = (id: string) => articles.filter((a) => isA(templates, a.articleType, id)).length;

  const run = async () => {
    const parent = await open({ directory: true, title: "Where the package goes" });
    if (typeof parent !== "string") return;
    const file = name.trim().replace(/[\\/:*?"<>|]/g, "-") || "Package";
    setBusy(true);
    setDone(null);
    try {
      const ids = [...chosen];
      setDone(
        await api.packageExport(world.id, `${parent}/${file}.awpack`, {
          name: name.trim(),
          types: what === "types" ? ids : [],
          documentsOf: what === "content" ? ids : [],
        }),
      );
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="transfer-part">
      <h3>Export</h3>
      <div className="line">
        <div className="segmented">
          <button className={what === "content" ? "on" : ""} onClick={() => setWhat("content")}>
            Content
          </button>
          <button className={what === "types" ? "on" : ""} onClick={() => setWhat("types")}>
            Types
          </button>
        </div>
        <button className="compact" onClick={() => setChosen(new Set(tree.map(([t]) => t.id)))}>
          All
        </button>
        <button className="compact" onClick={() => setChosen(new Set())}>
          None
        </button>
      </div>
      <ul className="transfer-types">
        {tree.map(([t, depth]) => (
          <li key={t.id} style={{ paddingLeft: depth * 18 }}>
            <label>
              <input type="checkbox" checked={chosen.has(t.id)} onChange={() => toggle(t.id)} />
              <span>
                {t.icon} {t.name}
              </span>
              {what === "content" && !t.group && <small className="small">{count(t.id)}</small>}
            </label>
          </li>
        ))}
      </ul>
      <div className="line">
        <input className="transfer-name" value={name} onChange={(e) => setName(e.target.value)} placeholder="Name" />
        <button className="primary" disabled={busy || chosen.size === 0} onClick={() => void run()}>
          {busy ? "Exporting" : "Export"}
        </button>
      </div>
      {done && (
        <p className="small">
          {done.typesNew} types · {done.documentsNew} documents · {done.imagesNew} images
        </p>
      )}
    </section>
  );
}

function ImportPart({ world, onImported, onError }: { world: WorldInfo; onImported: () => void; onError: (e: unknown) => void }) {
  const [path, setPath] = useState<string | null>(null);
  const [seen, setSeen] = useState<PackageSummary | null>(null);
  const [progress, setProgress] = useState<[number, number] | null>(null);
  const [skipped, setSkipped] = useState<string[]>([]);
  useEffect(() => {
    const stop = listen<[number, number]>("package-progress", (e) => setProgress(e.payload));
    return () => void stop.then((un) => un());
  }, []);

  const pick = async () => {
    const chosen = await open({ directory: true, title: "A package (.awpack)" });
    if (typeof chosen !== "string") return;
    try {
      setSeen(await api.packageRead(world.id, chosen));
      setPath(chosen);
    } catch (e) {
      onError(e);
    }
  };
  const run = async () => {
    if (!path) return;
    setProgress([0, 1]);
    try {
      const done = await api.packageImport(world.id, path);
      setSkipped(done.imagesSkipped);
      setSeen(await api.packageRead(world.id, path));
      onImported();
    } catch (e) {
      onError(e);
    } finally {
      setProgress(null);
    }
  };

  return (
    <section className="transfer-part">
      <h3>Import</h3>
      <div className="line">
        <button onClick={() => void pick()} disabled={!!progress}>
          From package
        </button>
        {seen && <strong>{seen.name || path?.split("/").pop()}</strong>}
      </div>
      {seen && (
        <>
          <dl className="transfer-summary">
            <dt>Types</dt>
            <dd>
              {seen.typesNew} new{seen.typesKnown > 0 && ` · ${seen.typesKnown} replaced`}
            </dd>
            <dt>Documents</dt>
            <dd>
              {seen.documentsNew} new{seen.documentsKnown > 0 && ` · ${seen.documentsKnown} here already`}
            </dd>
            <dt>Images</dt>
            <dd>
              {seen.imagesNew} new{seen.imagesKnown > 0 && ` · ${seen.imagesKnown} here already`}
            </dd>
          </dl>
          {skipped.length > 0 && <p className="small">Left out, no picture the app reads: {skipped.join(", ")}</p>}
          {progress ? (
            <div className="transfer-progress">
              <progress max={progress[1]} value={progress[0]} />
              <small className="small">
                {progress[0]} / {progress[1]}
              </small>
            </div>
          ) : (
            <button className="primary" disabled={seen.typesNew + seen.typesKnown + seen.documentsNew + seen.imagesNew === 0} onClick={() => void run()}>
              Import
            </button>
          )}
        </>
      )}
    </section>
  );
}
