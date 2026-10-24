import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { api, type NetworkView, type ServerEntry, type StorageView, type Upload, type WorldCheck, type WorldInfo } from "../../api/client";
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

/** How far a world on its way to its server is — empty once it is there. */
export function uploadText(u: Upload | undefined): string {
  if (!u) return "";
  const [records, all] = u.records;
  if (records < all) return `uploading ${Math.floor((records * 100) / all)} %`;
  const [images, every] = u.images;
  return images < every ? `uploading images ${images} of ${every}` : "";
}

/** What a check found, in a line. */
export function checkText(c: WorldCheck): string {
  const n = (x: number) => x.toLocaleString();
  const parts = c.lacking - c.repaired;
  const fixed = c.repaired > 0 ? ` · ${n(c.repaired)} given again` : "";
  if (c.missing === 0 && parts === 0) return `all there · ${n(c.records)} entries · ${n(c.intact + c.repaired)} image parts intact${fixed}`;
  const lack = [c.missing > 0 && `${n(c.missing)} entries`, parts > 0 && `${n(parts)} image parts`].filter(Boolean).join(" and ");
  return `${lack} not there yet${fixed}`;
}

/**
 * Where a world lives — one answer for the main menu and the world's
 * settings. On this device: in its own folder, or in another one (a drive,
 * a cloud folder). Elsewhere: on a server, or with its GM's device; a copy
 * here does not change that.
 */
export type WorldPlace = { kind: "device" | "folder" | "server" | "host"; name: string; title: string };

export function placeOf(world: WorldInfo, storage: StorageView | null, servers: ServerEntry[]): WorldPlace {
  if (world.server || world.serverId) {
    if (!world.server) return { kind: "server", name: "Server unavailable", title: "Add this world's server again to open it." };
    const server = servers.find((s) => s.code === world.server);
    return { kind: "server", name: server ? serverName(server) : serverName({ code: world.server, name: "" }), title: serverAddress(world.server) };
  }
  const root = parentOf(world.folder);
  const folder = storage?.locations.find((l) => !l.primary && same(l.path, root));
  if (folder) return { kind: "folder", name: nameOf(folder.path), title: folder.path };
  if (!world.isGm) {
    const gm = world.members.find((m) => m.role === "gm");
    return { kind: "host", name: gm ? `Hosted by ${gm.name}` : "Hosted by its GM", title: "" };
  }
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
  const [checking, setChecking] = useState<[number, number] | null>(null);
  const [checked, setChecked] = useState<WorldCheck | null>(null);
  useEffect(() => {
    const stop = listen<[string, number, number]>("copy-progress", (e) => e.payload[0] === "checking" && setChecking([e.payload[1], e.payload[2]]));
    return () => void stop.then((un) => un());
  }, []);
  useEffect(() => {
    const look = () =>
      Promise.all([api.storageStatus(), api.networkSettings()])
        .then(([s, n]) => {
          setStorage(s);
          setNetwork(n);
        })
        .catch(onError);
    look();
    // How far it is on its way to a server.
    const timer = setInterval(look, 3000);
    return () => clearInterval(timer);
  }, [world.id]);
  if (!network) return null;
  const servers = network.server_list;
  const place = placeOf(world, storage, servers);
  const server = servers.find((s) => s.code === world.server);
  const waiting = !!server?.waiting_worlds.includes(world.id);
  const uploading = uploadText(network.uploads[world.id]);
  const status =
    place.kind === "server"
      ? waiting
        ? "waits for the server's admin"
        : uploading ||
          (checking ? `checking images ${checking[0].toLocaleString()} of ${checking[1].toLocaleString()}` : checked ? checkText(checked) : "") || "online"
      : place.kind === "host"
        ? world.online || world.fleeting
          ? "worked on online"
          : "a copy lies on this device"
        : place.title;
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
  const check = async () => {
    setBusy(true);
    setChecked(null);
    setChecking([0, 0]);
    try {
      setChecked(await api.worldCheck(world.id));
    } catch (e) {
      onError(e);
    } finally {
      setChecking(null);
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
        {world.isGm && place.kind === "server" && !waiting && !uploading && (
          <button disabled={busy || !server?.reached} title="Whether all of it is on the server, every image checked" onClick={() => void check()}>
            {checking ? "Checking …" : "Check"}
          </button>
        )}
        {world.isGm && (
          <button disabled={busy} onClick={() => setMoving(true)}>
            Move or copy …
          </button>
        )}
      </div>
      {world.isGm && place.kind !== "server" && (
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
      {world.isGm && place.kind !== "server" && <Copies world={world} onChanged={onChanged} onError={onError} />}
      {moving && storage && (
        <MoveDialog world={world} place={place} storage={storage} servers={servers} onStorage={setStorage} onDone={onChanged} onClose={() => setMoving(false)} onError={onError} />
      )}
    </div>
  );
}

/**
 * Whether players get a copy of the world when it is offered — only an
 * encrypted one can be copied: each opens their part. Unencrypted, they
 * work online, with the GM's device or the server as host.
 */
function Copies({ world, onChanged, onError }: { world: WorldInfo; onChanged: () => void; onError: (e: unknown) => void }) {
  const [busy, setBusy] = useState(false);
  const turn = async (copies: boolean) => {
    setBusy(true);
    try {
      await api.networkCopiesSet(world.id, copies);
      onChanged();
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  };
  return (
    <label className={world.encrypted ? "switch-row" : "switch-row off"}>
      <input type="checkbox" checked={world.encrypted && world.copies} disabled={busy || !world.encrypted} onChange={(e) => void turn(e.target.checked)} />
      <span>
        <strong>Copies for players</strong>
        <small className="small">
          {!world.encrypted ? "only for encrypted worlds · players work online" : world.copies ? "on · players keep a copy, offline too" : "off · players work online"}
        </small>
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
    .map((s) => ({ value: `server:${s.code}`, label: `Server · ${serverName(s)}${s.code === world.server ? " · now" : ""}`, title: serverAddress(s.code), off: !s.reached || (!copy && s.code === world.server) }));
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
      ? "It lives on the server; players find it there. Your copy as GM stays on this device."
      : place.kind === "server"
        ? "The server lets it go; players find it on this device again."
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
            <small className="small">{progress ? `${STEPS[progress[0]] ?? progress[0]} ${progress[1].toLocaleString()} of ${progress[2].toLocaleString()}` : copy ? "Copying …" : "Moving …"}</small>
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
