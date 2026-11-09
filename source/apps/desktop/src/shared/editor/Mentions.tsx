import { FlowHandle } from "./FlowField";
import { useEffect, useMemo, useState, type RefObject } from "react";
import type { ArticleView, Template, TemplateField } from "../../api/client";
import { linker, linkIn, linkTo, MARK } from "../Links";
import { isA, typeOf as effectiveType } from "../types";

/**
 * Suggestions after `@@` — the way to write a link without knowing its form.
 *
 * `@@` offers types and documents. `@@Species:` narrows to the documents of
 * that type (and those below it); what follows filters them by name. Picking
 * a type writes `@@Species:` and keeps suggesting; picking a document writes
 * the whole link, closing `@@` included.
 *
 * Text of its own — “Elf” linking to the document “Elves” — comes two ways:
 * select the word and type `@`, then pick; or type it after a `|`:
 * `@@Elves|Elf`, then pick.
 */

type Field = HTMLTextAreaElement | HTMLInputElement | FlowHandle;

/** Whether the field has the focus — a flow field is focused through its root. */
const focused = (f: Field) => (f instanceof FlowHandle ? f.focused : document.activeElement === f);

/** The `@@…` being typed right before the caret. */
type Query = {
  /** Where the `@@` stands. */
  start: number;
  /** The caret. */
  end: number;
  /** The type typed so far — `null` while there is no colon yet. */
  of: string | null;
  name: string;
  /** The text typed after `|` — shown instead of the name. */
  label: string;
  /** What is typed after `#` — `null` while there is no `#`. */
  property: string | null;
};

type Choice =
  | { kind: "type"; type: Template }
  | { kind: "document"; article: ArticleView; type: Template | undefined }
  /** After `#`: a property of the document before it. */
  | { kind: "property"; article: ArticleView; field: TemplateField };

/** The link still open before the caret: a `@@` on this line not yet closed. */
function queryAt(text: string, caret: number): Query | null {
  const lineStart = text.lastIndexOf("\n", caret - 1) + 1;
  const line = text.slice(lineStart, caret);
  let at = -1;
  let from = 0;
  for (;;) {
    const open = line.indexOf(MARK, from);
    if (open < 0) break;
    const close = line.indexOf(MARK, open + MARK.length);
    if (close < 0) {
      at = open;
      break;
    }
    // A finished link is passed over; what is none may open the next one.
    from = linkIn(line.slice(open + MARK.length, close)) ? close + MARK.length : close;
  }
  const inner = at < 0 ? "" : line.slice(at + MARK.length);
  if (at < 0 || inner.length > 80 || /^\s/.test(inner)) return null;
  const bar = inner.indexOf("|");
  const whole = bar >= 0 ? inner.slice(0, bar) : inner;
  const hash = whole.lastIndexOf("#");
  const target = hash >= 0 ? whole.slice(0, hash) : whole;
  const colon = target.indexOf(":");
  return {
    start: lineStart + at,
    end: caret,
    of: colon >= 0 ? target.slice(0, colon) : null,
    name: colon >= 0 ? target.slice(colon + 1) : target,
    label: bar >= 0 ? inner.slice(bar + 1).trim() : "",
    property: hash >= 0 ? whole.slice(hash + 1) : null,
  };
}

/** Matches first that start with what was typed, then those that contain it. */
function rank<T>(items: T[], name: (t: T) => string, typed: string): T[] {
  const q = typed.trim().toLowerCase();
  return items
    .map((item) => ({ item, at: name(item).toLowerCase().indexOf(q) }))
    .filter((x) => x.at >= 0)
    .sort((x, y) => (x.at === 0) === (y.at === 0) ? name(x.item).localeCompare(name(y.item)) : x.at === 0 ? -1 : 1)
    .map((x) => x.item);
}

