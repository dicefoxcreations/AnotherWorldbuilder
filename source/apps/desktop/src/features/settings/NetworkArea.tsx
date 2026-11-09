import { Fragment, useEffect, useState } from "react";
import { Icon } from "../../shared/ui/Icons";
import { DeleteDialog, DeleteButton } from "../../shared/ui/Delete";
import { api, type NetworkView, type HeldWorld, type Person, type ServerOrder, type ServerState } from "../../api/client";
import { when } from "../wiki/Versions";
import { isMobile } from "../../shared/platform";
import { Reach, serverName } from "../worlds/WorldPlace";

/** Whether this device is on the network at all, and which worlds it offers there. */
export function NetworkArea({
  onChanged,
  onError,
}: {
  onChanged: (n: NetworkView) => void;
  onError: (e: unknown) => void;
}) {
  const [view, setView] = useState<NetworkView | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api.networkSettings().then(setView).catch(onError);
  }, []);

  const apply = async (step: Promise<NetworkView>) => {
    setBusy(true);
    try {
      const n = await step;
      setView(n);
      onChanged(n);
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  };

  if (!view) return null;
  return (
    <div className="type-page">
      <h2>Network</h2>
      <h3>How this device is reached</h3>
      <label className="switch-row">
        <input type="checkbox" checked={view.enabled} disabled={busy} onChange={(e) => apply(api.networkEnable(e.target.checked))} />
        <span>
          <strong>Local network</strong>
          <small className="small">{view.enabled ? (view.running ? "on · finds worlds here and offers yours" : "on — but it could not start") : "off"}</small>
        </span>
      </label>
      <label className="switch-row">
        <input type="checkbox" checked={view.internet} disabled={busy} onChange={(e) => apply(api.networkInternet(e.target.checked, view.stun))} />
        <span>
          <strong>From the internet</strong>
          <small className="small">
            {!view.internet
              ? "off · players reach this device only at home"
              : !view.running
                ? "on — but it could not start"
                : view.reachable.length > 0
                  ? `on · reachable via ${view.via.map((v) => (v === "ipv6" ? "IPv6" : v === "stun" ? "STUN" : "router")).join(", ")}`
                  : "on · not reachable from outside — forward the port on the router"}
          </small>
        </span>
      </label>
      {view.internet && (
        <>
          {view.code && (
            <div className="line readable-copy">
              <span className="small muted">Host code</span>
              <code title={view.code}>{view.code}</code>
              <button onClick={() => void navigator.clipboard.writeText(view.code)}>Copy</button>
            </div>
          )}
          <StunServers stun={view.stun} busy={busy} onSave={(stun) => apply(api.networkInternet(true, stun))} />
        </>
      )}
      {(view.enabled || view.internet) && <PortField port={view.port} busy={busy} onSave={(port) => apply(api.networkPort(port))} />}
      {view.active_port != null && view.active_port !== view.port && <p className="small muted" role="status">Port {view.port} is {view.server_port ? "AW Server's" : "taken"} — using {view.active_port}.</p>}

      <h3>Servers and hosts</h3>
      <Servers view={view} busy={busy} onSave={(servers) => apply(api.networkServers(servers))} onError={onError} />
      {!isMobile && (
        <div className="server-row own-server">
          <span className="grow small muted">A server of its own on this computer, for your group</span>
          <button onClick={() => api.serverLaunch().catch(onError)}>Open AW Server</button>
        </div>
      )}
    </div>
  );
}

/** The port to answer on, for a router's forwarding — empty: the usual one. */
function PortField({ port, busy, onSave }: { port: number; busy: boolean; onSave: (p: number | null) => void }) {
  const [text, setText] = useState(String(port));
  useEffect(() => setText(String(port)), [port]);
  const save = () => {
    const t = text.trim();
    const n = Number(t);
    setText(String(port));
    if (!t) onSave(null);
    else if (Number.isInteger(n) && n > 0 && n < 65536 && n !== port) {
      setText(t);
      onSave(n);
    }
  };
  return (
    <label className="stun-servers">
      <span className="small muted">Port</span>
      <input value={text} disabled={busy} onChange={(e) => setText(e.target.value)} onBlur={save} onKeyDown={(e) => e.key === "Enter" && save()} />
    </label>
  );
}

