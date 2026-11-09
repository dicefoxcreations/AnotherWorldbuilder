import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Icon } from "../../shared/ui/Icons";
import { api, type ArticleView, type Place, type Template, type WorldInfo } from "../../api/client";
import { ArticlePage, DeleteDocument } from "./ArticlePage";
import { VersionsPanel } from "./Versions";
import { HeaderTools } from "../../app/HeaderTools";
import { useSideBar } from "../../shared/ui/SideBar";
import { pageOwner } from "../../shared/types";
import { comboOf } from "../../shared/Shortcuts";
import { useWorldData } from "../worlds/WorldData";
import { ComparePages, useNews, useVersion } from "./DocumentCompare";
import { newSession, type Layout, type Opened, type Side } from "./EditorBasics";
import { DocumentEditor } from "./DocumentEditor";

/**
 * A document: read it, or edit it — as code, visually, or both side by side.
 *
 * The switches, as in common Markdown editors — they stand in the header bar:
 *
 * - **view ↔ edit** — reading the finished page, or changing it. Reading is
 *   always the page alone, in one column: code is something one writes.
 * - while editing, **code ↔ both ↔ visual** — side by side the code sits
 *   left, the visual side right.
 *
 * The code is the document as text: properties on top, the sections below
 * (see `aw_model::document`). The visual side edits the same document with
 * fields — a dropdown in the infobox changes the property in the code, and a
 * line typed in the code changes the dropdown. What may be saved, Rust
 * decides: text needs the right to edit, access needs the game master.
 *
 * There is no saving by hand: what is written is kept as it is typed. From
 * going into edit mode until leaving it is one editing session — one
 * version, see `VersionsPanel`.
 */

type Mode = "view" | "edit";
type Prefs = { mode: Mode; layout: Layout; side: Side };

const PREFS_KEY = "aw.document.view";
const DEFAULT_PREFS: Prefs = { mode: "view", layout: "single", side: "visual" };
/** The switches, remembered on this device for every document. */
function usePrefs(): [Prefs, (p: Partial<Prefs>) => void] {
  const [prefs, setPrefs] = useState<Prefs>(() => {
    try {
      return { ...DEFAULT_PREFS, ...JSON.parse(localStorage.getItem(PREFS_KEY) ?? "{}") };
    } catch {
      return DEFAULT_PREFS;
    }
  });
  const update = (p: Partial<Prefs>) =>
    setPrefs((old) => {
      const next = { ...old, ...p };
      try {
        localStorage.setItem(PREFS_KEY, JSON.stringify(next));
      } catch {
        // Without storage the switches simply are not remembered.
      }
      return next;
    });
  return [prefs, update];
}

/** Documents just made: they open for editing once, whatever the switch says. */
const freshlyMade = new Set<string>();
export const editOnOpen = (id: string) => void freshlyMade.add(id);

/** How tall the document's page is — what it holds, not the room it fills. */
function contentHeight(): number {
  const parts = [...document.querySelectorAll(".document-main > *")];
  return parts.reduce((most, p) => Math.max(most, p.getBoundingClientRect().bottom + window.scrollY), 0);
}

