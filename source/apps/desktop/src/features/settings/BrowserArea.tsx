import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type BrowserView } from "../../api/client";

/**
 * Browser extensions on this device that send pages into a world this device
 * leads: off until switched on, then paired one by one with a short code.
 */
export function BrowserArea({ onError }: { onError: (e: unknown) => void }) {
  const [view, setView] = useState<BrowserView | null>(null);
  const [taken, setTaken] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api.browserSettings().then(setView).catch(onError);
    const paired = listen<string>("browser-paired", () => api.browserSettings().then(setView).catch(() => {}));
    const imported = listen<{ name: string; documents: number; from: string }>("browser-imported", (e) =>
      setTaken((t) => [`${e.payload.name || "Package"} · ${e.payload.documents} new`, ...t].slice(0, 5)),
    );
    return () => {
      void paired.then((stop) => stop());
      void imported.then((stop) => stop());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const apply = async (step: Promise<BrowserView>) => {
    setBusy(true);
    try {
      setView(await step);
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
        <input type="checkbox" checked={view.enabled} disabled={busy} onChange={(e) => apply(api.browserEnable(e.target.checked))} />
        <span>
          <strong>Browser extension</strong>
          <small className="small">{!view.enabled ? "off" : view.open ? "on · this device only" : (view.problem ?? "on — but it could not start")}</small>
        </span>
      </label>
      {view.enabled && (
        <>
          <h3>Paired</h3>
          {view.browsers.length === 0 && <p className="small muted">None yet.</p>}
          {view.browsers.map((b) => (
            <div key={b.id} className="line">
              <span>{b.name}</span>
              <small className="small muted">{new Date(b.since).toLocaleDateString()}</small>
              <button className="compact" disabled={busy} onClick={() => apply(api.browserForget(b.id))}>
                Remove
              </button>
            </div>
          ))}
          <div className="line">
            {view.code ? (
              <>
                <span className="small muted">Code</span>
                <code className="pair-code">{view.code}</code>
              </>
            ) : (
              <button className="primary" disabled={busy || !view.open} onClick={() => apply(api.browserPair())}>
                Pair a browser
              </button>
            )}
          </div>
          {taken.length > 0 && (
            <>
              <h3>Received</h3>
              {taken.map((t, i) => (
                <p key={i} className="small">
                  {t}
                </p>
              ))}
            </>
          )}
        </>
      )}
    </div>
  );
}
