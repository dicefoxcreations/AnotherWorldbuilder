import { useEffect, useRef, useState, type ReactNode } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { api, type ArticleView, type IdentityInfo, type Template, type WorldInfo } from "../api/client";
import { label } from "../shared/ui/Visibility";
import { DiceButton } from "../features/dice/DiceTray";
import { KeysDialog } from "../shared/ui/KeysDialog";
import { lookAtStart, useUpdates } from "../shared/Updates";

/** What every player holds — a preview always has it, nobody need tick it. */
const EVERY_PLAYER = ["all", "players", "gm"];

/** Closes a dropdown on a click beside it or on Escape. */
function useOutsideClick(open: boolean, close: () => void) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const click = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) close();
    };
    const key = (e: KeyboardEvent) => e.key === "Escape" && close();
    document.addEventListener("mousedown", click);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("mousedown", click);
      document.removeEventListener("keydown", key);
    };
  }, [open, close]);
  return ref;
}

/** The avatar, or failing that the first letter of the name. */
export function Avatar({ me, large }: { me: IdentityInfo; large?: boolean }) {
  const className = large ? "avatar large" : "avatar";
  if (me.avatar) return <img className={className} src={me.avatar} alt="" />;
  return <span className={className}>{me.name.slice(0, 1).toUpperCase()}</span>;
}

/**
 * The header bar. Stands on every page, wherever one is.
 *
 * On the left the outline's switch, then the back button — a door out of
 * the world where the way back begins; none on the start page. In the middle the
 * world's name, without a menu: everything one wants to know about a world
 * or set on it lives in the options. On the far right always the avatar.
 *
 * Before the world's icons stands the slot for the page's own switches (see
 * `HeaderTools`) — a page's buttons come and go there, the ones on the far
 * right stay where they are.
 */