function choicesFor(query: Query, all: ArticleView[], templates: Template[], selfId?: string): Choice[] {
  const documents = all.filter((a) => a.id !== selfId);
  const typeOf = (a: ArticleView) => templates.find((t) => t.id === a.articleType);
  // After `#`: the properties of the document named before it.
  if (query.property !== null) {
    const article = linker(all, templates)({ of: query.of ?? "", name: query.name });
    const type = article && effectiveType(templates, article.articleType);
    if (!article || !type) return [];
    return rank(type.properties, (p) => p.label, query.property)
      .slice(0, 12)
      .map((field) => ({ kind: "property", article, field }));
  }
  if (query.of === null) {
    const types = rank(templates.filter((t) => !t.group), (t) => t.name, query.name).slice(0, 5);
    const docs = rank(documents, (a) => a.name, query.name).slice(0, 8);
    return [
      ...types.map((type): Choice => ({ kind: "type", type })),
      ...docs.map((article): Choice => ({ kind: "document", article, type: typeOf(article) })),
    ];
  }
  const of = query.of.trim().toLowerCase();
  const exact = templates.filter((t) => t.name.toLowerCase() === of);
  const types = exact.length > 0 ? exact : templates.filter((t) => t.name.toLowerCase().startsWith(of));
  const inTypes = documents.filter((a) => types.some((t) => isA(templates, a.articleType, t.id)));
  return rank(inTypes, (a) => a.name, query.name)
    .slice(0, 12)
    .map((article) => ({ kind: "document", article, type: typeOf(article) }));
}

/** Where the caret stands on screen — measured with an invisible copy of the field. */
export function caretPoint(field: Field, index: number): { x: number; y: number } {
  if (field instanceof FlowHandle) {
    const r = field.rectAt(index);
    return r ? { x: r.left, y: r.bottom } : { x: 0, y: 0 };
  }
  const style = getComputedStyle(field);
  const copy = document.createElement("div");
  for (const p of [
    "boxSizing", "width", "borderTopWidth", "borderRightWidth", "borderBottomWidth",
    "borderLeftWidth", "paddingTop", "paddingRight", "paddingBottom", "paddingLeft",
    "fontStyle", "fontVariant", "fontWeight", "fontSize", "lineHeight", "fontFamily",
    "letterSpacing", "wordSpacing", "tabSize", "textIndent",
  ] as const) {
    copy.style[p] = style[p];
  }
  Object.assign(copy.style, {
    position: "absolute",
    visibility: "hidden",
    top: "0",
    left: "-9999px",
    whiteSpace: field instanceof HTMLInputElement ? "pre" : "pre-wrap",
    overflowWrap: "break-word",
  });
  copy.textContent = field.value.slice(0, index);
  const mark = document.createElement("span");
  mark.textContent = "​";
  copy.appendChild(mark);
  document.body.appendChild(copy);
  const box = field.getBoundingClientRect();
  const line = parseFloat(style.lineHeight) || parseFloat(style.fontSize) * 1.4;
  const point = {
    x: box.left + parseFloat(style.borderLeftWidth) + mark.offsetLeft - field.scrollLeft,
    y: box.top + parseFloat(style.borderTopWidth) + mark.offsetTop - field.scrollTop + line,
  };
  copy.remove();
  return point;
}

