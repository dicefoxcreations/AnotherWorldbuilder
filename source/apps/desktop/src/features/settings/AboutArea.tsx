import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import logo from "../../assets/logo.png";

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
      <button className="compact" onClick={() => invoke("app_log_open").catch(() => {})}>
        Open the log folder
      </button>
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