export function HeaderBar({
  me,
  world,
  preview,
  onPreview,
  onBack,
  atStart,
  nav,
  onChanged,
  onError,
  onOptions,
  waiting,
  crumbs,
  toolsRef,
  names,
  people,
}: {
  /** Who else is in the world right now. */
  people?: ReactNode;
  me: IdentityInfo;
  world: WorldInfo | null;
  /** To name what a preview holds: `doc:…` by its document, `type:…` by its type. */
  names?: { articles: ArticleView[]; templates: Template[] };
  /** The path into the depth: world › type › article. */
  crumbs: { label: string; icon?: string; onClick?: () => void }[];
  preview: string[] | null;
  onPreview: (a: string[] | null) => void;
  onBack: () => void;
  /** Going back would leave the world: the button is a door. */
  atStart?: boolean;
  /** The outline on the left, inside a world: whether it stands open. */
  nav?: { shown: boolean; onToggle: () => void };
  onChanged: () => void;
  onError: (e: unknown) => void;
  onOptions: () => void;
  /** Requests to join and changes to review, waiting for the GM — shown on the gear. */
  waiting: number;
  /** Receives the slot the page's switches are put into. */
  toolsRef: (slot: HTMLElement | null) => void;
}) {
  // Updates are looked for at the start where this device says so; one found shows at the gear.
  const update = useUpdates();
  useEffect(lookAtStart, []);
  const [eye, setEye] = useState(false);
  const [profile, setProfile] = useState(false);
  const eyeRef = useOutsideClick(eye, () => setEye(false));
  const profileRef = useOutsideClick(profile, () => setProfile(false));
  // On a phone the tools stand in a menu behind ⋮.
  const [more, setMore] = useState(false);
  const moreRef = useOutsideClick(more, () => setMore(false));

  const previewActive = preview !== null;
  const held = (preview ?? []).filter((a) => !EVERY_PLAYER.includes(a));
  const named = (a: string) => (world ? label(a, world, false, names && { self: null, ...names }) : a);

  // What sits fixed below the bar — the versions — needs to know where it ends.
  const bar = useRef<HTMLElement>(null);
  useEffect(() => {
    const el = bar.current;
    if (!el) return;
    const publish = () => document.documentElement.style.setProperty("--header-height", `${el.offsetHeight}px`);
    const watch = new ResizeObserver(publish);
    watch.observe(el);
    publish();
    return () => watch.disconnect();
  }, []);

  return (
    <header className="header-bar" ref={bar}>
      {nav && (
        <button
          className={nav.shown ? "nav-toggle ghost icon round on" : "nav-toggle ghost icon round"}
          onClick={nav.onToggle}
          title={nav.shown ? "Hide outline" : "Show outline"}
        >
          <svg viewBox="0 0 24 24" width="17" height="17" aria-hidden="true">
            <path d="M5 7h14M5 12h14M5 17h14" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
          </svg>
        </button>
      )}

      {world ? (
        // At the start of the way back stands the door out of the world.
        <button className="back ghost icon round" onClick={onBack} title={atStart ? "Leave world" : "Back"}>
          {atStart ? (
            <svg
              viewBox="0 0 24 24"
              width="17"
              height="17"
              aria-hidden="true"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <path d="M14 4h4a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1h-4M10 8l-4 4 4 4M6 12h9" />
            </svg>
          ) : (
            <svg viewBox="0 0 24 24" width="17" height="17" aria-hidden="true">
              <path d="M14.5 5.5 8 12l6.5 6.5" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          )}
        </button>
      ) : (
        <span className="back placeholder" aria-hidden="true" />
      )}

      {world ? (
        <nav className="crumbs">
          {crumbs.map((k, i) => (
            <span key={`${k.label}-${i}`} className="crumb">
              {i > 0 && <span className="divider">›</span>}
              <button className="ghost" onClick={k.onClick} disabled={!k.onClick}>
                {k.icon && <span className="world-icon small">{k.icon}</span>}
                <span className="crumb-label">{k.label}</span>
              </button>
            </span>
          ))}
        </nav>
      ) : (
        <strong className="page-title">Your worlds</strong>
      )}

      {/* A phone: one row — the page, the dice, and the rest behind ⋮,
          each with its name, as apps do. */}
      <span className="phone-only">
        <DiceButton />
      </span>
      <div className="more-wrap" ref={moreRef}>
      <button className={more ? "more-button ghost icon round on" : "more-button ghost icon round"} onClick={() => setMore((o) => !o)} title="More">
        ⋮{waiting > 0 && <span className="badge">{waiting}</span>}
      </button>

      <div className={more ? "header-right open" : "header-right"}>
        {people}
        <div className="header-tools" ref={toolsRef} />

        <DiceButton />

        {world?.isGm && (
          <div className="dropdown" ref={eyeRef}>
            <button className={previewActive ? "ghost icon round on" : "ghost icon round"} onClick={() => setEye((o) => !o)} title="Preview as a player">
              👁
              {held.length > 0 && <span className="eye-count">{held.length}</span>}
            </button>
            {eye && <EyePopover named={named} world={world} preview={preview} onPreview={onPreview} onChanged={onChanged} onError={onError} />}
          </div>
        )}

        <button
          className="gear ghost icon round"
          onClick={onOptions}
          title={waiting > 0 ? `${waiting} waiting for you — requests to join or changes to review` : update.kind === "ready" ? `Update ${update.version} — under Updates` : world ? "Options of this world" : "Settings"}
        >
          ⚙{waiting > 0 ? <span className="badge">{waiting}</span> : update.kind === "ready" && <span className="badge">1</span>}
        </button>

        <div className="dropdown" ref={profileRef}>
          <button
            className={profile ? "ghost icon round on" : "ghost icon round"}
            onClick={() => setProfile((o) => !o)}
            title={`${me.name} — profile and keys`}
          >
            <Avatar me={me} />
          </button>
          {profile && <ProfilePopover me={me} onClose={() => setProfile(false)} onChanged={onChanged} onError={onError} />}
        </div>
      </div>
      </div>

      {previewActive && (
        <div className="preview-banner">
          <span>
            Previewing as <strong>{held.length ? `a player with ${held.map(named).join(", ")}` : "a player"}</strong>
          </span>
          <button onClick={() => onPreview(null)}>Back to the full view</button>
        </div>
      )}
    </header>
  );
}

