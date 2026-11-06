import { useContext, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, type FolderNode, type GalleryImage } from "../../api/client";
import { WorldImage } from "./WorldImage";
import { CurrentDocument, useWorldData } from "../../features/worlds/WorldData";
import { FolderTile } from "../../features/gallery/FolderTile";
import { folderAt, navigationPlaces } from "../../features/gallery/Sorting";

/**
 * Choosing an image: uploading a new one under a name, or taking one from
 * the gallery. Shows what this reader may see — the GM everything, in the
 * gallery's folders, starting in the one where the document stands in the
 * navigation. A search looks through all. What goes wrong it says itself.
 */
export function ImagePicker({
  world,
  onChosen,
  onRemove,
  onCancel,
}: {
  world: string;
  /** The image — with how it is put into a text, where known. */
  onChosen: (id: string, image?: GalleryImage) => void;
  /** Where an image stands already: takes it out. */
  onRemove?: () => void;
  onCancel: () => void;
}) {
  const [problem, setProblem] = useState<string | null>(null);
  const onError = (e: unknown) => setProblem(String(e));
  const [images, setImages] = useState<GalleryImage[] | null>(null);
  const [search, setSearch] = useState("");
  const [file, setFile] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);

  const data = useWorldData();
  const near = useContext(CurrentDocument);
  const gm = !!data?.world.isGm && !data.previewing;
  const [folders, setFolders] = useState<FolderNode[]>([]);
  // The folder one looks into — empty: the top.
  const [at, setAt] = useState("");

  useEffect(() => {
    api.gallery(world).then(setImages).catch(onError);
    if (!gm) return;
    Promise.all([api.galleryFolders(world), api.navigationLoad(world)])
      .then(([f, nav]) => {
        setFolders(f);
        // Where the document stands: its images most likely lie there.
        const doc = data?.articles.find((a) => a.id === near);
        const path = doc && navigationPlaces(nav ?? [], data!.articles, data!.templates).of(doc);
        const start = path && folderAt(f, path);
        if (start) setAt(start);
      })
      .catch(onError);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [world]);

  useEffect(() => {
    const key = (e: KeyboardEvent) => e.key === "Escape" && onCancel();
    document.addEventListener("keydown", key);
    return () => document.removeEventListener("keydown", key);
  }, [onCancel]);

  const choose = async () => {
    try {
      const path = await open({ multiple: false, filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }] });
      if (typeof path !== "string") return;
      setFile(path);
      const base = path.split(/[\\/]/).pop() ?? "";
      setName(base.replace(/\.[^.]+$/, ""));
    } catch (e) {
      onError(e);
    }
  };

  const upload = async () => {
    if (!file || !name.trim()) return;
    setBusy(true);
    try {
      onChosen(await api.imageUpload(world, file, name.trim()));
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  };

  const needle = search.trim().toLowerCase();
  const known = new Set(folders.map((f) => f.id));
  const folderOf = (i: GalleryImage) => (known.has(i.folder) ? i.folder : "");
  // In folders only for whoever has them, and not while searching.
  const browsing = folders.length > 0 && !needle;
  const shown = (images ?? []).filter((i) => (needle ? i.name.toLowerCase().includes(needle) : !browsing || folderOf(i) === at));
  const subfolders = browsing ? folders.filter((f) => f.parent === at).sort((a, b) => a.name.localeCompare(b.name)) : [];
  const trail: FolderNode[] = [];
  for (let f = folders.find((x) => x.id === at); f && trail.length < 32; f = folders.find((x) => x.id === f!.parent)) trail.unshift(f);
  const inside = (id: string) => {
    const set = new Set([id]);
    for (let grew = true; grew; ) {
      grew = false;
      for (const f of folders) if (set.has(f.parent) && !set.has(f.id)) (set.add(f.id), (grew = true));
    }
    return (images ?? []).filter((i) => set.has(folderOf(i))).length;
  };

  return (
    <div className="dialog-backdrop" onClick={onCancel}>
      <div className="image-picker" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
        <h3>Image</h3>
        <div className="line image-upload">
          <button onClick={choose} disabled={busy}>
            From file
          </button>
          {file && (
            <>
              <input
                autoFocus
                value={name}
                placeholder="Name"
                onChange={(e) => setName(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && void upload()}
              />
              <button className="primary" onClick={upload} disabled={busy || !name.trim()}>
                {busy ? "Uploading" : "Upload"}
              </button>
            </>
          )}
        </div>
        {problem && <p className="warning" role="status">{problem}</p>}
        <input className="image-search" value={search} placeholder="Search" onChange={(e) => setSearch(e.target.value)} />
        {browsing && (
          <nav className="image-trail">
            <button className="ghost compact" onClick={() => setAt("")}>
              Images
            </button>
            {trail.map((f) => (
              <span key={f.id}>
                ›{" "}
                <button className="ghost compact" onClick={() => setAt(f.id)}>
                  {f.name}
                </button>
              </span>
            ))}
          </nav>
        )}
        {images === null ? (
          <p className="small">Loading</p>
        ) : shown.length === 0 && subfolders.length === 0 ? (
          <p className="small">{images.length === 0 ? "No images yet." : "Nothing found."}</p>
        ) : (
          <div className="image-grid">
            {subfolders.map((f) => (
              <FolderTile key={f.id} name={f.name} count={inside(f.id)} onOpen={() => setAt(f.id)} />
            ))}
            {shown.map((i) => (
              <button key={i.id} className="image-tile" onClick={() => onChosen(i.id, i)} title={i.name || undefined}>
                <WorldImage world={world} id={i.id} preview />
                <span className="image-name">{i.name || " "}</span>
              </button>
            ))}
          </div>
        )}
        <div className="line">
          {onRemove && (
            <button className="danger" onClick={onRemove}>
              No image
            </button>
          )}
          <button onClick={onCancel}>Cancel</button>
        </div>
      </div>
    </div>
  );
}
