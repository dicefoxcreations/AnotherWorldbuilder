import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  api,
  type ArticleView,
  type IdentityInfo,
  type NetworkView,
  type StorageView,
  type Template,
  type WorldInfo,
} from "../api/client";
import { MainMenu } from "./MainMenu";
import { FAST_MS, pace, SLOW_MS } from "./live";
import { HeaderBar } from "./HeaderBar";
import { HeaderSlot } from "./HeaderTools";
import { ArticleArea } from "../features/wiki/Article";
import { FindBar } from "../shared/ui/FindBar";
import { WhoIsHere } from "./WhoIsHere";
import { QuickOpen } from "../shared/ui/QuickOpen";
import { KeysDialog } from "../shared/ui/KeysDialog";
import { WorldOverview, type Place } from "../features/worlds/WorldOverview";
import { Options } from "../features/settings/Options";
import { StorageSetup } from "../features/settings/Storage";
import { findType, pageOwner } from "../shared/types";
import { NavBar } from "../features/navigation/NavBar";
import { WorldDataContext } from "../features/worlds/WorldData";
import { LinkPreview } from "../features/wiki/LinkPreview";
import { DiceTray } from "../features/dice/DiceTray";
import { announce, isMainWindow, onWorldChanged, resumeOnlineWorld, suspendOnlineWorld } from "../api/changes";
import { wantsApart } from "../shared/apart";
import { listen } from "@tauri-apps/api/event";

/** How many places the trail in the header bar keeps. */
const TRAIL = 10;

const samePlace = (a: Place, b: Place) =>
  a.kind === b.kind &&
  (a.kind === "types" || (a.kind === "list" && b.kind === "list" && a.articleType === b.articleType) || (a.kind === "article" && b.kind === "article" && a.id === b.id));

/** Below this — a phone — the outline lies over the page instead of beside it. */
const NAV_BESIDE = "(min-width: 1000px)";

/**
 * Whether the outline on the left stands open. Beside the page it is this
 * device's choice and stays; over the page — a phone — it opens when asked
 * and closes once one goes somewhere.
 */
function useNavShown(): { shown: boolean; beside: boolean; toggle: () => void; went: () => void } {
  const [beside, setBeside] = useState(() => window.matchMedia(NAV_BESIDE).matches);
  useEffect(() => {
    const query = window.matchMedia(NAV_BESIDE);
    const changed = () => setBeside(query.matches);
    query.addEventListener("change", changed);
    return () => query.removeEventListener("change", changed);
  }, []);
  const [kept, setKept] = useState(() => {
    try {
      return localStorage.getItem("nav:shown") !== "0";
    } catch {
      return true;
    }
  });
  const [over, setOver] = useState(false);
  // Over the page, a click beside it closes it.
  useEffect(() => {
    if (beside || !over) return;
    const click = (e: MouseEvent) => {
      const at = e.target as Element;
      if (!at.closest(".nav-bar, .nav-toggle, .side-panel")) setOver(false);
    };
    document.addEventListener("mousedown", click);
    return () => document.removeEventListener("mousedown", click);
  }, [beside, over]);
  const toggle = () => {
    if (!beside) {
      setOver((o) => !o);
      return;
    }
    setKept((was) => {
      try {
        localStorage.setItem("nav:shown", was ? "0" : "1");
      } catch {
        // Not remembered — it opens and closes all the same.
      }
      return !was;
    });
  };
  return { shown: beside ? kept : over, beside, toggle, went: () => setOver(false) };
}

const LAST_WORLD = "world:last";
function lastWorld(): string | null {
  try {
    return localStorage.getItem(LAST_WORLD);
  } catch {
    return null;
  }
}
function keepLastWorld(id: string | null) {
  try {
    if (id) localStorage.setItem(LAST_WORLD, id);
    else localStorage.removeItem(LAST_WORLD);
  } catch {
    // Not remembered — the app starts on the main menu.
  }
}

