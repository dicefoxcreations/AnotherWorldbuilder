import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, type FolderNode, type GalleryImage, type ImageDefaults, type WorldInfo } from "../../api/client";
import { WorldImage } from "../../shared/ui/WorldImage";
import { ImagePicker } from "../../shared/ui/ImagePicker";
import { DeleteButton, DeleteDialog } from "../../shared/ui/Delete";
import { VisibilityPicker } from "../../shared/ui/Visibility";
import { useWorldData } from "../worlds/WorldData";
import { navigationPlaces, sortIntoFolders } from "./Sorting";
import { FolderTile } from "./FolderTile";

/**
 * The gallery of a world, in the options: a tree of folders on the left,
 * the chosen folder in the middle — its folders, then its images, as a file
 * browser shows them — the chosen image on the right. A search looks
 * through all of them. “Sort” puts each image in no folder where its
 * document stands in the navigation.
 *
 * The GM sorts: folders in folders, renamed, moved by dragging, deleted —
 * the images of a deleted folder then lie in none, its folders move up. The tree is sealed
 * for the GM; a player sees their own uploads and the images of parts they
 * can read, unsorted. Nothing is deleted by itself.
 */
type Filter = "all" | "used" | "versions" | "unused";

const FILTERS: { id: Filter; name: string }[] = [
  { id: "all", name: "All" },
  { id: "used", name: "Used" },
  { id: "versions", name: "Old versions only" },
  { id: "unused", name: "Unused" },
];

/** What the middle shows: every image, the top level, or a folder. */
type Shown = { kind: "all" } | { kind: "top" } | { kind: "folder"; id: string };

const newId = () => (typeof crypto !== "undefined" && "randomUUID" in crypto ? crypto.randomUUID() : `${Date.now()}-${Math.random()}`);

/** A folder and every folder below it. */
function below(folders: FolderNode[], id: string): Set<string> {
  const out = new Set([id]);
  for (let grew = true; grew; ) {
    grew = false;
    for (const f of folders) {
      if (out.has(f.parent) && !out.has(f.id)) {
        out.add(f.id);
        grew = true;
      }
    }
  }
  return out;
}

/** `Places › Coast` — for the folder choice of an image. */
function pathOf(folders: FolderNode[], id: string): string {
  const names: string[] = [];
  for (let at = folders.find((f) => f.id === id); at; at = folders.find((f) => f.id === at!.parent)) {
    names.unshift(at.name);
    if (names.length > 32) break;
  }
  return names.join(" › ");
}