/** Servers (aw-server) and other hosts to work with, by the code they show. */
function Servers({ view, busy, onSave, onError }: { view: NetworkView; busy: boolean; onSave: (s: string[]) => void; onError: (e: unknown) => void }) {
  const [code, setCode] = useState("");
  const [managing, setManaging] = useState<string | null>(null);
  // A server being edited, by its code, and what stands in its field.
  const [editing, setEditing] = useState<{ code: string; text: string } | null>(null);
  const add = () => {
    if (!code.trim()) return;
    onSave([...view.servers, code.trim()]);
    setCode("");
  };
  const change = () => {
    if (!editing || !editing.text.trim()) return;
    onSave(view.servers.map((c) => (c === editing.code ? editing.text.trim() : c)));
    setEditing(null);
  };
  return (
    <div className="servers">
      {view.server_list.map((entry) => {
        const s = entry.code;
        const admin = entry.admin;
        return (
          <Fragment key={s}>
            <div className="server-row">
              {editing?.code === s ? (
                <input
                  className="grow"
                  autoFocus
                  value={editing.text}
                  disabled={busy}
                  placeholder="Address or code"
                  onChange={(e) => setEditing({ code: s, text: e.target.value })}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") change();
                    if (e.key === "Escape") setEditing(null);
                  }}
                />
              ) : (
                <span className="grow">
                  <strong><Reach on={entry.reached} />{serverName(entry)}</strong>
                  <small className="small muted">
                    {[
                      !entry.reached ? "not reached" : admin ? "you are its admin" : "connected",
                      ...view.worlds.filter((w) => w.server === s).map((w) => `${w.icon} ${w.name}`),
                    ].join(" · ")}
                  </small>
                </span>
              )}
              {editing?.code === s ? (
                <>
                  <button disabled={busy || !editing.text.trim()} onClick={change}>Save</button>
                  <button disabled={busy} onClick={() => setEditing(null)}>Cancel</button>
                </>
              ) : (
                <>
                  {admin && <button onClick={() => setManaging(managing === s ? null : s)}>{managing === s ? "Done" : "Manage"}</button>}
                  <button disabled={busy} onClick={() => setEditing({ code: s, text: s.split("@")[1]?.split(",")[0] ?? s })}>Edit</button>
                  <button disabled={busy} onClick={() => onSave(view.servers.filter((x) => x !== s))}>
                    Remove
                  </button>
                </>
              )}
            </div>
            {managing === s && <ServerAdmin server={s} onError={onError} />}
          </Fragment>
        );
      })}
      <div className="line">
        <input className="grow" value={code} disabled={busy} placeholder="Address or code" onChange={(e) => setCode(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} />
        <button className="icon" title="Add" aria-label="Add" disabled={busy || !code.trim()} onClick={add}>
          <Icon name="add" />
        </button>
      </div>
    </div>
  );
}

function bytes(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(1)} GB`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(1)} MB`;
  return `${Math.max(1, Math.round(n / 1e3))} kB`;
}