export default function App() {
  const [storage, setStorage] = useState<StorageView | null>(null);
  const [me, setMe] = useState<IdentityInfo | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [worlds, setWorlds] = useState<WorldInfo[]>([]);
  const [openWorld, setOpenWorld] = useState<string | null>(null);
  const [templates, setTemplates] = useState<Template[]>([]);
  const [articles, setArticles] = useState<ArticleView[]>([]);
  const [preview, setPreview] = useState<string[] | null>(null);
  const [options, setOptions] = useState(false);
  const [network, setNetwork] = useState<NetworkView | null>(null);
  const [where, setPlace] = useState<Place>({ kind: "types" });
  const [newType, setNewType] = useState<string | null>(null);
  // A document this player waits on, changed in the form it was asked with.
  const [editing, setEditing] = useState<ArticleView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [opening, setOpening] = useState<string | null>(null);
  const [blocked, setBlocked] = useState<{ world: string; error: string } | null>(null);
  // Loading from or sending to a server, as far as it is — told by the native side.
  const [transfer, setTransfer] = useState<Transfer | null>(null);
  useEffect(() => {
    const stop = listen<Transfer>("online-progress", (e) => setTransfer(e.payload.total > 0 ? e.payload : null));
    return () => void stop.then((un) => un());
  }, []);
  const openingRequest = useRef(0);
  const currentWorld = useRef(openWorld);
  currentWorld.current = openWorld;
  const openingWorld = useRef<string | null>(null);
  const [tools, setTools] = useState<HTMLElement | null>(null);
  const nav = useNavShown();

  const report = useCallback((e: unknown) => setError(String(e)), []);
  const enter = useCallback(async (id: string) => {
    const request = ++openingRequest.current;
    openingWorld.current = id;
    setOpening(id);
    try {
      const connected = await api.worldConnect(id);
      if (request !== openingRequest.current) {
        void api.worldDisconnect(id).catch(() => {});
        return;
      }
      setWorlds((old) => [...old.filter((w) => w.id !== id), connected]);
      setBlocked(null);
      setError(null);
      if (currentWorld.current !== id) {
        setArticles([]);
        setTemplates([]);
      }
      setOpenWorld(id);
      setFresh((n) => n + 1);
    } catch (error) {
      if (request === openingRequest.current) report(error);
    } finally {
      if (request === openingRequest.current) {
        openingWorld.current = null;
        setOpening(null);
      }
    }
  }, [report]);

  useEffect(() => {
    const unavailable = (detail: { world: string; error: string }) => {
      suspendOnlineWorld(detail.world);
      if (detail.world === currentWorld.current || detail.world === openingWorld.current) setBlocked(detail);
    };
    // Its server answers again: the world goes on where it was.
    const available = (detail: { world: string }) => {
      resumeOnlineWorld(detail.world);
      setBlocked((b) => (b?.world === detail.world ? null : b));
      if (detail.world === currentWorld.current) setFresh((n) => n + 1);
    };
    const local = (event: Event) => unavailable((event as CustomEvent<{ world: string; error: string }>).detail);
    window.addEventListener("online-unavailable", local);
    let disposed = false;
    let stop: (() => void) | undefined;
    let stopAvailable: (() => void) | undefined;
    listen<{ world: string; error: string }>("online-unavailable", (event) => unavailable(event.payload))
      .then((unlisten) => { if (disposed) unlisten(); else stop = unlisten; }).catch(() => {});
    listen<{ world: string }>("online-available", (event) => available(event.payload))
      .then((unlisten) => { if (disposed) unlisten(); else stopAvailable = unlisten; }).catch(() => {});
    return () => { disposed = true; stop?.(); stopAvailable?.(); window.removeEventListener("online-unavailable", local); };
  }, []);
  const world = useMemo(
    () => worlds.find((w) => w.id === openWorld) ?? null,
    [worlds, openWorld],
  );

  const loadIdentity = useCallback(
    () => api.identityStatus().then(setMe).catch(report),
    [report],
  );

  // First where the data lies — only then who one is.
  useEffect(() => {
    api.storageStatus().then(setStorage).catch(report);
  }, [report]);
  const stored = storage?.state === "ready";
  useEffect(() => {
    if (stored) void loadIdentity().finally(() => setLoaded(true));
  }, [stored, loadIdentity]);

  const loadWorlds = useCallback(async () => {
    try {
      // Asked every round: only what changed may draw the app anew — with
      // thousands of documents a needless round is felt while typing.
      const next = await api.worldsList();
      setWorlds((old) => (JSON.stringify(old) === JSON.stringify(next) ? old : next));
    } catch (e) {
      report(e);
    }
  }, [report]);

  useEffect(() => {
    if (me) void loadWorlds();
  }, [me, loadWorlds]);

  const loadNetwork = useCallback(
    () => api.networkSettings().then(setNetwork).catch(report),
    [report],
  );
  useEffect(() => {
    if (me) void loadNetwork();
  }, [me, loadNetwork]);

  // Keep the network going whenever a profile is open: the local network,
  // and those met through shared folders.
  const [fresh, setFresh] = useState(0);
  const keeping = !!me;
  useEffect(() => {
    // Others can be here over the local network, the internet or a server.
    pace.network = !!network?.running;
    // One window keeps the network going — the main one; the others hear from it.
    if (!keeping || !isMainWindow) return;
    let gone = false;
    let timer: ReturnType<typeof setTimeout>;
    const round = async () => {
      try {
        const r = await api.networkSync();
        if (openWorld && r.updated.includes(openWorld)) setFresh((n) => n + 1);
        for (const w of r.updated) announce(w);
      } catch {
        // The next round tries again.
      }
      void loadWorlds();
      // Writing together, one wants to see it almost as it is typed.
      if (!gone) timer = setTimeout(() => void round(), pace.together ? FAST_MS : SLOW_MS);
    };
    void round();
    return () => {
      gone = true;
      clearTimeout(timer);
    };
  }, [network?.enabled, keeping, openWorld, loadWorlds]);

  const loadTemplates = useCallback(async () => {
    if (!openWorld) return;
    try {
      setTemplates(await api.templatesLoad(openWorld));
    } catch (e) {
      report(e);
    }
  }, [openWorld, report]);

  useEffect(() => {
    void loadTemplates();
  }, [loadTemplates, fresh]);

  // What another window changed in this world, this one loads again.
  useEffect(() => {
    if (!openWorld) return;
    return onWorldChanged(openWorld, () => {
      setFresh((n) => n + 1);
      void loadWorlds();
    });
  }, [openWorld, loadWorlds]);

  // A window opened apart starts in its world, at its place.
  const startAt = useMemo(() => {
    const q = new URLSearchParams(location.search);
    const at = q.get("world");
    if (!at) return null;
    let place: Place | null = null;
    try {
      place = JSON.parse(q.get("place") ?? "null") as Place | null;
    } catch {
      place = null;
    }
    return { world: at, place, part: q.get("part") };
  }, []);
  const [started, setStarted] = useState(false);
  useEffect(() => {
    if (!startAt || started || !worlds.some((w) => w.id === startAt.world)) return;
    setStarted(true);
    void enter(startAt.world);
    if (startAt.place) setPlace(startAt.place);
  }, [startAt, started, worlds, enter]);
  // The app opens where it was left: in the world open last, on this device.
  useEffect(() => {
    if (startAt || started || worlds.length === 0) return;
    setStarted(true);
    const last = lastWorld();
    if (last && worlds.some((w) => w.id === last)) void enter(last);
  }, [startAt, started, worlds, enter]);
  useEffect(() => {
    if (!startAt && started) keepLastWorld(openWorld);
  }, [startAt, started, openWorld]);
  // … and at a part of it, once that stands there.
  const [scrolled, setScrolled] = useState(false);
  useEffect(() => {
    if (!startAt?.part || scrolled || articles.length === 0) return;
    const timer = setTimeout(() => {
      const title = [...document.querySelectorAll(".block-title")].find((t) => t.textContent?.trim() === startAt.part);
      const block = title?.closest(".block");
      if (!block) return;
      setScrolled(true);
      block.scrollIntoView({ block: "start" });
      block.classList.add("flash");
    }, 400);
    return () => clearTimeout(timer);
  }, [startAt, scrolled, articles]);

  // Only the latest asking counts: a full view still on its way must not
  // land over the preview asked for after it. And whoever kept an older
  // `loadView` — a page's "changed, load again" — still loads as previewed now.
  const asked = useRef(0);
  const previewNow = useRef(preview);
  previewNow.current = preview;
  const loadView = useCallback(async () => {
    if (!openWorld) return;
    const mine = ++asked.current;
    try {
      const view = await api.view(openWorld, previewNow.current ?? undefined);
      if (mine === asked.current) setArticles(view);
    } catch (e) {
      if (mine === asked.current) report(e);
    }
  }, [openWorld, preview, report]);

  useEffect(() => {
    void loadView();
  }, [loadView, fresh]);

  /**
   * Where one stands. A page is an article like any other, but it stands
   * where its owner stands: the world's page is the world, a type's page the
   * type. So a link to it and a click on the type lead to the same place.
   */
  const place = useMemo((): Place => {
    if (where.kind !== "article" || !world) return where;
    const a = articles.find((x) => x.id === where.id);
    const owner = a && pageOwner(templates, world.home, a);
    if (owner?.kind === "world") return { kind: "types" };
    if (owner?.kind === "type") return { kind: "list", articleType: owner.type.id };
    return where;
  }, [where, world, articles, templates]);

  /**
   * The way one came, for the header bar: every place in the order it was
   * reached. Coming back to one cuts off what followed it, so the trail never
   * loops; the oldest fall off at the front.
   */
  const [trail, setTrail] = useState<Place[]>([]);
  // Where Back came from, for Forward — forgotten once one goes elsewhere.
  const ahead = useRef<Place[]>([]);
  const stepping = useRef(false);
  useEffect(() => {
    if (!stepping.current) ahead.current = [];
    stepping.current = false;
  }, [place]);
  useEffect(() => {
    if (!world) return;
    setTrail((was) => {
      const at = was.findIndex((p) => samePlace(p, place));
      if (at >= 0) return at === was.length - 1 ? was : was.slice(0, at + 1);
      return [...was, place].slice(-TRAIL);
    });
  }, [world, place]);

  const crumbs = useMemo(() => {
    if (!world) return [];
    const path: { label: string; icon?: string; onClick?: () => void }[] = [];
    const shown = trail.filter((p) => p.kind !== "article" || articles.some((a) => a.id === p.id));
    shown.forEach((p, i) => {
      const last = i === shown.length - 1 && !newType;
      const onClick = last ? undefined : () => setPlace(p);
      if (p.kind === "types") path.push({ label: world.name, icon: world.icon, onClick });
      else if (p.kind === "list") {
        const t = findType(templates, p.articleType);
        path.push({ label: t?.name ?? p.articleType, icon: t?.icon, onClick });
      } else path.push({ label: articles.find((a) => a.id === p.id)?.name ?? "???", onClick });
    });
    if (newType) path.push({ label: "New" });
    return path;
  }, [world, trail, templates, articles, newType]);

  /** A place in a window of its own — titled by what it shows. */
  const openApart = useCallback(
    (p: Place, part?: string) => {
      if (!openWorld) return;
      const title =
        p.kind === "article" ? articles.find((a) => a.id === p.id)?.name : p.kind === "list" ? findType(templates, p.articleType)?.name : world?.name;
      api.windowOpen(openWorld, p, title ?? "", part).catch(report);
    },
    [openWorld, articles, templates, world, report],
  );
  /** Where a click leads: here — or, with Ctrl or the middle button, apart. */
  const go = useCallback((p: Place) => (wantsApart() ? openApart(p) : setPlace(p)), [openApart]);

  const leave = () => {
    const disconnect = openingWorld.current ?? openWorld;
    ++openingRequest.current;
    openingWorld.current = null;
    setOpening(null);
    setBlocked(null);
    if (disconnect) void api.worldDisconnect(disconnect).catch(() => {});
    asked.current++;
    setOpenWorld(null);
    setPreview(null);
    setOptions(false);
    setArticles([]);
    setTemplates([]);
    setPlace({ kind: "types" });
    setTrail([]);
    setNewType(null);
  };

  /**
   * The back button goes the way one came. Where the trail begins, it goes
   * up the outline, not straight out of the world.
   */
  const back = () => {
    if (newType) {
      setNewType(null);
      return;
    }
    if (trail.length > 1) {
      stepping.current = true;
      ahead.current.push(place);
      setPlace(trail[trail.length - 2]);
      return;
    }
    if (place.kind === "article") {
      const found = articles.find((a) => a.id === place.id);
      setPlace(found?.articleType ? { kind: "list", articleType: found.articleType } : { kind: "types" });
      return;
    }
    if (place.kind === "list") {
      setPlace({ kind: "types" });
      return;
    }
    leave();
  };
  /** Forward: where Back came from. */
  const forward = () => {
    const p = ahead.current.pop();
    if (!p) return;
    stepping.current = true;
    setPlace(p);
  };
  // Alt+← and Alt+→, and the mouse's side buttons — in a world, not out of it.
  const steps = useRef({ back, forward, inWorld: !!world });
  steps.current = { back, forward, inWorld: !!world };
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (!e.altKey || e.ctrlKey || e.metaKey || !steps.current.inWorld) return;
      if (e.key === "ArrowLeft") (e.preventDefault(), steps.current.back());
      if (e.key === "ArrowRight") (e.preventDefault(), steps.current.forward());
    };
    const mouse = (e: MouseEvent) => {
      if (!steps.current.inWorld) return;
      if (e.button === 3) (e.preventDefault(), steps.current.back());
      if (e.button === 4) (e.preventDefault(), steps.current.forward());
    };
    document.addEventListener("keydown", key);
    document.addEventListener("mouseup", mouse);
    return () => {
      document.removeEventListener("keydown", key);
      document.removeEventListener("mouseup", mouse);
    };
  }, []);

  if (storage && !stored) return <StorageSetup status={storage} onDone={setStorage} />;
  if (!loaded) return <main className="middle">Loading …</main>;
  if (!me) return <Setup onDone={setMe} onError={report} />;

  return (
    <HeaderSlot.Provider value={tools}>
    {/* The dice belong to the open world — its 🎲 stands in the header bar. */}
    <WorldDice world={world}>
    <div inert={!!blocked || !!opening} className={world && nav.shown ? (nav.beside ? "frame with-nav" : "frame nav-over") : "frame"}>
      {/* The header bar stands on every page — the avatar on the far right
          is reachable from everywhere, the back button only where there is
          a way back. */}
      <HeaderBar
        me={me}
        world={world}
        preview={preview}
        onPreview={setPreview}
        onBack={back}
        atStart={!newType && trail.length <= 1 && place.kind === "types"}
        nav={world ? { shown: nav.shown, onToggle: nav.toggle } : undefined}
        onChanged={() => {
          void loadWorlds();
          void loadIdentity();
        }}
        onError={report}
        onOptions={() => setOptions(true)}
        // Requests waiting: this world's, or on the main menu all of them.
        waiting={(world ? [world] : worlds).reduce((n, w) => n + w.requests + (w.isGm ? w.waiting : 0), 0)}
        crumbs={crumbs}
        toolsRef={setTools}
        names={{ articles, templates }}
        people={
          world && (
            <WhoIsHere world={world.id} article={place.kind === "article" ? place.id : ""} articles={articles} onOpen={(id) => go({ kind: "article", id })} />
          )
        }
      />

      {error && !blocked && (
        <div className="error" role="alert">
          {error}
          <button className="compact" onClick={() => setError(null)}>close</button>
        </div>
      )}

      {!world ? (
        <MainMenu
          worlds={worlds}
          network={network}
          onOpen={(id) => void enter(id)}
          onChanged={loadWorlds}
          onNetwork={setNetwork}
          onSettings={() => setOptions(true)}
          onError={report}
        />
      ) : (
        <WorldDataContext.Provider
          value={{ world, templates, articles, onPlace: go, onChanged: () => void loadView(), onError: report, onApart: openApart, onTemplates: setTemplates, onNew: (t: string) => { setOptions(false); setEditing(null); setNewType(t); }, onEditPending: (a: ArticleView) => { setEditing(a); setNewType(a.articleType); }, previewing: preview !== null }}
        >
        <LinkPreview />
        <FindBar />
        <QuickOpen world={world.id} articles={articles} templates={templates} onPlace={go} />
        {nav.shown && (
          <NavBar
            world={world}
            templates={templates}
            articles={articles}
            place={place}
            onPlace={(p) => {
              setNewType(null);
              nav.went();
              go(p);
            }}
            editable={world.isGm && preview === null}
            previewing={preview !== null}
            fresh={fresh}
            onError={report}
          />
        )}
        <main className="world-body">
          {newType ? (
            <ArticleArea
              world={world}
              templates={templates}
              articles={articles}
              articleType={newType}
              pending={editing ?? undefined}
              key={editing?.id ?? newType}
              onDone={(id) => {
                setNewType(null);
                setEditing(null);
                if (id) setPlace({ kind: "article", id });
                void loadView();
                void loadWorlds();
              }}
              onCancel={() => {
                setNewType(null);
                setEditing(null);
              }}
              onError={report}
            />
          ) : (
            <WorldOverview
              world={world}
              templates={templates}
              articles={articles}
              place={place}
              onPlace={go}
              onNew={setNewType}
              // The templates and worlds too: a page's name is its type's or
              // its world's.
              onChanged={() => {
                void loadView();
                void loadTemplates();
                void loadWorlds();
              }}
              onError={report}
            />
          )}
        </main>
        </WorldDataContext.Provider>
      )}

      {options && (
        // The world's documents for what options show of them — a calendar's preview.
        <WorldDataContext.Provider value={world ? { world, templates, articles, onPlace: go, onChanged: () => void loadView(), onError: report, onApart: openApart, onTemplates: setTemplates, onNew: (t: string) => { setOptions(false); setEditing(null); setNewType(t); }, onEditPending: (a: ArticleView) => { setEditing(a); setNewType(a.articleType); }, previewing: preview !== null } : null}>
        <Options
          world={world}
          me={me}
          templates={templates}
          articles={articles}
          onChanged={setTemplates}
          onArticlesChanged={() => void loadView()}
          onWorldChanged={loadWorlds}
          onOpen={(id) => {
            setOptions(false);
            setNewType(null);
            setPlace({ kind: "article", id });
            void loadTemplates();
          }}
          onNetworkChanged={setNetwork}
          onLeave={world ? leave : undefined}
          onStorageChanged={() => {
            // Other folders: perhaps another profile, other worlds.
            setOpenWorld(null);
            void loadIdentity();
            void loadWorlds();
            void loadNetwork();
          }}
          onClose={() => {
            setOptions(false);
            void loadTemplates();
            void loadNetwork();
          }}
          onError={report}
        />
        </WorldDataContext.Provider>
      )}
    </div>
    {transfer && !opening && transfer.world === openWorld && <div className="transfer-note" role="status">{transferText(transfer)}</div>}
    {(opening || blocked) && <div className="dialog-backdrop online-status" role="alertdialog" aria-modal="true" aria-labelledby="online-status-title">
      <section className="card narrow">
        <h2 id="online-status-title">{opening ? "Opening world …" : "Server world unavailable"}</h2>
        <p>{opening ? (transfer?.world === opening ? transferText(transfer) : "Confirming access and loading the current world.") : blocked?.error.replace(/^\[online\]\s*/, "")}</p>
        {!opening && <p>Editing is paused. Changes without a server acknowledgement are not confirmed saved. Reconnect to continue using this server world.</p>}
        <div className="line">
          {!opening && blocked && <button className="primary" onClick={() => void enter(blocked.world)}>Reconnect</button>}
          <button onClick={leave}>Back to worlds</button>
        </div>
      </section>
    </div>}
    </WorldDice>
    </HeaderSlot.Provider>
  );
}

