import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, type StorageChoice, type StorageView } from "../../api/client";
import { FolderIcon, MoveIcon } from "../../shared/ui/Icons";

/** A path below another, written the way the system writes paths. */
function below(base: string, name: string) {
  const sep = base.includes("\\") ? "\\" : "/";
  return base.replace(/[\\/]+$/, "") + sep + name;
}

async function pickFolder(): Promise<string | null> {
  const path = await open({ directory: true, multiple: false });
  return typeof path === "string" ? path : null;
}

/** A labelled path with its actions beside it — here and in the settings alike. */
export function PathRow({
  label,
  value,
  onChange,
  children,
}: {
  label: string;
  value: string;
  /** Without it the path is only shown. */
  onChange?: (v: string) => void;
  children: React.ReactNode;
}) {
  return (
    <label className="storage-field">
      <strong>{label}</strong>
      <div className="line">
        <input
          value={value}
          readOnly={!onChange}
          onChange={(e) => onChange?.(e.target.value)}
          spellCheck={false}
        />
        {children}
      </div>
    </label>
  );
}

/** A square button with a drawn icon and a tooltip. */
export function IconButton({
  title,
  onClick,
  disabled,
  children,
}: {
  title: string;
  onClick: () => void;
  disabled?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button type="button" className="icon" title={title} aria-label={title} disabled={disabled} onClick={onClick}>
      {children}
    </button>
  );
}

/** A folder: typed, or picked. */
function FolderField({ label, value, onChange }: { label: string; value: string; onChange: (v: string) => void }) {
  return (
    <PathRow label={label} value={value} onChange={onChange}>
      <IconButton
        title="Choose folder"
        onClick={async () => {
          const path = await pickFolder();
          if (path) onChange(path);
        }}
      >
        <FolderIcon />
      </IconButton>
    </PathRow>
  );
}

/**
 * Where the app keeps its data — asked on first start, and again when the
 * place remembered is gone.
 *
 * Simple: one folder, `profile` and `worlds` below it. Advanced: each on its
 * own — the profile on this device, the worlds on an external drive, say.
 * Whatever of this app already lies there is taken as it is.
 */
