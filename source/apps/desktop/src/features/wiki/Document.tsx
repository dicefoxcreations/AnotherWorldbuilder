import { createContext, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState, type RefObject } from "react";
import {
  api,
  type ArticleView,
  type Bases,
  type BlockPart,
  type Caret,
  type Seen,
  type Version,
  type VersionDocument,
  type VersionMarks,
  type Access,
  type DocumentText,
  type GraphNode,
  type LineError,
  type Place,
  type Property,
  type Template,
  type TemplateField,
  type WorldInfo,
  type BlockPlace,
  type TemplateBlock,
} from "../../api/client";
import { FoldBlock, SideColumn } from "../../shared/ui/BlockFold";
import { addSection, BlockSettings, moveBlock, ownerOf, ShapeRows } from "./BlockSettings";
import { ArticlePage, DeleteDocument } from "./ArticlePage";
import { makeDocument } from "./Article";
import { nameFrom, nextNumber } from "../templates/NamePattern";
import { linkIcon } from "./DocLink";
import { ArticleHead } from "./ArticleHead";
import { AccessLine, RegionAccess, samePolicy } from "../../shared/ui/Visibility";
import { late, VersionsPanel } from "./Versions";
import { linker, linkIn, linksIn, linkText, linkTo, MARK, scanLinks, typeNames, type Link } from "../../shared/Links";
import { anchorName, anchorRange, anchorsIn, markSection, relinked, renameInText, withoutAnchor } from "../../shared/Anchors";
import { withClosingTag } from "../../shared/editor/Closing";
import { formatted } from "../../shared/editor/Format";
import { cutAtHeadings, headingLevel } from "../../shared/Sections";
import { keepTyped, sameContent } from "../../shared/editor/Content";
import { mapPosition, rebase, useSteadyCaret } from "../../shared/editor/Merge";
import { RemoteCarets, type RemoteCaret } from "../../shared/editor/RemoteCarets";
import { MarkedField } from "../../shared/editor/MarkedField";
import { changes, marked, type MarkKind, type Range } from "../../shared/Marks";
import { FAST_MS, pace } from "../../app/live";
import { HeaderTools } from "../../app/HeaderTools";
import { caretPoint, useMentions } from "../../shared/editor/Mentions";
import { sideBarOwner, useSideBar } from "../../shared/ui/SideBar";
import { AccessSection, Inspector, InspectorSection, PartAccessContext, PieceMoveContext, SectionAccess } from "./Inspector";
import { documentsOf, isA, mayCreate, propertyNamed, propertyValues, targetOf, typeOf } from "../../shared/types";
import { comboOf, loadShortcuts } from "../../shared/Shortcuts";
import { DEFAULT_CALENDAR } from "../../shared/Calendar";
import { WorldImage } from "../../shared/ui/WorldImage";
import {
  findRole,
  holdsRelations,
  isRelation,
  readRelations,
  relationFields,
  relationsOf,
  roleChoices,
  writeRelations,
  type RelationEntry,
  type RelationField,
  type WrittenRelation,
} from "../../shared/Relations";
import { RelationGraph, type GraphEdit } from "./RelationGraph";
import { graphText, mapText, pieces as graphPieces } from "../../shared/GraphText";
import { CurrentDocument, ReadableParts, useWorldData } from "../worlds/WorldData";
import { ImagePicker } from "../../shared/ui/ImagePicker";
import { ImageCard, imageSurface } from "./ImageCard";
import { AbilitiesCard, CreateCard, EmbedCard, ListCard, TimeCard, ViewCard } from "./ListCard";
import { RollScopeContext, type RollScope } from "../dice/DiceTray";
import { explain, formulaOf, lookupFor, sumOf, summedValues } from "../../shared/Sheet";
import { SheetEdit, ValuesProvider } from "./MarkdownText";
import { PropertyValue } from "./InfoboxForm";
import { EveryPick, MultiPick } from "../../shared/ui/MultiPick";
import { OurMomentInput, WrittenMomentPicker } from "../../shared/ui/DateInputs";
import { ListEditing } from "./Listing";
import { MapCard } from "./MapView";
import { imageTag, writeImageLine } from "../../shared/ImageTag";
import { FlowField, FlowHandle, FlowMarks } from "../../shared/editor/FlowField";
import type { Piece } from "../../shared/GraphText";

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
type Layout = "single" | "split";
type Side = "code" | "visual";
type Prefs = { mode: Mode; layout: Layout; side: Side };

const PREFS_KEY = "aw.document.view";
const DEFAULT_PREFS: Prefs = { mode: "view", layout: "single", side: "visual" };
/** How long typing may pause before what was typed is kept. */
const SAVE_AFTER_MS = 600;
/** … and while someone else writes in the same document. */
const SAVE_TOGETHER_MS = 150;
/** Typing on without a pause: what was typed is kept at least this often — the others see it come. */
const SAVE_AT_MOST_MS = 1000;

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

