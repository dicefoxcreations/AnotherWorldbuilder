import { createContext, useContext, useEffect, useRef, useState, type RefObject } from "react";
import { createPortal } from "react-dom";
import { useBack } from "../../shared/useBack";
import { Icon } from "../../shared/ui/Icons";
import { useInWindow } from "../../shared/ui/PopupMenu";
import { type ArticleView, type Template } from "../../api/client";
import { makeDocument } from "./Article";
import { nameFrom, nextNumber } from "../templates/NamePattern";
import { linker, linkText, linkTo, MARK, scanLinks, typeNames, type Link } from "../../shared/Links";
import { anchorName, anchorsIn, markSection } from "../../shared/Anchors";
import { formatted } from "../../shared/editor/Format";
import { headingLevel } from "../../shared/Sections";
import { caretPoint, useMentions } from "../../shared/editor/Mentions";
import { isA, mayCreate, typeOf, articleById } from "../../shared/types";
import { comboOf, loadShortcuts } from "../../shared/Shortcuts";
import { useWorldData } from "../worlds/WorldData";
import { ImagePicker } from "../../shared/ui/ImagePicker";
import { imageTag } from "../../shared/ImageTag";
import { FlowHandle } from "../../shared/editor/FlowField";
import { byText } from "../../shared/Collate";
import { perLists } from "../../shared/Cache";

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
        text: "<block view=«gm» edit=gm>\n\n</block>",
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
export const SectionBelow = createContext<(() => void) | null>(null);
/** Cuts a part's text into sections of the document's own, at its headings of one level. */
export const CutSections = createContext<((text: string, level: number) => void) | null>(null);

