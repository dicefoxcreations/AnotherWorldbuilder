import { useEffect, useState } from "react";
import logo from "../../assets/logo.png";
import { installUpdate, lookForUpdate, lookingAtStart, setLookingAtStart, useUpdates } from "../../shared/Updates";

/**
 * The app itself: what it is called, its version, who holds it — and the
 * libraries it is built from, each with its license text, as they must be
 * shipped with it (`tools/licenses/notices.mjs`).
 */
export function AboutArea() {
  const [version, setVersion] = useState("");
  const [notices, setNotices] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    import("@tauri-apps/api/app")
      .then((m) => m.getVersion())
      .then(setVersion)
      .catch(() => {});
  }, []);
  const show = () =>
    fetch("/licenses.txt")
      .then((r) => (r.ok ? r.text() : Promise.reject()))
      .then(setNotices)
      .catch(() => setFailed(true));
  return (
    <div className="type-page about">
      <div className="about-head">
        <img src={logo} alt="" width={96} height={96} />
        <h2>AnotherWorldbuilder{version && <small className="small"> {version}</small>}</h2>
      </div>
      <p className="small">© 2026 DiceFox Creations. All rights reserved.</p>
      <Updates />
      {notices === null ? (
        <button className="compact" onClick={show}>
          Third-party licenses
        </button>
      ) : (
        <pre className="licenses">{notices}</pre>
      )}
      {failed && <p className="small error">Not in this build.</p>}
    </div>
  );
}

/** Whether this device looks for updates, and what it found. */
function Updates() {
  const update = useUpdates();
  const [atStart, setAtStart] = useState(lookingAtStart);
  const text = {
    idle: "",
    checking: "Looking …",
    current: "This is the newest version.",
    ready: update.kind === "ready" ? `${update.version} is there.` : "",
    loading: update.kind === "loading" ? `Loading ${update.version} …` : "",
    failed: update.kind === "failed" ? `Not reached: ${update.error}` : "",
  }[update.kind];
  return (
    <div className="type-page">
      <label className="switch-row">
        <input
          type="checkbox"
          checked={atStart}
          onChange={(e) => {
            setLookingAtStart(e.target.checked);
            setAtStart(e.target.checked);
          }}
        />
        <span>
          <strong>Look for updates at the start</strong>
          <small className="small">Asks the release page on GitHub for the newest version.</small>
        </span>
      </label>
      <div className="line backup-plan">
        {update.kind === "ready" ? (
          <button className="primary" onClick={() => void installUpdate()}>Install and restart</button>
        ) : (
          <button disabled={update.kind === "checking" || update.kind === "loading"} onClick={() => void lookForUpdate()}>
            Look now
          </button>
        )}
        <small className="small muted grow">{text}</small>
      </div>
    </div>
  );
}
