import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { api, type NetworkView, type ServerEntry, type StorageView, type WorldInfo } from "../../api/client";
import { CloudIcon, FolderIcon, NetworkIcon, ServerIcon } from "../../shared/ui/Icons";

/** The folder a path lies in. */
export const parentOf = (path: string) => path.replace(/[\\/]+$/, "").replace(/[\\/][^\\/]*$/, "");
/** A folder's own name. */
export const nameOf = (path: string) => path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? path;
const same = (a: string, b: string) => a.replace(/[\\/]+$/, "") === b.replace(/[\\/]+$/, "");

/** What a server is called — its own name, or its address while it has not answered. */
export function serverName(entry: { code: string; name: string }): string {
  return entry.name || serverAddress(entry.code);
}

/** The address one entered — first in the code; the others the server names itself stay unseen. */
export function serverAddress(code: string): string {
  return code.split("@")[1]?.split(",")[0] ?? code;
}

/** Whether a server answers: a dot, green or grey. */
export function Reach({ on }: { on: boolean }) {
  return <span className={on ? "reach on" : "reach"} title={on ? "Reachable" : "Not reachable"} />;
}

/**
 * Where a world lives — one answer for the main menu and the world's
 * settings. On this device: in its own folder, or in another one (a drive,
 * a cloud folder). Elsewhere: on a server, or with its GM's device; a copy
 * here does not change that.
 */
export type WorldPlace = { kind: "device" | "folder" | "server" | "host"; name: string; title: string };

export function placeOf(world: WorldInfo, storage: StorageView | null, servers: ServerEntry[]): WorldPlace {
  if (world.server) {
    const server = servers.find((s) => s.code === world.server);
    // Not one of the servers: a GM's app that holds it.
    if (!server) return { kind: "host", name: "Hosted by its GM", title: serverAddress(world.server) };
    return { kind: "server", name: serverName(server), title: serverAddress(world.server) };
  }
  const root = parentOf(world.folder);
  const folder = storage?.locations.find((l) => !l.primary && same(l.path, root));
  if (folder) return { kind: "folder", name: nameOf(folder.path), title: folder.path };
  return { kind: "device", name: "This device", title: world.folder };
}

export function PlaceIcon({ place }: { place: WorldPlace }) {
  return place.kind === "server" ? <ServerIcon /> : place.kind === "folder" ? <CloudIcon /> : place.kind === "host" ? <NetworkIcon /> : <FolderIcon />;
}

/**
 * The world's part on where it lives. Everyone sees it; only the GM moves
 * or copies the world, offers it and lets players keep a copy.
 */
export function PlaceArea({ world, onChanged, onError }: { world: WorldInfo; onChanged: () => void; onError: (e: unknown) => void }) {
  const [storage, setStorage] = useState<StorageView | null>(null);
  const [network, setNetwork] = useState<NetworkView | null>(null);
  const [moving, setMoving] = useState(false);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    const look = () =>
      Promise.all([api.storageStatus(), api.networkSettings()])
        .then(([s, n]) => {
          setStorage(s);
          setNetwork(n);
        })
        .catch(onError);
    look();
  }, [world.id]);
  if (!network) return null;
  const servers = network.server_list;
  const place = placeOf(world, storage, servers);
  const status = place.kind === "server" || place.kind === "host" ? "online" : place.title;
  const offer = async (on: boolean) => {
    setBusy(true);
    try {
      await api.networkShare(world.id, on);
      onChanged();
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  };
  const reached = network.enabled || network.internet;
  return (
    <div className="type-page">
      <h3>Where it lives</h3>
      <div className="line world-place">
        <span className="world-place-icon">
          <PlaceIcon place={place} />
        </span>
        <span className="grow">
          <strong>{place.name}</strong>
          <small className="small muted" title={place.title}>
            {status}
          </small>
        </span>
        {world.isGm && (
          <button disabled={busy} onClick={() => setMoving(true)}>
            Move or copy …
          </button>
        )}
      </div>
      {world.isGm && !world.online && (
        <label className="switch-row">
          <input type="checkbox" checked={world.offered} disabled={busy} onChange={(e) => void offer(e.target.checked)} />
          <span>
            <strong>Offered to players</strong>
            <small className="small">
              {!world.offered ? "off · only in its folder" : reached ? "on · players find it over the network" : "on · the network is off on this device"}
            </small>
          </span>
        </label>
      )}
      {/* A world on a server is there and nowhere else: no copies of it, the GM's neither. */}
      {world.isGm && (world.online || world.offered) && (
        <Listed world={world} where={world.online ? "on the server" : "on the network"} onChanged={onChanged} onError={onError} />
      )}
      {moving && storage && (
        <MoveDialog world={world} place={place} storage={storage} servers={servers} onStorage={setStorage} onDone={onChanged} onClose={() => setMoving(false)} onError={onError} />
      )}
    </div>
  );
}