/** What the toolbar acts on: the text field one writes in, and where it stands. */
type FieldActions = { format: (f: { wrap?: string; line?: string }) => void; brick: (b: Brick) => void; link: () => void };
export type ToolbarField = {
  actions: RefObject<FieldActions | null>;
  el: HTMLElement;
  /** The bottom of the selection's first line on screen — the toolbar stands above it. */
  caret: () => { x: number; y: number } | null;
  /** Whether words are selected — only then the toolbar shows. */
  selected: () => boolean;
};
export const ToolbarTarget = createContext<RefObject<((field: ToolbarField | null) => void) | null> | null>(null);

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
export function Toolbar({ target }: { target: RefObject<((field: ToolbarField | null) => void) | null> }) {
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

/** What a brick is put into: a text field, or a flow field that acts like one. */
type Caretful = { selectionStart: number | null; setSelectionRange: (a: number, b: number) => void; focus: () => void };

/**
 * Inserts a brick at the caret and selects its placeholder («…»), so one
 * types straight over it.
 */

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
  onOutsource,
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
  /** Selected text: moved into a new document, embedded where it stood — asked in a dialog. */
  onOutsource?: () => void;
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
  const box = useInWindow(x, y);
  return (
    <div
      ref={box}
      className="brick-menu"
      style={{ left: x, top: y }}
      onMouseDown={(e) => e.stopPropagation()}
      onContextMenu={(e) => e.preventDefault()}
    >
      {link && <LinkGroup link={link} />}
      {sections && <SectionGroup sections={sections} />}
      {onOutsource && (
        <div className="brick-group">
          <button className="ghost" onClick={onOutsource}>
            <Icon name="document" /> Move into a new document …
          </button>
        </div>
      )}
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
  /** Every section of the world this reader can open — looked for once asked. */
  found: () => SectionEntry[];
  onLink: (s: SectionEntry) => void;
};
type SectionEntry = { article: ArticleView; anchor: string; shown: string; line: string };

/**
 * Every anchor of the world — once per world as read: it goes through every
 * text, and a right click only offers it.
 */
const allSections = perLists((all: ArticleView[], templates: Template[]) => sectionsOf(all, linker(all, templates)));

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
    ? sections
        .found()
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

/** Marks that come in pairs around words: bold, italic, struck, code. */
const MARKS = /^[*_~`]$/;

/** Selected text to become a document of its own — of the type and name asked. */
type OutsourceChoice = {
  types: Template[];
  /** Whether a type has a text section for the words. */
  writable: (t: Template) => boolean;
  onMake: (t: Template, name: string) => void;
};

/** The selection into a new document: first its type, then its name. */
function OutsourceDialog({ outsource, onClose }: { outsource: OutsourceChoice; onClose: () => void }) {
  const [type, setType] = useState<Template | null>(null);
  const [filter, setFilter] = useState("");
  const [name, setName] = useState("");
  const q = filter.trim().toLowerCase();
  const types = outsource.types.filter((t) => !q || t.name.toLowerCase().includes(q));
  const make = () => type && name.trim() && outsource.onMake(type, name.trim());
  useBack(true, onClose);
  // A portal still passes React's events up to the editor it was opened from:
  // clicks, keys and focus here are the dialog's alone.
  const own = (e: React.SyntheticEvent) => e.stopPropagation();
  const stop = { onClick: own, onMouseDown: own, onMouseUp: own, onKeyDown: own, onKeyUp: own, onInput: own, onFocus: own, onBlur: own, onContextMenu: own, onPointerDown: own };
  return createPortal(
    <div className="dialog-backdrop" {...stop} onMouseDown={(e) => (e.stopPropagation(), onClose())}>
      <section
        className="dialog outsource-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="outsource-title"
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.key === "Escape" && (e.stopPropagation(), onClose())}
      >
        <div className="line between">
          <h3 id="outsource-title">Move into a new document</h3>
          <button className="ghost icon round" title="Close" onClick={onClose}>
            <Icon name="close" />
          </button>
        </div>
        {!type ? (
          <>
            <input
              autoFocus
              placeholder="Type"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              onKeyDown={(e) => {
                const first = types.find(outsource.writable);
                if (e.key === "Enter" && first) setType(first);
              }}
            />
            <div className="outsource-types">
              {types.map((t) => (
                <button key={t.id} className="ghost" disabled={!outsource.writable(t)} onClick={() => setType(t)}>
                  <span className="type-icon">{t.icon}</span> {t.name}
                  {!outsource.writable(t) && <small className="small muted">no text section</small>}
                </button>
              ))}
              {types.length === 0 && <p className="small muted">No type is called so.</p>}
            </div>
          </>
        ) : (
          <>
            <button className="ghost outsource-chosen" title="Another type" onClick={() => setType(null)}>
              <Icon name="left" /> <span className="type-icon">{type.icon}</span> {type.name}
            </button>
            <input autoFocus placeholder="Name" value={name} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && make()} />
            <button className="primary" disabled={!name.trim()} onClick={make}>
              Create
            </button>
          </>
        )}
      </section>
    </div>,
    document.body,
  );
}

/** The documents whose names hold the words, the closest first. */
export function namedLike(all: ArticleView[], word: string, selfId?: string): ArticleView[] {
  const q = word.trim().toLowerCase();
  const score = (a: ArticleView) => {
    const n = a.name.toLowerCase();
    return n === q ? 0 : n.startsWith(q) ? 1 : n.includes(q) ? 2 : q.includes(n) && n.length > 3 ? 3 : -1;
  };
  return all
    .filter((a) => a.id !== selfId)
    .map((a) => ({ a, s: score(a) }))
    .filter((x) => x.s >= 0)
    .sort((x, y) => x.s - y.s || byText(x.a.name, y.a.name))
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
              <Icon name="add" /> New …
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
export type Links = { all: ArticleView[]; templates: Template[]; selfId?: string };

/**
 * A text field with a right-click menu for the document's elements — and
 * suggestions after `@@` for links.
 */
export function useBricks(gm: boolean, text: string, onText: (t: string) => void, links: Links, flow?: RefObject<FlowHandle>) {
  const own = useRef<HTMLTextAreaElement>(null);
  // A flow field takes the place of the text field where there is one.
  const field: RefObject<HTMLTextAreaElement | FlowHandle | null> = flow ?? own;
  // Where the right click was — and what was selected then, to format.
  const [menu, setMenu] = useState<{ x: number; y: number; sel: [number, number] } | null>(null);
  // Selected text being moved into a new document: where it stands, while its type and name are asked.
  const [outsourcing, setOutsourcing] = useState<[number, number] | null>(null);
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
        worldData && !worldData.previewing
          ? links.templates.filter((t) => mayCreate(worldData.world, t)).sort((a, b) => byText(a.name, b.name))
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
      found: () => allSections(links.all, links.templates),
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
  /**
   * The selection into a new document of the type and name asked: its text
   * there, and here an embed of it — it stands where it stood, kept in one
   * place. A property of the new type that points at this document's type
   * names this document (an event's session).
   */
  const outsourceFor = ([start, end]: [number, number]): OutsourceChoice | undefined => {
    // Not inside a pair of marks — `**bold**` stays whole on either side.
    let [from, to] = [start, end];
    while (from < to && MARKS.test(text[from]) && MARKS.test(text[from - 1] ?? "")) from++;
    while (to > from && MARKS.test(text[to - 1]) && MARKS.test(text[to] ?? "")) to--;
    const words = text.slice(from, to).trim();
    if (to <= from || !words || !worldData || worldData.previewing) return undefined;
    const source = articleById(links.all, links.selfId);
    return {
      types: links.templates.filter((t) => mayCreate(worldData.world, t)).sort((a, b) => byText(a.name, b.name)),
      // The words need a text section there — with what the type inherits.
      writable: (t) => (typeOf(links.templates, t.id) ?? t).blocks.some((b) => b.parts.length > 0),
      onMake: (t, name) => {
        setMenu(null);
        setOutsourcing(null);
        // The first property that may name the source names it.
        const preset: Record<string, unknown> = {};
        const named = source && ((typeOf(links.templates, t.id) ?? t).properties ?? []).find((f) => f.kind === "reference" && isA(links.templates, source.articleType, f.articleType));
        if (named?.kind === "reference") preset[named.key] = named.multiple ? [source!.id] : source!.id;
        makeDocument(worldData.world.id, links.templates, links.all, t, name, preset, words)
          .then((id) => {
            // By its name — by its id where another document has the name.
            const of = links.all.some((a) => a.name === name) ? id : /[\s"=>]/.test(name) ? `"${name.replaceAll('"', "'")}"` : name;
            const before = from > 0 && text[from - 1] !== "\n" ? "\n\n" : from > 1 && text[from - 2] !== "\n" ? "\n" : "";
            const after = to < text.length && text[to] !== "\n" ? "\n\n" : to < text.length - 1 && text[to + 1] !== "\n" ? "\n" : "";
            onText(`${text.slice(0, from)}${before}<embed of=${of}>${after}${text.slice(to)}`);
            worldData.onChanged();
          })
          .catch((e) => worldData.onError?.(e));
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
          onOutsource={
            outsourceFor(menu.sel)
              ? () => {
                  setOutsourcing(menu.sel);
                  setMenu(null);
                }
              : undefined
          }
          onHide={
            menu.sel[1] > menu.sel[0]
              ? () => {
                  const [from, to] = menu.sel;
                  setMenu(null);
                  // A region stands on lines of its own.
                  const before = from > 0 && text[from - 1] !== "\n" ? "\n" : "";
                  const after = to < text.length && text[to] !== "\n" ? "\n" : "";
                  onText(`${text.slice(0, from)}${before}<block view=gm edit=gm>\n${text.slice(from, to)}\n</block>${after}${text.slice(to)}`);
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
      {outsourcing && outsourceFor(outsourcing) && (
        <OutsourceDialog outsource={outsourceFor(outsourcing)!} onClose={() => setOutsourcing(null)} />
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
