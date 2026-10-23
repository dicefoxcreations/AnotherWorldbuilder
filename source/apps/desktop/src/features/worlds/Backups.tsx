import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, type BackupPlan, type BackupView, type WorldInfo } from "../../api/client";

/**
 * A world's backups — the GM's: by hand, or daily or weekly by themselves;
 * how many stay, and where. Each holds the whole world, its keys sealed for
 * the GM. See `backup.rs`.
 */
export function BackupArea({ world, onError }: { world: WorldInfo; onError: (e: unknown) => void }) {
  const [view, setView] = useState<BackupView | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    api.backupView(world.id).then(setView).catch(onError);
  }, [world.id]);
  if (!world.isGm || !view) return null;
  const run = async (step: Promise<BackupView>) => {
    setBusy(true);
    try {
      setView(await step);
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  };
  const plan = (patch: Partial<BackupPlan>) => run(api.backupPlanSet(world.id, { ...view.plan, ...patch }));
  const choose = async () => {
    const folder = await open({ directory: true, multiple: false });
    if (typeof folder === "string") void plan({ folder });
  };
  const last = view.backups[0];
  return (
    <div className="type-page">
      <h3>Backups</h3>
      <div className="line backup-plan">
        <select value={view.plan.every} disabled={busy} onChange={(e) => void plan({ every: e.target.value as BackupPlan["every"] })}>
          <option value="off">By hand</option>
          <option value="daily">Daily</option>
          <option value="weekly">Weekly</option>
        </select>
        <label className="line">
          Keep
          <input
            type="number"
            className="version-limit"
            min={1}
            max={365}
            key={view.plan.keep}
            defaultValue={view.plan.keep}
            onBlur={(e) => {
              const keep = Math.round(Number(e.target.value));
              if (keep >= 1 && keep !== view.plan.keep) void plan({ keep });
              else e.target.value = String(view.plan.keep);
            }}
            onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          />
        </label>
        <span className="grow" />
        <button disabled={busy} onClick={() => void run(api.backupNow(world.id))}>
          {busy ? "Backing up …" : "Back up now"}
        </button>
      </div>
      <div className="line world-place">
        <small className="small muted grow" title={view.folder}>
          {view.backups.length === 0 ? "none yet" : `${view.backups.length} · last ${new Date(last.at).toLocaleString()}`} · {view.folder}
        </small>
        <button className="compact" onClick={() => api.backupOpen(world.id).catch(onError)}>
          Show
        </button>
        <button className="compact" disabled={busy} onClick={() => void choose()}>
          Folder …
        </button>
        {view.plan.folder && (
          <button className="compact" disabled={busy} onClick={() => void plan({ folder: "" })}>
            Default
          </button>
        )}
      </div>
    </div>
  );
}