export function GalleryArea({ world, onOpen, onError }: { world: WorldInfo; onOpen: (article: string) => void; onError: (e: unknown) => void }) {
  const [images, setImages] = useState<GalleryImage[] | null>(null);
  const [folders, setFolders] = useState<FolderNode[]>([]);
  const [shown, setShown] = useState<Shown>({ kind: "top" });
  const data = useWorldData();
  const [sorting, setSorting] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [chosen, setChosen] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const gm = world.isGm;

  const load = () =>
    Promise.all([api.gallery(world.id), gm ? api.galleryFolders(world.id) : Promise.resolve([])])
      .then(([i, f]) => {
        setImages(i);
        setFolders(f);
      })
      .catch(onError);
  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [world.id]);

  const known = useMemo(() => new Set(folders.map((f) => f.id)), [folders]);
  // An image whose folder is gone lies in none.
  const folderOf = (i: GalleryImage) => (known.has(i.folder) ? i.folder : "");

  const saveFolders = (next: FolderNode[]) => {
    setFolders(next);
    api.galleryFoldersSet(world.id, next).catch((e) => {
      onError(e);
      void load();
    });
  };
  const move = (image: string, to: string) => api.imageMove(world.id, image, to).then(load).catch(onError);

  const upload = async () => {
    try {
      const path = await open({ multiple: true, filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }] });
      const paths = typeof path === "string" ? [path] : (path ?? []);
      if (paths.length === 0) return;
      setBusy(true);
      const into = shown.kind === "folder" ? shown.id : "";
      for (const p of paths) {
        const base = p.split(/[\\/]/).pop() ?? "Image";
        await api.imageUpload(world.id, p, base.replace(/\.[^.]+$/, ""), into);
      }
      await load();
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  };

  /** Each image in no folder into the folder of where its document stands. */
  const sort = async () => {
    if (!data || !images) return;
    try {
      const nav = (await api.navigationLoad(world.id)) ?? [];
      const places = navigationPlaces(nav, data.articles, data.templates);
      const { folders: all, moves } = sortIntoFolders(images, folders, places, data.articles, newId);
      setFolders(all);
      await api.galleryFoldersSet(world.id, all);
      for (const [n, [image, folder]] of moves.entries()) {
        setSorting(`${n + 1}/${moves.length}`);
        await api.imageMove(world.id, image, folder);
      }
    } catch (e) {
      onError(e);
    } finally {
      setSorting(null);
      void load();
    }
  };

  const needle = search.trim().toLowerCase();
  // Where one is: its folders first — none while searching, which looks through all.
  const here = shown.kind === "folder" ? shown.id : shown.kind === "top" ? "" : null;
  const subfolders = needle || here === null ? [] : folders.filter((f) => f.parent === here).sort((a, b) => a.name.localeCompare(b.name));
  const visible = (images ?? []).filter((i) => {
    if (needle && !i.name.toLowerCase().includes(needle)) return false;
    if (!needle && here !== null && folderOf(i) !== here) return false;
    if (filter === "used") return i.usedIn.length > 0;
    if (filter === "versions") return i.inVersions;
    if (filter === "unused") return i.usedIn.length === 0 && !i.inVersions;
    return true;
  });
  const current = images?.find((i) => i.id === chosen) ?? null;
  const title =
    needle ? "Found" : shown.kind === "all" ? "All images" : shown.kind === "top" ? "Images" : pathOf(folders, shown.id) || "Images";

  return (
    <div className="type-page wide gallery">
      <div className="gallery-head">
        <h2>{title}</h2>
        <input className="search" value={search} placeholder="Search" onChange={(e) => setSearch(e.target.value)} />
        {gm && (
          <div className="segmented">
            {FILTERS.map((f) => (
              <button key={f.id} className={filter === f.id ? "on" : ""} onClick={() => setFilter(f.id)}>
                {f.name}
              </button>
            ))}
          </div>
        )}
        {gm && (
          <button
            onClick={() => void sort()}
            disabled={sorting !== null || !data || images === null}
            title="Each image in no folder goes where its document stands in the navigation"
          >
            {sorting ? `Sorting ${sorting}` : "Sort"}
          </button>
        )}
        <button className="primary" onClick={upload} disabled={busy}>
          {busy ? "Uploading" : "Upload"}
        </button>
      </div>

      <div className={`gallery-body${gm ? "" : " flat"}${current ? " with-detail" : ""}`}>
        {gm && (
          <FolderTree
            folders={folders}
            images={images ?? []}
            folderOf={folderOf}
            shown={shown}
            onShow={setShown}
            onFolders={saveFolders}
            onMoveImage={move}
            onDeleteFolder={(f) => {
              // Its images lie in no folder from now on — an image whose
              // folder is gone lies in none. Its folders move up one level.
              saveFolders(folders.filter((x) => x.id !== f.id).map((x) => (x.parent === f.id ? { ...x, parent: f.parent } : x)));
              if (shown.kind === "folder" && shown.id === f.id) setShown({ kind: "top" });
            }}
          />
        )}

        <section className="gallery-grid-area">
          {images === null ? (
            <p className="small">Loading</p>
          ) : visible.length === 0 && subfolders.length === 0 ? (
            <p className="small">{images.length === 0 ? "No images yet." : "Nothing here."}</p>
          ) : (
            <div className="gallery-grid">
              {subfolders.map((f) => (
                <FolderTile
                  key={f.id}
                  name={f.name}
                  count={images.filter((i) => below(folders, f.id).has(folderOf(i))).length}
                  onOpen={() => setShown({ kind: "folder", id: f.id })}
                  onDropImage={(image) => void move(image, f.id)}
                />
              ))}
              {visible.map((i) => (
                <button
                  key={i.id}
                  className={`gallery-tile${chosen === i.id ? " on" : ""}`}
                  draggable={gm || i.own}
                  onDragStart={(e) => e.dataTransfer.setData("text/x-image", i.id)}
                  onClick={() => setChosen(chosen === i.id ? null : i.id)}
                  title={i.name || undefined}
                >
                  <WorldImage world={world.id} id={i.id} preview />
                  <span className="image-name">{i.name || "Image"}</span>
                  {gm && i.usedIn.length === 0 && <span className="tile-mark">{i.inVersions ? "old versions" : "unused"}</span>}
                </button>
              ))}
            </div>
          )}
        </section>

        {current && (
          <Detail
            key={current.id}
            world={world}
            image={current}
            folders={folders}
            folder={folderOf(current)}
            onOpen={onOpen}
            onChanged={load}
            onClose={() => setChosen(null)}
            onError={onError}
          />
        )}
      </div>
    </div>
  );
}

