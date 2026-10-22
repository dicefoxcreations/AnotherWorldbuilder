import { useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { api, type IdentityInfo } from "../../api/client";

/**
 * Backs one's keys up, or brings them back — locked with a passphrase.
 * Without them nobody opens an encrypted world again: not this device after
 * a new install, not another one.
 */
export function KeysDialog({
  mode,
  onClose,
  onRestored,
}: {
  mode: "backup" | "restore";
  onClose: () => void;
  onRestored?: (me: IdentityInfo) => void;
}) {
  const [phrase, setPhrase] = useState("");
  const [again, setAgain] = useState("");
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const backup = mode === "backup";
  const short = phrase.length > 0 && phrase.length < 10;
  const ready = backup ? phrase.length >= 10 && phrase === again : phrase.length > 0;

  const go = async () => {
    setProblem(null);
    try {
      if (backup) {
        const path = await save({ defaultPath: "keys.awkeys", filters: [{ name: "Key backup", extensions: ["awkeys"] }] });
        if (!path) return;
        setBusy(true);
        setDone(await api.keysBackup(path, phrase));
      } else {
        const path = await open({ multiple: false, filters: [{ name: "Key backup", extensions: ["awkeys"] }] });
        if (typeof path !== "string") return;
        setBusy(true);
        const me = await api.keysRestore(path, phrase);
        onRestored?.(me);
        onClose();
      }
    } catch (e) {
      setProblem(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="dialog-backdrop" onClick={() => !busy && onClose()}>
      <section className="dialog keys-dialog" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
        <h2>{backup ? "Back up keys" : "Restore keys"}</h2>
        {done ? (
          <>
            <p>Saved: {done}</p>
            <p className="small">Keep it somewhere else too — and the passphrase apart from it.</p>
            <button className="primary" onClick={onClose}>
              Done
            </button>
          </>
        ) : (
          <>
            <p className="small">
              {backup
                ? "Your identity and the keys of your worlds. Without them nothing encrypted opens again — not after a new install, not on another device."
                : "Your identity and world keys from a backup. The worlds themselves come from their folder."}
            </p>
            <label className="world-name-field">
              Passphrase
              <input type="password" autoFocus value={phrase} onChange={(e) => setPhrase(e.target.value)} />
            </label>
            {backup && (
              <label className="world-name-field">
                Again
                <input type="password" value={again} onChange={(e) => setAgain(e.target.value)} onKeyDown={(e) => e.key === "Enter" && ready && go()} />
              </label>
            )}
            {backup && short && <p className="small warning">At least 10 characters.</p>}
            {backup && again && phrase !== again && <p className="small warning">The two differ.</p>}
            {problem && <p className="small warning">{problem}</p>}
            <div className="line">
              <button className="primary" disabled={!ready || busy} onClick={go}>
                {busy ? "…" : backup ? "Save backup …" : "Choose backup …"}
              </button>
              <button disabled={busy} onClick={onClose}>
                Cancel
              </button>
            </div>
          </>
        )}
      </section>
    </div>
  );
}
