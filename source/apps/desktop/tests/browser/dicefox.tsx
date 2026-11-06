/** The real loader, with controlled progress and a simultaneous second scene. */
import { StrictMode, useState } from "react";
import { createRoot } from "react-dom/client";
import { DiceFox } from "../../src/shared/ui/DiceFox";
import { FoxLoader } from "../../src/shared/ui/FoxLoader";
import "../../src/styles/global.css";

function Fixture() {
  const [value, setValue] = useState("0");
  const [mounted, setMounted] = useState(true);
  const [light, setLight] = useState(false);
  const progress = value === "unknown" ? null : Number(value);
  return <main style={{ padding: 24, minHeight: "100vh", color: light ? "#192630" : "#ecf3fa", background: light ? "#f4efe5" : "#111b25" }}>
    <h1>DiceFox loader</h1>
    <div className="line" style={{ flexWrap: "wrap" }}>
      <label>Progress <select aria-label="Progress" value={value} onChange={event => setValue(event.target.value)}>
        {["0", "0.12", "0.28", "0.45", "0.5", "0.65", "0.76", "0.82", "0.84", "0.91", "1", "unknown", "-2", "2", "NaN", "Infinity"].map(p => <option key={p}>{p}</option>)}
      </select></label>
      <button onClick={() => setMounted(!mounted)}>Mount / unmount</button>
      <button onClick={() => setLight(!light)}>Light / dark</button>
    </div>
    <div style={{ display: "flex", gap: 24, flexWrap: "wrap", marginTop: 24 }}>
      <div data-fixture="loader" style={{ maxWidth: "100%" }}>
        {mounted && <FoxLoader progress={progress} label="Opening the world …" />}
      </div>
      <div data-fixture="complete" style={{ width: 300, maxWidth: "100%" }}><DiceFox /></div>
    </div>
  </main>;
}

createRoot(document.getElementById("root")!).render(<StrictMode><Fixture /></StrictMode>);
