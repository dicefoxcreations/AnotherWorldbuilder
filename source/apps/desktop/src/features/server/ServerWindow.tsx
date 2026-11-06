import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AreaNav, type Area } from "../../shared/ui/AreaNav";
import { AboutArea } from "../settings/AboutArea";
import { Icon } from "../../shared/ui/Icons";
import serverIcon from "../../assets/aw-server.png";

/**
 * AW Server's window (`--server`): the dedicated server on this computer —
 * started, stopped and set up here, running on in the tray when the window
 * is closed. Its worlds are moved here from the app, as onto a NAS. Its
 * areas are those of AW Server on a phone, in the frame of the app's options.
 */

type Status = { state: "starting" | "running" | "stopping" | "stopped" | "failed"; code: string; error: string };
type Settings = { admin: string; name: string; port: number; addresses: string[]; running: boolean };
type View = { settings: Settings; status: Status; folder: string; autostart: boolean; own: string | null };
type Kept = { id: string; name: string; icon: string; articles: number; members: number; closed: boolean; error: string };

const AREAS: Area<"server" | "settings" | "worlds" | "about">[] = [
  { id: "server", icon: "server", name: "Server" },
  { id: "settings", icon: "settings", name: "Settings" },
  { id: "worlds", icon: "world", name: "Worlds" },
  { id: "about", icon: "about", name: "About" },
];

const count = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;

const STATE: Record<Status["state"], string> = {
  starting: "Starting …",
  running: "Running",
  stopping: "Stopping …",
  stopped: "Stopped",
  failed: "Stopped by an error",
};