/** A server, as its admin manages it: worlds, GMs, everyone who came. */
function ServerAdmin({ server, onError }: { server: string; onError: (e: unknown) => void }) {
  const [state, setState] = useState<ServerState | null>(null);
  const [busy, setBusy] = useState(false);
  const [tab, setTab] = useState<"users" | "worlds">("users");
  const [deleting, setDeleting] = useState<HeldWorld | null>(null);
  const order = async (o: ServerOrder | null) => {
    setBusy(true);
    try {
      setState(await api.serverManage(server, o));
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  };
  useEffect(() => {
    void order(null);
  }, [server]);
  if (!state) return <p className="small muted">Asking the server …</p>;
  return (
    <div className="server-admin">
      <div className="server-tabs">
        <button className={tab === "users" ? "on" : ""} onClick={() => setTab("users")}>Users</button>
        <button className={tab === "worlds" ? "on" : ""} onClick={() => setTab("worlds")}>
          Worlds
          {state.worlds.some((w) => w.requests.length > 0) && <span className="badge">{state.worlds.reduce((n, w) => n + w.requests.length, 0)}</span>}
        </button>
      </div>
      {tab === "users" ? (
        <ServerUsers state={state} busy={busy} order={order} />
      ) : (
        <ServerWorlds state={state} busy={busy} order={order} onDelete={setDeleting} />
      )}
      {deleting && (
        <DeleteDialog
          title={`Delete ${deleting.name}`}
          name={deleting.name}
          warning="Gone from the server. Its GM and members keep what they hold."
          onCancel={() => setDeleting(null)}
          onDelete={() => {
            void order({ kind: "delete", world: deleting.world });
            setDeleting(null);
          }}
        />
      )}
    </div>
  );
}

export const seen = (p: Person) => (p.last ? `seen ${when(p.last * 1000)}` : "not seen yet");

/**
 * Everyone who came to a server, with their roles there: admin (the first
 * one stays), worldbuilder. Kicked, someone is out of every world here they
 * play in and turned away — until let back.
 */
function ServerUsers({ state, busy, order }: { state: ServerState; busy: boolean; order: (o: ServerOrder) => Promise<void> }) {
  const [adding, setAdding] = useState<string | null>(null);
  const [kicking, setKicking] = useState<string | null>(null);
  const [id, setId] = useState("");
  const first = state.admins[0]?.id;
  const everyone = new Map<string, Person>();
  for (const p of [...state.admins, ...state.worldbuilders, ...state.asking, ...state.users]) {
    const known = everyone.get(p.id);
    everyone.set(p.id, { ...p, name: p.name || known?.name || "", last: Math.max(p.last, known?.last ?? 0), blocked: p.blocked || !!known?.blocked });
  }
  const isAdmin = (who: string) => state.admins.some((a) => a.id === who);
  const builds = (who: string) => state.worldbuilders.some((a) => a.id === who);
  const asks = (who: string) => state.asking.some((a) => a.id === who);
  const people = [...everyone.values()].sort((a, b) => Number(isAdmin(b.id)) - Number(isAdmin(a.id)) || b.last - a.last);
  const add = (role: "admin" | "worldbuilder", who: string) =>
    order(role === "admin" ? { kind: "admin", id: who, on: true } : { kind: "allow", id: who });
  return (
    <>
      {people.map((p) => (
        <div key={p.id} className="server-row">
          <span className="grow">
            <span className="server-who">
            <strong>{p.name || p.id}</strong>
            {isAdmin(p.id) && (
              <span className="role-badge role-chip">
                Admin
                {p.id !== first && <button disabled={busy} title="No more admin" onClick={() => void order({ kind: "admin", id: p.id, on: false })}><Icon name="close" /></button>}
              </span>
            )}
            {builds(p.id) && (
              <span className="role-badge role-chip">
                Worldbuilder
                <button disabled={busy} title="May bring worlds no more" onClick={() => void order({ kind: "refuse", id: p.id })}><Icon name="close" /></button>
              </span>
            )}
            {asks(p.id) && (
              <span className="role-badge role-chip accent">
                Would bring a world
                <button disabled={busy} title="Allow" onClick={() => void order({ kind: "allow", id: p.id })}><Icon name="check" /></button>
                <button disabled={busy} title="Refuse" onClick={() => void order({ kind: "refuse", id: p.id })}><Icon name="close" /></button>
              </span>
            )}
            </span>
            <small className="small muted">{p.blocked ? "kicked" : p.id === first ? "first admin" : seen(p)}</small>
          </span>
          {kicking === p.id ? (
            <>
              <small className="small">Out of every world here, and turned away.</small>
              <button className="danger compact" disabled={busy} onClick={() => void order({ kind: "kick", id: p.id }).then(() => setKicking(null))}>Kick</button>
              <button className="compact" onClick={() => setKicking(null)}>Cancel</button>
            </>
          ) : adding === p.id ? (
            <>
              {!isAdmin(p.id) && <button className="compact" disabled={busy} onClick={() => void add("admin", p.id).then(() => setAdding(null))}>Admin</button>}
              {!isAdmin(p.id) && !builds(p.id) && <button className="compact" disabled={busy} onClick={() => void add("worldbuilder", p.id).then(() => setAdding(null))}>Worldbuilder</button>}
              <button className="compact" onClick={() => setAdding(null)}>Cancel</button>
            </>
          ) : p.blocked ? (
            <button className="compact" disabled={busy} onClick={() => void order({ kind: "unblock", id: p.id })}>Let back</button>
          ) : (
            !isAdmin(p.id) && (
              <>
                <button className="compact" disabled={busy} title="Give a role" onClick={() => setAdding(p.id)}>+</button>
                <button className="compact" disabled={busy} onClick={() => setKicking(p.id)}>Kick</button>
              </>
            )
          )}
        </div>
      ))}
      <div className="line">
        <input className="grow" value={id} disabled={busy} placeholder="Someone not here yet, by id" onChange={(e) => setId(e.target.value)} />
        <button className="compact" disabled={busy || !id.trim()} onClick={() => void add("worldbuilder", id.trim()).then(() => setId(""))}>Worldbuilder</button>
        <button className="compact" disabled={busy || !id.trim()} onClick={() => void add("admin", id.trim()).then(() => setId(""))}>Admin</button>
      </div>
    </>
  );
}

/**
 * The worlds on a server, each opened to its people: the GM first, then
 * the supporters, then the players — whom an admin takes out — and who
 * asks to join.
 */
function ServerWorlds({ state, busy, order, onDelete }: { state: ServerState; busy: boolean; order: (o: ServerOrder) => Promise<void>; onDelete: (w: HeldWorld) => void }) {
  const [open, setOpen] = useState<string | null>(null);
  if (state.worlds.length === 0) return <p className="small muted">None yet.</p>;
  return (
    <>
      {state.worlds.map((w) => {
        const named = w.supporters;
        const supporters = w.members.filter((m) => named.includes(m.id));
        const players = w.members.filter((m) => !named.includes(m.id));
        return (
          <Fragment key={w.world}>
            <div className="server-row">
              <button className="ghost compact" aria-label={open === w.world ? "Fold" : "Unfold"} onClick={() => setOpen(open === w.world ? null : w.world)}>
                {open === w.world ? <Icon name="chevron-down" /> : <Icon name="chevron-right" />}
              </button>
              <span className="grow" onClick={() => setOpen(open === w.world ? null : w.world)}>
                <strong>{w.name}</strong>{" "}
                {w.requests.length > 0 && <span className="badge">{w.requests.length}</span>}
                <small className="small muted">
                  {[w.bytes ? bytes(w.bytes) : "deleted", w.closed && "closed"].filter(Boolean).join(" · ")}
                </small>
              </span>
              {w.closed ? (
                <>
                  <button disabled={busy} onClick={() => void order({ kind: "open", world: w.world })}>Open</button>
                  {w.bytes > 0 && <DeleteButton what={w.name} onClick={() => onDelete(w)} />}
                </>
              ) : (
                <button disabled={busy} onClick={() => void order({ kind: "close", world: w.world })}>Close</button>
              )}
            </div>
            {open === w.world && (
              <ul className="server-people">
                {w.gm && (
                  <li>
                    <strong>{w.gm}</strong> <span className="role-badge gm">GM</span>
                  </li>
                )}
                {supporters.map((p) => (
                  <li key={p.id}>
                    <strong>{p.name || p.id}</strong> <span className="role-badge">Supporter</span>
                  </li>
                ))}
                {players.map((p) => (
                  <li key={p.id}>
                    <strong>{p.name || p.id}</strong> <span className="role-badge player">Player</span>
                    <button className="compact" disabled={busy} onClick={() => void order({ kind: "remove", world: w.world, id: p.id })}>Take out</button>
                  </li>
                ))}
                {w.requests.map((p) => (
                  <li key={p.id}>
                    <strong>{p.name || p.id}</strong> <span className="role-badge">Asks to join</span>
                    <button className="compact primary" disabled={busy} onClick={() => void order({ kind: "approve", world: w.world, id: p.id })}>Let in</button>
                  </li>
                ))}
                {supporters.length + players.length + w.requests.length === 0 && <li className="small muted">No players yet.</li>}
              </ul>
            )}
          </Fragment>
        );
      })}
    </>
  );
}

/** Whom to ask how this device looks from outside — public STUN servers; none: only the router. */
function StunServers({ stun, busy, onSave }: { stun: string[]; busy: boolean; onSave: (s: string[]) => void }) {
  const [text, setText] = useState(stun.join(", "));
  const save = () => {
    const next = text.split(",").map((s) => s.trim()).filter(Boolean);
    if (next.join(",") !== stun.join(",")) onSave(next);
  };
  return (
    <label className="stun-servers">
      <span className="small muted">STUN</span>
      <input value={text} disabled={busy} placeholder="none — only the router" onChange={(e) => setText(e.target.value)} onBlur={save} onKeyDown={(e) => e.key === "Enter" && save()} />
    </label>
  );
}
