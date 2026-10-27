import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { api, type BackupView, type ServerBackupPlan, type ServerBackups, type WorldInfo } from "../../api/client";

/**
 * A world's backups — the GM's. On this device by hand, every one kept,
 * each holding the whole world as it was. A world on a
 * server is backed up by the server itself, daily or weekly, as many kept as
 * asked for. See `backup.rs` and `server.rs`.
 */
export function BackupArea({ world, onError }: { world: WorldInfo; onError: (e: unknown) => void }) {
  const [view, setView] = useState<BackupView | null>(null);
  const [busy, setBusy] = useState(false);
  // From a server, how far it has come.
  const [share, setShare] = useState<number | null>(null);
  useEffect(() => {
    api.backupView(world.id).then(setView).catch(onError);
  }, [world.id]);
  useEffect(() => {
    const stop = listen<[string, number, number]>("copy-progress", (e) => e.payload[2] > 0 && setShare(e.payload[1] / e.payload[2]));
    return () => void stop.then((un) => un());
  }, []);
  if (!world.isGm || !view) return null;
  const run = async (step: Promise<BackupView>) => {
    setBusy(true);
    setShare(null);
    try {
      setView(await step);
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  };
  const folder = (folder: string) => run(api.backupPlanSet(world.id, { folder }));
  const choose = async () => {
    const chosen = await open({ directory: true, multiple: false });
    if (typeof chosen === "string") void folder(chosen);
  };
  const last = view.backups[0];
  return (
    <div className="type-page">
      <h3>Backups</h3>
      {world.server && <ServerBackupLine world={world} />}
      <div className="line backup-plan">
        <small className="small muted grow">
          {world.server ? "This device: " : ""}
          {view.backups.length === 0 ? "none yet" : `${view.backups.length} · last ${new Date(last.at).toLocaleString()}`}
        </small>
        <button disabled={busy} onClick={() => void run(api.backupNow(world.id))}>
          {busy ? (share !== null && world.server ? `Backing up … ${Math.floor(share * 100)} %` : "Backing up …") : "Back up now"}
        </button>
      </div>
      <div className="line world-place">
        <small className="small muted grow" title={view.folder}>
          {view.folder}
        </small>
        <button className="compact" onClick={() => api.backupOpen(world.id).catch(onError)}>
          Show
        </button>
        <button className="compact" disabled={busy} onClick={() => void choose()}>
          Folder …
        </button>
        {view.plan.folder && (
          <button className="compact" disabled={busy} onClick={() => void folder("")}>
            Default
          </button>
        )}
      </div>
    </div>
  );
}

/** What the server does by itself — asked of it, so it may not answer. */
function ServerBackupLine({ world }: { world: WorldInfo }) {
  const [state, setState] = useState<ServerBackups | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const ask = async (plan: ServerBackupPlan | null) => {
    setBusy(true);
    try {
      setState(await api.serverBackups(world.id, plan));
      setProblem(null);
    } catch (e) {
      setProblem(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };
  useEffect(() => {
    void ask(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [world.id]);
  if (!state) return problem ? <small className="small muted">Server: {problem}</small> : null;
  const plan = state.plan;
  return (
    <div className="line backup-plan">
      <span>Server</span>
      <select value={plan.every} disabled={busy} onChange={(e) => void ask({ ...plan, every: e.target.value as ServerBackupPlan["every"] })}>
        <option value="off">Off</option>
        <option value="daily">Daily</option>
        <option value="weekly">Weekly</option>
      </select>
      {plan.every !== "off" && (
        <label className="line">
          Keep
          <input
            type="number"
            className="version-limit"
            min={1}
            max={365}
            key={plan.keep}
            defaultValue={plan.keep}
            onBlur={(e) => {
              const keep = Math.round(Number(e.target.value));
              if (keep >= 1 && keep !== plan.keep) void ask({ ...plan, keep });
              else e.target.value = String(plan.keep);
            }}
            onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          />
        </label>
      )}
      <small className="small muted grow">{state.count === 0 ? "none yet" : `${state.count} · last ${new Date(state.last).toLocaleString()}`}</small>
    </div>
  );
}
