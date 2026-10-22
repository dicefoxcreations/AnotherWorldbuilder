import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { DeleteDialog, DeleteButton } from "../shared/ui/Delete";
import { CloudIcon, FolderIcon, NetworkIcon, ServerIcon } from "../shared/ui/Icons";
import { api, type NetWorld, type NetworkView, type ServerEntry, type StorageView, type WorldInfo } from "../api/client";
import { serverName, uploadText } from "../features/settings/Options";
import { HeaderTools } from "./HeaderTools";

/** The folder a path lies in. */
const parentOf = (path: string) => path.replace(/[\\/]+$/, "").replace(/[\\/][^\\/]*$/, "");
/** A folder's own name. */
const nameOf = (path: string) => path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? path;

/** One library, regardless of how a world reaches this device. */
export function MainMenu({ worlds, network, onOpen, onChanged, onNetwork, onError }: {
  worlds: WorldInfo[];
  network: NetworkView | null;
  onOpen: (id: string) => void;
  onChanged: () => void;
  onNetwork: (n: NetworkView) => void;
  onSettings: () => void;
  onError: (e: unknown) => void;
}) {
  const [storage, setStorage] = useState<StorageView | null>(null);
  const [nearby, setNearby] = useState<NetWorld[]>([]);
  const [dialog, setDialog] = useState<"new" | "open" | "server" | null>(null);
  const [code, setCode] = useState("");
  // The server being edited, by its code — none: one is added.
  const [editing, setEditing] = useState<string | null>(null);
  const [fresh, setFresh] = useState("");
  const [location, setLocation] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [status, setStatus] = useState<Record<string, string>>({});
  const [remove, setRemove] = useState<WorldInfo | null>(null);

  const servers: ServerEntry[] = network?.server_list ?? [];
  // Worlds out there: on the local network, or held by a server.
  const looking = !!network?.enabled || servers.length > 0;
  useEffect(() => {
    let live = true;
    let refreshing = false;
    const refresh = async () => {
      if (refreshing) return;
      refreshing = true;
      try {
        const [s, n] = await Promise.all([api.storageStatus(), looking ? api.networkWorlds() : Promise.resolve([])]);
        if (live) { setStorage(s); setNearby(n); }
        // The servers' names and states, as they answered meanwhile.
        if (live && servers.length > 0) onNetwork(await api.networkSettings());
      } catch (e) { if (live) onError(e); }
      finally { refreshing = false; }
    };
    refresh();
    const timer = setInterval(refresh, 3000);
    return () => { live = false; clearInterval(timer); };
  }, [looking, servers.length]);

  const onServer = (w: { server: string }) => !!w.server && servers.some(s => s.code === w.server);
  const chosen = location || storage?.worlds || "";
  const toServer = chosen.startsWith("server:") ? chosen.slice("server:".length) : "";
  const localPath = toServer ? storage?.worlds ?? "" : chosen;
  const known = new Set(worlds.map(w => w.id));
  const remoteNetwork = nearby.filter(w => !w.server && !known.has(w.id));
  const here = worlds.filter(w => !onServer(w));
  const opened = (id: string) => { setDialog(null); onOpen(id); };
  // A world in a location beside the device's own lies in a shared folder —
  // one a cloud app keeps in step; the group meets there.
  const shared = (path: string) => !!storage?.locations.some(l => !l.primary && l.path.replace(/[\\/]+$/, "") === path);

  const joinNetwork = async (w: NetWorld) => {
    setBusy(w.id);
    try {
      const result = await api.networkJoin(w.id, true);
      if (result.world) { onChanged(); opened(result.world.id); }
      else setStatus(s => ({ ...s, [w.id]: result.kind === "not-offered" ? "No longer available" : "Waiting for the GM" }));
    } catch (e) { onError(e); }
    finally { setBusy(null); }
  };
  const create = async () => {
    setBusy("create");
    try {
      const world = await api.worldCreate(fresh.trim(), toServer ? { kind: "server", server: toServer } : { kind: "local", path: localPath });
      setFresh(""); onChanged(); opened(world.id);
    } catch (e) { onError(e); }
    finally { setBusy(null); }
  };
  const addLocations = async () => {
    setBusy("locations");
    try {
      const paths = await open({ directory: true, multiple: true });
      if (!paths) return;
      const next = await api.storageLocations([
        ...(storage?.locations.filter(l => !l.primary).map(l => l.path) ?? []),
        ...(Array.isArray(paths) ? paths : [paths]),
      ]);
      setStorage(next); onChanged();
    } catch (e) { onError(e); }
    finally { setBusy(null); }
  };
  const saveServer = async () => {
    setBusy("server");
    try {
      const others = (network?.servers ?? []).filter(c => c !== editing);
      const n = await api.networkServers(editing ? (network?.servers ?? []).map(c => c === editing ? code.trim() : c) : [...others, code.trim()]);
      onNetwork(n); setCode(""); setEditing(null); setDialog(null);
    } catch (e) { onError(e); }
    finally { setBusy(null); }
  };
  const askToHost = async (s: ServerEntry) => {
    setBusy("server");
    try { onNetwork(await api.serverHostWorlds(s.code)); }
    catch (e) { onError(e); }
    finally { setBusy(null); }
  };
  const removeServer = async () => {
    setBusy("server");
    try {
      onNetwork(await api.networkServers((network?.servers ?? []).filter(c => c !== editing)));
      setCode(""); setEditing(null); setDialog(null);
    } catch (e) { onError(e); }
    finally { setBusy(null); }
  };
  const editServer = (s: ServerEntry | null) => {
    setEditing(s?.code ?? null);
    // The address it was added by — the first in its code.
    setCode(s ? s.code.split("@")[1]?.split(",")[0] ?? s.code : "");
    setDialog("server");
  };
  const importWorld = async () => {
    setBusy("import");
    try {
      const path = await open({ multiple: false, filters: [{ name: "World file", extensions: ["awworld", "json"] }] });
      if (typeof path !== "string") return;
      const world = await api.worldOpen(path, localPath);
      onChanged(); opened(world.id);
    } catch (e) { onError(e); }
    finally { setBusy(null); }
  };

  const serverOf = (code: string) => servers.find(s => s.code === code);
  const worldButton = (w: WorldInfo) => <button className="world-card" onClick={() => opened(w.id)}>
    <span className="world-icon">{w.icon}</span><strong>{w.name}</strong>
    <span className="world-location" title={w.folder}>
      {onServer(w) ? <><ServerIcon /> {serverName(serverOf(w.server)!)}{serverOf(w.server)!.waiting_worlds.includes(w.id) ? " · waits for its admin" : uploadText(network?.uploads[w.id]) ? ` · ${uploadText(network?.uploads[w.id])}` : w.online || w.fleeting ? " · online" : ""}</>
        : shared(parentOf(w.folder)) ? <><CloudIcon /> {nameOf(parentOf(w.folder))}</> : <><FolderIcon /> Local</>}
    </span>
    <span className={`role-badge ${w.role}`}>{w.isGm ? "GM" : "Player"}</span>
    <small>{w.articleCount} {w.articleCount === 1 ? "article" : "articles"}</small>
  </button>;

  const tile = (w: WorldInfo) => <div key={w.id} className="world-tile">
    {worldButton(w)}<DeleteButton what={w.name} onClick={() => setRemove(w)} />
  </div>;
  const joinTile = (w: NetWorld, where: React.ReactNode) => <div key={w.id} className="world-tile">
    <button className="world-card net" disabled={!!busy} onClick={() => joinNetwork(w)}>
      <span className="world-icon">{w.icon}</span><strong>{w.name}</strong><span className="world-location">{where}</span>
      <small>by {w.gm}</small><small>{busy === w.id ? "Opening …" : status[w.id] ?? "Ask to join"}</small>
    </button>
  </div>;

  return <div className="main-menu">
    <HeaderTools>
      <button className="ghost icon round" title="New world" onClick={() => setDialog("new")}>＋</button>
      <button className="ghost icon round" title="Open world" onClick={() => setDialog("open")}>📂</button>
    </HeaderTools>
    <h3 className="menu-section">Local and shared folders</h3>
    <div className="world-grid">
      {here.map(tile)}
    </div>
    {here.length === 0 && <p className="small muted">None yet — ＋ above makes one.</p>}
    {storage?.locations.some(l => !l.available) && <p className="warning">A world location is unavailable. Reconnect the drive or check Storage settings.</p>}

    {remoteNetwork.length > 0 && <>
      <h3 className="menu-section">Local network</h3>
      <div className="world-grid">{remoteNetwork.map(w => joinTile(w, <><NetworkIcon /> Local network</>))}</div>
    </>}

    <h3 className="menu-section line between">Servers<button className="ghost compact" onClick={() => editServer(null)}>＋ Add server</button></h3>
    {servers.map(s => {
      const mine = worlds.filter(w => w.server === s.code);
      const open = nearby.filter(w => w.server === s.code && !known.has(w.id));
      return <div key={s.code} className="server-section">
        <h4><ServerIcon /> {serverName(s)} <small className="muted">{!s.reached ? "not reached" : s.admin ? "admin" : ""}</small>
          <button className="ghost compact" title="Change its address, or remove it" onClick={() => editServer(s)}>Edit</button></h4>
        {s.waiting && <p className="small muted">A world of yours waits for the server's admin.</p>}
        {s.reached && !s.may_bring && <p className="small muted line">
          {s.asked ? "You asked to host worlds here — the admin decides." : "Only GMs the admin lets in host worlds here."}
          {!s.asked && <button className="ghost compact" disabled={!!busy} onClick={() => askToHost(s)}>Ask to host worlds</button>}
        </p>}
        <div className="world-grid">
          {mine.map(tile)}
          {open.map(w => joinTile(w, <><ServerIcon /> {serverName(s)}</>))}
          {mine.length + open.length === 0 && <p className="small muted">No worlds here yet.</p>}
        </div>
      </div>;
    })}
    {servers.length === 0 && <p className="small muted">None yet.</p>}

    {dialog === "server" && <div className="dialog-backdrop" onClick={() => !busy && setDialog(null)}>
      <section className="dialog world-location-dialog" role="dialog" aria-modal="true" aria-labelledby="add-server-title" onClick={e => e.stopPropagation()} onKeyDown={e => { if (e.key === "Escape" && !busy) setDialog(null); }}>
        <div className="line between"><h2 id="add-server-title">{editing ? "Edit server" : "Add server"}</h2><button disabled={!!busy} onClick={() => setDialog(null)}>Close</button></div>
        <label className="world-name-field">Address<input autoFocus value={code} onChange={e => setCode(e.target.value)} placeholder="worlds.example.org" onKeyDown={e => e.key === "Enter" && code.trim() && saveServer()} /></label>
        <p className="small">Its address and port, or the code it shows.</p>
        <div className="line between">
          <button className="primary" disabled={!!busy || !code.trim()} onClick={saveServer}>{busy === "server" ? "Asking …" : editing ? "Save" : "Add server"}</button>
          {editing && <button className="danger" disabled={!!busy} onClick={removeServer}>Remove server</button>}
        </div>
      </section>
    </div>}
    {(dialog === "new" || dialog === "open") && <div className="dialog-backdrop" onClick={() => !busy && setDialog(null)}>
      <section className="dialog world-location-dialog" role="dialog" aria-modal="true" aria-labelledby="world-location-title" onClick={e => e.stopPropagation()} onKeyDown={e => { if (e.key === "Escape" && !busy) setDialog(null); }}>
        <div className="line between"><h2 id="world-location-title">{dialog === "new" ? "New world" : "Open world"}</h2><button disabled={!!busy} onClick={() => setDialog(null)}>Close</button></div>
        {dialog === "new" && <label className="world-name-field">World name<input autoFocus value={fresh} onChange={e => setFresh(e.target.value)} placeholder="Name of the world" /></label>}
        <label className="world-name-field">{dialog === "new" ? "Create in" : "Import world files into"}
          <select value={dialog === "new" ? chosen : localPath} onChange={e => setLocation(e.target.value)}>
            {storage?.locations.map(l => <option key={l.path} value={l.path} disabled={!l.available}>{l.primary ? "This device" : "Shared"} · {l.path}{l.available ? "" : " · unavailable"}</option>)}
            {dialog === "new" && servers.filter(s => s.may_bring).map(s => <option key={s.code} value={`server:${s.code}`} disabled={!s.reached}>Server · {serverName(s)}{s.reached ? "" : " · not reached"}</option>)}
          </select>
        </label>
        {toServer && dialog === "new"
          ? <p className="small">It lives on the server; players find it there. Your copy as GM stays on this device.</p>
          : <>
            <button disabled={!!busy} onClick={addLocations}>Add a folder …</button>
            <p className="small">A folder your cloud app keeps in step is shared: the group meets there.</p>
          </>}
        {dialog === "open" && <>
          <div className="world-grid">{worlds.map(w => <div key={w.id}>{worldButton(w)}</div>)}</div>
          <button disabled={!!busy || !localPath} onClick={importWorld}>Import a world file …</button>
        </>}
        {dialog === "new" && <button className="primary" disabled={!!busy || !fresh.trim() || (!toServer && (!localPath || !storage?.locations.some(l => l.path === localPath && l.available)))} onClick={create}>{busy === "create" ? "Creating …" : "Create world"}</button>}
      </section>
    </div>}
    {remove && <DeleteDialog title={`Delete ${remove.icon} ${remove.name}`} name={remove.name}
      warning="Delete the world folder and this device's private world keys. In a shared folder, the cloud app deletes it for everyone there too."
      onCancel={() => setRemove(null)} onDelete={async confirmation => {
        try { await api.worldDelete(remove.id, confirmation); setRemove(null); onChanged(); }
        catch (e) { onError(e); }
      }} />}
  </div>;
}
