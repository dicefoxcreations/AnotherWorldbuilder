import { useEffect, useRef, useState } from "react";
import {
  api,
  FIELD_KINDS,
  type TemplateField,
  type ArticleView,
  type BlockPart,
  type Access,
  type Template,
  type LineError,
  type WorldInfo,
} from "../../api/client";
import { ArticleHead } from "../wiki/ArticleHead";
import { IconPicker } from "../../shared/ui/IconPicker";
import { MarkdownText } from "../wiki/MarkdownText";
import { VisibilityPicker } from "../../shared/ui/Visibility";
import { InfoboxForm } from "../wiki/InfoboxForm";
import { patternPreview } from "./NamePattern";
import { articleAccess, sameAccess, bracket } from "../../shared/AccessChange";
import { insertionSpot } from "../../shared/editor/Insert";
import { withClosingTag } from "../../shared/editor/Closing";
import { ValueList } from "./DataType";
import { BRICKS, type Brick } from "./TemplateLegend";
import { ancestors, effective, unfold, usersOf } from "../../shared/types";

/**
 * Defining a kind of article — as text, with a preview beside it.
 *
 * A type of article is defined rarely, and mostly by someone who knows what
 * they are building. For that one step typing is faster than clicking. So it
 * does not become flying blind, the right side shows at all times what comes
 * out when an article of this kind is created.
 *
 * What is stored is the structure, not the text: Rust reads it on every
 * keystroke and returns the template, or the errors with their line.
 */
