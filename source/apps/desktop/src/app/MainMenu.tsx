import { useEffect, useRef, useState } from "react";
import { useBack } from "../shared/useBack";
import { open } from "@tauri-apps/plugin-dialog";
import { DeleteDialog, DeleteButton } from "../shared/ui/Delete";
import { Icon, NetworkIcon, ServerIcon } from "../shared/ui/Icons";
import { api, type BackupFound, type NetWorld, type NetworkView, type ServerEntry, type StorageView, type WorldInfo } from "../api/client";
import { nameOf, placeOf, PlaceIcon, Reach, serverName } from "../features/worlds/WorldPlace";
import { HeaderTools } from "./HeaderTools";
import { isMobile } from "../shared/platform";

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
  const [dialog, setDialog] = useState<"new" | "open" | "server" | "restore" | null>(null);
  // A phone chooses no file: the backups where they go, to pick from.
  const [found, setFound] = useState<BackupFound[] | null>(null);
  const [code, setCode] = useState("");
  // The server being edited, by its code — none: one is added.
  const [editing, setEditing] = useState<string | null>(null);
  const [fresh, setFresh] = useState("");
  const [location, setLocation] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [status, setStatus] = useState<Record<string, string>>({});
  const [remove, setRemove] = useState<WorldInfo | null>(null);

  useBack(dialog !== null, () => setDialog(null));

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

  // On one of the servers — or with another device: its GM's app.
  const onServer = (w: WorldInfo) => servers.some(s => s.code === w.server);
  const chosen = location || storage?.worlds || "";
  const toServer = chosen.startsWith("server:") ? chosen.slice("server:".length) : "";
  const localPath = toServer ? storage?.worlds ?? "" : chosen;
  const known = new Set(worlds.map(w => w.id));
  const remoteNetwork = nearby.filter(w => !servers.some(s => s.code === w.server) && !known.has(w.id));
  const placed = (w: WorldInfo) => placeOf(w, storage, servers);
  // On this device: in its folder or another one here. With another device: its GM's.
  const here = worlds.filter(w => !w.server);
  const hosted = worlds.filter(w => w.server && !onServer(w));
  const opened = (id: string) => { setDialog(null); onOpen(id); };

  const joinNetwork = async (w: NetWorld) => {
    setBusy(w.id);
    try {
      const result = await api.networkJoin(w.id);
      if (result.world) { onChanged(); opened(result.world.id); }
      else setStatus(s => ({ ...s, [w.id]: result.kind === "not-offered" ? "No longer available" : "Waiting for the GM" }));
    } catch (e) { onError(e); }
    finally { setBusy(null); }
  };
  const openMailbox = async () => {
    try {
      const folder = await open({ directory: true, multiple: false });
      if (typeof folder !== "string") return;
      setBusy("mailbox");
      const world = await api.mailboxOpen(folder);
      onChanged(); opened(world.id);
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
  const restoreBackup = async () => {
    setBusy("import");
    try {
      const path = await open({ multiple: false, filters: [{ name: "Backup", extensions: ["awbackup", "tar"] }] });
      if (typeof path !== "string") return;
      const world = await api.backupRestore(path);
      onChanged(); opened(world.id);
    } catch (e) { onError(e); }
    finally { setBusy(null); }
  };

  const restoreFound = async (b: BackupFound) => {
    setBusy(b.path);
    try {
      const world = await api.backupRestore(b.path);
      onChanged(); opened(world.id);
    } catch (e) { onError(e); }
    finally { setBusy(null); }
  };
  // One file picked — a phone's backup from before it was set up anew.
  const picker = useRef<HTMLInputElement>(null);
  const restorePicked = async (file: File | undefined) => {
    if (!file) return;
    setBusy("file");
    try {
      // In pieces, as Base64: Android's bridge carries text only.
      const piece = 3 << 20;
      let path = "";
      for (let at = 0; at === 0 || at < file.size; at += piece) {
        const bytes = new Uint8Array(await file.slice(at, at + piece).arrayBuffer());
        let text = "";
        for (let i = 0; i < bytes.length; i += 0x8000) text += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
        path = await api.backupReceive(btoa(text), at === 0);
      }
      const world = await api.backupRestore(path);
      onChanged(); opened(world.id);
    } catch (e) { onError(e); }
    finally { setBusy(null); if (picker.current) picker.current.value = ""; }
  };
  const showFound = () => {
    setFound(null);
    setDialog("restore");
    api.backupFound().then(setFound).catch(onError);
  };

  const serverOf = (code: string) => servers.find(s => s.code === code);
  const worldButton = (w: WorldInfo) => {
    const place = placed(w);
    const note = w.online && w.server ? "online" : "";
    return <button className="world-card" onClick={() => opened(w.id)}>
    <span className="world-icon">{w.icon}</span><strong>{w.name}</strong>
    <span className="world-location" title={place.title}>
      {onServer(w) && <Reach on={!!serverOf(w.server)?.reached} />}<PlaceIcon place={place} /> {place.name}{note && ` · ${note}`}
    </span>
    <span className={`role-badge ${w.role}`}>{w.isGm ? "GM" : "Player"}</span>
    <small>{w.articleCount} {w.articleCount === 1 ? "article" : "articles"}</small>
  </button>;
  };

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
      <button className="ghost icon round" title="New world" onClick={() => setDialog("new")}><Icon name="add" /></button>
      {isMobile
        ? <button className="ghost icon round" title="Restore a backup" onClick={showFound}><Icon name="history" /></button>
        : <button className="ghost icon round" title="Open world" onClick={() => setDialog("open")}><Icon name="folder-open" /></button>}
    </HeaderTools>
    <h3 className="menu-section">On this device</h3>
    <div className="world-grid">
      {here.map(tile)}
    </div>
    {here.length === 0 && <p className="small muted line">None yet.<button className="ghost compact" onClick={() => setDialog("new")}><Icon name="add" /> New world</button></p>}
    {storage?.locations.some(l => !l.available) && <p className="warning">A world location is unavailable. Reconnect the drive or check Storage settings.</p>}

    {hosted.length + remoteNetwork.length > 0 && <>
      <h3 className="menu-section">Other devices</h3>
      <div className="world-grid">{hosted.map(tile)}{remoteNetwork.map(w => joinTile(w, <><NetworkIcon /> Local network</>))}</div>
    </>}

    <h3 className="menu-section line between">Servers<button className="ghost compact" onClick={() => editServer(null)}><Icon name="add" /> Add server</button></h3>
    {servers.map(s => {
      const mine = worlds.filter(w => w.server === s.code);
      const open = nearby.filter(w => w.server === s.code && !known.has(w.id));
      return <div key={s.code} className="server-section">
        <h4><Reach on={s.reached} /><ServerIcon /> {serverName(s)} <small className="muted">{!s.reached ? "not reached" : s.admin ? "admin" : ""}</small>
          <button className="ghost compact" title="Change its address, or remove it" onClick={() => editServer(s)}>Edit</button></h4>
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
    {dialog === "restore" && <div className="dialog-backdrop" onClick={() => !busy && setDialog(null)}>
      <section className="dialog world-location-dialog" role="dialog" aria-modal="true" aria-labelledby="restore-title" onClick={e => e.stopPropagation()}>
        <div className="line between"><h2 id="restore-title">Restore a backup</h2><button disabled={!!busy} onClick={() => setDialog(null)}>Close</button></div>
        {found === null ? <p className="small muted">Looking …</p>
          : found.length === 0 ? <p className="small muted">None yet — Options → World → Back up now.</p>
          : <div className="restore-list">{found.map(b => <button key={b.path} disabled={!!busy} onClick={() => void restoreFound(b)}>
              <strong>{b.name}</strong> <small className="muted">{busy === b.path ? "Restoring …" : new Date(b.at).toLocaleString()}</small>
            </button>)}</div>}
        <button disabled={!!busy} onClick={() => picker.current?.click()}>{busy === "file" ? "Restoring …" : "From a file …"}</button>
        <input ref={picker} type="file" accept=".tar,application/x-tar" hidden onChange={e => void restorePicked(e.target.files?.[0])} />
      </section>
    </div>}
    {(dialog === "new" || dialog === "open") && <div className="dialog-backdrop" onClick={() => !busy && setDialog(null)}>
      <section className="dialog world-location-dialog" role="dialog" aria-modal="true" aria-labelledby="world-location-title" onClick={e => e.stopPropagation()} onKeyDown={e => { if (e.key === "Escape" && !busy) setDialog(null); }}>
        <div className="line between"><h2 id="world-location-title">{dialog === "new" ? "New world" : "Open world"}</h2><button disabled={!!busy} onClick={() => setDialog(null)}>Close</button></div>
        {dialog === "new" && <label className="world-name-field">World name<input autoFocus value={fresh} onChange={e => setFresh(e.target.value)} placeholder="Name of the world" /></label>}
        <label className="world-name-field">{dialog === "new" ? "Create in" : "Import world files into"}
          <select value={dialog === "new" ? chosen : localPath} onChange={e => setLocation(e.target.value)}>
            {storage?.locations.map(l => <option key={l.path} value={l.path} disabled={!l.available}>{l.primary ? "This device" : `Folder · ${nameOf(l.path)}`}{l.available ? "" : " · unavailable"}</option>)}
            {dialog === "new" && servers.filter(s => s.may_bring).map(s => <option key={s.code} value={`server:${s.code}`} disabled={!s.reached}>Server · {serverName(s)}{s.reached ? "" : " · not reached"}</option>)}
          </select>
        </label>
        {toServer && dialog === "new"
          ? <p className="small">It lives on the server. Opening and saving require a server connection, for the GM too.</p>
          : !isMobile && <>
            <button disabled={!!busy} onClick={addLocations}>Add a folder …</button>
            <p className="small">A folder your cloud app keeps in step is shared: the group meets there.</p>
          </>}
        {dialog === "open" && <>
          <div className="world-grid">{worlds.map(w => <div key={w.id}>{worldButton(w)}</div>)}</div>
          <button disabled={!!busy} onClick={restoreBackup}>Restore a backup …</button>
          <button disabled={!!busy} onClick={openMailbox}>Open a mailbox …</button>
        </>}
        {dialog === "new" && <button className="primary" disabled={!!busy || !fresh.trim() || (!toServer && (!localPath || !storage?.locations.some(l => l.path === localPath && l.available)))} onClick={create}>{busy === "create" ? "Creating …" : "Create world"}</button>}
      </section>
    </div>}
    {remove && <DeleteDialog title={`Delete ${remove.icon} ${remove.name}`} name={remove.name}
      warning={remove.server ? (remove.isGm ? "Deletes the world on its server, for everyone." : "You leave the world; it stays for the others.") : "Deletes the world folder. In a shared folder, the cloud app deletes it for everyone there too."}
      onCancel={() => setRemove(null)} onDelete={async confirmation => {
        try { await api.worldDelete(remove.id, confirmation); setRemove(null); onChanged(); }
        catch (e) { onError(e); }
      }} />}
  </div>;
}
