import { Fragment, useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { StorageArea } from "./Storage";
import { DeleteDialog, DeleteButton } from "../../shared/ui/Delete";
import {
  api,
  type ArticleView,
  type IdentityInfo,
  type JoinRequest,
  type MemberInfo,
  type WaitingVersion,
  type NetworkView,
  type HeldWorld,
  type Person,
  type ServerOrder,
  type ServerState,
  type SwitchProgress,
  type Template,
  type WorldInfo,
} from "../../api/client";
import { TemplateEditor } from "../templates/TemplateEditor";
import { IconPicker } from "../../shared/ui/IconPicker";
import { findType, isA, realTypes, subtypes, usersOf } from "../../shared/types";
import { when } from "../wiki/Versions";
import { GalleryArea } from "../gallery/Gallery";
import { ShortcutsArea } from "./ShortcutsArea";
import { SearchPick, type Pickable } from "../../shared/ui/SearchPick";
import { typeAttribute } from "../../shared/ui/Visibility";
import { CalendarArea } from "./CalendarArea";
import { AppearanceArea } from "./Appearance";
import { DiceArea, SavedRolls } from "./DiceArea";
import { AboutArea } from "./AboutArea";
import { USUAL_DICE } from "../dice/DiceTray";
import { TransferArea } from "./Transfer";
import { KeysDialog } from "../../shared/ui/KeysDialog";
import { PlaceArea, serverName } from "../worlds/WorldPlace";

/** The areas of the options. More will follow. */
/** The areas, in three groups: the open world's, this device's, the app's. */
const AREAS = [
  { id: "world", icon: "🌍", name: "World", group: "world" },
  { id: "players", icon: "👥", name: "Players", group: "world" },
  { id: "types", icon: "📄", name: "Types", group: "world" },
  { id: "images", icon: "🖼", name: "Images", group: "world" },
  { id: "calendar", icon: "◷", name: "Calendar", group: "world" },
  { id: "dice", icon: "🎲", name: "Dice", group: "world" },
  { id: "transfer", icon: "⇅", name: "Transfer", group: "world" },
  { id: "layout", icon: "🎨", name: "Appearance", group: "device" },
  { id: "keys", icon: "⌨", name: "Shortcuts", group: "device" },
  { id: "network", icon: "🌐", name: "Network", group: "device" },
  { id: "storage", icon: "💾", name: "Storage", group: "device" },
  { id: "about", icon: "ℹ", name: "About", group: "app" },
] as const;

const GROUPS = { world: "This world", device: "This device", app: "App" } as const;

type AreaId = (typeof AREAS)[number]["id"];

export function Options({
  world,
  me,
  templates,
  articles,
  onChanged,
  onArticlesChanged,
  onWorldChanged,
  onOpen,
  onNetworkChanged,
  onStorageChanged,
  onLeave,
  onClose,
  onError,
}: {
  /** The open world — none on the main menu, then only the network. */
  world: WorldInfo | null;
  me: IdentityInfo;
  templates: Template[];
  articles: ArticleView[];
  onChanged: (t: Template[]) => void;
  onArticlesChanged: () => void;
  onWorldChanged: () => void;
  /** Opens a document — the options close for it. */
  onOpen: (id: string) => void;
  onNetworkChanged: (n: NetworkView) => void;
  onStorageChanged: () => void;
  /** Back to the main menu, to another world. */
  onLeave?: () => void;
  onClose: () => void;
  onError: (e: unknown) => void;
}) {
  // What waits for the GM comes first.
  const due = world ? world.requests + (world.isGm ? world.waiting : 0) : 0;
  const [area, setArea] = useState<AreaId>(world ? (due > 0 ? "players" : "world") : "network");
  // Without a world only what belongs to the device as a whole.
  // Outside a world only what belongs to this device.
  // The world's basics are the GM's to set.
  // Outside a world: what belongs to the device — the dice's look among it.
  const areas = world
    ? AREAS.filter((b) => b.id !== "transfer" || world.isGm)
    : AREAS.filter((b) => b.group !== "world" || b.id === "dice").filter((b) => b.id !== "keys");
  const groupOf = (b: (typeof AREAS)[number]) => (world || b.group !== "world" ? GROUPS[b.group] : GROUPS.device);

  useEffect(() => {
    const key = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    document.addEventListener("keydown", key);
    return () => document.removeEventListener("keydown", key);
  }, [onClose]);

  return (
    <div className="options-backdrop" onClick={onClose}>
      <div className="options" onClick={(e) => e.stopPropagation()}>
        <nav className="options-nav">
          <div className="nav-head">
            {world ? (
              <>
                <span className="world-icon small">{world.icon}</span>
                <strong>{world.name}</strong>
              </>
            ) : (
              <strong>Settings</strong>
            )}
          </div>
          {/* Without a world open, what is left of its areas belongs to this device. */}
          {areas.map((b, i) => (
            <Fragment key={b.id}>
              {(i === 0 || groupOf(areas[i - 1]) !== groupOf(b)) && <h5 className="options-group">{groupOf(b)}</h5>}
              <button className={b.id === area ? "ghost on" : "ghost"} onClick={() => setArea(b.id)}>
                <span className="nav-icon">{b.icon}</span>
                {b.name}
                {b.id === "players" && due > 0 && <span className="badge">{due}</span>}
              </button>
            </Fragment>
          ))}
          {onLeave && (
            <button className="ghost" onClick={onLeave}>
              <span className="nav-icon">⇄</span>
              Switch world
            </button>
          )}
          <button className="close" onClick={onClose}>
            Close
          </button>
        </nav>

        <div className="options-body">
          {area === "network" && <NetworkArea onChanged={onNetworkChanged} onError={onError} />}
          {area === "storage" && <StorageArea onMoved={onStorageChanged} onError={onError} />}
          {area === "layout" && <AppearanceArea />}
          {area === "dice" && (
            <DiceArea
              world={world?.isGm ? <WorldDice world={world} onChanged={onWorldChanged} onError={onError} /> : undefined}
              rolls={world ? <SavedRolls world={world} onError={onError} /> : undefined}
            />
          )}
          {area === "about" && <AboutArea />}
          {world && area === "types" && (
            <TypesArea
              world={world}
              templates={templates}
              articles={articles}
              onChanged={onChanged}
              onArticlesChanged={onArticlesChanged}
              onOpen={onOpen}
              onError={onError}
            />
          )}
          {world && area === "players" && (
            <PlayersArea
              world={world}
              me={me}
              templates={templates}
              articles={articles}
              onChanged={onWorldChanged}
              onOpen={onOpen}
              onError={onError}
            />
          )}
          {world && area === "world" && (
            <>
              <InfoArea world={world} onChanged={onWorldChanged} onError={onError} />
              <PlaceArea world={world} onChanged={onWorldChanged} onError={onError} />
              {world.isGm && <BasicsArea world={world} onChanged={onWorldChanged} onError={onError} />}
              <ReadableCopy world={world} onChanged={onWorldChanged} onError={onError} />
            </>
          )}
          {world && area === "images" && <GalleryArea world={world} onOpen={onOpen} onError={onError} />}
          {world && area === "keys" && <ShortcutsArea world={world} templates={templates} />}
          {world && area === "calendar" && <CalendarArea world={world} onChanged={onWorldChanged} onError={onError} />}
          {world && area === "transfer" && (
            <TransferArea
              world={world}
              templates={templates}
              articles={articles}
              onImported={() => {
                onArticlesChanged();
                onWorldChanged();
              }}
              onError={onError}
            />
          )}

        </div>
      </div>
    </div>
  );
}

/** The dice that can be rolled here, one or more polyhedral, or pool systems' d6 alone. */
const OFFERED = [2, 3, 4, 6, 8, 10, 12, 20, 100];

/**
 * The world's dice — what the dice bar offers to add by a click. Any die may
 * still be rolled when written (`3d7`); this is only the bar. The GM's.
 */
function WorldDice({ world, onChanged, onError }: { world: WorldInfo; onChanged: () => void; onError: (e: unknown) => void }) {
  const now = world.dice?.length ? world.dice : USUAL_DICE;
  const [own, setOwn] = useState("");
  const set = (sides: number[]) => api.worldDiceSet(world.id, sides.sort((a, b) => a - b)).then(onChanged).catch(onError);
  // One stays: none would mean the usual set again.
  const toggle = (n: number) => (now.includes(n) ? now.length > 1 && set(now.filter((d) => d !== n)) : set([...now, n]));
  const extra = now.filter((n) => !OFFERED.includes(n));
  const n = Number(own.trim().replace(/^[dDwW]/, ""));
  const sides = Number.isInteger(n) && n >= 2 && n <= 1000 ? n : null;
  const add = () => {
    if (!sides) return;
    setOwn("");
    void set([...now.filter((d) => d !== sides), sides]);
  };
  return (
    <div className="world-dice">
      <h3>In this world</h3>
      <small className="small">In the dice bar, for everyone.</small>
      <div className="time-chips">
        {[...OFFERED, ...extra].map((n) => (
          <button key={n} className={`chip${now.includes(n) ? " on" : ""}`} onClick={() => toggle(n)}>
            d{n}
          </button>
        ))}
        <span className="world-dice-add">
          <input className="world-dice-own" value={own} placeholder="d7" onChange={(e) => setOwn(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} />
          <button className="compact" title="Add this die" disabled={!sides} onClick={add}>
            +
          </button>
        </span>
      </div>
    </div>
  );
}

/** What holds for the whole world — the GM's. */
function BasicsArea({ world, onChanged, onError }: { world: WorldInfo; onChanged: () => void; onError: (e: unknown) => void }) {
  const run = (step: Promise<unknown>) => step.then(onChanged).catch(onError);
  return (
    <div className="type-page">
      <label className="switch-row">
        <input
          type="number"
          className="version-limit"
          min={1}
          max={1000}
          key={world.versionLimit}
          defaultValue={world.versionLimit}
          onBlur={(e) => {
            const max = Math.round(Number(e.target.value));
            if (max >= 1 && max !== world.versionLimit) run(api.worldVersionsSet(world.id, max));
            else e.target.value = String(world.versionLimit);
          }}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
        />
        <span>
          <strong>Versions per document</strong>
          <small className="small">The oldest go once there are more.</small>
        </span>
      </label>
      <Encryption world={world} onChanged={onChanged} onError={onError} />
    </div>
  );
}

/**
 * Encrypted or not. Unencrypted, the world lies in its folder as it is, with
 * a readable copy beside it — no views, no rights. Encrypted, they count;
 * without a backup of one's keys nothing opens on a new device. Switching a
 * big world takes minutes: the dialog shows how far it is.
 */
function Encryption({ world, onChanged, onError }: { world: WorldInfo; onChanged: () => void; onError: (e: unknown) => void }) {
  const [asking, setAsking] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [keys, setKeys] = useState(false);
  const [progress, setProgress] = useState<SwitchProgress | null>(null);
  const [started, setStarted] = useState(0);
  const [now, setNow] = useState(0);
  useEffect(() => {
    if (!busy) return;
    const timer = setInterval(() => {
      setNow(Date.now());
      api.worldSwitchProgress(world.id).then(setProgress).catch(() => {});
    }, 500);
    return () => clearInterval(timer);
  }, [busy, world.id]);
  const turn = async (on: boolean) => {
    setBusy(true);
    setProgress(null);
    setStarted(Date.now());
    setNow(Date.now());
    try {
      await api.worldEncryptionSet(world.id, on);
      setAsking(null);
      onChanged();
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  };
  const seconds = Math.max(0, Math.round((now - started) / 1000));
  const share = progress ? (progress.step - 1 + (progress.total > 0 ? progress.done / progress.total : 0.5)) / progress.steps : 0;
  return (
    <>
      <label className="switch-row">
        <input type="checkbox" checked={world.encrypted} disabled={busy} onChange={(e) => setAsking(e.target.checked)} />
        <span>
          <strong>Encrypted</strong>
          <small className="small">
            {world.encrypted
              ? "on · views and rights hold everywhere"
              : world.server
                ? "off · the server keeps views and rights · it reads everything"
                : "off · the host keeps views and rights · whoever has the folder reads everything"}
          </small>
        </span>
      </label>
      {world.readableInWorld && (
        <div className="line readable-copy">
          <span className="small muted">Readable copy</span>
          <code title={world.readable}>{world.readable}</code>
          <button onClick={() => api.storageOpen(world.readable).catch(onError)}>Show</button>
        </div>
      )}
      {asking !== null && (
        <div className="dialog-backdrop" onClick={() => !busy && setAsking(null)}>
          <section className="dialog" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
            {asking ? (
              <>
                <h2>Encrypt the world?</h2>
                <p>
                  <strong>Back up your keys first.</strong> Without them nothing in this world opens again — not after a new install, not on another device.
                </p>
                <p className="small">Views and rights count again. The readable copy goes. What lay open until now stays readable to whoever kept a copy.</p>
                {!busy && <button onClick={() => setKeys(true)}>Back up keys …</button>}
              </>
            ) : (
              <>
                <h2>Turn encryption off?</h2>
                <p>Everything lies in the world's folder as it is — GM parts, secrets, all of it. Whoever has the folder reads and edits everything: on a server, the server and its admin; in a shared folder, everyone in it.</p>
                <p className="small">Players working online still get only what views and rights allow — the host keeps them.</p>
                <p className="small">A readable copy appears beside it: a text per document, the images as image files.</p>
              </>
            )}
            {busy && (
              <div className="switch-progress">
                <div className="progress-bar">
                  <div style={{ width: `${Math.round(share * 100)}%` }} />
                </div>
                <small className="small">
                  {progress ? `${progress.step} of ${progress.steps} · ${progress.what}${progress.total > 0 ? ` · ${progress.done.toLocaleString()} of ${progress.total.toLocaleString()}` : ""}` : "Starting"} · {Math.floor(seconds / 60)}:{String(seconds % 60).padStart(2, "0")}
                </small>
              </div>
            )}
            <div className="line">
              <button className={asking ? "primary" : "danger"} disabled={busy} onClick={() => turn(asking)}>
                {busy ? "Switching …" : asking ? "Encrypt" : "Turn off"}
              </button>
              {!busy && <button onClick={() => setAsking(null)}>Cancel</button>}
            </div>
          </section>
        </div>
      )}
      {keys && <KeysDialog mode="backup" onClose={() => setKeys(false)} />}
    </>
  );
}

/**
 * A readable copy of one's own, on this device: a text per document, images
 * as files — of an encrypted world, of what this device can read. An
 * unencrypted world has its copy in its folder already.
 */
function ReadableCopy({ world, onChanged, onError }: { world: WorldInfo; onChanged: () => void; onError: (e: unknown) => void }) {
  const [busy, setBusy] = useState(false);
  const turn = async (on: boolean) => {
    setBusy(true);
    try {
      await api.worldReadableSet(world.id, on);
      onChanged();
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  };
  const possible = world.encrypted && !world.online && !world.fleeting;
  const own = possible && !!world.readable;
  return (
    <div className="type-page">
      <label className={possible ? "switch-row" : "switch-row off"}>
        <input type="checkbox" checked={own} disabled={busy || !possible} onChange={(e) => void turn(e.target.checked)} />
        <span>
          <strong>Readable copy on this device</strong>
          <small className="small">
            {!possible
              ? world.online || world.fleeting
                ? "not for a world worked on online"
                : "only for encrypted worlds · unencrypted, the copy lies in the world's folder"
              : own
                ? "on · a text per document, images as files, in the clear"
                : "off"}
          </small>
        </span>
      </label>
      {own && (
        <div className="line readable-copy">
          <code title={world.readable}>{world.readable}</code>
          <button onClick={() => api.storageOpen(world.readable).catch(onError)}>Show</button>
        </div>
      )}
    </div>
  );
}

function TypesArea({
  world,
  templates,
  articles,
  onChanged,
  onArticlesChanged,
  onOpen,
  onError,
}: {
  world: WorldInfo;
  templates: Template[];
  articles: ArticleView[];
  onChanged: (t: Template[]) => void;
  onArticlesChanged: () => void;
  onOpen: (id: string) => void;
  onError: (e: unknown) => void;
}) {
  const [open, setOpen] = useState<string | null>(null);
  const [draft, setDraft] = useState<Template | null>(null);
  const [saved, setSaved] = useState(false);

  // Opening a type edits a copy of its own.
  useEffect(() => {
    if (open === null) return;
    const found = templates.find((t) => t.id === open);
    if (found) {
      setDraft(structuredClone(found));
      setSaved(false);
    }
  }, [open, templates]);

  /**
   * A new type comes into being as a draft, not as a file.
   *
   * It used to be saved immediately on creation, and the name was then asked
   * for again in a second field — two fields for the same thing, and the upper
   * one without effect. Now there is only the field in the editor, and it is
   * saved only at the press of a button.
   */
  const create = (group: boolean) => {
    setDraft({
      id: "",
      name: "",
      icon: group ? "🧩" : "📄",
      supertype: "",
      group,
      rules: [],
      description: "",
      properties: [],
      infobox: { align: "none", width: "auto", show: [] },
      auth: null,
      namePattern: "%%input%%",
      page: "",
      blocks: [],
    });
    setOpen(null);
    setSaved(false);
  };

  const save = () => {
    if (!draft) return;
    const name = draft.name.trim();
    if (!name) {
      onError(draft.group ? "The group needs a name." : "The type needs a name.");
      return;
    }
    // The name may occur only once: references in the text point at it
    // (`type=Place`), and two types of the same name could not be told apart
    // there. The id behind it is untouched by this — it never changes, not
    // even on a rename.
    const duplicate = templates.some(
      (t) => t.id !== draft.id && t.name.trim().toLowerCase() === name.toLowerCase(),
    );
    if (duplicate) {
      onError(`“${name}” already exists — references could not tell the two apart.`);
      return;
    }

    api
      .templateSave(world.id, { ...draft, name })
      .then((all) => {
        onChanged(all);
        // A new type has a new page, a renamed one a renamed page.
        onArticlesChanged();
        // The id comes from Rust: a new type gets it there.
        const stored = all.find((t) => t.name === name);
        if (stored) {
          setDraft(stored);
          setOpen(stored.id);
        }
        setSaved(true);
        setTimeout(() => setSaved(false), 2000);
      })
      .catch(onError);
  };

  const [remove, setRemove] = useState<Template | null>(null);
  const [unfolded, setUnfolded] = useState<Set<string>>(new Set());

  const back = () => {
    setDraft(null);
    setOpen(null);
  };

  if (draft)
    return (
      <TemplateEditor
        world={world}
        draft={draft}
        templates={templates}
        articles={articles}
        onChange={setDraft}
        onSave={save}
        onBack={back}
        onArticlesChanged={onArticlesChanged}
        onOpen={onOpen}
        onError={onError}
        saved={saved}
      />
    );

  // A subtype sits inside its supertype, folded away until opened — a
  // character opens to show the NPC. A type whose supertype is gone stands
  // at the top. By name within each level, so things can be found again.
  const byName = (list: Template[]) => [...list].sort((a, b) => a.name.localeCompare(b.name));
  const types = realTypes(templates);
  const groups = byName(templates.filter((t) => t.group));
  const roots = byName(types.filter((t) => !t.supertype || !findType(templates, t.supertype)));
  const toggle = (id: string) =>
    setUnfolded((all) => {
      const next = new Set(all);
      if (!next.delete(id)) next.add(id);
      return next;
    });

  const row = (t: Template, seen: Set<string>): React.ReactNode => {
    // A supertype chain that runs in a circle ends here, not in the stack.
    if (seen.has(t.id)) return null;
    const inner = new Set(seen).add(t.id);
    const children = byName(subtypes(templates, t.id));
    const isUnfolded = unfolded.has(t.id);
    const count = articles.filter((a) => isA(templates, a.articleType, t.id)).length;
    return (
      <li key={t.id}>
        <div className="type-row">
          <button
            className="type-fold ghost icon compact"
            disabled={children.length === 0}
            aria-label={isUnfolded ? `Fold ${t.name}` : `Unfold ${t.name}`}
            onClick={() => toggle(t.id)}
          >
            {children.length === 0 ? "" : isUnfolded ? "▾" : "▸"}
          </button>
          <button className="type-open" onClick={() => setOpen(t.id)} title={t.description || undefined}>
            <span className="type-icon">{t.icon || "📄"}</span>
            <strong>{t.name}</strong>
            <small className="small type-counts">
              {count} {count === 1 ? "document" : "documents"} · {t.properties.length}{" "}
              {t.properties.length === 1 ? "property" : "properties"} · {t.blocks.length}{" "}
              {t.blocks.length === 1 ? "block" : "blocks"}
            </small>
          </button>
          <DeleteButton what={t.name} onClick={() => setRemove(t)} />
        </div>
        {isUnfolded && children.length > 0 && (
          <ul className="type-tree">{children.map((c) => row(c, inner))}</ul>
        )}
      </li>
    );
  };

  return (
    <div className="type-page">
      <div className="line between">
        <h2>Types</h2>
        <button className="primary" onClick={() => create(false)}>
          ＋ New
        </button>
      </div>

      {types.length === 0 ? (
        <p className="small empty">No type yet.</p>
      ) : (
        <ul className="type-tree top">{roots.map((t) => row(t, new Set()))}</ul>
      )}

      {/* Property groups: properties several types take in — the abilities
          of characters, monsters and feats alike. */}
      <div className="line between groups-head">
        <h3>Property groups</h3>
        <button onClick={() => create(true)}>＋ Group</button>
      </div>
      {groups.length === 0 ? (
        <p className="small empty">No group yet.</p>
      ) : (
        <ul className="type-tree top">
          {groups.map((g) => {
            const users = usersOf(templates, g.id).filter((t) => !t.group);
            return (
              <li key={g.id}>
                <div className="type-row">
                  <button className="type-fold ghost icon compact" disabled aria-hidden="true" tabIndex={-1} />
                  <button className="type-open" onClick={() => setOpen(g.id)} title={g.description || undefined}>
                    <span className="type-icon">{g.icon || "🧩"}</span>
                    <strong>{g.name}</strong>
                    <small className="small type-counts">
                      {g.properties.length} {g.properties.length === 1 ? "property" : "properties"}
                      {users.length > 0 && ` · in ${users.map((u) => u.name).join(", ")}`}
                    </small>
                  </button>
                  <DeleteButton what={g.name} onClick={() => setRemove(g)} />
                </div>
              </li>
            );
          })}
        </ul>
      )}

      {remove && (
        <DeleteDialog
          title={`Delete ${remove.icon || "📄"} ${remove.name}`}
          name={remove.name}
          warning={
            remove.group
              ? "The group goes with its properties."
              : "The template goes — with every field and block you put together in it. Documents of this type stay, but have no template any more, and references to it offer nothing."
          }
          onCancel={() => setRemove(null)}
          onDelete={() => {
            api
              .templateDelete(world.id, remove.id)
              .then((remaining) => {
                setRemove(null);
                onChanged(remaining);
              })
              .catch(onError);
          }}
        />
      )}

    </div>
  );
}

function InfoArea({
  world,
  onChanged,
  onError,
}: {
  world: WorldInfo;
  onChanged: () => void;
  onError: (e: unknown) => void;
}) {
  const [tokenOk, setTokenOk] = useState<boolean | null>(null);

  useEffect(() => {
    if (world.isGm) api.worldTokenCheck(world.id).then(setTokenOk).catch(() => setTokenOk(false));
  }, [world.id, world.isGm]);

  return (
    <div className="type-page">
      <h2>{world.name}</h2>
      {world.isGm && (
        <div className="line">
          <IconPicker
            value={world.icon}
            onChange={(icon) =>
              api.worldIconSet(world.id, icon).then(onChanged).catch(onError)
            }
            large
          />
          <span className="small">The world's icon — it shows in the bar and in the main menu.</span>
        </div>
      )}
      <p className="small">
        {world.isGm
          ? "You are the GM of this world."
          : "You got this world from your GM."}
      </p>

      <dl className="keys wide">
        <dt>Name</dt>
        <dd>{world.name}</dd>
        <dt>Id</dt>
        <dd>
          <code className="wrap">{world.id}</code>
        </dd>
        <dt>Public world key</dt>
        <dd>
          <code className="wrap">{world.publicKey}</code>
        </dd>
        <dt>Folder on this device</dt>
        <dd>
          <code className="wrap">{world.folder}</code>
        </dd>
        <dt>Articles</dt>
        <dd>{world.articleCount}</dd>
      </dl>

      {world.isGm && (
        <p className={tokenOk === false ? "warning" : "small"}>
          {tokenOk === false
            ? "The world token is missing or does not match. Without it this world is lost."
            : "The world token is stored in your profile. Back up both your profile and your worlds — no server can replace lost keys."}
        </p>
      )}
    </div>
  );
}

/** The attributes every member has anyway — not worth a mention. */
const PLAIN = ["gm", "players", "all"];


/**
 * What a player is assigned to: documents (their character), whole types and
 * attributes. A part whose view says `self` opens to whoever is assigned to
 * its document or its type.
 */
function Assigned({
  world,
  templates,
  articles,
  chosen,
  onAdd,
  onRemove,
}: {
  world: WorldInfo;
  templates: Template[];
  articles: ArticleView[];
  chosen: string[];
  onAdd: (a: string) => void;
  onRemove: (a: string) => void;
}) {
  const types = new Map(templates.map((t) => [t.id, t]));
  const items: Pickable[] = [
    ...articles
      .filter((a) => types.has(a.articleType))
      .map((a) => ({
        id: `doc:${a.id.replace(/^art:/, "")}`,
        name: a.name,
        icon: types.get(a.articleType)?.icon,
        hint: types.get(a.articleType)?.name,
      })),
    ...templates.map((t) => ({ id: typeAttribute(t.id), name: `All ${t.name}`, icon: t.icon, hint: "type" })),
    ...world.attributes
      .filter((a) => !PLAIN.includes(a) && !a.startsWith("doc:") && !a.startsWith("type:"))
      .map((a) => ({ id: a, name: a })),
  ];
  return (
    <div className="assigned">
      <SearchPick items={items} chosen={chosen} onAdd={onAdd} onRemove={onRemove} placeholder="Assign…" />
    </div>
  );
}

function PlayersArea({
  world,
  me,
  templates,
  articles,
  onChanged,
  onOpen,
  onError,
}: {
  world: WorldInfo;
  me: IdentityInfo;
  templates: Template[];
  articles: ArticleView[];
  onChanged: () => void;
  /** Opens a document — the options close for it. */
  onOpen: (id: string) => void;
  onError: (e: unknown) => void;
}) {
  const [requests, setRequests] = useState<JoinRequest[]>([]);
  const [waiting, setWaiting] = useState<WaitingVersion[]>([]);
  const [id, setId] = useState("");
  const [remove, setRemove] = useState<{ id: string; name: string } | null>(null);
  const [handover, setHandover] = useState<MemberInfo | null>(null);

  // `world.requests` changes when someone new asks — then the list is read again.
  useEffect(() => {
    if (!world.isGm) return;
    api.joinRequests(world.id).then(setRequests).catch(onError);
  }, [world.id, world.isGm, world.requests]);
  // `world.waiting` changes when a version comes in or is decided.
  useEffect(() => {
    if (!world.isGm) return;
    api.waitingVersions(world.id).then(setWaiting).catch(onError);
  }, [world.id, world.isGm, world.waiting]);

  const run = async (step: Promise<unknown>) => {
    try {
      await step;
      onChanged();
    } catch (e) {
      onError(e);
    }
  };

  const invite = () => run(api.memberInvite(world.id, id.trim()).then(() => setId("")));

  const admit = async () => {
    try {
      const path = await open({
        multiple: false,
        filters: [{ name: "Player id", extensions: ["awid", "json"] }],
      });
      if (typeof path !== "string") return;
      await api.memberAdd(world.id, path, ["players"]);
      onChanged();
    } catch (e) {
      onError(e);
    }
  };

  const handOver = async (id: string, name: string) => {
    try {
      const path = await save({
        defaultPath: `${world.name} for ${name}.awworld`,
        filters: [{ name: "World file", extensions: ["awworld"] }],
      });
      if (!path) return;
      await api.worldShare(world.id, id, path);
    } catch (e) {
      onError(e);
    }
  };

  if (!world.isGm)
    return (
      <div className="type-page">
        <h2>Players</h2>
        <ul className="members large">
          <li>
            <div>
              <strong>{me.name}</strong> <span className="role-badge player">Player</span>
              <small className="small">{world.attributes.filter((a) => !PLAIN.includes(a)).join(", ")}</small>
            </div>
          </li>
        </ul>
      </div>
    );

  return (
    <div className="type-page">
      <h2>Players</h2>

      <label className="switch-row">
        <input
          type="checkbox"
          checked={world.moderated}
          onChange={(e) => run(api.worldModerationSet(world.id, e.target.checked))}
        />
        <span>
          <strong>Review changes</strong>
          <small className="small">What players write waits for you before the others see it.</small>
        </span>
      </label>

      {waiting.length > 0 && (
        <>
          <h3>Waiting for review</h3>
          <ul className="members large">
            {waiting.map((w) => (
              <li key={w.session}>
                <div>
                  <strong>{w.name}</strong>
                  <small className="small">
                    {w.author} · {when(w.to)}
                  </small>
                </div>
                <button className="compact" onClick={() => onOpen(w.article)}>
                  Open
                </button>
              </li>
            ))}
          </ul>
        </>
      )}

      {requests.length > 0 && (
        <>
          <h3>Asking to join</h3>
          <ul className="members large requests">
            {requests.map((r) => (
              <li key={r.id}>
                <div>
                  <strong>{r.name}</strong>
                  <small className="small">{r.fingerprint}</small>
                </div>
                <div className="line">
                  <button className="primary compact" onClick={() => run(api.joinAnswer(world.id, r.id, true))}>
                    Accept
                  </button>
                  <button className="compact" onClick={() => run(api.joinAnswer(world.id, r.id, false))}>Decline</button>
                </div>
              </li>
            ))}
          </ul>
        </>
      )}

      <ul className="members large">
        {world.members.map((m) => {
          const extra = m.attributes.filter((a) => !PLAIN.includes(a));
          const assign = (fresh: string[]) => run(api.memberAttributesSet(world.id, m.id, fresh));
          return (
            <li key={m.id}>
              <div>
                <strong>{m.name}</strong>{" "}
                <span className={`role-badge ${m.role}`}>{m.role === "gm" ? "GM" : "Player"}</span>
                <small className="small">{m.fingerprint}</small>
                {world.isGm && m.role !== "gm" && (
                  <Assigned
                    world={world}
                    templates={templates}
                    articles={articles}
                    chosen={extra}
                    onAdd={(a) => assign([...extra, a])}
                    onRemove={(a) => assign(extra.filter((x) => x !== a))}
                  />
                )}
                {!world.isGm && extra.length > 0 && <small className="small">{extra.join(", ")}</small>}
                {world.handover === m.id && (
                  <small className="accent">Becomes GM as soon as they connect.</small>
                )}
              </div>
              {m.id !== me.id && (
                <div className="line">
                  {world.moderated && (
                    <button
                      className={m.trusted ? "compact on" : "compact"}
                      title={m.trusted ? "Their changes count at once" : "Their changes wait for review"}
                      onClick={() => run(api.memberTrustSet(world.id, m.id, !m.trusted))}
                    >
                      Trusted
                    </button>
                  )}
                  {world.handover === m.id ? (
                    <button className="compact" onClick={() => run(api.gmHandover(world.id, null))}>Keep GM</button>
                  ) : (
                    <button className="compact" onClick={() => setHandover(m)}>Make GM</button>
                  )}
                  <button className="compact" onClick={() => handOver(m.id, m.name)}>World file</button>
                  <DeleteButton what={m.name} onClick={() => setRemove({ id: m.id, name: m.name })} />
                </div>
              )}
            </li>
          );
        })}
        {world.invited.map((i) => (
          <li key={i}>
            <div>
              <code>{i}</code> <span className="role-badge">Invited</span>
            </div>
            <DeleteButton what={i} onClick={() => run(api.memberRemove(world.id, i))} />
          </li>
        ))}
      </ul>

      <h3>Add a player</h3>
      <div className="line">
        <input
          className="grow"
          placeholder="id:…"
          value={id}
          onChange={(e) => setId(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && id.trim() && invite()}
        />
        <button onClick={invite} disabled={!id.trim()}>
          Add
        </button>
        <button onClick={admit}>From file</button>
      </div>

      {remove && (
        <DeleteDialog
          title={`Remove ${remove.name}`}
          name={remove.name}
          warning="What they have stays with them. Nothing new reaches them — their keys turn over."
          onCancel={() => setRemove(null)}
          onDelete={() => {
            const who = remove.id;
            setRemove(null);
            void run(api.memberRemove(world.id, who));
          }}
        />
      )}

      {handover && (
        <HandoverDialog
          world={world}
          member={handover}
          onCancel={() => setHandover(null)}
          onDone={() => {
            setHandover(null);
            onChanged();
          }}
          onError={onError}
        />
      )}
    </div>
  );
}

/**
 * Handing the world over: the member gets all of it — token, keys,
 * members — and this device keeps playing in it as a player. Over the
 * network it happens when they next connect; a file is sealed for them
 * alone, and this device steps down at once.
 */
function HandoverDialog({
  world,
  member,
  onCancel,
  onDone,
  onError,
}: {
  world: WorldInfo;
  member: MemberInfo;
  onCancel: () => void;
  onDone: () => void;
  onError: (e: unknown) => void;
}) {
  const [input, setInput] = useState("");
  const matches = input.trim() === world.name.trim();

  const asFile = async () => {
    try {
      const path = await save({
        defaultPath: `${world.name} — GM for ${member.name}.awworld`,
        filters: [{ name: "World file", extensions: ["awworld"] }],
      });
      if (!path) return;
      await api.gmHandoverFile(world.id, member.id, path);
      onDone();
    } catch (e) {
      onError(e);
    }
  };

  return (
    <div className="dialog-backdrop" onClick={onCancel}>
      <div className="delete-dialog" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
        <h3>
          Make {member.name} the GM
        </h3>
        <p className="small">
          {member.name} gets the whole world. You stay in it as a player and can no longer change
          types or members.
        </p>
        <label>
          Type the world's name to confirm:
          <input
            autoFocus
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder={world.name}
            onKeyDown={(e) => e.key === "Escape" && onCancel()}
          />
        </label>
        <div className="line">
          <button
            className="primary"
            disabled={!matches}
            onClick={() => api.gmHandover(world.id, member.id).then(onDone).catch(onError)}
          >
            Over the network
          </button>
          <button disabled={!matches} onClick={asFile}>
            As a file
          </button>
          <button onClick={onCancel}>Cancel</button>
        </div>
      </div>
    </div>
  );
}

/** Whether this device is on the network at all, and which worlds it offers there. */
function NetworkArea({
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
      <label className="switch-row">
        <input type="checkbox" checked={view.folders} disabled={busy} onChange={(e) => apply(api.networkFolders(e.target.checked))} />
        <span>
          <strong>Peer to peer through shared folders</strong>
          <small className="small">{view.folders ? "on · devices of a world in a cloud folder meet there and connect directly" : "off · shared folders only swap files"}</small>
        </span>
      </label>

      <h3>Servers and hosts</h3>
      <Servers view={view} busy={busy} onSave={(servers) => apply(api.networkServers(servers))} onError={onError} />
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
                  <strong>{serverName(entry)}</strong>
                  <small className="small muted" title={s}>
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
            {entry.waiting && <small className="small muted">A world of yours waits for the server's admin.</small>}
            {managing === s && <ServerAdmin server={s} onError={onError} />}
          </Fragment>
        );
      })}
      <input value={code} disabled={busy} placeholder="Address or code" onChange={(e) => setCode(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} onBlur={add} />
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
  const [deleting, setDeleting] = useState<HeldWorld | null>(null);
  const [gm, setGm] = useState("");
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
  const seen = (p: Person) => (p.last ? `seen ${when(p.last * 1000)}` : "not seen yet");
  return (
    <div className="server-admin">
      <h4>Worlds</h4>
      {state.worlds.length === 0 && <p className="small muted">None yet.</p>}
      {state.worlds.map((w) => (
        <div key={w.world} className="server-row">
          <span className="grow">
            <strong>{w.name}</strong>
            <small className="small muted">
              {[w.gm, w.bytes ? bytes(w.bytes) : "deleted", w.encrypted ? "encrypted" : "unencrypted", w.closed && "closed"].filter(Boolean).join(" · ")}
            </small>
          </span>
          {w.closed ? (
            <>
              <button disabled={busy} onClick={() => void order({ kind: "open", world: w.world })}>Open</button>
              {w.bytes > 0 && <DeleteButton what={w.name} onClick={() => setDeleting(w)} />}
            </>
          ) : (
            <button disabled={busy} onClick={() => void order({ kind: "close", world: w.world })}>Close</button>
          )}
        </div>
      ))}
      <h4>GMs</h4>
      {state.gms.map((p, i) => (
        <div key={p.id} className="server-row">
          <span className="grow">
            <strong>{p.name || p.id}</strong>
            <small className="small muted">{i === 0 ? "admin" : seen(p)}</small>
          </span>
          {i > 0 && <button disabled={busy} onClick={() => void order({ kind: "refuse", id: p.id })}>Remove</button>}
        </div>
      ))}
      {state.asking.map((p) => (
        <div key={p.id} className="server-row">
          <span className="grow">
            <strong>{p.name || p.id}</strong>
            <small className="small muted">would bring a world</small>
          </span>
          <button disabled={busy} onClick={() => void order({ kind: "allow", id: p.id })}>Allow</button>
          <button disabled={busy} onClick={() => void order({ kind: "refuse", id: p.id })}>Refuse</button>
        </div>
      ))}
      <input
        value={gm}
        disabled={busy}
        placeholder="Allow a GM by id"
        onChange={(e) => setGm(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && gm.trim()) {
            void order({ kind: "allow", id: gm.trim() });
            setGm("");
          }
        }}
      />
      <h4>Users</h4>
      {state.users.map((p) => (
        <div key={p.id} className="server-row">
          <span className="grow">
            <strong>{p.name}</strong>
            <small className="small muted">{p.blocked ? "turned away" : seen(p)}</small>
          </span>
          {p.id !== state.gms[0]?.id &&
            (p.blocked ? (
              <button disabled={busy} onClick={() => void order({ kind: "unblock", id: p.id })}>Let in</button>
            ) : (
              <button disabled={busy} onClick={() => void order({ kind: "block", id: p.id })}>Block</button>
            ))}
        </div>
      ))}
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