export function TemplateEditor({
  world,
  draft,
  templates,
  articles,
  onChange,
  onSave,
  onBack,
  onArticlesChanged,
  onOpen,
  onError,
  saved,
}: {
  world: WorldInfo;
  draft: Template;
  /** All types of the world — for the supertype and what it passes down. */
  templates: Template[];
  /** For the values of a data type and for references in the preview. */
  articles: ArticleView[];
  onChange: (t: Template) => void;
  onSave: () => void;
  onBack: () => void;
  onArticlesChanged: () => void;
  /** Opens a document of this type — leaving the options. */
  onOpen: (id: string) => void;
  onError: (e: unknown) => void;
  saved: boolean;
}) {
  // The template in a ref: the reader below always needs the current state
  // as its base. Without it a text change overwrote the name just typed with
  // the old one — it could not be changed at all.
  const draftRef = useRef(draft);
  draftRef.current = draft;

  const [text, setText] = useState<string | null>(null);
  const [errors, setErrors] = useState<LineError[]>([]);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [repaired, setRepaired] = useState(false);
  const [side, setSide] = useState<"preview" | "documents">("preview");
  // Where the caret belongs after a repair. Without this it jumps to the end
  // of the text and you suddenly type below </article>.
  const caretRef = useRef<number | null>(null);
  const fieldRef = useRef<HTMLTextAreaElement>(null);
  const numbersRef = useRef<HTMLDivElement>(null);

  // After a repair, keep writing where one was.
  useEffect(() => {
    if (caretRef.current === null) return;
    const field = fieldRef.current;
    const position = Math.min(caretRef.current, field?.value.length ?? 0);
    caretRef.current = null;
    field?.focus();
    field?.setSelectionRange(position, position);
  }, [text]);

  // The note about the restored skeleton goes away on its own.
  useEffect(() => {
    if (!repaired) return;
    const timer = setTimeout(() => setRepaired(false), 6000);
    return () => clearTimeout(timer);
  }, [repaired]);

  // Translate the structure into text once, when opening.
  useEffect(() => {
    if (text !== null) return;
    api
      .templateWrite(world.id, draftRef.current)
      .then(setText)
      .catch(() => setText(""));
  }, [text]);

  // And back on every change. No waiting: the preview should keep up with
  // typing, and the call goes to Rust locally, not over the network.
  useEffect(() => {
    if (text === null) return;
    let unsubscribed = false;
    api
      .templateRead(world.id, text, draftRef.current)
      .then((result) => {
        if (unsubscribed) return;
        setErrors(result.errors);
        if (result.template) onChange(result.template);
        // The skeleton belongs to every article. If part of it was deleted,
        // the text comes back completed — with a word about it, so it does
        // not look as though the editor miscounted.
        if (result.repair !== null && result.repair !== text) {
          caretRef.current = fieldRef.current?.selectionStart ?? 0;
          setText(result.repair);
          setRepaired(true);
        }
      })
      .catch(() => {});
    return () => {
      unsubscribed = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text]);

  /**
   * Inserts a brick as a line of its own — in the body.
   *
   * Everything but the skeleton belongs between <body> and </body>. If the
   * cursor was elsewhere, the brick still lands there: a tag between
   * <article> and <body> would only be an error to clear away afterwards.
   *
   * The placeholder between «» is selected afterwards — so you type straight
   * over it without hunting for it.
   */
  const insert = (brick: Brick) => {
    const field = fieldRef.current;
    if (field === null || text === null) return;

    // A property goes on top, between --- — which comes into being with the
    // first one.
    if (brick.where === "properties") {
      const line = brick.line.replace(/[«»]/g, "");
      const lines = text.split("\n");
      const first = lines.findIndex((l) => l.trim() !== "");
      const closing =
        first >= 0 && lines[first].trim() === "---"
          ? lines.findIndex((l, i) => i > first && l.trim() === "---")
          : -1;
      const before = closing >= 0 ? lines.slice(0, closing).join("\n") + "\n" : "---\n";
      const after = closing >= 0 ? lines.slice(closing).join("\n") : `---\n\n${text}`;
      setText(`${before}${line}\n${after}`);
      setMenu(null);
      const open = brick.line.indexOf("«");
      const close = brick.line.indexOf("»");
      const start = before.length;
      queueMicrotask(() => {
        field.focus();
        if (open >= 0 && close > open) field.setSelectionRange(start + open, start + close - 1);
        else field.setSelectionRange(start + line.length, start + line.length);
      });
      return;
    }

    const { end, indent } = insertionSpot(
      text,
      field.selectionStart,
      brick.where === "head",
    );

    const raw = brick.line
      .split("\n")
      .map((z) => indent + z)
      .join("\n");
    const line = raw.replace(/[«»]/g, "");
    const open = raw.indexOf("«");
    const close = raw.indexOf("»");

    const before = text.slice(0, end);
    const fresh = `${before}\n${line}${text.slice(end)}`;
    setText(fresh);
    setMenu(null);

    const start = before.length + 1;
    queueMicrotask(() => {
      field.focus();
      if (open >= 0 && close > open) {
        field.setSelectionRange(start + open, start + close - 1);
      } else {
        field.setSelectionRange(start + line.length, start + line.length);
      }
    });
  };

  const errorLines = new Set(errors.map((f) => f.line).filter((z) => z > 0));

  // The others, without the stored version of this type — it is being
  // replaced by the draft. A supertype may be any type that does not already
  // lie below this one; otherwise the chain would bite its own tail.
  const others = templates.filter((t) => t.id !== draft.id);
  const possibleSupertypes = others.filter(
    (t) => !t.group && (!draft.id || !ancestors(templates, t).some((a) => a.id === draft.id)),
  );
  const group = draft.group;

  const goToLine = (nr: number) => {
    const field = fieldRef.current;
    if (!field || nr < 1) return;
    const lines = field.value.split("\n");
    const start = lines.slice(0, nr - 1).reduce((n, z) => n + z.length + 1, 0);
    field.focus();
    field.setSelectionRange(start, start + (lines[nr - 1]?.length ?? 0));
  };

  return (
    <div className="template-editor">
      <div className="editor-head">
        <button onClick={onBack}>← Overview</button>
        <IconPicker value={draft.icon} onChange={(icon) => onChange({ ...draft, icon })} />
        <input
          className={draft.name.trim() ? "type-name large" : "type-name large missing"}
          value={draft.name}
          onChange={(e) => onChange({ ...draft, name: e.target.value })}
          placeholder={group ? "Name of the group" : "Name of the type"}
          title={group ? "Name of the group — you cannot save without one" : "Name of the type — you cannot save without one"}
          autoFocus={!draft.name}
        />
        {/* What the right side shows: what an article of this type looks
            like, or the documents it has — for a list of values like
            male/female, that is where they are added. */}
        {!group && <>
        <div className="segmented" role="group" aria-label="Right side">
          <button className={side === "preview" ? "on" : ""} onClick={() => setSide("preview")}>
            Preview
          </button>
          <button
            className={side === "documents" ? "on" : ""}
            onClick={() => setSide("documents")}
          >
            Documents
          </button>
        </div>
        {/* The supertype: what this type is a kind of. It passes down its
            fields and blocks, and everything that points at it accepts this
            type too. */}
        <label className="supertype-picker" title="What this type is a kind of">
          <span className="small">is a</span>
          <select
            value={draft.supertype}
            onChange={(e) => onChange({ ...draft, supertype: e.target.value })}
          >
            <option value="">— nothing above —</option>
            {possibleSupertypes
              .sort((a, b) => a.name.localeCompare(b.name))
              .map((t) => (
                <option key={t.id} value={t.id}>
                  {t.icon} {t.name}
                </option>
              ))}
          </select>
        </label>
        </>}
        <span className="filler" />
        <button
          onClick={() => api.legendOpen().catch(() => {})}
          title="Open the legend in its own window"
        >
          ? Legend
        </button>
        <button
          className="primary"
          onClick={onSave}
          disabled={errors.length > 0 || !draft.name.trim()}
          title={draft.name.trim() ? "" : group ? "The group needs a name" : "The type needs a name"}
        >
          {saved ? "Saved ✓" : "Save"}
        </button>
      </div>

      <div className="two-columns">
        <div className="text-side">
          {/* The errors name line numbers — so the numbers have to be there. */}
          <div className="text-field">
            <div className="line-numbers" ref={numbersRef} aria-hidden="true">
              {(text ?? "").split("\n").map((_, i) => (
                <span key={i} className={errorLines.has(i + 1) ? "stuck" : ""}>
                  {i + 1}
                </span>
              ))}
            </div>
            <textarea
              ref={fieldRef}
              className="template-text"
              value={text ?? ""}
              onChange={(e) => {
                const field = e.currentTarget;
                const closed = withClosingTag(text ?? "", field.value, field.selectionStart);
                setText(closed?.text ?? field.value);
                if (closed) queueMicrotask(() => field.setSelectionRange(closed.caret, closed.caret));
              }}
              onScroll={(e) => {
                // The numbers scroll along, without a scrollbar of their own.
                if (numbersRef.current) {
                  numbersRef.current.scrollTop = e.currentTarget.scrollTop;
                }
              }}
              onContextMenu={(e) => {
                if (group) return;
                e.preventDefault();
                setMenu({ x: e.clientX, y: e.clientY });
              }}
              placeholder={group ? "---\nStrength: number\nDexterity: number\n---\n\nA layout, if it has one: **Strength** %%Strength%%" : undefined}
              spellCheck={false}
            />
          </div>
          {errors.length > 0 ? (
            <ul className="error-list">
              {errors.map((f, i) => (
                <li key={i}>
                  <button className="danger compact" onClick={() => goToLine(f.line)} disabled={f.line < 1}>
                    {f.line > 0 ? `Line ${f.line}` : "Template"}
                  </button>
                  {f.text}
                </li>
              ))}
            </ul>
          ) : !group && draft.blocks.length === 0 && draft.properties.length === 0 ? (
            // The skeleton is always there, so what is empty is never the
            // field but the body. The note stands outside the field: a
            // placeholder *in* the field looks like content that is already
            // there and cannot be told from real text.
            <p className="empty-note small">
              Nothing in the <code>&lt;body&gt;</code> yet. <strong>Right-click</strong> inserts
              building blocks — or <strong>? Legend</strong>, top right.
            </p>
          ) : null}
          {repaired && (
            <p className="repaired-note small">
              Skeleton put back. Every article has{" "}
              <code>&lt;article&gt;</code>, <code>&lt;name&gt;</code> and{" "}
              <code>&lt;body&gt;</code> — you can change them, not delete them.
            </p>
          )}
        </div>

        <div className="preview-side">
          {/* Access can be clicked here; the change is written back as text
              on the left. It is set anew in the process — the price of
              keeping both sides on the same data. */}
          {group ? (
            <GroupPreview group={draft} templates={templates} />
          ) : side === "documents" ? (
            <ValueList
              world={world}
              template={draft}
              templates={templates}
              articles={articles}
              onArticlesChanged={onArticlesChanged}
              onOpen={onOpen}
              onError={onError}
            />
          ) : (
            <Preview
              world={world}
              template={draft}
              templates={others}
              documents={articles.filter((a) => a.articleType === draft.id && !a.pending).map((a) => a.id)}
              saved={saved}
              onApplied={onArticlesChanged}
              onError={onError}
              onTemplate={(fresh) => {
                onChange(fresh);
                api.templateWrite(world.id, fresh).then(setText).catch(() => {});
              }}
            />
          )}
        </div>
      </div>

      {menu && (
        <BlockMenu
          x={menu.x}
          y={menu.y}
          onPick={insert}
          onClose={() => setMenu(null)}
        />
      )}
    </div>
  );
}

const kindName = (f: TemplateField) =>
  f.kind === "number" && f.unit ? `Number, ${f.unit}` : (FIELD_KINDS.find((k) => k.value === f.kind)?.text ?? f.kind);

/** A property group: its properties, and the types that take it in. */
function GroupPreview({ group, templates }: { group: Template; templates: Template[] }) {
  const all = [...templates.filter((t) => t.id !== group.id), group];
  const fields = unfold(all, group.properties);
  const users = usersOf(all, group.id).filter((t) => !t.group);
  const layout = group.blocks[0]?.parts[0]?.text ?? "";
  return (
    <div className="group-preview">
      <dl className="bound-fields">
        {fields.map((f) => (
          <div key={f.key} className="property-row">
            <dt>{f.label}</dt>
            <dd className="small muted">
              {kindName(f)}
              {f.default ? ` = ${f.default}` : ""}
            </dd>
          </div>
        ))}
        {group.rules.map((r) => (
          <div key={`rule-${r.name}`} className="property-row">
            <dt>{r.params.length ? `${r.name}(${r.params.join(", ")})` : r.name}</dt>
            <dd className="small muted">= {r.formula}</dd>
          </div>
        ))}
      </dl>
      <p className="small muted">
        {users.length > 0 ? `In ${users.map((u) => `${u.icon} ${u.name}`).join(", ")}` : "Not in any type yet."}
      </p>
      {layout.trim() && (
        <section className="block box">
          <MarkdownText text={layout} resolve={() => undefined} onPlace={() => {}} />
        </section>
      )}
    </div>
  );
}

/** The right-click menu: pick a line instead of typing it. */
function BlockMenu({
  x,
  y,
  onPick,
  onClose,
}: {
  x: number;
  y: number;
  onPick: (b: Brick) => void;
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

  return (
    <div
      className="brick-menu"
      style={{ left: Math.min(x, window.innerWidth - 300), top: Math.min(y, window.innerHeight - 380) }}
      onMouseDown={(e) => e.stopPropagation()}
      onContextMenu={(e) => e.preventDefault()}
    >
      {/* Only what belongs in the body. The skeleton is there anyway, and
          the values are not lines but parts of tags — inserting either would
          have produced nothing but errors. Both are in the legend. */}
      {BRICKS.map((group) => {
        const insertable = group.bricks.filter((b) => !b.where || b.where === "properties");
        if (insertable.length === 0) return null;
        return (
          <div key={group.title} className="brick-group">
            <h5>{group.title}</h5>
            {insertable.map((b) => (
              // The name only. The explanation stands in the legend — in the
              // menu it sat two lines deep under every entry and blocked the
              // list more than it explained it.
              <button key={b.name} className="ghost" onClick={() => onPick(b)} title={b.what}>
                {b.name}
              </button>
            ))}
          </div>
        );
      })}
    </div>
  );
}

/**
 * A row of access settings to click.
 *
 * The same row at every spot that can say `view=` and `edit=` — the whole
 * article, a block, a piece of flowing text. Whoever has understood 👁 and ✎
 * once has understood them everywhere.
 */
function AccessRow({
  world,
  access,
  onChange,
}: {
  world: WorldInfo;
  access: Access;
  onChange: (part: Partial<Access>) => void;
}) {
  return (
    <div className="access-row">
      <span className="mark-short" title="Who may see">
        👁
      </span>
      <VisibilityPicker
        world={world}
        value={access.view}
        onChange={(view) => onChange({ view })}
        small
      />
      <span className="mark-divider" />
      <span className="mark-short" title="Who may change">
        ✎
      </span>
      <VisibilityPicker
        world={world}
        value={access.edit}
        onChange={(edit) => onChange({ edit })}
        small
      />
    </div>
  );
}

/** What an article of this type will look like — with empty values. */
function Preview({
  world,
  template,
  templates,
  documents,
  saved,
  onApplied,
  onError,
  onTemplate,
}: {
  world: WorldInfo;
  template: Template;
  /** The other types — the supertype chain comes from them. */
  templates: Template[];
  /** The documents of this type there are. */
  documents: string[];
  saved: boolean;
  onApplied: () => void;
  onError: (e: unknown) => void;
  onTemplate: (t: Template) => void;
}) {
  const [applying, setApplying] = useState<string | null>(null);
  const apply = () => {
    const access = bracket(template);
    setApplying("…");
    api
      .typeAccessApply(world.id, template.id, documents, access.view, access.edit)
      .then((n) => {
        setApplying(`${n} changed`);
        onApplied();
      })
      .catch((e) => {
        setApplying(null);
        onError(e);
      });
  };
  // What an article of this type really gets: its own and the inherited.
  const whole = effective(templates, template);
  const ownBlocks = new Set(template.blocks.map((b) => b.key));
  const inheritedFrom = (blockKey: string) =>
    ancestors(templates, template).find((t) => t.blocks.some((b) => b.key === blockKey));

  /** Changes the access of a part and hands the new template upwards. */
  const changeAccess = (blockKey: string, partKey: string, part: Partial<BlockPart>) =>
    onTemplate({
      ...template,
      blocks: template.blocks.map((b) =>
        b.key !== blockKey
          ? b
          : { ...b, parts: b.parts.map((t) => (t.key === partKey ? { ...t, ...part } : t)) },
      ),
    });

  const nothing = () => undefined;

  return (
    <article className="article-page preview">
      <ArticleHead
        icon={template.icon || "📄"}
        title={
          // If the template holds a pattern, the preview shows it filled in:
          // “Session log 1” says more than “%%count%%”.
          <span className="name-placeholder">
            {template.namePattern === "%%input%%"
              ? "Name of the article"
              : patternPreview(template.namePattern)}
          </span>
        }
        type={<span className="chip">{template.name || template.id}</span>}
        marks={
          // The article's permission: the name is its base layer — whoever
          // cannot open it finds the article nowhere. It never has access of
          // its own; that would be a second statement for the same decision.
          <>
            <AccessRow
              world={world}
              access={bracket(template)}
              onChange={(part) => onTemplate(articleAccess(template, part))}
            />
            <div className="access-row">
              <span className="mark-short" title="Who besides the GM may make documents of it">
                ＋
              </span>
              <VisibilityPicker world={world} value={template.create || "gm"} onChange={(create) => onTemplate({ ...template, create: create === "gm" ? "" : create })} small />
              {documents.length > 0 && (
                <button
                  className="compact"
                  disabled={!saved || applying === "…"}
                  title={saved ? "Who sees and edits, for the documents there are — sections with access of their own keep it" : "Save the type first"}
                  onClick={apply}
                >
                  {applying ?? `Apply to ${documents.length} documents`}
                </button>
              )}
            </div>
          </>
        }
      />

      <div className={`article-body align-${whole.infobox.align}`}>
        <InfoboxForm
          infobox={whole.infobox}
          properties={whole.properties}
          values={{}}
          articles={[]}
          templates={[...templates, template]}
          articleType={template.id}
          showEmpty
        />

        <div className="flowing-text">
          {whole.blocks.map((block) => {
            // An inherited block is changed where it comes from, not here.
            if (!ownBlocks.has(block.key)) {
              const from = inheritedFrom(block.key);
              return (
                <section key={block.key} className={`block ${block.style} inherited`}>
                  <div className="block-mark">
                    {block.title && <span className="block-title">{block.title}</span>}
                    <span className="small">from {from?.name ?? "the supertype"}</span>
                  </div>
                  {block.parts.map((part) => (
                    <MarkdownText key={part.key} text={part.text} resolve={nothing} onPlace={() => {}} />
                  ))}
                </section>
              );
            }
            return (
            <section key={block.key} className={`block ${block.style}`}>
              {block.title && (
                <div className="block-mark">
                  <span className="block-title">{block.title}</span>
                </div>
              )}
              {/* One box of its own per part, with access of its own — so the
                  picture shows where an inner block cuts the text apart. */}
              {block.parts.map((part) => {
                const access = { view: part.view, edit: part.edit };
                // Whoever says nothing of their own inherits from the article —
                // and then nothing stands here either. An access row above
                // every paragraph says the same thing three times and cuts up
                // the reading; it only appears when the pointer rests on it.
                const inherited = sameAccess(access, bracket(template));
                return (
                  <div
                    key={part.key}
                    className={`block-part${inherited ? "" : " guarded"}`}
                  >
                    <div className={inherited ? "access-slot quiet" : "access-slot"}>
                      <AccessRow
                        world={world}
                        access={access}
                        onChange={(t) => changeAccess(block.key, part.key, t)}
                      />
                    </div>
                    <MarkdownText text={part.text} resolve={nothing} onPlace={() => {}} />
                  </div>
                );
              })}
            </section>
            );
          })}
        </div>
      </div>
    </article>
  );
}