/**
 * Whether the server — or the GM's device, for a world offered on the
 * network — names the world to everyone who asks. Off, only its GM, its
 * members and the ids let in ahead of time (Players → by id) see it.
 */
function Listed({ world, where, onChanged, onError }: { world: WorldInfo; where: string; onChanged: () => void; onError: (e: unknown) => void }) {
  const [busy, setBusy] = useState(false);
  const turn = async (listed: boolean) => {
    setBusy(true);
    try {
      await api.worldListedSet(world.id, listed);
      onChanged();
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  };
  return (
    <label className="switch-row">
      <input type="checkbox" checked={world.listed} disabled={busy} onChange={(e) => void turn(e.target.checked)} />
      <span>
        <strong>Seen by everyone {where}</strong>
        <small className="small">{world.listed ? "on · anyone there may ask to join" : "off · only members and players added by id"}</small>
      </span>
    </label>
  );
}

/** What a long copy or move is doing, step by step. */
const STEPS: Record<string, string> = {
  documents: "1 of 3 · reading documents",
  images: "2 of 3 · packing images",
  writing: "3 of 3 · writing the copy",
  moving: "moving files",
};

/** Bytes as one reads them. */
function size(n: number): string {
  return n < 1024 * 1024 ? `${Math.ceil(n / 1024).toLocaleString()} KB` : `${(n / 1024 / 1024).toFixed(1)} MB`;
}

/**
 * Moving or copying a world: into a folder here — its own, a drive's, a
 * cloud folder — or onto a server. Moved, it is there and nowhere else;
 * copied, a world of its own comes into being beside it.
 */
function MoveDialog({
  world,
  place,
  storage,
  servers,
  onStorage,
  onDone,
  onClose,
  onError,
}: {
  world: WorldInfo;
  place: WorldPlace;
  storage: StorageView;
  servers: ServerEntry[];
  /** A folder chosen here becomes one of Storage's world locations. */
  onStorage: (s: StorageView) => void;
  onDone: () => void;
  onClose: () => void;
  onError: (e: unknown) => void;
}) {
  const [copy, setCopy] = useState(false);
  const [to, setTo] = useState("");
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<[string, number, number] | null>(null);
  useEffect(() => {
    const stop = listen<[string, number, number]>("copy-progress", (e) => setProgress(e.payload));
    return () => void stop.then((un) => un());
  }, []);
  // Where its folder lies — the GM's copy, on a server too.
  const root = parentOf(world.folder);
  const folders = storage.locations.map((l) => ({
    value: `folder:${l.path}`,
    label: (l.primary ? "This device" : `Folder · ${nameOf(l.path)}`) + (!world.server && same(l.path, root) ? " · now" : ""),
    title: l.path,
    off: !l.available || (!copy && !world.server && same(l.path, root)),
  }));
  const hosts = servers
    .filter((s) => s.may_bring || s.code === world.server)
    .map((s) => ({ value: `server:${s.code}`, label: `Server · ${serverName(s)}${s.code === world.server ? " · now" : ""}`, title: serverAddress(s.code), off: !s.reached || copy || s.code === world.server }));
  const choices = [...folders, ...hosts];
  const chosen = choices.find((c) => c.value === to && !c.off) ?? choices.find((c) => !c.off);
  const toServer = chosen?.value.startsWith("server:") ? chosen.value.slice("server:".length) : "";
  const toFolder = chosen?.value.startsWith("folder:") ? chosen.value.slice("folder:".length) : "";
  const pick = async () => {
    try {
      const path = await open({ directory: true, multiple: false });
      if (typeof path !== "string") return;
      if (!storage.locations.some((l) => same(l.path, path))) {
        onStorage(await api.storageLocations([...storage.locations.filter((l) => !l.primary).map((l) => l.path), path]));
      }
      setTo(`folder:${path}`);
    } catch (e) {
      onError(e);
    }
  };
  const leavesShared = place.kind === "folder" && !copy && !!toFolder && !same(toFolder, root);
  const what = copy
    ? "A new world of its own, with types, documents and images — without players and versions."
    : toServer
      ? "It lives on the server alone; players find it there."
      : world.online
        ? "It comes here; the server lets it go."
        : leavesShared
          ? "It moves there with everything — and leaves the shared folder for everyone in it."
          : "It moves there with everything.";
  const go = async () => {
    if (!chosen) return;
    setBusy(true);
    setProgress(null);
    try {
      const destination = toServer
        ? ({ kind: "server", server: toServer } as const)
        : ({ kind: "local", path: !copy && same(toFolder, root) ? "" : toFolder } as const);
      if (copy) await api.worldCopy(world.id, destination);
      else await api.worldMove(world.id, destination);
      onDone();
      onClose();
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="dialog-backdrop" onClick={() => !busy && onClose()}>
      <section className="dialog world-location-dialog" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()} onKeyDown={(e) => e.key === "Escape" && !busy && onClose()}>
        <h2>
          {copy ? "Copy" : "Move"} {world.icon} {world.name}
        </h2>
        <div className="segmented">
          <button className={copy ? "" : "on"} disabled={busy} onClick={() => setCopy(false)}>
            Move
          </button>
          <button className={copy ? "on" : ""} disabled={busy} onClick={() => setCopy(true)}>
            Copy
          </button>
        </div>
        <label className="world-name-field">
          To
          <select value={chosen?.value ?? ""} disabled={busy} onChange={(e) => (e.target.value === "pick" ? void pick() : setTo(e.target.value))}>
            {choices.map((c) => (
              <option key={c.value} value={c.value} title={c.title} disabled={c.off}>
                {c.label}
              </option>
            ))}
            <option value="pick">Another folder …</option>
          </select>
        </label>
        {toFolder && <small className="small muted folder-path">{toFolder}</small>}
        <p className="small">{what}</p>
        {busy && (
          <div className="switch-progress">
            <div className="progress-bar">
              <div style={{ width: progress ? `${Math.round((progress[1] * 100) / Math.max(1, progress[2]))}%` : "0%" }} />
            </div>
            <small className="small">
              {progress
                ? progress[0] === "transfer"
                  ? `${size(progress[1])} of ${size(progress[2])}`
                  : `${STEPS[progress[0]] ?? progress[0]} ${progress[1].toLocaleString()} of ${progress[2].toLocaleString()}`
                : copy
                  ? "Copying …"
                  : "Moving …"}
            </small>
          </div>
        )}
        <div className="line">
          <button className="primary" disabled={busy || !chosen} onClick={() => void go()}>
            {busy ? (copy ? "Copying …" : "Moving …") : copy ? "Copy" : "Move"}
          </button>
          {!busy && <button onClick={onClose}>Cancel</button>}
        </div>
      </section>
    </div>
  );
}