export function useMentions({
  field,
  text,
  onText,
  all,
  templates,
  selfId,
}: {
  field: RefObject<Field | null>;
  text: string;
  onText: (t: string) => void;
  all: ArticleView[];
  templates: Template[];
  selfId?: string;
}) {
  const [caret, setCaret] = useState<number | null>(null);
  const [active, setActive] = useState(0);
  // Escape closes the list for this one `@@` — the next one opens it again.
  const [dismissed, setDismissed] = useState<number | null>(null);
  // A selected word `@` was typed over: it becomes the link's text.
  const [selected, setSelected] = useState<{ start: number; text: string } | null>(null);

  const readCaret = () => {
    const f = field.current;
    setCaret(f && focused(f) ? f.selectionStart : null);
  };
  // After typing: the caret moved with the text.
  useEffect(readCaret, [text]); // eslint-disable-line react-hooks/exhaustive-deps

  const query = useMemo(() => {
    const q = caret === null ? null : queryAt(text, caret);
    return q && q.start !== dismissed ? q : null;
  }, [text, caret, dismissed]);
  const choices = useMemo(
    () => (query ? choicesFor(query, all, templates, selfId) : []),
    [query, all, templates, selfId],
  );
  useEffect(() => setActive(0), [query?.start, query?.of, query?.name]);
  useEffect(() => {
    if (query && selected && query.start !== selected.start) setSelected(null);
  }, [query, selected]);
  const label = query?.label || (query && selected?.start === query.start ? selected.text : "");

  const replace = (from: number, to: number, written: string) => {
    const next = text.slice(0, from) + written + text.slice(to);
    const at = from + written.length;
    onText(next);
    setCaret(at);
    queueMicrotask(() => {
      field.current?.focus();
      field.current?.setSelectionRange(at, at);
    });
  };

  const pick = (choice: Choice) => {
    if (!query) return;
    if (choice.kind === "type") {
      // Keeps suggesting: the documents of this type come next.
      replace(query.start, query.end, `${MARK}${choice.type.name}:`);
    } else if (choice.kind === "property") {
      replace(query.start, query.end, linkTo(templates, choice.article, label, choice.field.label));
      setSelected(null);
    } else {
      replace(query.start, query.end, linkTo(templates, choice.article, label));
      setSelected(null);
    }
  };

  const open = query !== null && choices.length > 0;

  const handlers = {
    onSelect: readCaret,
    onClick: readCaret,
    onBlur: () => setCaret(null),
    onKeyDown: (e: React.KeyboardEvent) => {
      const f = field.current;
      if (e.key === "@" && f && f.selectionStart !== f.selectionEnd) {
        const from = f.selectionStart ?? 0;
        const to = f.selectionEnd ?? from;
        const word = text.slice(from, to);
        if (word.trim() && !/[\n@|]/.test(word)) {
          // The selection stays as the filter — and becomes the link's text.
          e.preventDefault();
          setSelected({ start: from, text: word });
          replace(from, to, `${MARK}${word}`);
          return;
        }
      }
      if (!open) return;
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        const step = e.key === "ArrowDown" ? 1 : -1;
        setActive((a) => (a + step + choices.length) % choices.length);
      } else if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        pick(choices[Math.min(active, choices.length - 1)]);
      } else if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        if (selected?.start === query.start) {
          // Nothing picked: the word stands there again as it was.
          replace(query.start, query.end, selected.text);
          setSelected(null);
        } else setDismissed(query.start);
      }
    },
  };

  let element = null;
  if (open && field.current) {
    const point = caretPoint(field.current, query.start);
    element = (
      <div
        className="mention-menu"
        style={{
          left: Math.max(8, Math.min(point.x, window.innerWidth - 290)),
          top: Math.min(point.y + 2, window.innerHeight - 260),
        }}
        // The field keeps the focus — otherwise the list would close itself.
        onMouseDown={(e) => e.preventDefault()}
      >
        {label && <div className="mention-label">Shown as “{label}”</div>}
        {choices.map((c, i) => (
          <button
            key={c.kind === "type" ? `t:${c.type.id}` : c.kind === "property" ? `p:${c.field.key}` : c.article.id}
            className={i === active ? "ghost on" : "ghost"}
            onMouseEnter={() => setActive(i)}
            onClick={() => pick(c)}
          >
            {c.kind === "type" ? (
              <>
                <span className="mention-icon">{c.type.icon || "📁"}</span>
                <span className="mention-name">{c.type.name}:</span>
                <span className="mention-hint">type</span>
              </>
            ) : c.kind === "property" ? (
              <>
                <span className="mention-icon">#</span>
                <span className="mention-name">{c.field.label}</span>
                <span className="mention-hint">of {c.article.name}</span>
              </>
            ) : (
              <>
                <span className="mention-icon">{c.type?.icon || "📄"}</span>
                <span className="mention-name">{c.article.name}</span>
                <span className="mention-hint">{c.type?.name ?? "page"}</span>
              </>
            )}
          </button>
        ))}
      </div>
    );
  }

  return { handlers, element };
}