export function ServerWindow() {
  const [view, setView] = useState<View | null>(null);
  const [draft, setDraft] = useState<Settings | null>(null);
  const [worlds, setWorlds] = useState<Kept[]>([]);
  const [problem, setProblem] = useState("");
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);
  const [area, setArea] = useState<(typeof AREAS)[number]["id"]>("server");

  const shown = (v: View) => {
    setView(v);
    // Not set up yet: the app's own profile here is its admin, until saved otherwise.
    setDraft((d) => d ?? { ...v.settings, admin: v.settings.admin || v.own || "" });
  };
  const fail = (e: unknown) => setProblem(String(e));
  const ask = async (command: string, args?: Record<string, unknown>) => {
    setBusy(true);
    setProblem("");
    try {
      shown(await invoke<View>(command, args));
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    invoke<View>("server_view").then(shown).catch(fail);
    const told = listen<Status>("server-status", (e) => setView((v) => (v ? { ...v, status: e.payload } : v)));
    return () => void told.then((stop) => stop());
  }, []);
  const state = view?.status.state;
  useEffect(() => {
    invoke<Kept[]>("server_worlds").then(setWorlds).catch(() => setWorlds([]));
  }, [state]);

  if (!view || !draft) return null;
  const { status } = view;
  const running = status.state === "running" || status.state === "starting";
  const moving = status.state === "starting" || status.state === "stopping";
  const changed = JSON.stringify({ ...draft, running: false }) !== JSON.stringify({ ...view.settings, running: false });
  const set = (part: Partial<Settings>) => setDraft({ ...draft, ...part });
  const ready = /^(id:)?[0-9a-f]{16}$/i.test(draft.admin.trim());
  // Starting saves what is set first.
  const begin = async () => {
    setBusy(true);
    setProblem("");
    try {
      if (changed) await invoke<View>("server_save", { settings: draft });
      shown(await invoke<View>("server_start"));
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };
  const copy = () =>
    void navigator.clipboard.writeText(status.code).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });

  return (
    <div className="server-window">
      <div className="options">
        <AreaNav
          head={
            <>
              <img src={serverIcon} alt="" width={24} height={24} />
              <strong>AW Server</strong>
            </>
          }
          areas={AREAS}
          on={area}
          onPick={setArea}
        />
        <div className="options-body">
          {area === "server" && (
            <div className="type-page">
              <section className={`server-hero ${status.state}`}>
                <img src={serverIcon} alt="" width={64} height={64} />
                <div className="server-hero-text">
                  <strong>{view.settings.name}</strong>
                  <span className="server-pill">{STATE[status.state]}</span>
                  <small className="small muted">Port {view.settings.port}</small>
                </div>
                <label className="server-power" title={running ? "Stop" : changed ? "Save and start" : "Start"}>
                  <input
                    type="checkbox"
                    aria-label={running ? "Stop" : "Start"}
                    checked={running}
                    disabled={busy || moving || (!running && !ready)}
                    onChange={() => (running ? void ask("server_stop") : void begin())}
                  />
                </label>
              </section>
              {status.state === "failed" && <p className="small error">{status.error}</p>}
              {!ready && !running && (
                <div className="line">
                  <span className="small muted">An admin ID first.</span>
                  <button className="ghost compact" onClick={() => setArea("settings")}>
                    Settings
                  </button>
                </div>
              )}
              {status.code && (
                <div className="server-code-box">
                  <span className="small muted">Code</span>
                  <code className="small server-code">{status.code}</code>
                  <button className="icon" title={copied ? "Copied" : "Copy"} aria-label="Copy" onClick={copy}>
                    <Icon name={copied ? "check" : "copy"} />
                  </button>
                </div>
              )}
              <label className="switch-row">
                <input type="checkbox" checked={view.autostart} disabled={busy} onChange={(e) => ask("server_autostart", { on: e.target.checked })} />
                <span>
                  <strong>Start with this computer</strong>
                </span>
              </label>
              {problem && <p className="small error">{problem}</p>}
            </div>
          )}

          {area === "settings" && (
            <div className="type-page">
              <h2>Settings</h2>
              <label className="field">
                <span className="small muted">Admin ID</span>
                <span className="line">
                  <input value={draft.admin} placeholder="id:…" onChange={(e) => set({ admin: e.target.value })} />
                  {view.own && draft.admin !== view.own && (
                    <button className="ghost compact" onClick={() => set({ admin: view.own ?? "" })}>
                      Use mine
                    </button>
                  )}
                </span>
              </label>
              <label className="field">
                <span className="small muted">Name</span>
                <input value={draft.name} onChange={(e) => set({ name: e.target.value })} />
              </label>
              <label className="field">
                <span className="small muted">Port</span>
                <input value={String(draft.port)} inputMode="numeric" onChange={(e) => set({ port: Number(e.target.value.replace(/\D/g, "")) || 0 })} />
              </label>
              <details className="server-advanced" open={draft.addresses.length > 0}>
                <summary>Advanced</summary>
                <label className="field">
                  <span className="small muted">Public address</span>
                  <input
                    value={draft.addresses.join(", ")}
                    placeholder="automatic"
                    onChange={(e) => set({ addresses: e.target.value.split(",").map((a) => a.trim()).filter(Boolean) })}
                  />
                  <span className="small muted">Only for a fixed name, such as a DynDNS address, with its port.</span>
                </label>
              </details>
              {changed && (
                <div className="line">
                  <button className="primary" disabled={busy} onClick={() => ask("server_save", { settings: draft })}>
                    {running ? "Save and restart" : "Save"}
                  </button>
                  <button disabled={busy} onClick={() => setDraft(view.settings)}>
                    Reset
                  </button>
                </div>
              )}
              {problem && <p className="small error">{problem}</p>}
            </div>
          )}

          {area === "worlds" && (
            <div className="type-page">
              <div className="line between">
                <h2>Worlds</h2>
                <button className="icon" title="Open folder" aria-label="Open folder" onClick={() => invoke("server_folder_open").catch(fail)}>
                  <Icon name="folder-open" />
                </button>
              </div>
              {worlds.length === 0 && <p className="small muted">None yet — move one here from the app.</p>}
              <div className="server-worlds">
                {worlds.map((w) => (
                  <div key={w.id} className="server-world">
                    <span className="world-icon small">{w.icon}</span>
                    <span className="server-world-text">
                      <strong>{w.name}</strong>
                      <small className="small muted">
                        {w.error || `${count(w.articles, "article")} · ${count(w.members, "member")}${w.closed ? " · closed" : ""}`}
                      </small>
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}

          {area === "about" && <AboutArea name="AW Server" icon={serverIcon} />}
        </div>
      </div>
    </div>
  );
}