function ProfilePopover({ me, onClose, onChanged, onError }: { me: IdentityInfo; onClose: () => void; onChanged: () => void; onError: (e: unknown) => void }) {
  const [name, setName] = useState(me.name);
  const [renaming, setRenaming] = useState(false);

  const pickImage = async () => {
    try {
      const path = await open({
        multiple: false,
        filters: [{ name: "Image", extensions: ["png", "jpg", "jpeg", "gif", "webp"] }],
      });
      if (typeof path !== "string") return;
      await api.avatarSet(path);
      onChanged();
    } catch (e) {
      onError(e);
    }
  };

  const [copied, setCopied] = useState(false);
  const [keys, setKeys] = useState<"backup" | null>(null);
  const copyId = () =>
    navigator.clipboard
      .writeText(me.id)
      .then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      })
      .catch(onError);

  const saveCard = async () => {
    try {
      const path = await save({
        defaultPath: `${me.name}.awid`,
        filters: [{ name: "Player id", extensions: ["awid"] }],
      });
      if (!path) return;
      const where = await api.identityExport(path);
      alert(`Player id saved:\n${where}\n\nSend it to your GM.`);
    } catch (e) {
      onError(e);
    }
  };

  return (
    <div className="popover right">
      <div className="pop-head">
        <Avatar me={me} large />
        <div>
          {renaming ? (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                api
                  .identityRename(name.trim())
                  .then(() => {
                    setRenaming(false);
                    onChanged();
                  })
                  .catch(onError);
              }}
            >
              <input value={name} onChange={(e) => setName(e.target.value)} autoFocus />
              <button className="primary" type="submit">
                ✓
              </button>
            </form>
          ) : (
            <>
              <strong>{me.name}</strong>
              <small className="small">{me.fingerprint}</small>
            </>
          )}
        </div>
      </div>

      <div className="line">
        <button onClick={pickImage}>{me.avatar ? "Change picture" : "Pick a picture"}</button>
        {me.avatar && <button onClick={() => api.avatarRemove().then(onChanged).catch(onError)}>remove</button>}
        {!renaming && <button onClick={() => setRenaming(true)}>rename</button>}
      </div>
      <p className="small">The name is only a label. Your access hangs on the key and is untouched by renaming.</p>

      <section>
        <h4>Your keys</h4>
        <dl className="keys">
          <dt>Id</dt>
          <dd>
            <code className="wrap">{me.id}</code>
          </dd>
          <dt>Public key</dt>
          <dd>
            <code className="wrap">{me.publicKey}</code>
          </dd>
          <dt>File</dt>
          <dd>
            <code className="wrap">{me.file}</code>
          </dd>
        </dl>
        <p className="small">The private part stays on this device — unless you back it up. Lose it and access is gone; no server can help.</p>
        <div className="line">
          <button onClick={copyId}>{copied ? "Copied ✓" : "Copy id"}</button>
          <button onClick={saveCard}>Export player id</button>
          <button onClick={() => setKeys("backup")}>Back up keys …</button>
        </div>
        <p className="small">To join a world on the network, ask there — or give your GM this id first, then they let you in without a request.</p>
      </section>

      <button className="close" onClick={onClose}>
        Close
      </button>
      {keys && <KeysDialog mode={keys} onClose={() => setKeys(null)} />}
    </div>
  );
}

function EyePopover({
  named,
  world,
  preview,
  onPreview,
  onChanged,
  onError,
}: {
  named: (attribute: string) => string;
  world: WorldInfo;
  preview: string[] | null;
  onPreview: (a: string[] | null) => void;
  onChanged: () => void;
  onError: (e: unknown) => void;
}) {
  const [fresh, setFresh] = useState("");
  const toggle = (a: string) => {
    const now = preview ?? [];
    onPreview(now.includes(a) ? now.filter((x) => x !== a) : [...now, a]);
  };

  return (
    <div className="popover right">
      <h4>Preview as</h4>
      <p className="small">A player, with what is ticked — decrypted as their device would.</p>

      <div className="segmented full">
        <button className={preview === null ? "on" : ""} onClick={() => onPreview(null)}>
          Full GM view
        </button>
        <button className={preview !== null ? "on" : ""} onClick={() => preview === null && onPreview([])}>
          Player
        </button>
      </div>

      <div className="chips">
        {world.attributes
          .filter((a) => !EVERY_PLAYER.includes(a))
          .map((a) => (
            <label key={a} className={preview?.includes(a) ? "chip on" : "chip"}>
              <input type="checkbox" checked={preview?.includes(a) ?? false} onChange={() => toggle(a)} />
              {named(a)}
            </label>
          ))}
      </div>

      {world.members.some((m) => m.role !== "gm") && (
        <div className="quick-pick">
          {world.members
            .filter((m) => m.role !== "gm")
            .map((m) => (
              <button key={m.id} onClick={() => onPreview(m.attributes)}>
                like {m.name}
              </button>
            ))}
        </div>
      )}

      <form
        onSubmit={(e) => {
          e.preventDefault();
          api
            .attributeCreate(world.id, fresh.trim())
            .then(() => {
              setFresh("");
              onChanged();
            })
            .catch(onError);
        }}
      >
        <input value={fresh} onChange={(e) => setFresh(e.target.value)} placeholder="class:rogue" />
        <button type="submit" disabled={!fresh.trim()}>
          Add
        </button>
      </form>
    </div>
  );
}
