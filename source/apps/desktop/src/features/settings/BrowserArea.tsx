import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type BrowserView } from "../../api/client";

/**
 * Browser extensions on this device that send pages into a world this device
 * leads: off until switched on, then found by the extension itself.
 */
export function BrowserArea({ onError }: { onError: (e: unknown) => void }) {
  const [view, setView] = useState<BrowserView | null>(null);
  const [taken, setTaken] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api.browserSettings().then(setView).catch(onError);
    const imported = listen<{ name: string; documents: number; from: string }>("browser-imported", (e) =>
      setTaken((t) => [`${e.payload.name || "Package"} · ${e.payload.documents} new`, ...t].slice(0, 5)),
    );
    return () => {
      void imported.then((stop) => stop());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const enable = async (enabled: boolean) => {
    setBusy(true);
    try {
      setView(await api.browserEnable(enabled));
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  };

  if (!view) return null;
  return (
    <div className="type-page">
      <h2>Browser</h2>
      <label className="switch-row">
        <input type="checkbox" checked={view.enabled} disabled={busy} onChange={(e) => enable(e.target.checked)} />
        <span>
          <strong>Browser extension</strong>
          <small className="small">{!view.enabled ? "off" : view.open ? "on · this device only" : (view.problem ?? "on — but it could not start")}</small>
        </span>
      </label>
      {view.enabled && taken.length > 0 && (
        <>
          <h3>Received</h3>
          {taken.map((t, i) => (
            <p key={i} className="small">
              {t}
            </p>
          ))}
        </>
      )}
    </div>
  );
}
