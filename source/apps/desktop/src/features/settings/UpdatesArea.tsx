import { useState } from "react";
import { installUpdate, lookForUpdate, lookingAtStart, setLookingAtStart, useUpdates } from "../../shared/Updates";
import { isMobile } from "../../shared/platform";

/** Whether this device looks for updates — off until switched on — and what it found. */
export function UpdatesArea() {
  const update = useUpdates();
  const [atStart, setAtStart] = useState(lookingAtStart);
  // Android installs a newer APK over this one; there is nothing to look for here.
  if (isMobile)
    return (
      <div className="type-page">
        <h3>Updates</h3>
        <p className="small muted">Install the newer APK over this one — the data stays.</p>
      </div>
    );
  const text = {
    idle: "",
    checking: "Looking …",
    current: "This is the newest version.",
    ready: update.kind === "ready" ? `${update.version} is there.` : "",
    loading: update.kind === "loading" ? `Loading ${update.version} …` : "",
    failed: update.kind === "failed" ? `Failed: ${update.error}` : "",
  }[update.kind];
  return (
    <div className="type-page">
      <h3>Updates</h3>
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
