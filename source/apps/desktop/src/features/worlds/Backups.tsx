import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { isMobile } from "../../shared/platform";
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
      {/* A phone has no folder to choose: where they are, for a file manager. */}
      {isMobile && <small className="small muted">{view.folder.includes("/Documents/") ? `Documents/${view.folder.split("/Documents/")[1]}` : "In the app — gone with it"}</small>}
      {!isMobile && <div className="line world-place">
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
      </div>}
    </div>
  );
}

const WEEK = 7 * 24 * 60;
const DAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

/** The plan's time as this device shows it: the server keeps it in UTC. */
function localOf(plan: ServerBackupPlan): { day: number; minutes: number } {
  const local = (((plan.day * 1440 + plan.at - new Date().getTimezoneOffset()) % WEEK) + WEEK) % WEEK;
  return { day: Math.floor(local / 1440), minutes: local % 1440 };
}

/** A local day and time back into the plan, in UTC. */
function utcOf(plan: ServerBackupPlan, day: number, minutes: number): ServerBackupPlan {
  const utc = (((day * 1440 + minutes + new Date().getTimezoneOffset()) % WEEK) + WEEK) % WEEK;
  return { ...plan, day: Math.floor(utc / 1440), at: utc % 1440 };
}

const clock = (minutes: number) => `${String(Math.floor(minutes / 60)).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`;

/** How far the server's clock is off this device's, in whole minutes; it backs up by its own. */
function skew(state: ServerBackups, at: number): number {
  return state.now ? Math.round((state.now - at) / 60000) : 0;
}

/** What the server does by itself — asked of it, so it may not answer. */
function ServerBackupLine({ world }: { world: WorldInfo }) {
  const [state, setState] = useState<ServerBackups | null>(null);
  // When this device heard it, to set the server's clock against.
  const [heard, setHeard] = useState(0);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const ask = async (plan: ServerBackupPlan | null) => {
    setBusy(true);
    try {
      setState(await api.serverBackups(world.id, plan));
      setHeard(Date.now());
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
  const when = localOf(plan);
  // Full hours; a time set otherwise before stays among them.
  const hours = [...new Set([...Array.from({ length: 24 }, (_, h) => h * 60), when.minutes])].sort((a, b) => a - b);
  const off = skew(state, heard);
  return (
    <div className="line backup-plan">
      <span>Server</span>
      <select value={plan.every} disabled={busy} onChange={(e) => void ask({ ...plan, every: e.target.value as ServerBackupPlan["every"] })}>
        <option value="off">Off</option>
        <option value="daily">Daily</option>
        <option value="weekly">Weekly</option>
      </select>
      {plan.every === "weekly" && (
        <select value={when.day} disabled={busy} onChange={(e) => void ask(utcOf(plan, Number(e.target.value), when.minutes))}>
          {DAYS.map((d, i) => (
            <option key={d} value={i}>
              {d}
            </option>
          ))}
        </select>
      )}
      {plan.every !== "off" && (
        <select value={when.minutes} disabled={busy} onChange={(e) => void ask(utcOf(plan, when.day, Number(e.target.value)))}>
          {hours.map((m) => (
            <option key={m} value={m}>
              {clock(m)}
            </option>
          ))}
        </select>
      )}
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
      <small className="small muted grow">{state.count === 0 ? "none yet" : `${state.count} · last ${new Date(state.last).toLocaleString([], { dateStyle: "short", timeStyle: "short" })}`}</small>
      {state.now > 0 && (
        <small className={Math.abs(off) > 2 ? "small warning" : "small muted"} title="The server backs up by its own clock">
          Server clock {new Date(state.now).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
          {Math.abs(off) > 2 && ` · ${Math.abs(off)} min ${off > 0 ? "ahead" : "behind"}`}
        </small>
      )}
    </div>
  );
}