export function StorageSetup({
  status,
  onDone,
}: {
  status: StorageView;
  onDone: (s: StorageView) => void;
}) {
  const [advanced, setAdvanced] = useState(!!status.profile && !status.together);
  const [base, setBase] = useState(status.together ?? status.suggested);
  const [profile, setProfile] = useState(status.profile || below(status.suggested, "profile"));
  const [worlds, setWorlds] = useState(status.worlds || below(status.suggested, "worlds"));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const choose = async () => {
    setBusy(true);
    setError(null);
    try {
      const choice: StorageChoice = advanced ? { profile, worlds } : { base };
      onDone(await api.storageChoose(choice));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const ready = advanced ? profile.trim() && worlds.trim() : base.trim();
  return (
    <main className="middle">
      <div className="card narrow storage-setup">
        <h1>{status.state === "unset" ? "Welcome" : "Data not found"}</h1>
        {status.problem ? (
          <p className="warning">{status.problem}</p>
        ) : (
          <p>Where should your data be stored?</p>
        )}
        <div className="segmented">
          <button className={advanced ? "" : "on"} onClick={() => setAdvanced(false)}>
            Simple
          </button>
          <button className={advanced ? "on" : ""} onClick={() => setAdvanced(true)}>
            Advanced
          </button>
        </div>

        {advanced ? (
          <>
            <FolderField label="Profile" value={profile} onChange={setProfile} />
            <FolderField label="Worlds" value={worlds} onChange={setWorlds} />
          </>
        ) : (
          <FolderField label="Folder" value={base} onChange={setBase} />
        )}
        {error && <p className="warning">{error}</p>}
        <div className="line">
          <button className="primary" disabled={busy || !ready} onClick={choose}>
            Continue
          </button>
          {status.state !== "unset" && (
            <button onClick={() => api.storageStatus().then(onDone)}>Try again</button>
          )}
        </div>
      </div>
    </main>
  );
}

/** A change about to happen: which part, where to, and whether data lies there. */
type Pending = { part: "profile" | "worlds"; path: string; occupied: boolean };

/** The settings' part on where the data lies. */
export function StorageArea({
  onMoved,
  onError,
}: {
  /** The data lies elsewhere now — perhaps someone else's profile, other worlds. */
  onMoved: () => void;
  onError: (e: unknown) => void;
}) {
  const [view, setView] = useState<StorageView | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api.storageStatus().then(setView).catch(onError);
  }, []);

  if (!view) return null;

  const choiceFor = (p: Pending): StorageChoice => ({
    profile: p.part === "profile" ? p.path : view.profile,
    worlds: p.part === "worlds" ? p.path : view.worlds,
    additional: view.locations.filter(l => !l.primary).map(l => l.path),
  });

  const change = async (part: Pending["part"]) => {
    try {
      const path = await pickFolder();
      if (!path) return;
      const found = await api.storagePeek(choiceFor({ part, path, occupied: false }));
      setPending({ part, path, occupied: found[part] });
    } catch (e) {
      onError(e);
    }
  };

  const apply = async (carry: boolean) => {
    if (!pending) return;
    setBusy(true);
    try {
      setView(await api.storageMove(choiceFor(pending), carry));
      setPending(null);
      onMoved();
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  };

  const setLocations = async (paths: string[]) => {
    setBusy(true);
    try { setView(await api.storageLocations(paths)); onMoved(); }
    catch (e) { onError(e); }
    finally { setBusy(false); }
  };
  const addLocations = async () => {
    try {
      const picked = await open({ directory: true, multiple: true });
      if (!picked) return;
      await setLocations([...view.locations.filter(l => !l.primary).map(l => l.path), ...(Array.isArray(picked) ? picked : [picked])]);
    } catch (e) { onError(e); }
  };

  const rows: [Pending["part"], string, string][] = [
    ["profile", "Profile", view.profile],
    ["worlds", "Default world location", view.worlds],
  ];
  return (
    <div className="type-page">
      <h2>Storage</h2>
      <div className="storage-rows">
        {rows.map(([part, label, path]) => (
          <PathRow key={part} label={label} value={path}>
            <IconButton title="Open folder" onClick={() => api.storageOpen(path).catch(onError)}>
              <FolderIcon />
            </IconButton>
            <IconButton title="Move" disabled={busy} onClick={() => change(part)}>
              <MoveIcon />
            </IconButton>
          </PathRow>
        ))}
      </div>

      <h3>Additional world locations</h3>
      <p className="small">Folders with worlds. One your cloud app keeps in step is shared: the group meets there.</p>
      {view.locations.filter(l => !l.primary).map(location => <div key={location.path} className="folder-box">
        <PathRow label={location.available ? "Worlds" : "Unavailable"} value={location.path}>
          <IconButton title="Open folder" disabled={!location.available} onClick={() => api.storageOpen(location.path).catch(onError)}><FolderIcon /></IconButton>
          <button disabled={busy} onClick={() => setLocations(view.locations.filter(l => !l.primary && l.path !== location.path).map(l => l.path))}>Remove location</button>
        </PathRow>
      </div>)}
      <button disabled={busy} onClick={addLocations}>Add world locations …</button>
      <p className="small">Removing a location leaves its files and your private keys in place.</p>

      {pending && (
        <div className="dialog-backdrop" onClick={() => setPending(null)}>
          <div className="dialog" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
            <h3>{pending.part === "profile" ? "Profile" : "Worlds"}</h3>
            <code className="folder-path">{pending.path}</code>
            <p>{pending.occupied ? "There is data here already. Use it?" : "Take your current data along?"}</p>
            <div className="line">
              {!pending.occupied && (
                <button className="primary" disabled={busy} onClick={() => apply(true)}>
                  Move data here
                </button>
              )}
              <button className={pending.occupied ? "primary" : ""} disabled={busy} onClick={() => apply(false)}>
                {pending.occupied ? "Use it" : "Only switch"}
              </button>
              <button disabled={busy} onClick={() => setPending(null)}>
                Cancel
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