/** A fresh editing session's name. */
function newSession(): string {
  return typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
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
  // Waiting on the GM's device: not stored yet — changed in its form, no versions.
  const pending = !!article.pending;
  const editPending = useWorldData()?.onEditPending;
  const canEdit = !pending && !previewing && (world.isGm || article.layers.some((l) => l.mayEdit));
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
      onChanged={onChanged}
      onError={onError}
      actions={actions}
    />
  );

  const switchMode = () => {
    if (mode !== "edit") setSession(newSession());
    setJustMade(false);
    setPrefs({ mode: mode === "edit" ? "view" : "edit" });
  };
  // Ctrl+E: editing and reading, as the ✎ button.
  const latestSwitch = useRef(switchMode);
  latestSwitch.current = canEdit && !pending ? switchMode : () => {};
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (comboOf(e) !== "ctrl+e") return;
      e.preventDefault();
      latestSwitch.current();
    };
    document.addEventListener("keydown", key);
    return () => document.removeEventListener("keydown", key);
  }, []);

  const modeSwitch = pending ? (
    editPending && (
      <button className="ghost icon round" onClick={() => editPending(article)} title="Edit">
        ✎
      </button>
    )
  ) : canEdit && (
    <button
      className={mode === "edit" ? "ghost icon round on" : "ghost icon round"}
      onClick={switchMode}
      title={mode === "edit" ? "Back to reading (Ctrl+E)" : "Edit (Ctrl+E)"}
    >
      ✎
    </button>
  );
  // This document in a window of its own, to lay beside another.
  const onApart = useWorldData()?.onApart;
  const apartButton = onApart && (
    <button className="ghost icon round apart-button" onClick={() => onApart({ kind: "article", id: article.id })} title="Open in a new window (Ctrl+click on a link does too)">
      ⧉
    </button>
  );
  // The versions share the side bar with a graph's roles: one at a time.
  useSideBar(`versions:${article.id}`, history, () => {
    setHistory(false);
    setChosen(null);
  });
  // What came in since one last looked is marked in the list while it is open.
  const [fresh, setFresh] = useState<string[]>([]);
  const historyButton = !pending && (
    <button
      className={history ? "ghost icon round on" : "ghost icon round"}
      onClick={() => {
        if (!history) {
          setFresh(news.items.map((v) => v.session));
          news.dismiss();
        } else setChosen(null);
        setHistory(!history);
      }}
      title={news.items.length > 0 ? "New versions came in — written offline, or not in the text yet" : "Versions of this document"}
    >
      🕘
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
      onCarry={(s) => act(api.versionCarry(world.id, article.id, s))}
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
            templates={templates}
            all={all}
            layout={prefs.layout}
            side={prefs.side}
            compare={chosen ? version.doc : null}
            marks={chosen ? version.marks : null}
            session={session}
            switches={switches}
            deleteButton={<DeleteDocument world={world} article={article} templates={templates} onPlace={onPlace} onChanged={onChanged} onError={onError} />}
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
                  onChanged={onChanged}
                  onError={onError}
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

/**
 * Something was not kept: a red mark in the header, the explanation beside it
 * on hover — and on click, so the button inside can be reached.
 */
function SaveProblem({ title, onRetry, children }: { title: string; onRetry?: () => void; children: React.ReactNode }) {
  // Hover shows it; a click keeps it open until the next click.
  const [open, setOpen] = useState(false);
  return (
    <div className={open ? "save-problem open" : "save-problem"}>
      <button className="save-problem-mark danger icon round" aria-label={title} aria-expanded={open} onClick={() => setOpen(!open)}>
        ⚠
      </button>
      <div className="popover right" role="alert">
        <h4>{title}</h4>
        {children}
        {onRetry && <button className="compact" onClick={onRetry}>Try again</button>}
      </div>
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

/** The chosen version: as it stood, and whether it can be taken back. */
function useVersion(world: string, article: string, chosen: string | null, revision: unknown, onError: (e: unknown) => void) {
  const [doc, setDoc] = useState<VersionDocument | null>(null);
  const [marks, setMarks] = useState<VersionMarks | null>(null);
  const [mayRevert, setMayRevert] = useState(false);
  useEffect(() => {
    setDoc(null);
    setMarks(null);
    setMayRevert(false);
    if (!chosen) return;
    let gone = false;
    api
      .versionDocument(world, article, chosen)
      .then((d) => !gone && setDoc(d))
      .catch(onError);
    api
      .versionMarks(world, article, chosen)
      .then((m) => !gone && setMarks(m))
      .catch(() => {});
    api
      .versionCompare(world, article, chosen)
      .then((c) => !gone && setMayRevert(c.some((x) => x.mayRevert)))
      .catch(() => {});
    return () => {
      gone = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [world, article, chosen, revision]);
  return { doc, marks, mayRevert };
}

/**
 * How a comparison reads. A version in the text stands left as it was,
 * marked with what is gone since; now stands right with what is new. A
 * version that waits — or was turned down — stands left as the text would be
 * with it: what it would add is green there, what it would take is red in
 * now.
 */
function sides(marks: VersionMarks | null) {
  const counted = !marks || marks.review === "counted";
  return {
    label: counted ? "This version" : marks.review === "waiting" ? "With this version" : "With the turned-down version",
    then: (counted ? "removed" : "added") as MarkKind,
    now: (counted ? "added" : "removed") as MarkKind,
  };
}

/** Two finished pages side by side: the version, and now. */
function ComparePages({
  then,
  now,
  marks,
  page,
}: {
  then: ArticleView;
  now: ArticleView;
  marks: VersionMarks | null;
  page: (a: ArticleView) => React.ReactNode;
}) {
  const [left, right] = markedViews(then, now, marks);
  return (
    <div className="two-columns compare-pages">
      <div>
        <h4 className="compare-label">{sides(marks).label}</h4>
        {page(left)}
      </div>
      <div>
        <h4 className="compare-label">Now</h4>
        {page(right)}
      </div>
    </div>
  );
}

/**
 * Both versions of a page with their differences marked in their texts —
 * by the journal's word where it has one, otherwise by comparing the texts.
 */
function markedViews(then: ArticleView, now: ArticleView, marks: VersionMarks | null): [ArticleView, ArticleView] {
  const whole = (text: string): Range[] => [{ start: 0, end: text.length }];
  const kinds = sides(marks);
  const left = then.layers.map((l) => {
    const other = now.layers.find((n) => n.id === l.id);
    const gone = marks ? (marks.then.layers[l.id] ?? []) : other ? changes(l.text, other.text).removed : whole(l.text);
    return { ...l, text: marked(l.text, gone, kinds.then) };
  });
  const right = now.layers.map((l) => {
    const other = then.layers.find((t) => t.id === l.id);
    const added = marks ? (marks.now.layers[l.id] ?? []) : other ? changes(other.text, l.text).added : whole(l.text);
    return { ...l, text: marked(l.text, added, kinds.now) };
  });
  return [
    { ...then, layers: left },
    { ...now, layers: right },
  ];
}

/**
 * What came in that one should know of: versions written offline that
 * arrived since one last looked, and versions not in the text yet. Remembered
 * per document on this device.
 */
function useNews(world: string, article: string, revision: unknown) {
  const key = `aw.seen.${world}.${article}`;
  const read = () => {
    try {
      return Number(localStorage.getItem(key) ?? "0") || 0;
    } catch {
      return 0;
    }
  };
  const [versions, setVersions] = useState<Version[]>([]);
  const [seenUntil, setSeenUntil] = useState(read);
  useEffect(() => {
    setSeenUntil(read());
    api.articleVersions(world, article).then(setVersions).catch(() => setVersions([]));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [world, article, revision]);
  const remember = (until: number) => {
    setSeenUntil(until);
    try {
      localStorage.setItem(key, String(until));
    } catch {
      // Without storage the note simply comes again.
    }
  };
  const items = versions.filter((v) => v.stale || v.review === "waiting" || (late(v) && v.arrived > seenUntil));
  return {
    items,
    seen: (v: Version) => remember(Math.max(seenUntil, v.arrived)),
    dismiss: () => remember(Math.max(seenUntil, ...versions.map((v) => v.arrived))),
  };
}

type Status = "saved" | "saving" | "invalid" | "failed";



function DocumentEditor({
  world,
  article,
  templates,
  all,
  layout,
  side,
  compare,
  marks,
  session,
  switches,
  historyButton,
  deleteButton,
  onSaved,
  onError,
}: {
  world: WorldInfo;
  article: ArticleView;
  templates: Template[];
  all: ArticleView[];
  /** Deleting the document — here, while editing, not beside what one reads. */
  deleteButton?: React.ReactNode;
  layout: Layout;
  side: Side;
  /** A version to show beside the document — the editor then shows both. */
  compare: VersionDocument | null;
  /** Where that version and now differ, as the journal knows it. */
  marks: VersionMarks | null;
  session: string;
  switches: React.ReactNode;
  historyButton: React.ReactNode;
  onSaved: () => void;
  onError: (e: unknown) => void;
}) {
  const [text, setText] = useState<string | null>(null);
  const [doc, setDoc] = useState<DocumentText | null>(null);
  // The properties are data, not text: folded away until asked for — in
  // the code and on the visual side alike.
  const [propsOpen, setPropsOpen] = useState(false);
  const [errors, setErrors] = useState<LineError[]>([]);
  const [status, setStatus] = useState<Status>("saved");
  const [problem, setProblem] = useState<string | null>(null);
  // The document before this keystroke — the blocks keep their ids from it.
  const docRef = useRef<DocumentText | null>(null);
  docRef.current = doc;
  const textRef = useRef<string | null>(null);
  textRef.current = text;
  // What is kept: the code as it stands, and the state of each layer it
  // stands on. Saving writes the difference against these.
  const kept = useRef<{ code: string; base: Bases } | null>(null);
  const busy = useRef(false);
  // Since when what was typed waits to be kept.
  const unsavedSince = useRef<number | null>(null);
  // Changes on the visual side whose code is still being written.
  const writing = useRef(0);
  // Whether this session wrote anything — only then does leaving load anew.
  const wrote = useRef(false);
  // The save on its way, if any — leaving waits for it.
  const pending = useRef<Promise<unknown>>(Promise.resolve());
  // A change made on the visual side is already in `doc`; reading its code
  // back would only race the next keystroke.
  const fromVisual = useRef(false);
  // Where one's own caret stands, and who else is here.
  const caret = useRef<Caret | null>(null);
  const [others, setOthers] = useState<Seen[]>([]);
  // The toolbar learns here which field one writes in.
  const toolbarTarget = useRef<((field: ToolbarField | null) => void) | null>(null);

  /** Takes in the document as it now stands — keeping what was typed since. */
  const adopt = (code: string, document: DocumentText, base: Bases, sent: string) => {
    const now = textRef.current ?? sent;
    kept.current = { code, base };
    if (now === sent) {
      fromVisual.current = true;
      setText(code);
      setDoc(keepTyped(document, docRef.current));
    } else {
      // Typed on while this was on its way: that goes on top.
      setText(rebase(sent, now, code));
    }
  };

  useEffect(() => {
    api
      .documentOpen(world.id, article.id)
      .then((r) => {
        kept.current = { code: r.code, base: r.base };
        fromVisual.current = true;
        setText(r.code);
        setDoc(r.document);
      })
      .catch(onError);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [world.id, article.id]);

  // Code typed → the visual side follows.
  useEffect(() => {
    if (text === null || docRef.current === null) return;
    if (fromVisual.current) {
      fromVisual.current = false;
      return;
    }
    let gone = false;
    api
      .documentRead(world.id, article.id, text, docRef.current)
      .then((r) => {
        if (gone) return;
        setErrors(r.errors);
        setDoc(r.document);
      })
      .catch(() => {});
    return () => {
      gone = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text]);

  const save = async () => {
    const sending = textRef.current;
    const sentDoc = docRef.current;
    if (busy.current || sending === null || !kept.current || sending === kept.current.code) return;
    busy.current = true;
    unsavedSince.current = null;
    setStatus("saving");
    try {
      const saving = api.documentSave(world.id, article.id, sending, kept.current.base, session);
      pending.current = saving.catch(() => {});
      const r = await saving;
      wrote.current = true;
      // What was saved means what was typed: the text stays as typed — an
      // empty line, an empty block one is about to fill. Only where the saved
      // document says something else (someone else wrote, a link got its
      // type) does the code follow it.
      const meant = await api.documentRead(world.id, article.id, sending, r.document).catch(() => null);
      if (meant && meant.errors.length === 0 && sameContent(meant.document, r.document)) {
        kept.current = { code: sending, base: r.base };
        // Typed on meanwhile — perhaps not yet in the code: the field stays as it is.
        const untouched = writing.current === 0 && docRef.current === sentDoc && textRef.current === sending;
        if (untouched) setDoc(keepTyped(r.document, docRef.current));
      } else {
        adopt(r.code, r.document, r.base, sending);
      }
      setStatus("saved");
      setProblem(null);
    } catch (e) {
      setStatus("failed");
      setProblem(String(e));
    } finally {
      busy.current = false;
    }
  };

  // Kept as it is typed — after a short pause, and only what reads.
  useEffect(() => {
    if (text === null || !kept.current || text === kept.current.code) return;
    if (errors.length > 0) {
      setStatus("invalid");
      return;
    }
    unsavedSince.current ??= Date.now();
    const pause = others.length > 0 ? SAVE_TOGETHER_MS : SAVE_AFTER_MS;
    const timer = setTimeout(() => void save(), Math.min(pause, Math.max(0, unsavedSince.current + SAVE_AT_MOST_MS - Date.now())));
    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text, errors]);

  // Where one writes goes out, where the others write comes in.
  // Alone, now and then is enough to notice someone coming; together, often.
  useEffect(() => {
    let gone = false;
    let tick = 0;
    const tell = () => {
      if (!pace.network) {
        pace.together = false;
        setOthers([]);
        return;
      }
      if (!pace.together && tick++ % 5 !== 0) return;
      api
        .presence(world.id, article.id, caret.current)
        .then((seen) => {
          if (gone) return;
          setOthers(seen);
          pace.together = seen.length > 0;
        })
        .catch(() => {});
    };
    tell();
    const timer = setInterval(tell, FAST_MS);
    return () => {
      gone = true;
      clearInterval(timer);
      pace.together = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [world.id, article.id]);

  // Leaving keeps what was typed last, ends the session — it becomes a
  // version — and the page shows what now stands.
  useEffect(
    () => () => {
      const last = textRef.current;
      void pending.current
        .then(() => {
          const k = kept.current;
          if (!k || last === null || last === k.code) return null;
          wrote.current = true;
          return api.documentSave(world.id, article.id, last, k.base, session);
        })
        .then(() => api.sessionEnd(world.id, session))
        // Only what was written loads the world again: with thousands of
        // documents, a look into the editor should not cost a reload.
        .then(() => wrote.current && onSaved(), onError);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );

  // What others wrote arrives while one writes: it comes in, what one typed
  // stays.
  useEffect(() => {
    if (!kept.current || busy.current || writing.current > 0) return;
    const sent = kept.current.code;
    api
      .documentOpen(world.id, article.id)
      .then((r) => {
        if (busy.current || writing.current > 0 || !kept.current || kept.current.code !== sent) return;
        if (r.code === sent) {
          kept.current = { code: sent, base: r.base };
          return;
        }
        adopt(r.code, r.document, r.base, sent);
      })
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [article]);

  // Visual change → the code follows.
  const changeDoc = (next: DocumentText) => {
    setDoc(next);
    docRef.current = next;
    writing.current++;
    api
      .documentWrite(next)
      .then((code) => {
        fromVisual.current = true;
        setText(code);
        setErrors([]);
      })
      .catch(onError)
      .finally(() => writing.current--);
  };

  if (text === null || doc === null) return <p className="note">Opening …</p>;

  const links: Links = { all, templates, selfId: article.id };
  // Their places count in the code as kept; what was typed since moves them.
  const base = kept.current?.code ?? text;
  const codeCarets: RemoteCaret[] = others.flatMap((o) =>
    o.caret
      ? [{ id: o.id, name: o.name, start: mapPosition(base, text, o.caret.codeStart), end: mapPosition(base, text, o.caret.codeEnd) }]
      : [],
  );
  const partCarets = (key: string): RemoteCaret[] =>
    others.flatMap((o) =>
      o.caret && o.caret.part === key ? [{ id: o.id, name: o.name, start: o.caret.partStart, end: o.caret.partEnd }] : [],
    );
  // Beside a version: what differs marked in both. The journal's marks fit
  // the text as kept; while something typed is not kept yet, comparing the
  // texts stands in for them.
  const kinds = sides(marks);
  const exact = marks !== null && text === marks.nowCode;
  const newInCode = !compare ? [] : exact ? marks!.now.code : changes(compare.code, text).added;
  const partText = (d: DocumentText, key: string) =>
    d.blocks.flatMap((b) => b.parts).find((p) => p.key === key)?.text ?? "";
  const partMarks = (side: "then" | "now") => (key: string): Range[] => {
    if (!compare) return [];
    if (side === "then" && marks) return marks.then.parts[key] ?? [];
    if (side === "now" && exact) return marks!.now.parts[key] ?? [];
    const then = partText(compare.document, key);
    const now = partText(doc, key);
    return side === "now" ? changes(then, now).added : changes(then, now).removed;
  };
  const thenInCode: Range[] = !compare ? [] : marks ? marks.then.code : changes(compare.code, text).removed;
  const code = (
    <CodeEditor
      text={text}
      errors={errors}
      gm={world.isGm}
      links={links}
      propsOpen={propsOpen}
      marks={{ kind: kinds.now, ranges: newInCode }}
      carets={codeCarets}
      onPropsOpen={setPropsOpen}
      onText={setText}
      onCaret={(c) => (caret.current = c)}
    />
  );
  const visual = (
    <VisualEditor
      world={world}
      article={article}
      templates={templates}
      all={all}
      doc={doc}
      propsOpen={propsOpen}
      onPropsOpen={setPropsOpen}
      links={links}
      carets={partCarets}
      marks={{ kind: kinds.now, of: partMarks("now") }}
      onDoc={changeDoc}
      onCaret={(c) => (caret.current = c)}
    />
  );
  // The version, as it stood — to read, not to write.
  const then = compare && (
    <div>
      <h4 className="compare-label">{kinds.label}</h4>
      {side === "code" ? (
        <pre className="compare-side code">
          {pieces(compare.code, thenInCode).map((s, i) => (
            <span key={i} className={s.marked ? `mark-${kinds.then}` : undefined}>
              {s.text}
            </span>
          ))}
        </pre>
      ) : (
        <fieldset disabled className="then">
          <VisualEditor
            world={world}
            article={article}
            templates={templates}
            all={all}
            doc={compare.document}
            propsOpen={propsOpen}
            onPropsOpen={setPropsOpen}
            links={links}
            carets={() => []}
            marks={{ kind: kinds.then, of: partMarks("then") }}
            onDoc={() => {}}
            onCaret={() => {}}
          />
        </fieldset>
      )}
    </div>
  );

  const dirty = kept.current !== null && text !== kept.current.code;
  return (
    <ToolbarTarget.Provider value={toolbarTarget}>
      <HeaderTools>
        {switches}
        {status === "invalid" || status === "failed" ? (
          <SaveProblem
            title={status === "invalid" ? "Not kept yet" : "Not kept"}
            onRetry={status === "failed" ? () => void save() : undefined}
          >
            {status === "invalid" ? (
              <>
                <p>The code does not read — nothing is kept until it does. Fix the marked line:</p>
                <ul>
                  {errors.map((e) => (
                    <li key={`${e.line} ${e.text}`}>
                      <strong>Line {e.line}</strong> {e.text}
                    </li>
                  ))}
                </ul>
              </>
            ) : (
              <p>Saving failed: {problem ?? "unknown error"}. What you typed is still here.</p>
            )}
          </SaveProblem>
        ) : status === "saving" || dirty ? (
          <span className="save-state" title="Saving …">
            …
          </span>
        ) : (
          <span className="save-state" title="Kept">
            ✓
          </span>
        )}
        {historyButton}
      </HeaderTools>
      {/* The head as it reads — who sees and who changes the document, the GM's to set. */}
      <CurrentDocument.Provider value={article.id}>
        <ArticleHead
          icon={typeOf(templates, article.articleType)?.icon || (article.articleType ? "📄" : world.icon)}
          title={doc.name || article.name}
          type={typeOf(templates, article.articleType) ? <span className="chip">{typeOf(templates, article.articleType)!.name}</span> : null}
          marks={
            <>
              <AccessLine
                world={world}
                view={doc.access.view}
                edit={doc.access.edit}
                onView={world.isGm ? (view) => changeDoc({ ...doc, access: { ...doc.access, view } }) : undefined}
                onEdit={world.isGm ? (edit) => changeDoc({ ...doc, access: { ...doc.access, edit } }) : undefined}
              />
              {deleteButton}
            </>
          }
        />
      </CurrentDocument.Provider>
      {!then && <Toolbar target={toolbarTarget} />}
      {then ? (
        <div className="two-columns document-columns compare-live">
          {then}
          <div>
            <h4 className="compare-label">Now</h4>
            {side === "code" ? code : visual}
          </div>
        </div>
      ) : layout === "split" ? (
        <div className="two-columns document-columns">
          {code}
          <div className="preview-side">{visual}</div>
        </div>
      ) : side === "code" ? (
        code
      ) : (
        visual
      )}
    </ToolbarTarget.Provider>
  );
}

/** A text cut into what is marked and what is not. */
function pieces(text: string, ranges: Range[]): { text: string; marked: boolean }[] {
  const out: { text: string; marked: boolean }[] = [];
  let at = 0;
  for (const r of [...ranges].sort((a, b) => a.start - b.start)) {
    if (r.start < at) continue;
    if (r.start > at) out.push({ text: text.slice(at, r.start), marked: false });
    out.push({ text: text.slice(r.start, r.end), marked: true });
    at = r.end;
  }
  if (at < text.length) out.push({ text: text.slice(at), marked: false });
  return out;
}

// --- Inserting ------------------------------------------------------------

type Brick = { name: string; text: string; gmOnly?: boolean; picksImage?: boolean };

/**
 * What the right-click menu offers — the elements of a document's text, one
 * each: what kind of list, which timeline, is chosen in the side bar once it
 * stands there.
 */
const BRICKS: { title: string; bricks: Brick[] }[] = [
  {
    title: "Text",
    bricks: [
      { name: "Heading", text: "## «Heading»" },
      { name: "Link to a document", text: "@@" },
      { name: "Image", text: "", picksImage: true },
      { name: "Rule", text: "\n---\n" },
    ],
  },
  {
    title: "Elements",
    bricks: [
      { name: "List", text: '<list type="«Place»">' },
      { name: "Timeline", text: "<timeline>" },
      { name: "Calendar", text: "<calendar>" },
      { name: "Relation graph", text: "<graph>\n</graph>" },
      { name: "Map", text: "<map src=«img»>\n</map>" },
      { name: "Create button", text: '<create type="«Event»">', gmOnly: true },
      { name: "Another page", text: "<embed of=«Page»>" },
    ],
  },
  {
    title: "Stats",
    bricks: [
      { name: "Roll", text: "[[«1d20 + 2»]]" },
      { name: "Check", text: "[[«+4»]]" },
      { name: "Ability row", text: "<abilities>" },
      { name: "Group layout", text: "<view of=«Group»>" },
    ],
  },
  {
    title: "Access",
    bricks: [
      {
        name: "Secret region",
        text: "<block view=«gm» edit=gm level=gm-only>\n\n</block>",
        gmOnly: true,
      },
    ],
  },
];

/** What a right click on selected text offers: marks around it, or a kind of line. */
const FORMATS: { name: string; wrap?: string; line?: string; title: string }[] = [
  { name: "B", wrap: "**", title: "Bold" },
  { name: "I", wrap: "*", title: "Italic" },
  { name: "</>", wrap: "`", title: "Code" },
  { name: "@@", wrap: "@@", title: "Link to the document so named" },
  { name: "H1", line: "# ", title: "Heading" },
  { name: "H2", line: "## ", title: "Smaller heading" },
  { name: "❞", line: "> ", title: "Quote" },
  { name: "•", line: "- ", title: "List" },
];

/** Makes a new section of the type below the block one writes in — the GM's, where the document has a type. */
const SectionBelow = createContext<(() => void) | null>(null);
/** Cuts a part's text into sections of the document's own, at its headings of one level. */
const CutSections = createContext<((text: string, level: number) => void) | null>(null);

/** What the toolbar acts on: the text field one writes in, and where it stands. */
type FieldActions = { format: (f: { wrap?: string; line?: string }) => void; brick: (b: Brick) => void; link: () => void };
type ToolbarField = {
  actions: RefObject<FieldActions | null>;
  el: HTMLElement;
  /** The bottom of the selection's first line on screen — the toolbar stands above it. */
  caret: () => { x: number; y: number } | null;
  /** Whether words are selected — only then the toolbar shows. */
  selected: () => boolean;
};
const ToolbarTarget = createContext<RefObject<((field: ToolbarField | null) => void) | null> | null>(null);

/** The toolbar's buttons — what one does to selected words. */
const TOOLS: { name: React.ReactNode; title: string; format?: { wrap?: string; line?: string }; link?: true }[] = [
  { name: <strong>B</strong>, title: "Bold (Ctrl+B)", format: { wrap: "**" } },
  { name: <em>I</em>, title: "Italic (Ctrl+I)", format: { wrap: "*" } },
  { name: "H", title: "Heading", format: { line: "## " } },
  { name: "•", title: "List", format: { line: "- " } },
  { name: "❞", title: "Quote", format: { line: "> " } },
  { name: "@@", title: "Link (Ctrl+K)", link: true },
];

/**
 * Bold, italic, a link … over selected words, on the code side and the
 * visual side alike — and nowhere while one only writes or reads. Pressing
 * a button leaves the selection where it was.
 */
function Toolbar({ target }: { target: RefObject<((field: ToolbarField | null) => void) | null> }) {
  const [field, setField] = useState<ToolbarField | null>(null);
  const bar = useRef<HTMLDivElement>(null);
  useEffect(() => {
    target.current = setField;
    return () => void (target.current = null);
  }, [target]);
  // Stands over the caret's line: moved when the caret, the text or the page moves.
  useEffect(() => {
    if (!field) return;
    const scroller = scrollParent(field.el);
    let frame = 0;
    const place = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        const el = bar.current;
        const at = field.caret();
        if (!el || !at) return;
        const top = scroller ? scroller.getBoundingClientRect().top + 4 : 4;
        const box = field.el.getBoundingClientRect();
        const h = el.offsetHeight;
        const line = parseFloat(getComputedStyle(field.el).lineHeight) || 22;
        // Over the line one writes in — below it where the top would cut it off.
        const above = at.y - line - h - 4;
        el.style.top = `${above >= top ? above : at.y + 4}px`;
        el.style.left = `${Math.max(4, Math.min(at.x - 12, window.innerWidth - el.offsetWidth - 8))}px`;
        el.style.visibility = !field.selected() || box.bottom < top || box.top > window.innerHeight ? "hidden" : "visible";
      });
    };
    place();
    const events = ["selectionchange", "input", "keyup", "mouseup"] as const;
    for (const e of events) document.addEventListener(e, place);
    window.addEventListener("scroll", place, true);
    window.addEventListener("resize", place);
    return () => {
      cancelAnimationFrame(frame);
      for (const e of events) document.removeEventListener(e, place);
      window.removeEventListener("scroll", place, true);
      window.removeEventListener("resize", place);
    };
  }, [field]);
  if (!field) return null;
  return (
    <div ref={bar} className="edit-toolbar" role="toolbar" aria-label="Format">
      {TOOLS.map((t) => (
        <button
          key={t.title}
          className="ghost compact"
          title={t.title}
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => {
            const actions = field.actions.current;
            if (!actions) return;
            if (t.format) actions.format(t.format);
            else if (t.link) actions.link();
          }}
        >
          {t.name}
        </button>
      ))}
    </div>
  );
}

/** The nearest box that scrolls around an element — the toolbar keeps below its top. */
function scrollParent(el: HTMLElement): HTMLElement | null {
  for (let at = el.parentElement; at; at = at.parentElement) {
    if (/(auto|scroll)/.test(getComputedStyle(at).overflowY)) return at;
  }
  return null;
}

/**
 * Inserts a brick at the caret and selects its placeholder («…»), so one
 * types straight over it.
 */
/** What a brick is put into: a text field, or a flow field that acts like one. */
type Caretful = { selectionStart: number | null; setSelectionRange: (a: number, b: number) => void; focus: () => void };

function insertAt(field: Caretful, current: string, brick: Brick, onText: (t: string) => void) {
  const at = field.selectionStart ?? current.length;
  const lineStart = current.lastIndexOf("\n", at - 1) + 1;
  const blockLevel = brick.text.startsWith("<") || brick.text.startsWith("#");
  // Tags and headings stand on a line of their own.
  const before = blockLevel && at > lineStart ? "\n" : "";
  const raw = before + brick.text + (blockLevel ? "\n" : "");
  const open = raw.indexOf("«");
  const close = raw.indexOf("»");
  const clean = raw.replace(/[«»]/g, "");
  onText(current.slice(0, at) + clean + current.slice(at));
  queueMicrotask(() => {
    field.focus();
    if (open >= 0 && close > open) field.setSelectionRange(at + open, at + close - 1);
    else field.setSelectionRange(at + clean.length, at + clean.length);
  });
}

function BrickMenu({
  x,
  y,
  gm,
  onPick,
  onSection,
  onFormat,
  onCut,
  link,
  onHide,
  sections,
  onClose,
}: {
  x: number;
  y: number;
  gm: boolean;
  onPick: (b: Brick) => void;
  /** A new section below this one — where there can be one. */
  onSection?: () => void;
  /** Text is selected: what can be done to it. */
  onFormat?: (f: { wrap?: string; line?: string }) => void;
  /** On a heading: its text cut into sections at the headings as high. */
  onCut?: { level: number; run: () => void };
  /** Selected words: the documents they may name — or a new one. */
  link?: LinkChoice;
  /** Selected text: into a region only the GM sees. */
  onHide?: () => void;
  /** Sections: marking the selection as one, and linking to one. */
  sections?: SectionChoice;
  onClose: () => void;
}) {
  useEffect(() => {
    const close = () => onClose();
    const key = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", key);
    };
  }, [onClose]);
  // All of it in the window: moved up as far as it is tall.
  const box = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const el = box.current;
    if (el) el.style.top = `${Math.max(8, Math.min(y, window.innerHeight - el.offsetHeight - 8))}px`;
  });
  return (
    <div
      ref={box}
      className="brick-menu"
      style={{ left: Math.min(x, window.innerWidth - 260), top: Math.min(y, window.innerHeight - 320) }}
      onMouseDown={(e) => e.stopPropagation()}
      onContextMenu={(e) => e.preventDefault()}
    >
      {link && <LinkGroup link={link} />}
      {sections && <SectionGroup sections={sections} />}
      {onFormat && (
        <div className="brick-group">
          <h5>Selected</h5>
          <div className="brick-formats">
            {FORMATS.map((f) => (
              <button key={f.name} className="ghost compact" title={f.title} onClick={() => onFormat(f)}>
                {f.name === "B" ? <strong>B</strong> : f.name === "I" ? <em>I</em> : f.name}
              </button>
            ))}
          </div>
          {gm && onHide && (
            <button className="ghost" onClick={onHide}>
              Hide from players
            </button>
          )}
        </div>
      )}
      {/* What is put in at the caret — not over selected words. */}
      {!onFormat && BRICKS.map((group) => {
        const bricks = group.bricks.filter((b) => gm || !b.gmOnly);
        if (bricks.length === 0) return null;
        return (
          <div key={group.title} className="brick-group">
            <h5>{group.title}</h5>
            {bricks.map((b) => (
              <button key={b.name} className="ghost" onClick={() => onPick(b)}>
                {b.name}
              </button>
            ))}
          </div>
        );
      })}
      {!onFormat && onSection && (
        <div className="brick-group">
          <h5>Document</h5>
          <button className="ghost" onClick={onSection}>
            New section below
          </button>
          {onCut && (
            <button className="ghost" title="Each heading as high becomes a section of its own" onClick={onCut.run}>
              {"#".repeat(onCut.level)} headings to sections
            </button>
          )}
        </div>
      )}
    </div>
  );
}

/** Sections of the text: what is selected marked as one, and a link to any one. */
type SectionChoice = {
  /** Marks the selection as a section of this name — where something is selected. */
  mark?: (name: string) => void;
  /** Every section of the world this reader can open. */
  found: SectionEntry[];
  onLink: (s: SectionEntry) => void;
};
type SectionEntry = { article: ArticleView; anchor: string; shown: string; line: string };

/** Every anchor of the documents — named, or by the words of its line, links read by their names. */
function sectionsOf(all: ArticleView[], resolve: (link: Link) => ArticleView | undefined): SectionEntry[] {
  const out: SectionEntry[] = [];
  for (const article of all) {
    const seen = new Set<string>();
    for (const layer of article.layers) {
      for (const { anchor, line } of anchorsIn(layer.text ?? "")) {
        if (seen.has(anchor)) continue;
        seen.add(anchor);
        // What one reads of the line: links by their text, no marks.
        const read = scanLinks(line)
          .map((p) => (typeof p === "string" ? p : p.text !== p.name ? p.text : (resolve(p)?.name ?? "")))
          .join("")
          .replace(/[*_`|>#]/g, " ")
          .replace(/^\s*([-+]|\d+[.)])\s+/, "")
          .replace(/\s+/g, " ")
          .trim();
        const named = !/^[0-9a-f]{6}$/.test(anchor);
        const name = anchor.replace(/-/g, " ");
        out.push({ article, anchor, shown: named ? name.charAt(0).toUpperCase() + name.slice(1) : read.split(" ").slice(0, 6).join(" "), line: read });
      }
    }
  }
  return out;
}

/** In the right-click menu: sections — the selection marked as one, a link to one. */
function SectionGroup({ sections }: { sections: SectionChoice }) {
  const [mode, setMode] = useState<"" | "mark" | "link">("");
  const [input, setInput] = useState("");
  const q = input.trim().toLowerCase();
  const found = mode === "link"
    ? sections.found
        .filter((s) => !q || `${s.article.name} ${s.shown} ${s.line}`.toLowerCase().includes(q))
        .slice(0, 8)
    : [];
  return (
    <div className="brick-group">
      <h5>Section</h5>
      {mode === "" && (
        <>
          {sections.mark && (
            <button className="ghost" onClick={() => setMode("mark")}>
              Mark as section …
            </button>
          )}
          <button className="ghost" onClick={() => setMode("link")}>
            Link to a section …
          </button>
        </>
      )}
      {mode === "mark" && (
        <input
          className="brick-filter"
          autoFocus
          placeholder="Name of the section"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && input.trim() && sections.mark?.(input)}
        />
      )}
      {mode === "link" && (
        <>
          <input
            className="brick-filter"
            autoFocus
            placeholder="Find a section"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && found[0] && sections.onLink(found[0])}
          />
          {found.map((s) => (
            <button key={`${s.article.id}^${s.anchor}`} className="ghost link-choice" title={s.line} onClick={() => sections.onLink(s)}>
              <span>{s.shown}</span>
              <span className="small">{s.article.name}</span>
            </button>
          ))}
          {found.length === 0 && <p className="small muted brick-note">No section found.</p>}
        </>
      )}
    </div>
  );
}

/** What selected words may link to: the documents named so, or one to make. */
type LinkChoice = {
  word: string;
  found: ArticleView[];
  templates: Template[];
  onLink: (a: ArticleView) => void;
  /** The types a new document may have — none where this member makes none here. */
  types: Template[];
  onNew: (t: Template) => void;
};

/** The documents whose names hold the words, the closest first. */
function namedLike(all: ArticleView[], word: string, selfId?: string): ArticleView[] {
  const q = word.trim().toLowerCase();
  const score = (a: ArticleView) => {
    const n = a.name.toLowerCase();
    return n === q ? 0 : n.startsWith(q) ? 1 : n.includes(q) ? 2 : q.includes(n) && n.length > 3 ? 3 : -1;
  };
  return all
    .filter((a) => a.id !== selfId)
    .map((a) => ({ a, s: score(a) }))
    .filter((x) => x.s >= 0)
    .sort((x, y) => x.s - y.s || x.a.name.localeCompare(y.a.name))
    .slice(0, 6)
    .map((x) => x.a);
}

/** In the right-click menu: link the selected words — to what they name, or to a new document. */
function LinkGroup({ link }: { link: LinkChoice }) {
  const [choosing, setChoosing] = useState(false);
  const [filter, setFilter] = useState("");
  const q = filter.trim().toLowerCase();
  const types = link.types.filter((t) => !q || t.name.toLowerCase().includes(q));
  return (
    <div className="brick-group">
      <h5>Link “{link.word.length > 24 ? `${link.word.slice(0, 23)}…` : link.word}”</h5>
      {!choosing ? (
        <>
          {link.found.map((a) => (
            <button key={a.id} className="ghost link-choice" onClick={() => link.onLink(a)}>
              <span>{a.name}</span>
              <span className="small">{typeOf(link.templates, a.articleType)?.name}</span>
            </button>
          ))}
          {link.found.length === 0 && <p className="small muted brick-note">No document of that name.</p>}
          {link.types.length > 0 && (
            <button className="ghost" onClick={() => setChoosing(true)}>
              ＋ New …
            </button>
          )}
        </>
      ) : (
        <>
          <input
            className="brick-filter"
            autoFocus
            placeholder="Type"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && types[0]) link.onNew(types[0]);
            }}
          />
          <div className="brick-types">
            {types.map((t) => (
              <button key={t.id} className="ghost" onClick={() => link.onNew(t)}>
                {t.icon} {t.name}
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

/** What `@@` may suggest: the documents this reader can open, and their types. */
type Links = { all: ArticleView[]; templates: Template[]; selfId?: string };

/**
 * A text field with a right-click menu for the document's elements — and
 * suggestions after `@@` for links.
 */
function useBricks(gm: boolean, text: string, onText: (t: string) => void, links: Links, flow?: RefObject<FlowHandle>) {
  const own = useRef<HTMLTextAreaElement>(null);
  // A flow field takes the place of the text field where there is one.
  const field: RefObject<HTMLTextAreaElement | FlowHandle | null> = flow ?? own;
  // Where the right click was — and what was selected then, to format.
  const [menu, setMenu] = useState<{ x: number; y: number; sel: [number, number] } | null>(null);
  // Choosing an image: where the caret stood, to put it there.
  const [picking, setPicking] = useState<{ start: number; end: number } | null>(null);
  const worldData = useWorldData();
  const sectionBelow = useContext(SectionBelow);
  const cutSections = useContext(CutSections);
  const mentions = useMentions({ field, text, onText, ...links });
  // The toolbar acts on the field focused last — with its text as it stands now.
  const toolbar = useContext(ToolbarTarget);
  const actions = useRef<FieldActions | null>(null);
  actions.current = {
    format: (f) => {
      const at = field.current;
      if (!at) return;
      const [next, a, b] = formatted(text, at.selectionStart ?? 0, at.selectionEnd ?? 0, f);
      onText(next);
      queueMicrotask(() => {
        field.current?.focus();
        field.current?.setSelectionRange(a, b);
      });
    },
    brick: (b) => {
      const at = field.current;
      if (!at) return;
      if (b.picksImage) setPicking({ start: at.selectionStart ?? 0, end: at.selectionEnd ?? 0 });
      else insertAt(at, text, b, onText);
    },
    link: () => openLinkMenu(),
  };
  /** What selected words may link to — none where they cannot be a link. */
  const linkFor = ([from, to]: [number, number]): LinkChoice | undefined => {
    const word = text.slice(from, to);
    if (to <= from || !word.trim() || word.length > 80 || /[\n@|]/.test(word)) return undefined;
    const put = (written: string) => {
      setMenu(null);
      onText(text.slice(0, from) + written + text.slice(to));
      const at = from + written.length;
      // After the field has drawn the link — before, the place would be counted in the old text.
      requestAnimationFrame(() => {
        field.current?.focus();
        field.current?.setSelectionRange(at, at);
      });
    };
    // Its own words stay what one reads where they differ from the name.
    const label = (name: string) => (name === word.trim() ? "" : word);
    return {
      word: word.trim(),
      found: namedLike(links.all, word, links.selfId),
      templates: links.templates,
      onLink: (a) => put(linkTo(links.templates, a, label(a.name))),
      types:
        worldData && worldData.world.isGm && !worldData.previewing
          ? links.templates.filter((t) => mayCreate(worldData.world, t)).sort((a, b) => a.name.localeCompare(b.name))
          : [],
      onNew: (t) => {
        if (!worldData) return;
        setMenu(null);
        const pattern = t.namePattern || "%%input%%";
        const count = nextNumber(pattern, links.all.filter((a) => a.articleType === t.id).map((a) => a.name));
        const name = nameFrom(pattern, word.trim(), count);
        makeDocument(worldData.world.id, links.templates, links.all, t, name)
          .then(() => {
            put(linkText(typeNames(links.templates, t.id)[0] ?? "", name, label(name), ""));
            worldData.onChanged();
          })
          .catch((e) => worldData.onError?.(e));
      },
    };
  };
  /** Sections: the selection marked as one, a link to one put in at the selection. */
  const resolve = linker(links.all, links.templates);
  const sectionsFor = ([from, to]: [number, number]): SectionChoice => {
    const put = (next: string, at: number) => {
      setMenu(null);
      onText(next);
      requestAnimationFrame(() => {
        field.current?.focus();
        field.current?.setSelectionRange(at, at);
      });
    };
    return {
      mark:
        to > from
          ? (name) => {
              const base = anchorName(name);
              if (!base) return;
              // Unique in the document: a second of the same name gets a number.
              const taken = new Set(sectionsOf(links.all.filter((a) => a.id === links.selfId), resolve).map((s) => s.anchor));
              let anchor = base;
              for (let n = 2; taken.has(anchor) || anchorsIn(text).some((a) => a.anchor === anchor); n++) anchor = `${base}-${n}`;
              const next = markSection(text, from, to, anchor);
              if (next) put(next, to);
            }
          : undefined,
      found: sectionsOf(links.all, resolve),
      onLink: (s) => {
        const words = text.slice(from, to);
        const shown = words.trim() && !/[\n@|]/.test(words) ? words : s.shown;
        const written =
          s.article.id === links.selfId
            ? `${MARK}#^${s.anchor}|${shown.trim()}${MARK}`
            : linkText(typeNames(links.templates, s.article.articleType)[0] ?? "", s.article.name, shown, `^${s.anchor}`);
        put(text.slice(0, from) + written + text.slice(to), from + written.length);
      },
    };
  };
  /** The selection — or, where nothing is selected, the word the caret stands in. */
  const chosenWords = (): [number, number] => {
    const f = field.current;
    const a = f?.selectionStart ?? 0;
    const b = f?.selectionEnd ?? a;
    if (b > a) return [a, b];
    const left = /[\p{L}\p{N}'’-]*$/u.exec(text.slice(0, a))?.[0].length ?? 0;
    const right = /^[\p{L}\p{N}'’-]*/u.exec(text.slice(a))?.[0].length ?? 0;
    return [a - left, a + right];
  };
  /** The menu at the selection — or the word at the caret — with its links on top. */
  function openLinkMenu() {
    const f = field.current;
    if (!f) return;
    const sel = chosenWords();
    const at = caretPoint(f, sel[0]);
    setMenu({ x: at.x, y: at.y + 4, sel });
  }
  /** The keys of the field: bold, italic, a link — and a key per type, as the options set them. */
  const keyDown = (e: React.KeyboardEvent) => {
    mentions.handlers.onKeyDown(e);
    if (e.defaultPrevented) return;
    const combo = comboOf(e);
    if (!combo) return;
    if (combo === "ctrl+b" || combo === "ctrl+i") {
      e.preventDefault();
      actions.current?.format({ wrap: combo === "ctrl+b" ? "**" : "*" });
    } else if (combo === "ctrl+k") {
      e.preventDefault();
      openLinkMenu();
    } else if (worldData) {
      const shortcut = loadShortcuts(worldData.world.id).find((x) => x.combo === combo);
      const type = shortcut && links.templates.find((t) => t.id === shortcut.type);
      if (!type) return;
      e.preventDefault();
      const choice = linkFor(chosenWords());
      if (!choice) return;
      // The document of that name and type — or a new one, where the GM may make it.
      const named = links.all.find(
        (a) => a.id !== links.selfId && isA(links.templates, a.articleType, type.id) && a.name.toLowerCase() === choice.word.toLowerCase(),
      );
      if (named) choice.onLink(named);
      else if (choice.types.some((t) => t.id === type.id)) choice.onNew(type);
    }
  };
  const props = {
    ref: own,
    ...mentions.handlers,
    onKeyDown: keyDown,
    onFocus: (e: React.FocusEvent<HTMLElement>) =>
      toolbar?.current?.({
        actions,
        el: e.currentTarget,
        caret: () => (field.current ? caretPoint(field.current, field.current.selectionStart ?? 0) : null),
        selected: () => !!field.current && (field.current.selectionEnd ?? 0) > (field.current.selectionStart ?? 0),
      }),
    onBlur: () => {
      mentions.handlers.onBlur();
      toolbar?.current?.(null);
    },
    onContextMenu: (e: React.MouseEvent) => {
      e.preventDefault();
      const f = field.current;
      setMenu({ x: e.clientX, y: e.clientY, sel: [f?.selectionStart ?? 0, f?.selectionEnd ?? 0] });
    },
  };
  const element = (
    <>
      {menu && (
        <BrickMenu
          x={menu.x}
          y={menu.y}
          gm={gm}
          onPick={(b) => {
            setMenu(null);
            if (b.picksImage) {
              if (field.current) setPicking({ start: field.current.selectionStart, end: field.current.selectionEnd });
              return;
            }
            if (field.current) insertAt(field.current, text, b, onText);
          }}
          onFormat={
            menu.sel[1] > menu.sel[0]
              ? (f) => {
                  const [next, a, b] = formatted(text, menu.sel[0], menu.sel[1], f);
                  setMenu(null);
                  onText(next);
                  queueMicrotask(() => {
                    field.current?.focus();
                    field.current?.setSelectionRange(a, b);
                  });
                }
              : undefined
          }
          onSection={
            sectionBelow
              ? () => {
                  setMenu(null);
                  sectionBelow();
                }
              : undefined
          }
          link={linkFor(menu.sel)}
          sections={sectionsFor(menu.sel)}
          onHide={
            menu.sel[1] > menu.sel[0]
              ? () => {
                  const [from, to] = menu.sel;
                  setMenu(null);
                  // A region stands on lines of its own.
                  const before = from > 0 && text[from - 1] !== "\n" ? "\n" : "";
                  const after = to < text.length && text[to] !== "\n" ? "\n" : "";
                  onText(`${text.slice(0, from)}${before}<block view=gm edit=gm level=gm-only>\n${text.slice(from, to)}\n</block>${after}${text.slice(to)}`);
                }
              : undefined
          }
          onCut={(() => {
            // On a heading: each heading as high a section of its own.
            const from = text.lastIndexOf("\n", menu.sel[0] - 1) + 1;
            const level = headingLevel(text.slice(from, text.indexOf("\n", from) < 0 ? text.length : text.indexOf("\n", from)));
            return cutSections && level > 0
              ? {
                  level,
                  run: () => {
                    setMenu(null);
                    cutSections(text, level);
                  },
                }
              : undefined;
          })()}
          onClose={() => setMenu(null)}
        />
      )}
      {picking && worldData && (
        <ImagePicker
          world={worldData.world.id}
          onChosen={(id, image) => {
            const at = picking;
            setPicking(null);
            if (!field.current) return;
            field.current.setSelectionRange(at.start, at.end);
            insertAt(field.current, text, { name: "Image", text: imageTag(id, image) }, onText);
          }}
          onCancel={() => setPicking(null)}
        />
      )}
      {mentions.element}
    </>
  );
  return { props, element };
}

// --- Code -----------------------------------------------------------------

/** Properties as a tree: each by itself, or a group with what lies in it. */
type Run<T> =
  | { kind: "item"; p: T; i: number }
  | { kind: "group"; name: string; path: string; items: Run<T>[]; count: number };

/**
 * Properties in runs: those of one group together, under its name — a
 * group in a group inside it. The others each on their own.
 */
function sections<T>(list: T[], pathOf: (x: T) => string[]): Run<T>[] {
  const root: Run<T>[] = [];
  list.forEach((p, i) => {
    let into = root;
    const path = pathOf(p);
    path.forEach((name, depth) => {
      const last = into[into.length - 1];
      const group =
        last?.kind === "group" && last.name === name
          ? last
          : (() => {
              const fresh: Run<T> = { kind: "group", name, path: path.slice(0, depth + 1).join(" › "), items: [], count: 0 };
              into.push(fresh);
              return fresh;
            })();
      if (group.kind !== "group") return;
      group.count++;
      into = group.items;
    });
    into.push({ kind: "item", p, i });
  });
  return root;
}

/** The properties on top — `--- … ---` and the blank lines after — and the rest. */
function splitProperties(text: string): [string, string] {
  const head = /^(?:[ \t]*\n)*---[ \t]*\n(?:[\s\S]*?\n)?---[ \t]*(?:\n|$)\n*/.exec(text)?.[0] ?? "";
  return [head, text.slice(head.length)];
}

function CodeEditor({
  text,
  errors,
  gm,
  links,
  propsOpen,
  marks,
  carets,
  onPropsOpen,
  onText,
  onCaret,
}: {
  text: string;
  errors: LineError[];
  gm: boolean;
  links: Links;
  propsOpen: boolean;
  /** What differs from a version, in the code. */
  marks: { kind: MarkKind; ranges: Range[] };
  /** Where the others write, in the code. */
  carets: RemoteCaret[];
  onPropsOpen: (open: boolean) => void;
  onText: (t: string) => void;
  onCaret: (c: Caret) => void;
}) {
  const numbers = useRef<HTMLDivElement>(null);
  const [head, rest] = splitProperties(text);
  // Folded, the field holds only what comes after the properties; the line
  // numbers still count from the top, so errors keep their line.
  const folded = !propsOpen && head !== "";
  const shown = folded ? rest : text;
  const onShown = (t: string) => onText(folded ? head + t : t);
  const offset = folded ? head.split("\n").length - 1 : 0;
  const count = head.split("\n").filter((l) => l.trim() && l.trim() !== "---").length;

  // Whoever types the properties in by hand should not see them vanish the
  // moment the closing --- stands there.
  const hadHead = useRef(head !== "");
  useEffect(() => {
    if (head !== "" && !hadHead.current) onPropsOpen(true);
    hadHead.current = head !== "";
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [head !== ""]);

  const bricks = useBricks(gm, shown, onShown, links);
  const caret = useSteadyCaret(bricks.props.ref, shown);
  const errorLines = new Set(errors.map((e) => e.line));
  // Folded, the field starts after the properties.
  const skip = folded ? head.length : 0;
  const report = (e: React.SyntheticEvent<HTMLTextAreaElement>) =>
    onCaret({ code: text, start: e.currentTarget.selectionStart + skip, end: e.currentTarget.selectionEnd + skip });
  const theirs = carets
    .map((c) => ({ ...c, start: c.start - skip, end: c.end - skip }))
    .filter((c) => c.end >= 0);
  const shownMarks = marks.ranges
    .map((m) => ({ start: m.start - skip, end: m.end - skip }))
    .filter((m) => m.end > 0)
    .map((m) => ({ start: Math.max(0, m.start), end: m.end }));
  return (
    <div className="text-side">
      {head !== "" && (
        <button className="fold code-fold ghost compact" onClick={() => onPropsOpen(!propsOpen)} aria-expanded={propsOpen}>
          {propsOpen ? "▾" : "▸"} Properties <small className="small">{count}</small>
        </button>
      )}
      <div className="text-field">
        <div className="line-numbers" ref={numbers} aria-hidden="true">
          {shown.split("\n").map((_, i) => (
            <span key={i} className={errorLines.has(i + 1 + offset) ? "stuck" : ""}>
              {i + 1 + offset}
            </span>
          ))}
        </div>
        <div className="caret-box">
        <textarea
          {...bricks.props}
          {...caret}
          onSelect={(e) => {
            caret.onSelect();
            report(e);
          }}
          onKeyUp={(e) => {
            caret.onKeyUp();
            report(e);
          }}
          className="template-text"
          value={shown}
          onChange={(e) => {
            const field = e.currentTarget;
            const closed = withClosingTag(shown, field.value, field.selectionStart);
            onShown(closed?.text ?? field.value);
            if (closed) queueMicrotask(() => field.setSelectionRange(closed.caret, closed.caret));
          }}
          onScroll={(e) => {
            if (numbers.current) numbers.current.scrollTop = e.currentTarget.scrollTop;
          }}
          spellCheck={false}
          placeholder="Write here. @@ links a document, right-click inserts lists, images …"
        />
        <MarkedField field={bricks.props.ref} text={shown} ranges={shownMarks} kind={marks.kind} />
        <RemoteCarets field={bricks.props.ref} carets={theirs} />
        </div>
      </div>
      {errors.length > 0 && (
        <ul className="error-list">
          {errors.map((e, i) => (
            <li key={i}>
              <span>{e.line > 0 ? `Line ${e.line}` : "Document"}</span>
              {e.text}
            </li>
          ))}
        </ul>
      )}
      {bricks.element}
    </div>
  );
}

// --- Visual ---------------------------------------------------------------

/** Exported for the test bench, which draws it on documents in memory. */
export function VisualEditor({
  world,
  article,
  templates,
  all,
  doc,
  propsOpen,
  onPropsOpen,
  links,
  carets,
  marks,
  onDoc,
  onCaret,
}: {
  world: WorldInfo;
  article: ArticleView;
  templates: Template[];
  all: ArticleView[];
  doc: DocumentText;
  propsOpen: boolean;
  onPropsOpen: (open: boolean) => void;
  links: Links;
  /** Where the others write, by part. */
  carets: (part: string) => RemoteCaret[];
  /** What changed against a version, by part. */
  marks: { kind: MarkKind; of: (part: string) => Range[] };
  onDoc: (d: DocumentText) => void;
  onCaret: (c: Caret) => void;
}) {
  const type = typeOf(templates, article.articleType);
  const fieldFor = (name: string) => propertyNamed(type, name);
  const worldData = useWorldData();
  const reload = worldData?.onChanged;
  // The block chosen by its title — its settings in the side bar.
  const [picked, setPicked] = useState<string | null>(null);
  useSideBar(`block:${picked ?? ""}`, !!picked, () => setPicked(null));
  const pickedBlock = picked ? type?.blocks.find((x) => x.key === picked) : undefined;
  // A section of the document's own — not the type's: its settings are the document's.
  const typed = (key: string) => !!type?.blocks.some((x) => x.key === key);
  const pickedOwn = picked && !pickedBlock ? doc.blocks.find((x) => x.key === picked && !typed(x.key)) : undefined;
  // A new section gets its key when first saved: the side bar keeps to it by
  // where it stands.
  const pickedAt = useRef(-1);
  useEffect(() => {
    if (!picked) return;
    const at = doc.blocks.findIndex((x) => x.key === picked);
    if (at >= 0) pickedAt.current = at;
    else if (!typed(picked) && doc.blocks[pickedAt.current] && !typed(doc.blocks[pickedAt.current].key)) setPicked(doc.blocks[pickedAt.current].key);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [doc, picked]);
  /**
   * A part's text cut at its headings of one level: what stands before the
   * first stays, each heading starts a section of the document's own below
   * — with the part's access.
   */
  const cutSections = (b: number, p: number, text: string, level: number) => {
    const block = doc.blocks[b];
    const part = block.parts[p];
    const { before, sections } = cutAtHeadings(text, level);
    if (sections.length === 0) return;
    const stamp = Date.now();
    const fresh: TemplateBlock[] = sections.map((s, i) => ({
      key: `new-${stamp}-${i}`,
      title: s.title,
      style: "text",
      // As large as the heading was: the page looks as before.
      heading: Math.min(level, 4),
      parts: [{ key: `new-${stamp}-${i}`, text: s.text, view: part.view, edit: part.edit, level: part.level }],
    }));
    const kept = { ...block, parts: block.parts.map((x, i) => (i === p ? { ...x, text: before } : x)) };
    onDoc({ ...doc, blocks: [...doc.blocks.slice(0, b), kept, ...fresh, ...doc.blocks.slice(b + 1)] });
  };
  /** A new section of the document's own, below `after`. */
  const ownSectionBelow = (after: string) => {
    const key = `new-${Date.now()}`;
    const block: TemplateBlock = {
      key,
      title: "New section",
      style: "text",
      parts: [{ key, text: "", view: doc.access.view, edit: doc.access.edit, level: "open" }],
    };
    const at = doc.blocks.findIndex((x) => x.key === after) + 1;
    onDoc({ ...doc, blocks: [...doc.blocks.slice(0, at), block, ...doc.blocks.slice(at)] });
    pickedAt.current = at;
    setPicked(key);
  };
  const [moveFailed, setMoveFailed] = useState("");
  const graphs = useGraphEdit(world, article, templates, all, doc, onDoc, reload);

  /**
   * The fields for the properties some element binds — in the infobox or
   * in a piece of text. Reading shows their values; here they are changed.
   */
  const bound = (shown: { property: string; label: string }[]) => {
    const rows = shown.flatMap((s) => {
      // The infobox names a property by key; the document by its name.
      const name = (fieldFor(s.property)?.label ?? s.property).toLowerCase();
      const i = doc.properties.findIndex((p) => p.name.toLowerCase() === name);
      return i < 0 ? [] : [{ i, label: s.label || doc.properties[i].name }];
    });
    if (rows.length === 0) return null;
    return (
      <dl className="bound-fields">
        {rows.map(({ i, label }) => (
          <div key={i} className="property-row">
            <dt>{label}</dt>
            <dd>
              <PropertyInput
                field={fieldFor(doc.properties[i].name)}
                value={doc.properties[i].value}
                templates={templates}
                articleType={article.articleType}
                stored={article}
                selfId={article.id}
                all={all}
                links={links}
                onValue={(value) => setProperty(i, { value })}
              />
            </dd>
          </div>
        ))}
      </dl>
    );
  };

  const setProperty = (i: number, p: Partial<Property>) =>
    onDoc({
      ...doc,
      properties: doc.properties.map((x, j) => (j === i ? { ...x, ...p } : x)),
    });

  /**
   * A section renamed: its anchors and this document's links to it here,
   * the links of other documents to it there — each saved as a version.
   */
  const renameSection = async (old: string, fresh: string) => {
    onDoc({ ...doc, blocks: doc.blocks.map((block) => ({ ...block, parts: block.parts.map((part) => ({ ...part, text: renameInText(part.text, old, fresh) })) })) });
    const resolve = linker(links.all, links.templates);
    const others = links.all.filter((a) => a.id !== article.id && a.layers.some((l) => (l.text ?? "").includes(`#^${old}`)));
    for (const other of others) {
      const opened = await api.documentOpen(world.id, other.id);
      const next = relinked(opened.code, old, fresh, (target) => {
        const link = linkIn(target);
        return !!link && resolve(link)?.id === article.id;
      });
      if (next === opened.code) continue;
      const session = newSession();
      await api.documentSave(world.id, other.id, next, opened.base, session);
      await api.sessionEnd(world.id, session);
    }
    if (others.length) reload?.();
  };
  /** Links of other documents to a section of this one. */
  const linkedElsewhere = (anchor: string) =>
    links.all.filter((a) => a.id !== article.id && a.layers.some((l) => (l.text ?? "").includes(`@@${article.id}#^${anchor}`))).length;
  /** A section's anchors taken out — the text stays. */
  const removeSection = (anchor: string) =>
    onDoc({ ...doc, blocks: doc.blocks.map((block) => ({ ...block, parts: block.parts.map((part) => ({ ...part, text: withoutAnchor(part.text, anchor) })) })) });
  /** Every anchor of the document, to keep names apart. */
  const anchorsHere = () => new Set(doc.blocks.flatMap((block) => block.parts.flatMap((part) => anchorsIn(part.text).map((a) => a.anchor))));
  const sectionActions: SectionActions = { rename: renameSection, remove: removeSection, linkedElsewhere, taken: anchorsHere };

  const setPart = (b: number, p: number, part: Partial<BlockPart>) =>
    onDoc({
      ...doc,
      blocks: doc.blocks.map((block, i) =>
        i !== b
          ? block
          : { ...block, parts: block.parts.map((x, j) => (j === p ? { ...x, ...part } : x)) },
      ),
    });

  const removePart = (b: number, p: number) =>
    onDoc({
      ...doc,
      blocks: doc.blocks.map((x, i) =>
        i === b ? { ...x, parts: x.parts.filter((_, j) => j !== p) } : x,
      ),
    });

  const base = article.visibility;

  // Whoever edits has every part of the text before them.
  const readableParts = new Set(doc.blocks.flatMap((b) => b.parts.map((p) => p.key)));
  // A list from a property picks into the text of that property, here.
  const resolve = linker(all, templates);
  const propertyIndex = (key: string) => {
    const field = type?.properties.find((f) => f.key === key);
    return field ? doc.properties.findIndex((p) => p.name.toLowerCase() === field.label.toLowerCase()) : -1;
  };
  /** A property's picks as the text has them: each link, its id, the words after it. */
  const entries = (value: string) => {
    const out: { id: string; text: string; count: string }[] = [];
    for (const piece of scanLinks(value)) {
      if (typeof piece !== "string") {
        const a = resolve(piece);
        out.push({ id: a?.id ?? "", text: a ? linkTo(templates, a) : linkText(piece.of, piece.name), count: "" });
      } else if (out.length) out[out.length - 1].count = piece.replace(/,/g, " ").trim();
    }
    return out;
  };
  const listEditing = {
    count: (key: string, id: string) => {
      const i = propertyIndex(key);
      return i < 0 ? "" : (entries(doc.properties[i].value).find((e) => e.id === id)?.count ?? "");
    },
    setCount: (key: string, id: string, n: string) => {
      const i = propertyIndex(key);
      const field = type?.properties.find((f) => f.key === key);
      const picked = all.find((a) => a.id === id);
      if (i < 0 || !picked) return;
      let es = entries(doc.properties[i].value);
      es = es.some((e) => e.id === id) ? es.map((e) => (e.id === id ? { ...e, count: n } : e)) : [...es, { id, text: linkTo(templates, picked), count: n }];
      // Every document is there anyway: only a number says anything.
      if (field?.kind === "reference" && field.every) es = es.filter((e) => e.count.trim() !== "" && Number(e.count) !== 0);
      setProperty(i, { value: es.map((e) => e.text + (e.count.trim() ? ` ${e.count.trim()}` : "")).join(", ") });
    },
    ids: (key: string) => {
      const i = propertyIndex(key);
      return i < 0 ? [] : linksIn(doc.properties[i].value).flatMap((l) => resolve(l)?.id ?? []);
    },
    set: (key: string, ids: string[]) => {
      const i = propertyIndex(key);
      if (i < 0) return;
      const value = ids.flatMap((id) => all.filter((a) => a.id === id)).map((a) => linkTo(templates, a)).join(", ");
      setProperty(i, { value });
    },
  };
  // A group's layout while writing: its values are changed where they
  // stand — the level in a class, a feat, a score — as in the properties.
  const saved = useMemo(() => summedValues(templates, all, article), [templates, all, article]);
  const propertyAt = (field: TemplateField) => doc.properties.findIndex((p) => p.name.toLowerCase() === field.label.toLowerCase());
  const valueOf = (name: string): React.ReactNode => {
    const field = fieldFor(name);
    if (!field) return name.trim().toLowerCase() === "name" ? doc.name : <code className="value-name">%%{name}%%</code>;
    const i = propertyAt(field);
    if (i < 0 || field.kind === "image" || field.kind === "relation")
      return <PropertyValue field={field} value={saved[field.key]} articles={all} templates={templates} articleType={article.articleType} />;
    return (
      <span className="sheet-input">
        <PropertyInput
          field={field}
          value={doc.properties[i].value}
          templates={templates}
          articleType={article.articleType}
          stored={article}
          selfId={article.id}
          all={all}
          links={links}
          onValue={(value) => setProperty(i, { value })}
        />
      </span>
    );
  };
  // A score's own value, beside the tile that shows what it comes to.
  const sheetEdit = (name: string): React.ReactNode => {
    const field = fieldFor(name);
    const i = field ? propertyAt(field) : -1;
    if (!field || i < 0) return null;
    return (
      <input
        className="ability-own"
        inputMode="decimal"
        value={doc.properties[i].value}
        title={`${field.label} — own value`}
        onChange={(e) => setProperty(i, { value: e.target.value })}
      />
    );
  };
  // Rolls and the ability row work while writing too — on the numbers as saved.
  const rolls: RollScope = useMemo(
    () => ({
      document: article.name,
      lookup: lookupFor(templates, all, article),
      explain: (name) => explain(sumOf(templates, all, article, name)),
      numbers: (type?.properties ?? []).filter((p) => p.kind === "number").map((p) => p.label),
      secret: false,
    }),
    [templates, all, article, type],
  );
  const [openGroups, setOpenGroups] = useState<string[]>([]);
  // A group folded under its name; open, what lies in it, one step further in.
  const drawRuns = (runs: Run<Property>[], depth: number): React.ReactNode =>
    runs.map((r) => {
      if (r.kind === "item") return propertyRow(r.p, r.i);
      const open = openGroups.includes(r.path);
      return (
        <div key={`group-${r.path}`} className="property-group" style={{ ["--depth" as string]: depth + 1 }}>
          <button
            className="fold ghost compact"
            onClick={() => setOpenGroups((all) => (open ? all.filter((g) => g !== r.path) : [...all, r.path]))}
            aria-expanded={open}
          >
            {open ? "▾" : "▸"} {r.name} <small className="small">{r.count}</small>
          </button>
          {open && drawRuns(r.items, depth + 1)}
        </div>
      );
    });
  const propertyRow = (p: Property, i: number) => {
    const field = fieldFor(p.name);
    return (
      <div key={i} className="property-row">
        <dt>
          {field ? (
            p.name
          ) : (
            <input
              className="property-name"
              value={p.name}
              onChange={(e) => setProperty(i, { name: e.target.value })}
            />
          )}
        </dt>
        <dd>
          <PropertyInput
            field={field}
            value={p.value}
            templates={templates}
            articleType={article.articleType}
            stored={article}
            selfId={article.id}
            all={all}
            links={links}
            onValue={(value) => setProperty(i, { value })}
          />
          {!field && (
            <button
              className="bubble-x"
              title="Remove this property"
              onClick={() =>
                onDoc({ ...doc, properties: doc.properties.filter((_, j) => j !== i) })
              }
            >
              ×
            </button>
          )}
        </dd>
      </div>
    );
  };
  // A block as it reads, its parts written in place. A block on `top`
  // stands above the infobox, as on the page.
  const drawBlock = (written: TemplateBlock, b: number) => {
    // Title, style, place and folding are the type's — as it stands now,
    // so a change in the side bar shows at once.
    const shape = type?.blocks.find((x) => x.key === written.key);
    const block = shape ? { ...written, title: shape.title, style: shape.style, place: shape.place, width: shape.width, fold: shape.fold, heading: shape.heading, new: shape.new } : written;
    const own = !typed(block.key);
    const pickable = world.isGm && (own || !!ownerOf(templates, article.articleType, block.key));
    const below = !pickable
      ? null
      : own
        ? () => ownSectionBelow(block.key)
        : () =>
          addSection(world.id, templates, article.articleType, block.key)
            .then(({ all, key }) => {
              setMoveFailed("");
              worldData?.onTemplates?.(all);
              worldData?.onChanged();
              setPicked(key);
            })
            .catch((e) => setMoveFailed(String(e)));
    return (
    <SectionBelow.Provider key={block.key} value={below}>
    <FoldBlock
      key={block.key}
      article={article.id}
      block={block}
      onPick={pickable ? () => setPicked(block.key) : undefined}
      picked={picked === block.key}
      access={
        // Only what differs from the document's — the rest goes as the document does.
        block.parts[0] && !(samePolicy(block.parts[0].view, doc.access.view) && samePolicy(block.parts[0].edit, doc.access.edit)) && (
          <AccessLine
            world={world}
            view={samePolicy(block.parts[0].view, doc.access.view) ? undefined : block.parts[0].view}
            edit={samePolicy(block.parts[0].edit, doc.access.edit) ? undefined : block.parts[0].edit}
            onView={world.isGm ? (view) => setPart(b, 0, { view }) : undefined}
            onEdit={world.isGm ? (edit) => setPart(b, 0, { edit }) : undefined}
          />
        )
      }
      onMove={
        pickable && !own
          ? (key, after) =>
              moveBlock(world.id, templates, article.articleType, key, block.key, after)
                .then((all) => {
                  setMoveFailed("");
                  worldData?.onTemplates?.(all);
                })
                .catch((e) => setMoveFailed(String(e)))
          : undefined
      }
    >
      {block.parts.map((part, p) => (
            <div key={part.key}>
              {/* Only from the last part: what follows it would stand before the new sections. */}
              <CutSections.Provider value={own && pickable && p === block.parts.length - 1 ? (text, level) => cutSections(b, p, text, level) : null}>
              <PartEditor
                world={world}
                part={part}
                title={block.title}
                inherited={part.view === base && part.level === "open"}
                sections={sectionActions}
                docAccess={doc.access}
                region={
                  // The first part's stands at the section's title.
                  p > 0 && !(samePolicy(part.view, doc.access.view) && samePolicy(part.edit, doc.access.edit))
                }
                removable={world.isGm && block.parts.length > 1}
                links={links}
                carets={carets(part.key)}
                marks={{ kind: marks.kind, ranges: marks.of(part.key) }}
                onPart={(x) => setPart(b, p, x)}
                onRemove={() => removePart(b, p)}
                onCaret={(start, end) => onCaret({ part: part.key, start, end })}
                graph={(g, i) => <RelationGraph query={g} edit={{ ...graphs.edit, onGraph: graphs.graphFor(b, p, i) }} />}
              />
              </CutSections.Provider>
              {bound(boundIn(part.text))}
            </div>
      ))}
    </FoldBlock>
    </SectionBelow.Provider>
    );
  };
  // In the order and the places the type gives them now — a move in the
  // side bar or by dragging shows at once.
  const rank = (key: string) => {
    const i = type?.blocks.findIndex((x) => x.key === key) ?? -1;
    return i < 0 ? Infinity : i;
  };
  const arranged = (where: BlockPlace) =>
    doc.blocks
      .map((block, b) => ({ block, b }))
      .filter(({ block }) => (type?.blocks.find((x) => x.key === block.key)?.place ?? block.place ?? "flow") === where)
      .sort((x, y) => rank(x.block.key) - rank(y.block.key) || x.b - y.b)
      .map(({ block, b }) => drawBlock(block, b));
  return (
    <CurrentDocument.Provider value={article.id}>
    <ListEditing.Provider value={listEditing}>
    <ReadableParts.Provider value={readableParts}>
    <RollScopeContext.Provider value={rolls}>
    <ValuesProvider value={valueOf}>
    <SheetEdit.Provider value={sheetEdit}>
    <div className="visual-editor">
      {moveFailed && <p className="small error">{moveFailed}</p>}
      {pickedBlock && (
        <BlockSettings
          key={pickedBlock.key}
          world={world}
          templates={templates}
          articleType={article.articleType}
          block={pickedBlock}
          onClose={() => setPicked(null)}
          onAdded={setPicked}
          access={(() => {
            const b = doc.blocks.findIndex((x) => x.key === pickedBlock.key);
            return b < 0 ? null : <SectionAccess world={world} block={doc.blocks[b]} onPart={(p, patch) => setPart(b, p, patch)} />;
          })()}
        />
      )}
      {pickedOwn && (
        <OwnSectionSettings
          key={pickedOwn.key}
          block={pickedOwn}
          doc={doc}
          own={(key) => !typed(key)}
          onDoc={onDoc}
          onBelow={() => ownSectionBelow(pickedOwn.key)}
          onClose={() => setPicked(null)}
        />
      )}
      <section className={propsOpen ? "properties" : "properties folded"}>
        <button className="fold ghost compact" onClick={() => onPropsOpen(!propsOpen)} aria-expanded={propsOpen}>
          {propsOpen ? "▾" : "▸"} Properties <small className="small">{doc.properties.length}</small>
        </button>
        {propsOpen && (
          <>
          {doc.properties.length === 0 && <p className="small muted">None yet.</p>}
          <dl>
            {drawRuns(sections(doc.properties, (p) => fieldFor(p.name)?.inGroup ?? []), 0)}
          </dl>
          <button
            className="compact"
            onClick={() =>
              onDoc({
                ...doc,
                properties: [...doc.properties, { name: uniqueName(doc.properties), value: "" }],
              })
            }
          >
            ＋ Property
          </button>
          </>
        )}
      </section>

      {arranged("top")}

      <div className={`article-body align-${type?.infobox.align ?? "none"}`}>
      {type && <InfoboxEdit
        world={world}
        article={article}
        type={type}
        doc={doc}
        templates={templates}
        all={all}
        links={links}
        onProperty={setProperty}
        onAccess={(a) => onDoc({ ...doc, infobox: samePolicy(a.view, doc.access.view) && samePolicy(a.edit, doc.access.edit) && (a.level ?? "open") === "open" ? undefined : a })}
        onChanged={reload}
      />}

      <div className="flowing-text">
      {arranged("flow")}
      </div>
      <SideColumn>{arranged("side")}</SideColumn>
      </div>
      {arranged("bottom")}
    </div>
    </SheetEdit.Provider>
    </ValuesProvider>
    </RollScopeContext.Provider>
    </ReadableParts.Provider>
    </ListEditing.Provider>
    </CurrentDocument.Provider>
  );
}

/**
 * A section of the document's own, chosen by its title: its name, where it
 * stands among the document's own sections, a new one below, or out. They are
 * the document's — the type's sections are set in `BlockSettings`.
 */
function OwnSectionSettings({
  block,
  doc,
  own,
  onDoc,
  onBelow,
  onClose,
}: {
  block: TemplateBlock;
  doc: DocumentText;
  own: (key: string) => boolean;
  onDoc: (d: DocumentText) => void;
  onBelow: () => void;
  onClose: () => void;
}) {
  const [title, setTitle] = useState(block.title);
  const patch = (p: Partial<TemplateBlock>) => onDoc({ ...doc, blocks: doc.blocks.map((b) => (b.key === block.key ? { ...b, ...p } : b)) });
  const world = useWorldData()?.world;
  const commit = () => title.trim() !== block.title && patch({ title: title.trim() });
  const mine = doc.blocks.filter((b) => own(b.key));
  const at = mine.findIndex((b) => b.key === block.key);
  const swap = (other: TemplateBlock) => {
    const i = doc.blocks.indexOf(block);
    const j = doc.blocks.indexOf(other);
    const blocks = [...doc.blocks];
    [blocks[i], blocks[j]] = [blocks[j], blocks[i]];
    onDoc({ ...doc, blocks });
  };
  return (
    <Inspector title={`Section · ${block.title || "untitled"}`} onClose={onClose}>
      {world && (
        <SectionAccess
          world={world}
          block={block}
          onPart={(p, x) => patch({ parts: block.parts.map((part, j) => (j === p ? { ...part, ...x } : part)) })}
        />
      )}
      <InspectorSection title="This document">
        <div className="inspector-access block-settings">
          <span className="small muted">Title</span>
          <input value={title} placeholder="none" onChange={(e) => setTitle(e.target.value)} onBlur={commit} onKeyDown={(e) => e.key === "Enter" && commit()} />
          <ShapeRows block={block} onPatch={patch} part="look" />
          <span className="small muted">Order</span>
          <div className="segmented">
            <button disabled={at <= 0} onClick={() => swap(mine[at - 1])}>
              ↑ Up
            </button>
            <button disabled={at < 0 || at >= mine.length - 1} onClick={() => swap(mine[at + 1])}>
              ↓ Down
            </button>
          </div>
          <ShapeRows block={block} onPatch={patch} part="more" />
        </div>
        <button className="compact" onClick={onBelow}>
          ＋ Section below
        </button>
        <button
          className="danger compact"
          disabled={doc.blocks.length <= 1}
          onClick={() => {
            onDoc({ ...doc, blocks: doc.blocks.filter((b) => b.key !== block.key) });
            onClose();
          }}
        >
          Remove the section
        </button>
      </InspectorSection>
    </Inspector>
  );
}

/**
 * The infobox while editing — where it stands and as it looks when read,
 * its values changed in place. An image field is no text: choosing one
 * writes it right away, as on the page.
 */
function InfoboxEdit({
  world,
  article,
  type,
  doc,
  templates,
  all,
  links,
  onProperty,
  onAccess,
  onChanged,
}: {
  world: WorldInfo;
  article: ArticleView;
  type: Template;
  doc: DocumentText;
  /** Who sees and changes the infobox — the GM's, like a section's. */
  onAccess: (a: Access) => void;
  templates: Template[];
  all: ArticleView[];
  links: Links;
  onProperty: (i: number, p: Partial<Property>) => void;
  onChanged?: () => void;
}) {
  const [picking, setPicking] = useState<string | null>(null);
  const box = type.infobox;
  const access = doc.infobox ?? doc.access;
  if (box.show.length === 0 || box.align === "none") return null;
  const stored = propertyValues(article);
  const number = Number(box.width);
  const style = Number.isFinite(number) && number > 0 ? { width: number } : undefined;
  const rows = box.show.flatMap((s) => {
    const field = propertyNamed(type, s.property);
    if (!field) return [];
    const label = s.label || field.label;
    if (field.kind === "image") {
      const value = stored[field.key];
      const id = typeof value === "string" && value.startsWith("img:") ? value : null;
      const sized = !!(field.width && field.height);
      return [
        <button
          key={field.key}
          className={`infobox-image ${field.align}${sized ? "" : " natural"} choosable`}
          style={sized ? { aspectRatio: `${field.width} / ${field.height}`, width: `${field.width}px`, maxWidth: "100%" } : { width: "100%" }}
          onClick={() => setPicking(field.key)}
          title="Choose an image"
        >
          {id ? <WorldImage world={world.id} id={id} alt={label} /> : <span className="image-placeholder">🖼</span>}
        </button>,
      ];
    }
    const i = doc.properties.findIndex((p) => p.name.toLowerCase() === field.label.toLowerCase());
    if (i < 0) return [];
    return [
      <div key={field.key} className="infobox-row">
        <dt>{label}</dt>
        <dd>
          <PropertyInput
            field={field}
            value={doc.properties[i].value}
            templates={templates}
            articleType={article.articleType}
            stored={article}
            selfId={article.id}
            all={all}
            links={links}
            onValue={(value) => onProperty(i, { value })}
          />
        </dd>
      </div>,
    ];
  });
  return (
    <aside className={`infobox ${box.align} ${style ? "fixed" : box.width} editing`} style={style}>
      {(world.isGm || access !== doc.access) && (
        <div className="infobox-access">
          <AccessLine
            world={world}
            view={access.view}
            edit={access.edit}
            onView={world.isGm ? (view) => onAccess({ ...access, view }) : undefined}
            onEdit={world.isGm ? (edit) => onAccess({ ...access, edit }) : undefined}
          />
        </div>
      )}
      <dl>{rows}</dl>
      {picking && (
        <ImagePicker
          world={world.id}
          onChosen={(image) => {
            const field = picking;
            setPicking(null);
            api.imageFieldSet(world.id, article.id, field, image).then(onChanged).catch(() => {});
          }}
          onRemove={
            stored[picking]
              ? () => {
                  const field = picking;
                  setPicking(null);
                  api.imageFieldSet(world.id, article.id, field, null).then(onChanged).catch(() => {});
                }
              : undefined
          }
          onCancel={() => setPicking(null)}
        />
      )}
    </aside>
  );
}

/**
 * Editing relations from a graph.
 *
 * What concerns the document being edited goes into its text, like any
 * change on the visual side — saving writes the other side. Everything else
 * is written right away, both sides at once.
 */
function useGraphEdit(
  world: WorldInfo,
  article: ArticleView,
  templates: Template[],
  all: ArticleView[],
  doc: DocumentText,
  onDoc: (d: DocumentText) => void,
  reload: (() => void) | undefined,
): { edit: GraphEdit; graphFor: (block: number, part: number, piece: number) => (next: GraphNode | null) => void } {
  // Several changes may come in one go — taking someone out removes each of
  // their lines, then their place. Each builds on the one before, not on
  // what this render saw.
  const latest = useRef(doc);
  latest.current = doc;
  const change = (next: DocumentText) => {
    latest.current = next;
    onDoc(next);
  };
  const own = relationFields(templates, article.articleType);
  const overrides = useMemo(() => {
    const resolve = linker(all, templates);
    const mine = new Map<string, RelationEntry[]>();
    for (const f of own) {
      const roles = roleChoices(templates, all, f);
      const p = doc.properties.find((x) => x.name.toLowerCase() === f.label.toLowerCase());
      const written = readRelations(p?.value ?? "").flatMap((w) => {
        const t = resolve(w.link);
        return t ? [{ with: t.id, role: findRole(roles, w.role)?.id ?? "", ownRole: findRole(roles, w.ownRole)?.id ?? "" }] : [];
      });
      // What this reader cannot see is not in the text — but still there.
      const locked = relationsOf(article, f.key).filter((e) => !all.some((a) => a.id === e.with));
      mine.set(f.key, [...written, ...locked]);
    }
    return new Map([[article.id, mine]]);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [doc.properties, all, templates, article]);

  const name = (id: string) => all.find((a) => a.id === id)?.name ?? "";
  /** Changes what this document's text says about `other`. */
  const writeOwn = (other: string, property: string, roles: [string, string] | null) => {
    const label = templates.flatMap((t) => t.properties).find((f) => f.key === property)?.label ?? "";
    const field = own.find((f) => f.key === property) ?? own.find((f) => f.label.toLowerCase() === label.toLowerCase());
    const target = all.find((a) => a.id === other);
    if (!field || !target) return false;
    const resolve = linker(all, templates);
    const doc = latest.current;
    const i = doc.properties.findIndex((p) => p.name.toLowerCase() === field.label.toLowerCase());
    const rows = readRelations(i >= 0 ? doc.properties[i].value : "");
    const entry: WrittenRelation | null = roles && {
      link: { of: typeNames(templates, target.articleType)[0] ?? "", name: target.name, property: "", text: target.name },
      role: name(roles[0]),
      ownRole: name(roles[1]),
    };
    const at = rows.findIndex((r) => resolve(r.link)?.id === other);
    const kept = rows.filter((r) => resolve(r.link)?.id !== other);
    if (entry) kept.splice(at >= 0 ? at : kept.length, 0, entry);
    const value = writeRelations(kept);
    change({
      ...doc,
      properties:
        i >= 0
          ? doc.properties.map((p, j) => (j === i ? { ...p, value } : p))
          : [...doc.properties, { name: field.label, value }],
    });
    return true;
  };

  return {
    edit: {
      overrides,
      onSet: async (changes) => {
        // What concerns this document goes into its text; the rest is
        // written in one go, and the world read again once.
        const rest = changes.filter(
          ({ from, to, property, roles }) =>
            !(from === article.id && writeOwn(to, property, roles)) &&
            !(to === article.id && writeOwn(from, property, roles && [roles[1], roles[0]])),
        );
        if (rest.length === 0) return;
        await api.relationsSet(
          world.id,
          rest.map((c) => ({ article: c.from, property: c.property, with: c.to, roles: c.roles })),
        );
        reload?.();
      },
    },
    // A graph writes its own text — the piece of the part it stands in.
    graphFor: (block, part, piece) => (next) => {
      const doc = latest.current;
      const rewrite = (text: string) =>
        graphPieces(text)
          .map((p, i) => (i !== piece ? p.text : next ? graphText(next) : ""))
          .join("");
      change({
        ...doc,
        blocks: doc.blocks.map((b, i) =>
          i !== block ? b : { ...b, parts: b.parts.map((x, j) => (j === part ? { ...x, text: rewrite(x.text) } : x)) },
        ),
      });
    },
  };
}

/** The properties the elements in a piece of text bind: `<select bind=Species>`. */
function boundIn(text: string): { property: string; label: string }[] {
  return [...text.matchAll(/<\w+\b[^>]*?\bbind=(?:"([^"]*)"|([^\s>]+))[^>]*>/g)].map((m) => {
    const label = /\blabel=(?:"([^"]*)"|([^\s>]+))/.exec(m[0]);
    return { property: (m[1] ?? m[2]).trim(), label: label ? (label[1] ?? label[2]) : "" };
  });
}

/** The ids a reference's value holds — one or several. */
const idsOfValue = (v: unknown): string[] => (Array.isArray(v) ? v.map(String) : v ? [String(v)] : []);

const uniqueName = (props: Property[]) => {
  let n = 1;
  const taken = new Set(props.map((p) => p.name.toLowerCase()));
  while (taken.has(n === 1 ? "property" : `property ${n}`)) n++;
  return n === 1 ? "Property" : `Property ${n}`;
};

/**
 * The input for one property — shaped by the field it stands for.
 *
 * A reference offers the documents of its type; picking one writes
 * `@@Type:Name@@` into the property, and so into the code.
 */
function PropertyInput({
  field,
  value,
  templates,
  articleType,
  stored,
  selfId,
  all,
  links,
  onValue,
}: {
  field: TemplateField | undefined;
  value: string;
  templates: Template[];
  articleType: string;
  /** The document as saved — what its text leaves out, it still holds. */
  stored?: ArticleView;
  selfId: string;
  all: ArticleView[];
  links: Links;
  onValue: (v: string) => void;
}) {
  const calendar = useWorldData()?.world.calendar ?? DEFAULT_CALENDAR;
  if (isRelation(field)) {
    return <RelationInput field={field} value={value} templates={templates} stored={stored} selfId={selfId} all={all} onValue={onValue} />;
  }
  if (field?.kind === "reference" && field.from) {
    // What points here: shown, never set — it changes where it points from.
    const self = all.find((a) => a.id === selfId);
    const ids = self ? idsOfValue(summedValues(templates, all, self)[field.key]) : [];
    const names = ids.map((id) => all.find((a) => a.id === id)?.name).filter(Boolean);
    return (
      <span className="small muted" title={`Set by their “${field.from}”`}>
        {names.length ? names.join(", ") : "—"}
      </span>
    );
  }
  if (field?.kind === "reference") {
    const target = targetOf(templates, articleType, field);
    const choices = target ? documentsOf(all, templates, target).filter((a) => a.id !== selfId) : [];
    const resolve = linker(all, templates);
    // Each link with what stands after it — a pick's own number: `@@Class:Fighter@@ 3`.
    const chosen: { link: Link; article: ArticleView | undefined; count: string }[] = [];
    for (const piece of scanLinks(value)) {
      if (typeof piece !== "string") chosen.push({ link: piece, article: resolve(piece), count: "" });
      else if (chosen.length) chosen[chosen.length - 1].count = piece.replace(/,/g, " ").trim();
    }
    // A link nobody here can resolve — a closed document, a typo in the
    // code — is written back as it stood instead of vanishing.
    const write = (items: typeof chosen) =>
      items
        .map((c) => (c.article ? linkTo(templates, c.article) : linkText(c.link.of, c.link.name)) + (field.count && c.count ? ` ${c.count}` : ""))
        .join(", ");
    // Every document of the type, always: only their numbers change, and only
    // a number other than 0 is written.
    if (field.every) {
      const countOf = (c: ArticleView) => chosen.find((x) => x.article?.id === c.id)?.count ?? "";
      return (
        <EveryPick
          items={choices.map((c) => ({ key: c.id, name: c.name, count: countOf(c) }))}
          count={field.count || "Number"}
          onCount={(i, n) => {
            const next = choices.map((c, j) => ({ link: { of: "", name: c.name, property: "", text: c.name }, article: c, count: j === i ? n.trim() : countOf(c) }));
            onValue(write(next.filter((c) => c.count !== "" && Number(c.count) !== 0)));
          }}
        />
      );
    }
    if (field.multiple) {
      const open = choices.filter((c) => !chosen.some((x) => x.article?.id === c.id));
      return (
        <MultiPick
          chosen={chosen.map((c, i) => ({ key: c.article?.id ?? `?${i}`, name: c.article?.name ?? `${c.link.name} (?)`, count: c.count }))}
          open={open}
          onRemove={(i) => onValue(write(chosen.filter((_, j) => j !== i)))}
          onAdd={(c) => onValue(write([...chosen, { link: { of: "", name: c.name, property: "", text: c.name }, article: c, count: field.count ? "1" : "" }]))}
          count={field.count || undefined}
          onCount={(i, n) => onValue(write(chosen.map((c, j) => (j === i ? { ...c, count: n.trim() } : c))))}
        />
      );
    }
    const current = chosen[0];
    const known = current?.article && choices.some((c) => c.id === current.article?.id);
    return (
      <select
        value={current ? (known ? current.article!.id : "?") : ""}
        onChange={(e) => {
          const picked = choices.find((c) => c.id === e.target.value);
          if (picked) onValue(linkTo(templates, picked));
          else if (e.target.value === "") onValue("");
        }}
      >
        <option value="">— none —</option>
        {current && !known && <option value="?">{current.link.name} (?)</option>}
        {choices.map((c) => (
          <option key={c.id} value={c.id}>
            {c.name}
          </option>
        ))}
      </select>
    );
  }
  if (field?.kind === "world-date") return <WrittenMomentPicker calendar={calendar} value={value} onValue={onValue} />;
  if (field?.kind === "date-time") return <OurMomentInput value={value} onValue={onValue} />;
  if (field?.kind === "number") {
    // Empty, a reckoned number shows what its formula comes to — to be
    // overwritten by a value of its own.
    const formula = formulaOf(field);
    const reckons = formula !== undefined && !value.trim() && stored ? sumOf(templates, all, stored, field.label).own : undefined;
    const hint = formula === undefined ? undefined : reckons === undefined ? "formula" : String(reckons);
    // Text, not a number field: that one shows 24 as “24,0” and swallows what it cannot read.
    return (
      <span className="number-input">
        <input
          inputMode="decimal"
          size={Math.max(2, (value || hint || "").length + 1)}
          value={value}
          placeholder={hint}
          title={formula === undefined ? undefined : `[[${formula}]]`}
          onChange={(e) => onValue(e.target.value)}
        />
        {field.unit && <span className="unit">{field.unit}</span>}
      </span>
    );
  }
  return (
    <TextProperty
      multiline={field?.kind === "text" && field.multiline}
      value={value}
      links={links}
      onValue={onValue}
    />
  );
}

/**
 * Relations, one line each: who, what they are to this document, and what
 * it is to them. Picking writes the property's text — and so the code.
 *
 * An entry whose person this reader cannot open is not in the text at all;
 * it stands here greyed out and cannot be changed. Saving keeps it.
 */
function RelationInput({
  field,
  value,
  templates,
  stored,
  selfId,
  all,
  onValue,
}: {
  field: RelationField;
  value: string;
  templates: Template[];
  stored?: ArticleView;
  selfId: string;
  all: ArticleView[];
  onValue: (v: string) => void;
}) {
  const resolve = linker(all, templates);
  const rows = readRelations(value);
  const people = documentsOf(all, templates, field.articleType).filter((a) => a.id !== selfId);
  const roles = roleChoices(templates, all, field);
  const name = (id: string) => all.find((a) => a.id === id)?.name ?? "???";
  const locked = (stored ? relationsOf(stored, field.key) : []).filter((e) => !all.some((a) => a.id === e.with));
  const saved = stored ? relationsOf(stored, field.key) : [];
  const write = (next: WrittenRelation[]) => onValue(writeRelations(next));
  const set = (i: number, part: Partial<WrittenRelation>) => write(rows.map((r, j) => (j === i ? { ...r, ...part } : r)));
  const linkFor = (a: ArticleView): WrittenRelation["link"] => ({
    of: typeNames(templates, a.articleType)[0] ?? "",
    name: a.name,
    property: "",
    text: a.name,
  });
  // Written, saved — and the other side says nothing back.
  const oneSided = (target: ArticleView | undefined) => {
    if (!target || !saved.some((e) => e.with === target.id)) return false;
    const theirs = relationFields(templates, target.articleType).find(
      (f) => f.key === field.key || f.label.toLowerCase() === field.label.toLowerCase(),
    );
    return !!theirs && holdsRelations(target, theirs) && !relationsOf(target, theirs.key).some((e) => e.with === selfId);
  };
  const rolePicker = (text: string, onPick: (name: string) => void) => {
    const known = findRole(roles, text);
    return (
      <select value={known ? known.id : text ? "?" : ""} onChange={(e) => onPick(roles.find((r) => r.id === e.target.value)?.name ?? "")}>
        <option value="">—</option>
        {text && !known && <option value="?">{text} (?)</option>}
        {roles.map((r) => (
          <option key={r.id} value={r.id}>
            {r.name}
          </option>
        ))}
      </select>
    );
  };
  const present = new Set(rows.map((r) => resolve(r.link)?.id));
  // Whom one can still add: everyone of the property's type not listed yet.
  const addable = people.filter((p) => !present.has(p.id));
  return (
    <div className="relation-rows">
      {rows.map((row, i) => {
        const target = resolve(row.link);
        return (
          <div key={i} className="relation-row">
            <select
              value={target ? target.id : "?"}
              onChange={(e) => {
                const picked = people.find((p) => p.id === e.target.value);
                if (picked) set(i, { link: linkFor(picked) });
              }}
            >
              {!target && <option value="?">{row.link.name} (?)</option>}
              {people.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
            {rolePicker(row.role, (role) => set(i, { role }))}
            <span className="muted">/</span>
            {rolePicker(row.ownRole, (ownRole) => set(i, { ownRole }))}
            {oneSided(target) && (
              <span className="small muted" title="The other document does not say it back">
                one-sided
              </span>
            )}
            <button className="bubble-x" title="Remove" onClick={() => write(rows.filter((_, j) => j !== i))}>
              ×
            </button>
          </div>
        );
      })}
      {locked.map((e, i) => (
        <div key={`locked-${i}`} className="relation-row locked" title="Not yours to change">
          <span>???</span>
          <span>{e.role ? name(e.role) : "—"}</span>
          <span className="muted">/</span>
          <span>{e.ownRole ? name(e.ownRole) : "—"}</span>
        </div>
      ))}
      {addable.length > 0 ? (
        <select
          className="compact"
          value=""
          onChange={(e) => {
            const picked = people.find((p) => p.id === e.target.value);
            if (picked) write([...rows, { link: linkFor(picked), role: "", ownRole: "" }]);
          }}
        >
          <option value="">＋ Relation</option>
          {addable.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      ) : (
        <span className="small muted">Related to everyone already.</span>
      )}
    </div>
  );
}

/** A property written as text — where `@@` links a document, as in the body. */
function TextProperty({
  multiline,
  value,
  links,
  onValue,
}: {
  multiline: boolean;
  value: string;
  links: Links;
  onValue: (v: string) => void;
}) {
  const area = useRef<HTMLTextAreaElement>(null);
  const line = useRef<HTMLInputElement>(null);
  const mentions = useMentions({ field: multiline ? area : line, text: value, onText: onValue, ...links });
  return (
    <>
      {multiline ? (
        <textarea
          {...mentions.handlers}
          ref={area}
          className="grows"
          rows={3}
          value={value}
          onChange={(e) => onValue(e.target.value)}
        />
      ) : (
        <input {...mentions.handlers} ref={line} size={Math.max(4, value.length + 1)} value={value} onChange={(e) => onValue(e.target.value)} />
      )}
      {mentions.element}
    </>
  );
}

/**
 * One part of a section: its text, with what stands in it. Who may see and
 * change it is set in the side bar — it opens for the game master as soon
 * as the caret is in the part, unless something else holds the bar.
 */
/** What a section chosen by its anchor can be given: a new name, or none. */
type SectionActions = {
  rename: (old: string, fresh: string) => Promise<void>;
  remove: (anchor: string) => void;
  /** How many other documents link to it. */
  linkedElsewhere: (anchor: string) => number;
  /** The anchors the document has. */
  taken: () => Set<string>;
};

/** A section in the side bar: its name to change, and taking it out. */
function SectionInspector({ anchor, actions, onRenamed, onClose }: { anchor: string; actions: SectionActions; onRenamed: (fresh: string) => void; onClose: () => void }) {
  const [name, setName] = useState(anchor);
  const [problem, setProblem] = useState("");
  const [busy, setBusy] = useState(false);
  const elsewhere = actions.linkedElsewhere(anchor);
  const rename = async () => {
    const fresh = anchorName(name);
    if (!fresh || fresh === anchor) return setName(anchor);
    if (actions.taken().has(fresh)) return setProblem(`“${fresh}” is taken in this document.`);
    setBusy(true);
    setProblem("");
    try {
      await actions.rename(anchor, fresh);
      onRenamed(fresh);
    } catch (e) {
      setProblem(String(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Inspector title="Section" onClose={onClose}>
      <InspectorSection title="Name">
        <input
          value={name}
          disabled={busy}
          onChange={(e) => setName(e.target.value)}
          onBlur={() => void rename()}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
        />
        <small className="small muted">
          ^{anchorName(name) || anchor}
          {elsewhere > 0 && ` · linked from ${elsewhere} other ${elsewhere === 1 ? "document" : "documents"} — renamed there too`}
        </small>
        {problem && <small className="small error">{problem}</small>}
      </InspectorSection>
      <button
        className="danger compact inspector-remove"
        disabled={busy}
        title={elsewhere > 0 ? "Its links then lead to the document only" : undefined}
        onClick={() => {
          actions.remove(anchor);
          onClose();
        }}
      >
        Remove the section
      </button>
    </Inspector>
  );
}

function PartEditor({
  world,
  part,
  title,
  inherited,
  removable,
  links,
  carets,
  marks,
  onPart,
  onRemove,
  onCaret,
  graph,
  region,
  docAccess,
  sections,
}: {
  world: WorldInfo;
  part: BlockPart;
  /** The section's title — to name the part by. */
  title: string;
  inherited: boolean;
  /** A region with access of its own: says whom it is for at its top. */
  region?: boolean;
  /** The document's access — what a region's differs from. */
  docAccess: { view: string; edit: string };
  /** What is done to a section chosen by its anchor — across the document. */
  sections: SectionActions;
  removable: boolean;
  links: Links;
  carets: RemoteCaret[];
  marks: { kind: MarkKind; ranges: Range[] };
  onPart: (p: Partial<BlockPart>) => void;
  onRemove: () => void;
  onCaret: (start: number, end: number) => void;
  /** Draws a graph standing in the text — the `i`-th piece of it. */
  graph: (g: GraphNode, i: number) => React.ReactNode;
}) {
  const guarded = !inherited;
  // The part is one field: its text, with the images and graphs standing in
  // it — as surfaces, where the text runs past them as it does when read.
  const shown = graphPieces(part.text);
  const images = shown.filter((p) => p.kind === "image");
  // A part that holds nothing but one image, with access of its own: the
  // others see its stand-in.
  const ownPart = !inherited && images.length === 1 && shown.every((p) => p.kind === "image" || !p.text.trim());
  /** The text with the `i`-th piece written anew — or taken out. */
  const withPiece = (i: number, text: string | null) =>
    shown.map((p, j) => (j !== i ? p.text : text === null ? "" : text + (p.text.endsWith("\n") ? "\n" : ""))).join("");
  // Paragraphs and elements in order — what an element moves past.
  const units = useMemo(
    () =>
      shown.flatMap((p, j) =>
        p.kind === "text"
          ? p.text.split(/\n[ \t]*\n/).map((t) => ({ text: t.replace(/^\n+|\n+$/g, ""), piece: -1 })).filter((u) => u.text.trim())
          : [{ text: p.text.replace(/^\n+|\n+$/g, ""), piece: j }],
      ),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [part.text],
  );
  /** Element `i` past the paragraph or element above (-1) or below (1). */
  const moves = (i: number) => {
    const at = units.findIndex((u) => u.piece === i);
    const by = (step: -1 | 1) => {
      const to = at + step;
      if (at < 0 || to < 0 || to >= units.length) return undefined;
      return () => {
        const next = [...units];
        [next[at], next[to]] = [next[to], next[at]];
        onPart({ text: next.map((u) => u.text).join("\n\n") + "\n" });
      };
    };
    return { up: by(-1), down: by(1) };
  };
  const flow = useRef(new FlowHandle());
  // Nothing but one element in it: the element's access is the part's.
  const surfaces = shown.filter((p) => p.kind !== "text");
  const alone = surfaces.length === 1 && shown.every((p) => p.kind !== "text" || !p.text.trim());
  const [inspecting, setInspecting] = useState(false);
  // A section chosen by a click on its anchor: its name in the side bar, its place marked.
  const [chosenSection, setChosenSection] = useState<string | null>(null);
  useSideBar(`anchor:${part.key}`, !!chosenSection, () => setChosenSection(null));
  const sectionPlace = chosenSection ? anchorRange(part.text, chosenSection) : null;
  // As in the right-click menu: a new section below this one.
  const sectionBelow = useContext(SectionBelow);
  const [dismissed, setDismissed] = useState(false);
  useSideBar(`part:${part.key}`, inspecting, () => {
    setInspecting(false);
    setDismissed(false);
  });
  // The caret in the part opens its access — taking the bar from anything but
  // the versions, which one keeps open on purpose.
  const caretIn = (start: number, end: number) => {
    onCaret(start, end);
    const holder = sideBarOwner();
    // Only a section of several parts has a part's own access to set; one
    // of a single part sets it on the section.
    if (world.isGm && removable && !inspecting && !dismissed && !chosenSection && !holder?.startsWith("versions:")) setInspecting(true);
  };
  return (
    <PartAccessContext.Provider value={{ world, part, title, alone, onPart, onRemove: removable ? onRemove : undefined }}>
    <div
      className={`block-part${guarded ? " guarded" : ""}${inspecting ? " inspected" : ""}${
        part.level !== "open" ? ` level-${part.level}` : ""
      }`}
    >
      {region && (
        <RegionAccess world={world} view={part.view} edit={part.edit} docView={docAccess.view} docEdit={docAccess.edit} onClick={world.isGm ? () => (setDismissed(false), setInspecting(true)) : undefined} />
      )}
      {inspecting && (
        <Inspector title={title ? `Part · ${title}` : "Part"} onClose={() => (setInspecting(false), setDismissed(true))}>
          <AccessSection />
          {sectionBelow && (
            <InspectorSection title="Section">
              <button className="compact" onClick={sectionBelow}>
                ＋ Section below
              </button>
            </InspectorSection>
          )}
          {removable && (
            <button className="danger compact inspector-remove" onClick={onRemove}>
              Remove this part
            </button>
          )}
        </Inspector>
      )}
      {chosenSection && (
        <SectionInspector
          key={chosenSection}
          anchor={chosenSection}
          actions={sections}
          onRenamed={setChosenSection}
          onClose={() => setChosenSection(null)}
        />
      )}
      <PartFlow
        world={world}
        text={part.text}
        flow={flow}
        links={links}
        carets={carets}
        marks={marks}
        onAnchor={(name) => {
          setInspecting(false);
          setChosenSection(name);
        }}
        section={sectionPlace}
        surface={(piece, i) => (
          <PieceMoveContext.Provider value={moves(i)}>
          {piece.kind === "graph" ? (
            <div className="part-graph">{graph(piece.graph, i)}</div>
          ) : piece.kind === "list" ? (
            <ListCard list={piece.list} onChange={(next) => onPart({ text: withPiece(i, next) })} />
          ) : piece.kind === "timeline" ? (
            <TimeCard tag="timeline" attrs={piece.timeline} onChange={(next) => onPart({ text: withPiece(i, next) })} />
          ) : piece.kind === "calendar" ? (
            <TimeCard tag="calendar" attrs={piece.calendar} onChange={(next) => onPart({ text: withPiece(i, next) })} />
          ) : piece.kind === "create" ? (
            <CreateCard attrs={piece.create} onChange={(next) => onPart({ text: withPiece(i, next) })} />
          ) : piece.kind === "abilities" ? (
            <AbilitiesCard attrs={piece.abilities} onChange={(next) => onPart({ text: withPiece(i, next) })} />
          ) : piece.kind === "view" ? (
            <ViewCard attrs={piece.view} onChange={(next) => onPart({ text: withPiece(i, next) })} />
          ) : piece.kind === "embed" ? (
            <EmbedCard attrs={piece.embed} onChange={(next) => onPart({ text: withPiece(i, next) })} />
          ) : piece.kind === "map" ? (
            <MapCard world={world} map={piece.map} onChange={(next) => onPart({ text: withPiece(i, next ? mapText(next) : null) })} />
          ) : piece.kind === "image" ? (
            <ImageCard
              world={world}
              image={piece.image}
              ownPart={ownPart}
              onChange={(next) => onPart({ text: withPiece(i, next ? writeImageLine(next) : null) })}
            />
          ) : null}
          </PieceMoveContext.Provider>
        )}
        onText={(text) => onPart({ text })}
        onCaret={caretIn}
      />
    </div>
    </PartAccessContext.Provider>
  );
}

/**
 * The text of a part as one flow field — with the right-click bricks, the
 * `@@` suggestions, the marks of a comparison and the carets of others.
 */
function PartFlow({
  world,
  text,
  flow,
  links,
  carets,
  marks,
  surface,
  onText,
  onCaret,
  onAnchor,
  section,
}: {
  world: WorldInfo;
  text: string;
  flow: RefObject<FlowHandle>;
  links: Links;
  carets: RemoteCaret[];
  marks: { kind: MarkKind; ranges: Range[] };
  surface: (piece: Piece, i: number) => React.ReactNode;
  onText: (text: string) => void;
  onCaret: (start: number, end: number) => void;
  /** A click on an anchor — see `FlowField`. */
  onAnchor?: (name: string) => void;
  /** The chosen section's place in the text, marked. */
  section?: { start: number; end: number } | null;
}) {
  const bricks = useBricks(world.isGm, text, onText, links, flow);
  const { ref: _unused, ...handlers } = bricks.props;
  void _unused;
  // A link as it is read: the name it names — or its own text — and grey where it leads nowhere.
  const resolve = useMemo(() => linker(links.all, links.templates), [links.all, links.templates]);
  const current = useContext(CurrentDocument);
  const linkLook = (written: string) => {
    const link = linkIn(written.slice(MARK.length, -MARK.length));
    if (!link) return { text: written, dead: true };
    // `#^2f9e17`: a line of the target — of this document without a name.
    const anchor = link.property.startsWith("^") ? link.property.slice(1) : undefined;
    const target = anchor && !link.name ? links.all.find((a) => a.id === current) : resolve(link);
    return {
      text: link.text !== link.name ? link.text : (target?.name ?? link.name),
      dead: !target,
      id: target?.id,
      anchor,
      icon: target && linkIcon(links.templates, target.articleType),
    };
  };
  return (
    <>
      <FlowField
        handle={flow}
        text={text}
        className="block-text"
        placeholder="Write here. @@ links a document, right-click inserts lists, images …"
        surface={surface}
        linkLook={linkLook}
        onAnchor={onAnchor}
        surfaceClass={(p) =>
          p.kind === "image"
            ? imageSurface(p.image)
            : p.kind
        }
        onText={onText}
        onCaret={onCaret}
        handlers={handlers}
      >
        <FlowMarks handle={flow} text={text} ranges={marks.ranges} kind={marks.kind} />
        {section && <FlowMarks handle={flow} text={text} ranges={[section]} kind="section" />}
        <RemoteCarets field={flow} carets={carets} />
      </FlowField>
      {bricks.element}
    </>
  );
}