export function DocumentPage({
  world,
  article,
  templates,
  all,
  onPlace,
  onChanged,
  onError,
  actions,
}: {
  world: WorldInfo;
  article: ArticleView;
  templates: Template[];
  all: ArticleView[];
  onPlace: (o: Place) => void;
  onChanged: () => void;
  onError: (e: unknown) => void;
  /** What the owner of a page adds to its head — see `ArticlePage`. */
  actions?: React.ReactNode;
}) {
  const [prefs, setPrefs] = usePrefs();
  const [history, setHistory] = useState(false);
  const [chosen, setChosen] = useState<string | null>(null);
  const news = useNews(world.id, article.id, article);
  const version = useVersion(world.id, article.id, chosen, article, onError);
  // Going into edit mode starts a session; leaving it ends it.
  const [session, setSession] = useState(newSession);
  // The editor holds the whole document — a preview only reads, or it would
  // show what the preview hides.
  const previewing = !!useWorldData()?.previewing;
  const canEdit = !previewing && (world.isGm || article.layers.some((l) => l.mayEdit));
  // Previewing: whether the one previewed could write here — shown, not pressed.
  const theyMayEdit = previewing && article.layers.some((l) => l.mayEdit);
  // Just made: it opens for writing.
  const [justMade, setJustMade] = useState(() => freshlyMade.delete(article.id));
  const mode: Mode = canEdit ? (justMade ? "edit" : prefs.mode) : "view";

  const page = (
    <ArticlePage
      world={world}
      article={article}
      templates={templates}
      all={all}
      onPlace={onPlace}
      actions={actions}
    />
  );

  // Opened before the switch: the editor stands there whole at once, in the
  // place of the page — nothing between them that could move the page.
  const [opened, setOpened] = useState<{ id: string; doc: Opened } | null>(null);
  const opening = useRef(false);
  // Both sides draw in steps — the editor its surfaces after its text, the
  // page its text after its head. Meanwhile the page keeps its height, or
  // the browser would scroll a shorter page up; as soon as the other side
  // is as tall, it lets go.
  const held = useRef(0);
  // Which holding is the latest: letting go of an earlier one leaves it be.
  const holding = useRef(0);
  const hold = () => {
    holding.current++;
    held.current = contentHeight();
    // On the body: WebKit lets a page scroll by it, not by the root's own height.
    document.body.style.minHeight = `${document.documentElement.scrollHeight}px`;
  };
  useLayoutEffect(() => {
    const need = held.current;
    if (!need) return;
    held.current = 0;
    // Looked at each frame, at most two seconds: its parts come and go
    // meanwhile — let go once it stood whole two frames in a row.
    const until = Date.now() + 2000;
    let whole = 0;
    let frame = 0;
    const mine = holding.current;
    const release = () => {
      cancelAnimationFrame(frame);
      if (holding.current === mine) document.body.style.minHeight = "";
    };
    const look = () => {
      whole = contentHeight() >= need - 1 ? whole + 1 : 0;
      if (whole >= 2 || Date.now() > until) release();
      else frame = requestAnimationFrame(look);
    };
    frame = requestAnimationFrame(look);
    return release;
  }, [mode]);
  const switchMode = () => {
    if (mode === "edit") {
      setOpened(null);
      hold();
      setPrefs({ mode: "view" });
      return;
    }
    if (opening.current) return;
    opening.current = true;
    api
      .documentOpen(world.id, article.id)
      .then((doc) => {
        setOpened({ id: article.id, doc });
        setSession(newSession());
        setJustMade(false);
        hold();
        setPrefs({ mode: "edit" });
      })
      .catch(onError)
      .finally(() => (opening.current = false));
  };
  // Ctrl+E: editing and reading, as the ✎ button.
  const latestSwitch = useRef(switchMode);
  latestSwitch.current = canEdit ? switchMode : () => {};
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (comboOf(e) !== "ctrl+e") return;
      e.preventDefault();
      latestSwitch.current();
    };
    document.addEventListener("keydown", key);
    return () => document.removeEventListener("keydown", key);
  }, []);

  const modeSwitch = theyMayEdit ? (
    <button className="ghost icon round" disabled title="They may edit this" data-label="They may edit">
      <Icon name="edit" />
    </button>
  ) : canEdit && (
    <button
      className={mode === "edit" ? "ghost icon round on" : "ghost icon round"}
      onClick={switchMode}
      title={mode === "edit" ? "Back to reading (Ctrl+E)" : "Edit (Ctrl+E)"}
      data-label={mode === "edit" ? "Back to reading" : "Edit"}
    >
      <Icon name="edit" />
    </button>
  );
  // This document in a window of its own, to lay beside another.
  const onApart = useWorldData()?.onApart;
  const apartButton = onApart && (
    <button className="ghost icon round apart-button" onClick={() => onApart({ kind: "article", id: article.id })} title="Open in a new window (Ctrl+click on a link does too)">
      <Icon name="apart" />
    </button>
  );
  // The versions share the side bar with a graph's roles: one at a time.
  useSideBar(`versions:${article.id}`, history, () => {
    setHistory(false);
    setChosen(null);
  });
  // What came in since one last looked is marked in the list while it is open.
  const [fresh, setFresh] = useState<string[]>([]);
  const historyButton = (
    <button
      className={history ? "ghost icon round on" : "ghost icon round"}
      onClick={() => {
        if (!history) {
          setFresh(news.items.map((v) => v.session));
          news.dismiss();
        } else setChosen(null);
        setHistory(!history);
      }}
      title={news.items.length > 0 ? "New versions came in — written offline, or waiting for review" : "Versions of this document"}
    >
      <Icon name="history" />
      {news.items.length > 0 && <span className="badge">{news.items.length}</span>}
    </button>
  );
  const act = (step: Promise<unknown>) =>
    step
      .then(() => {
        setChosen(null);
        onChanged();
      })
      .catch(onError);
  const versions = history && (
    <VersionsPanel
      world={world.id}
      article={article.id}
      revision={article}
      chosen={chosen}
      fresh={fresh}
      mayRevert={version.mayRevert}
      gm={world.isGm}
      onChosen={setChosen}
      onRevert={(s) => act(api.versionRevert(world.id, article.id, s, session))}
      onVerdict={(s, accepted) => act(api.versionVerdict(world.id, article.id, s, accepted))}
      onDelete={(s) => act(api.versionDelete(world.id, article.id, s))}
      onError={onError}
    />
  );

  if (mode === "edit") {
    // Code, both side by side, or visual — one switch for three states.
    // Beside a version there is no room for a second page.
    const pane = prefs.layout === "split" && !chosen ? "split" : prefs.side;
    const switches = (
      <>
        {modeSwitch}
        <div className="segmented" role="group" aria-label="Pane">
          <button
            className={pane === "code" ? "on" : ""}
            onClick={() => setPrefs({ layout: "single", side: "code" })}
            title="Code"
          >
            <PaneIcon pane="code" />
          </button>
          <button
            className={pane === "split" ? "on" : ""}
            onClick={() => setPrefs({ layout: "split" })}
            disabled={!!chosen}
            title={chosen ? "Not beside a version" : "Code and visual side by side"}
          >
            <PaneIcon pane="split" />
          </button>
          <button
            className={pane === "visual" ? "on" : ""}
            onClick={() => setPrefs({ layout: "single", side: "visual" })}
            title="Visual"
          >
            <PaneIcon pane="visual" />
          </button>
        </div>
      </>
    );
    return (
      <div className={history ? "document with-versions" : "document"}>
        <div className="document-main">
          <DocumentEditor
            key={`${article.id} ${session}`}
            world={world}
            article={article}
            opened={opened?.id === article.id ? opened.doc : null}
            templates={templates}
            all={all}
            layout={prefs.layout}
            side={prefs.side}
            compare={chosen ? version.doc : null}
            marks={chosen ? version.marks : null}
            session={session}
            switches={switches}
            deleteButton={<DeleteDocument world={world} article={article} templates={templates} onPlace={onPlace} onChanged={onChanged} onError={onError} />}
            onRetype={
              pageOwner(templates, world.home, article)
                ? undefined
                : (articleType, parents) =>
                    api
                      .articleTypeSet(world.id, article.id, articleType, parents)
                      .then(() => api.documentOpen(world.id, article.id))
                      .then((doc) => {
                        // The editor anew: it writes what the new type has.
                        setOpened({ id: article.id, doc });
                        setSession(newSession());
                        onChanged();
                      })
                      .catch(onError)
            }
            historyButton={
              <>
                {apartButton}
                {historyButton}
              </>
            }
            onSaved={onChanged}
            onError={onError}
          />
        </div>
        {versions}
      </div>
    );
  }

  return (
    <div className={history ? "document with-versions" : "document"}>
      <div className="document-main">
        <HeaderTools>
          {modeSwitch}
          {apartButton}
          {historyButton}
        </HeaderTools>
        {chosen ? (
          version.doc ? (
            <ComparePages
              then={version.doc.view}
              now={article}
              marks={version.marks}
              page={(a) => (
                <ArticlePage
                  world={world}
                  article={a}
                  templates={templates}
                  all={all}
                  onPlace={onPlace}
                />
              )}
            />
          ) : (
            <p className="note">Opening the version …</p>
          )
        ) : (
          page
        )}
      </div>
      {versions}
    </div>
  );
}

/** Code as brackets, visual as a page with lines, both as the two halves. */
function PaneIcon({ pane }: { pane: "code" | "split" | "visual" }) {
  const line = { fill: "none", stroke: "currentColor", strokeWidth: 1.6, strokeLinecap: "round", strokeLinejoin: "round" } as const;
  return (
    <svg viewBox="0 0 20 20" width="16" height="16" aria-hidden="true">
      {pane === "code" ? (
        <path d="M7.5 6 3.5 10l4 4M12.5 6l4 4-4 4" {...line} />
      ) : (
        <>
          <rect x="3" y="3.5" width="14" height="13" rx="2" {...line} />
          {pane === "split" ? <path d="M10 3.5v13" {...line} /> : <path d="M6.5 8h7M6.5 11h7M6.5 14h4" {...line} />}
        </>
      )}
    </svg>
  );
}