function Setup({
  onDone,
  onError,
}: {
  onDone: (i: IdentityInfo) => void;
  onError: (e: unknown) => void;
}) {
  const [name, setName] = useState("");
  const [restore, setRestore] = useState(false);
  return (
    <main className="middle">
      <div className="card narrow">
        <h1>Your name</h1>
        <p>Others in your worlds see this name. You can change it later.</p>
        <p className="small">
          Been here before, on this or another device?{" "}
          <button className="link-button" onClick={() => setRestore(true)}>
            Restore your keys from a backup
          </button>
        </p>
        {restore && <KeysDialog mode="restore" onClose={() => setRestore(false)} onRestored={onDone} />}
        <form
          onSubmit={(e) => {
            e.preventDefault();
            api.identityCreate(name.trim()).then(onDone).catch(onError);
          }}
        >
          <input
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. Marla"
          />
          <button className="primary" type="submit" disabled={!name.trim()}>
            Continue
          </button>
        </form>
      </div>
    </main>
  );
}

/** The open world's dice around everything — none outside a world. */
function WorldDice({ world, children }: { world: WorldInfo | null; children: React.ReactNode }) {
  return world ? (
    <DiceTray key={world.id} world={world}>
      {children}
    </DiceTray>
  ) : (
    <>{children}</>
  );
}

/** Loading from or sending to a server: `done` of `total`. */
type Transfer = { world: string; what: "loading" | "sending"; done: number; total: number };

function transferText(t: Transfer): string {
  const share = Math.min(100, Math.floor((t.done * 100) / t.total));
  return `${t.what === "sending" ? "Sending to the server" : "Loading from the server"} · ${share} %`;
}
