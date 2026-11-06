import { useRef, useState } from "react";
import { useBack } from "../useBack";
import { createPortal } from "react-dom";
import { open, save } from "@tauri-apps/plugin-dialog";
import { api, type IdentityInfo } from "../../api/client";
import { isMobile } from "../platform";

/**
 * Backs one's identity up, or brings it back — locked with a passphrase.
 * Without it, a new install or another device is someone else to the worlds
 * one plays in.
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
  useBack(true, onClose);
  const [again, setAgain] = useState("");
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const backup = mode === "backup";
  const short = phrase.length > 0 && phrase.length < 10;
  const ready = backup ? phrase.length >= 10 && phrase === again : phrase.length > 0;

  // A phone has no paths to choose: the backup goes to its Documents, and
  // one is brought back from the file picked, as bytes.
  const picker = useRef<HTMLInputElement>(null);
  const restorePicked = async (file: File | undefined) => {
    if (!file) return;
    setProblem(null);
    setBusy(true);
    try {
      const me = await api.keysRestoreBytes(new Uint8Array(await file.arrayBuffer()), phrase);
      onRestored?.(me);
      onClose();
    } catch (e) {
      setProblem(String(e));
    } finally {
      setBusy(false);
    }
  };

  const go = async () => {
    setProblem(null);
    if (isMobile && !backup) {
      picker.current?.click();
      return;
    }
    try {
      if (isMobile) {
        setBusy(true);
        setDone(await api.keysBackupPhone(phrase));
      } else if (backup) {
        const path = await save({ defaultPath: "keys.awkeys", filters: [{ name: "Profile backup", extensions: ["awkeys"] }] });
        if (!path) return;
        setBusy(true);
        setDone(await api.keysBackup(path, phrase));
      } else {
        const path = await open({ multiple: false, filters: [{ name: "Profile backup", extensions: ["awkeys"] }] });
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

  // Above everything: opened from the profile, which on a phone sits in the ⋮ menu.
  return createPortal(
    <div className="dialog-backdrop" onClick={() => !busy && onClose()}>
      <section className="dialog keys-dialog" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
        <h2>{backup ? "Back up your profile" : "Restore your profile"}</h2>
        {done ? (
          <>
            <p>Saved: {isMobile && done.includes("/Documents/") ? `Documents/${done.split("/Documents/")[1]}` : done}</p>
            <p className="small">Keep it somewhere else too — and the passphrase apart from it.</p>
            <button className="primary" onClick={onClose}>
              Done
            </button>
          </>
        ) : (
          <>
            <p className="small">
              {backup
                ? "Your identity — what makes you GM or player in your worlds."
                : "Your identity from a backup."}
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
            {isMobile && <input ref={picker} type="file" hidden onChange={(e) => void restorePicked(e.target.files?.[0])} />}
            <div className="line">
              <button className="primary" disabled={!ready || busy} onClick={go}>
                {busy ? "…" : backup ? (isMobile ? "Save backup" : "Save backup …") : "Choose backup …"}
              </button>
              <button disabled={busy} onClick={onClose}>
                Cancel
              </button>
            </div>
          </>
        )}
      </section>
    </div>,
    document.body,
  );
}