// --- The tree -------------------------------------------------------------------

function FolderTree({
  folders,
  images,
  folderOf,
  shown,
  onShow,
  onFolders,
  onMoveImage,
  onDeleteFolder,
}: {
  folders: FolderNode[];
  images: GalleryImage[];
  folderOf: (i: GalleryImage) => string;
  shown: Shown;
  onShow: (s: Shown) => void;
  onFolders: (f: FolderNode[]) => void;
  onMoveImage: (image: string, folder: string) => void;
  onDeleteFolder: (f: FolderNode) => void;
}) {
  const [closed, setClosed] = useState<Set<string>>(new Set());
  const [editing, setEditing] = useState<{ id: string; name: string; fresh: boolean } | null>(null);
  const [deleting, setDeleting] = useState<FolderNode | null>(null);

  // How many images lie in a folder and below it.
  const count = (id: string) => {
    const set = below(folders, id);
    return images.filter((i) => set.has(folderOf(i))).length;
  };
  const children = (parent: string) => folders.filter((f) => f.parent === parent).sort((a, b) => a.name.localeCompare(b.name));

  const add = (parent: string) => {
    const id = newId();
    onFolders([...folders, { id, name: "New folder", parent }]);
    if (parent) setClosed((c) => new Set([...c].filter((x) => x !== parent)));
    setEditing({ id, name: "New folder", fresh: true });
  };
  const rename = () => {
    if (!editing) return;
    const name = editing.name.trim();
    if (name) onFolders(folders.map((f) => (f.id === editing.id ? { ...f, name } : f)));
    setEditing(null);
  };
  /** Drops what was dragged into `parent`: an image, or a folder — never into itself. */
  const drop = (parent: string, e: React.DragEvent) => {
    const image = e.dataTransfer.getData("text/x-image");
    if (image) return onMoveImage(image, parent);
    const folder = e.dataTransfer.getData("text/x-folder");
    if (folder && !below(folders, folder).has(parent)) onFolders(folders.map((f) => (f.id === folder ? { ...f, parent } : f)));
  };

  const row = (f: FolderNode, depth: number): React.ReactNode => {
    const kids = children(f.id);
    const open = !closed.has(f.id);
    return (
      <li key={f.id}>
        <Drop
          className={`folder-row${shown.kind === "folder" && shown.id === f.id ? " on" : ""}`}
          depth={depth}
          onDrop={(e) => drop(f.id, e)}
          // Not while it is being named: one selects the text, not the folder.
          draggable={editing?.id !== f.id}
          onDragStart={(e) => e.dataTransfer.setData("text/x-folder", f.id)}
          onClick={() => onShow({ kind: "folder", id: f.id })}
        >
          <span
            className={`fold${kids.length ? "" : " none"}`}
            onClick={(e) => {
              e.stopPropagation();
              setClosed((c) => {
                const n = new Set(c);
                if (n.has(f.id)) n.delete(f.id);
                else n.add(f.id);
                return n;
              });
            }}
          >
            {kids.length ? (open ? "▾" : "▸") : ""}
          </span>
          {editing?.id === f.id ? (
            <input
              autoFocus
              className="folder-name"
              value={editing.name}
              onFocus={(e) => editing.fresh && e.currentTarget.select()}
              onClick={(e) => e.stopPropagation()}
              onChange={(e) => setEditing({ ...editing, name: e.target.value })}
              onBlur={rename}
              onKeyDown={(e) => {
                if (e.key === "Enter") rename();
                if (e.key === "Escape") setEditing(null);
              }}
            />
          ) : (
            <span className="folder-label">{f.name}</span>
          )}
          <span className="folder-count">{count(f.id)}</span>
          <span className="folder-actions" onClick={(e) => e.stopPropagation()}>
            <button className="ghost icon" title="A folder in this one" onClick={() => add(f.id)}>
              ＋
            </button>
            <button className="ghost icon" title="Rename" onClick={() => setEditing({ id: f.id, name: f.name, fresh: false })}>
              ✎
            </button>
            <DeleteButton what={f.name} onClick={() => setDeleting(f)} />
          </span>
        </Drop>
        {open && kids.length > 0 && <ul>{kids.map((k) => row(k, depth + 1))}</ul>}
      </li>
    );
  };

  return (
    <nav className="folder-tree">
      <Drop className={`folder-row${shown.kind === "top" ? " on" : ""}`} depth={0} onDrop={(e) => drop("", e)} onClick={() => onShow({ kind: "top" })}>
        <span className="fold none" />
        <span className="folder-label">Images</span>
        <span className="folder-count" title="In no folder">{images.filter((i) => folderOf(i) === "").length}</span>
      </Drop>
      <Drop className={`folder-row${shown.kind === "all" ? " on" : ""}`} depth={0} onDrop={(e) => drop("", e)} onClick={() => onShow({ kind: "all" })}>
        <span className="fold none" />
        <span className="folder-label">All images</span>
        <span className="folder-count">{images.length}</span>
      </Drop>
      <ul>{children("").map((f) => row(f, 0))}</ul>
      <button className="ghost compact add-folder" onClick={() => add("")}>
        ＋ Folder
      </button>
      {deleting && (
        <DeleteDialog
          title={`Delete ${deleting.name}`}
          name={deleting.name}
          warning="Its images go to “In no folder”, its folders move up one level. No image is deleted."
          onCancel={() => setDeleting(null)}
          onDelete={() => {
            onDeleteFolder(deleting);
            setDeleting(null);
          }}
        />
      )}
    </nav>
  );
}

/** A row things can be dropped on — it lights up while something hovers. */
function Drop({
  className,
  depth,
  onDrop,
  children,
  ...rest
}: {
  className: string;
  depth: number;
  onDrop: (e: React.DragEvent) => void;
  children: React.ReactNode;
} & React.HTMLAttributes<HTMLDivElement> & { draggable?: boolean }) {
  const [over, setOver] = useState(false);
  return (
    <div
      {...rest}
      role="button"
      tabIndex={0}
      className={`${className}${over ? " over" : ""}`}
      style={{ paddingLeft: 6 + depth * 16 }}
      onDragOver={(e) => {
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(e) => {
        e.preventDefault();
        setOver(false);
        onDrop(e);
      }}
    >
      {children}
    </div>
  );
}

// --- The chosen image --------------------------------------------------------------

function Detail({
  world,
  image,
  folders,
  folder,
  onOpen,
  onChanged,
  onClose,
  onError,
}: {
  world: WorldInfo;
  image: GalleryImage;
  folders: FolderNode[];
  folder: string;
  onOpen: (article: string) => void;
  onChanged: () => void;
  onClose: () => void;
  onError: (e: unknown) => void;
}) {
  const mine = world.isGm || image.own;
  const [name, setName] = useState(image.name);
  const [standIn, setStandIn] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const run = (step: Promise<unknown>) => step.then(onChanged).catch(onError);
  const rename = () => {
    if (name.trim() && name.trim() !== image.name) void run(api.imageRename(world.id, image.id, name.trim()));
    else setName(image.name);
  };
  const defaults = (d: ImageDefaults) => {
    if (JSON.stringify(d) !== JSON.stringify(image.defaults)) void run(api.imageDefaultsSet(world.id, image.id, d));
  };
  const d = image.defaults;
  const paths = folders.map((f) => ({ id: f.id, path: pathOf(folders, f.id) })).sort((a, b) => a.path.localeCompare(b.path));

  return (
    <aside className="gallery-detail">
      <div className="detail-head">
        <h3>{image.name || "Image"}</h3>
        <button className="ghost icon round" onClick={onClose} title="Close">
          ×
        </button>
      </div>
      <WorldImage world={world.id} id={image.id} className="detail-image" />
      {image.width > 0 && (
        <small className="small">
          {image.width} × {image.height} · {size(image.size)} · {new Date(image.uploaded).toLocaleDateString()}
        </small>
      )}
      {!image.complete && <small className="muted">Still arriving</small>}
      {mine && (
        <label className="detail-field">
          <small className="small">Name</small>
          <input value={name} onChange={(e) => setName(e.target.value)} onBlur={rename} onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()} />
        </label>
      )}
      {world.isGm && (
        <label className="detail-field">
          <small className="small">Folder</small>
          <select value={folder} onChange={(e) => void run(api.imageMove(world.id, image.id, e.target.value))}>
            <option value="">None</option>
            {paths.map((p) => (
              <option key={p.id} value={p.id}>
                {p.path}
              </option>
            ))}
          </select>
        </label>
      )}
      {world.isGm && (
        <div className="detail-field">
          <small className="small">Used in</small>
          {image.usedIn.length > 0 ? (
            <div className="chips">
              {image.usedIn.map((u) => (
                <button key={u.id} className="chip" onClick={() => onOpen(u.id)}>
                  {u.name || "Document"}
                </button>
              ))}
            </div>
          ) : (
            <span className="muted small">{image.inVersions ? "Only old versions" : "Nowhere"}</span>
          )}
        </div>
      )}
      {mine && (
        <div className="detail-field">
          <small className="small">When inserted</small>
          {d.view ? (
            <>
              <div className="detail-defaults">
                <span className="mark-short" title="Who may see">👁</span>
                <VisibilityPicker world={world} value={d.view} onChange={(view) => defaults({ ...d, view, edit: d.edit || "gm" })} small />
                <button className="ghost icon" onClick={() => defaults({ view: "", edit: "", hidden: "", hiddenCaption: "" })} title="Visible where it stands">
                  ×
                </button>
              </div>
              <div className="detail-defaults">
                <button className="stand-in" onClick={() => setStandIn(true)} title="What the others see">
                  {d.hidden ? <WorldImage world={world.id} id={d.hidden} preview /> : <span className="image-placeholder unknown">?</span>}
                </button>
                <input
                  key={d.hiddenCaption}
                  defaultValue={d.hiddenCaption}
                  placeholder="What the others read"
                  onBlur={(e) => defaults({ ...d, hiddenCaption: e.target.value.trim() })}
                  onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                />
              </div>
            </>
          ) : (
            <button className="compact" onClick={() => defaults({ ...d, view: "gm", edit: "gm" })}>
              Hidden from players
            </button>
          )}
        </div>
      )}
      {world.isGm && (
        <div className="detail-foot">
          <DeleteButton what={image.name || "this image"} onClick={() => setDeleting(true)} />
        </div>
      )}
      {standIn && (
        <ImagePicker
          world={world.id}
          onChosen={(id) => {
            defaults({ ...d, hidden: id });
            setStandIn(false);
          }}
          onRemove={d.hidden ? () => (defaults({ ...d, hidden: "" }), setStandIn(false)) : undefined}
          onCancel={() => setStandIn(false)}
        />
      )}
      {deleting && (
        <DeleteDialog
          title={`Delete ${image.name || "image"}`}
          name={image.name || "image"}
          warning={
            image.usedIn.length > 0
              ? `Still used in ${image.usedIn.map((u) => u.name).join(", ")}. Gone for good, on every device.`
              : "Gone for good, on every device."
          }
          onCancel={() => setDeleting(false)}
          onDelete={() => {
            setDeleting(false);
            onClose();
            void run(api.imageDelete(world.id, [image.id]));
          }}
        />
      )}
    </aside>
  );
}

function size(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}
