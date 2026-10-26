import { useEffect, useMemo, useRef, useState } from "react";
import {
  api,
  PROPERTIES,
  type ArticleView,
  type NewLayer,
  type Template,
  type WorldInfo,
} from "../../api/client";
import { ArticleHead } from "./ArticleHead";
import { BlockEditor } from "./BlockEditor";
import { VisibilityPicker } from "../../shared/ui/Visibility";
import { InfoboxForm } from "./InfoboxForm";
import { ImagePicker } from "../../shared/ui/ImagePicker";
import { linker } from "../../shared/Links";
import { needsInput, nameFrom, nextNumber, fillCount } from "../templates/NamePattern";
import { isA, typeOf } from "../../shared/types";
import { editOnOpen } from "./Document";

type Values = Record<string, unknown>;

export const slugify = (text: string) =>
  text
    .toLowerCase()
    .replace(/ä/g, "ae")
    .replace(/ö/g, "oe")
    .replace(/ü/g, "ue")
    .replace(/ß/g, "ss")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");

export function ArticleArea(props: {
  world: WorldInfo;
  templates: Template[];
  articles: ArticleView[];
  articleType: string;
  /** A document this player asked for and waits on — to change before the GM's device makes it. */
  pending?: ArticleView;
  /** Done — with the new document's id where it is to be opened. */
  onDone: (id?: string) => void;
  onCancel: () => void;
  onError: (e: unknown) => void;
}) {
  // Made here at once: then it is written in the editor every document has.
  // A player's goes to the GM's device first — until it is made there, it
  // is filled in its form.
  return props.world.isGm && !props.pending ? <QuickArticle {...props} /> : <NewArticle {...props} />;
}

/**
 * A new document, made at once and opened for editing — in the same editor
 * as every other, code or visual. Only a name is asked for, and only where
 * the type does not build it itself.
 */
function QuickArticle({
  world,
  templates,
  articles,
  articleType,
  onDone,
  onCancel,
  onError,
}: {
  world: WorldInfo;
  templates: Template[];
  articles: ArticleView[];
  articleType: string;
  onDone: (id?: string) => void;
  onCancel: () => void;
  onError: (e: unknown) => void;
}) {
  const template = useMemo(() => typeOf(templates, articleType), [templates, articleType]);
  const [input, setInput] = useState("");
  const pattern = template?.namePattern || "%%input%%";
  const count = useMemo(
    () => nextNumber(pattern, articles.filter((a) => a.articleType === articleType).map((a) => a.name)),
    [pattern, articles, articleType],
  );
  const name = nameFrom(pattern, input.trim(), count);
  const open = needsInput(pattern);
  // Made once — not again when the page draws anew.
  const made = useRef(false);

  const create = () => {
    if (!template || !name.trim() || made.current) return;
    made.current = true;
    makeDocument(world.id, templates, articles, template, name)
      .then((id) => {
        editOnOpen(id);
        onDone(id);
      })
      .catch((e) => {
        made.current = false;
        onError(e);
      });
  };

  // The type builds the name: nothing to ask.
  useEffect(() => {
    if (!open) create();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!template) return <p className="note">This article type does not exist.</p>;
  if (!open) return <p className="note">Creating {name} …</p>;
  return (
    <article className="article-page">
      <ArticleHead
        icon={template.icon || "📄"}
        title={
          <form
            className="name-pattern"
            onSubmit={(e) => {
              e.preventDefault();
              create();
            }}
          >
            {pattern.split("%%input%%").map((part, i) => (
              <span key={i}>
                {i > 0 && (
                  <input
                    className="name-input"
                    autoFocus
                    value={input}
                    onChange={(e) => setInput(e.target.value)}
                    onKeyDown={(e) => e.key === "Escape" && onCancel()}
                    placeholder="Name"
                    size={Math.max(12, input.length + 2)}
                  />
                )}
                {fillCount(part, count)}
              </span>
            ))}
          </form>
        }
        type={<span className="chip">{template.name}</span>}
      />
      <div className="line">
        <button className="primary" disabled={!input.trim()} onClick={create}>
          Create
        </button>
        <button onClick={onCancel}>Cancel</button>
      </div>
    </article>
  );
}

/** Makes a document of a type at once, as the type starts it — the GM's way; `preset` values win over the type's. Its id. */
export async function makeDocument(
  world: string,
  templates: Template[],
  articles: ArticleView[],
  template: Template,
  name: string,
  preset: Values = {},
): Promise<string> {
  const start = startTexts(template);
  const keys = Object.keys(start);
  const visibility = template.auth?.view ?? "all";
  const written = await api.textsWritten(world, template.id, keys.map((k) => start[k]));
  const texts = Object.fromEntries(keys.map((k, i) => [k, written[i]]));
  const layers = layersFor(template, { ...defaults(template, articles, templates), ...preset }, texts, {}, visibility);
  return api.articleCreate(world, template.id, slugify(name), name, visibility, layers);
}

/**
 * Creating an article.
 *
 * The same page as the finished article — only that the values are still
 * input fields here. Whoever writes should see what comes into being.
 */
function NewArticle({
  world,
  templates,
  articles,
  articleType,
  pending,
  onDone,
  onCancel,
  onError,
}: {
  world: WorldInfo;
  templates: Template[];
  articles: ArticleView[];
  articleType: string;
  pending?: ArticleView;
  onDone: (id?: string) => void;
  onCancel: () => void;
  onError: (e: unknown) => void;
}) {
  // With everything it inherits: a city gets the fields and blocks of a
  // place too.
  const template = useMemo(() => typeOf(templates, articleType), [templates, articleType]);

  // What is typed is not necessarily the name: the template can prescribe a
  // pattern that puts something in front or counts up.
  const [input, setInput] = useState(pending?.name ?? "");
  // Asked for already: its name stands as it was asked.
  const pattern = pending ? "%%input%%" : template?.namePattern || "%%input%%";
  const count = useMemo(
    () =>
      nextNumber(
        pattern,
        articles.filter((a) => a.articleType === articleType).map((a) => a.name),
      ),
    [pattern, articles, articleType],
  );
  const name = nameFrom(pattern, input.trim(), count);
  const open = needsInput(pattern);
  // The base layer. It follows what the template's <article> says — without
  // a word of its own that is `all`.
  const [visibility, setVisibility] = useState(() => template?.auth?.view ?? "all");
  // A new document starts with what its type gives as defaults.
  const [values, setValues] = useState<Values>(() =>
    pending
      ? Object.assign({}, ...pending.layers.filter((l) => l.section === PROPERTIES || l.section.startsWith(`${PROPERTIES}:`)).map((l) => l.fields as Values))
      : defaults(template, articles, templates),
  );
  // By part key, not by block: every part becomes a layer of its own.
  const [texts, setTexts] = useState<Record<string, string>>({});
  // The type's texts store ids; whoever writes sees names.
  useEffect(() => {
    // Asked for already: its texts as asked; else what the type starts with.
    const start = pending
      ? Object.fromEntries(pending.layers.filter((l) => !l.section.startsWith(PROPERTIES)).map((l) => [l.section, l.text]))
      : startTexts(template);
    const keys = Object.keys(start);
    api
      .textsWritten(world.id, articleType, keys.map((k) => start[k]))
      .then((written) => setTexts(Object.fromEntries(keys.map((k, i) => [k, written[i]]))))
      .catch(onError);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [world.id, template?.id]);
  const [views, setViews] = useState<Record<string, string>>({});
  // The image field an image is being chosen for — a portrait, a coat of arms.
  const [pickingFor, setPickingFor] = useState<string | null>(null);

  const viewOf = (key: string, fallback: string) => views[key] ?? fallback;

  const save = () => {
    if (!template || !name.trim()) return;

    const layers = layersFor(template, values, texts, views, visibility);

    // A player's new document goes to the GM's device, which makes it —
    // here it is theirs at once, under the id it will have.
    if (!world.isGm) {
      api
        .articlePropose(world.id, pending?.id ?? null, articleType, slugify(name), name, layers)
        .then((id) => onDone(id))
        .catch(onError);
      return;
    }
    api
      .articleCreate(world.id, articleType, slugify(name), name, visibility, layers)
      .then(() => onDone())
      .catch(onError);
  };

  if (!template) return <p className="note">This article type does not exist.</p>;

  return (
    <article className="article-page">
      <ArticleHead
        icon={template.icon || "📄"}
        title={
          open ? (
            // If the pattern holds more than the input, one sees the frame
            // around it — otherwise one types against a rule one does not
            // know.
            <span className="name-pattern">
              {pattern.split("%%input%%").map((part, i) => (
                <span key={i}>
                  {i > 0 && (
                    <input
                      className="name-input"
                      autoFocus
                      value={input}
                      onChange={(e) => setInput(e.target.value)}
                      placeholder="Name of the article"
                      size={Math.max(12, input.length + 2)}
                    />
                  )}
                  {fillCount(part, count)}
                </span>
              ))}
            </span>
          ) : (
            // Without %%input%% the name is already settled.
            <span className="name-fixed" title="The template builds the name itself">
              {name}
            </span>
          )
        }
        type={
          <>
            <span className="chip">{template.name}</span>
            {name && <code className="small">{slugify(name)}</code>}
          </>
        }
        marks={
          <VisibilityPicker world={world} value={visibility} onChange={setVisibility} small />
        }
      />

      <div className={`article-body align-${template.infobox.align}`}>
        <InfoboxForm
          infobox={template.infobox}
          properties={template.properties}
          values={values}
          articles={articles}
          templates={templates}
          articleType={articleType}
          editing
          onValue={(key, value) => setValues((v) => ({ ...v, [key]: value }))}
          onImage={setPickingFor}
        />
        {pickingFor && (
          <ImagePicker
            world={world.id}
            onChosen={(id) => {
              setValues((v) => ({ ...v, [pickingFor]: id }));
              setPickingFor(null);
            }}
            onRemove={
              values[pickingFor]
                ? () => {
                    setValues((v) => {
                      const { [pickingFor]: _gone, ...rest } = v;
                      void _gone;
                      return rest;
                    });
                    setPickingFor(null);
                  }
                : undefined
            }
            onCancel={() => setPickingFor(null)}
          />
        )}

        <div className="flowing-text">
          {template.blocks.length === 0 && (
            <p className="note">
              This type has no text block yet. You can add one under Options.
            </p>
          )}
          {template.blocks.map((block) => (
            <section key={block.key} className={`block ${block.style}`}>
              {block.title && <div className="block-mark"><span className="block-title">{block.title}</span></div>}
              {block.parts.map((part) => (
                <BlockEditor
                  key={part.key}
                  world={world}
                  title=""
                  style={block.style}
                  text={texts[part.key] ?? ""}
                  policy={viewOf(part.key, part.view)}
                  inherited={viewOf(part.key, part.view) === visibility}
                  articles={articles}
                  templates={templates}
                  onText={(fresh) => setTexts((t) => ({ ...t, [part.key]: fresh }))}
                  onPolicy={(fresh) => setViews((s) => ({ ...s, [part.key]: fresh }))}
                />
              ))}
            </section>
          ))}
        </div>
      </div>

      <div className="line">
        <button className="primary" disabled={!name.trim()} onClick={save}>
          Save
        </button>
        <button onClick={onCancel}>Cancel</button>
      </div>
    </article>
  );
}

/** The text each part of a new document starts with — the template's. */
export const startTexts = (template: Template | undefined): Record<string, string> =>
  Object.fromEntries((template?.blocks ?? []).flatMap((b) => b.parts.map((t) => [t.key, t.text])));

/**
 * The layers of a new document — the same however it is made.
 *
 * One per part of a block, plus one for the properties. All flat: there are
 * no enclosing sections, and so nothing to AND-combine. A property with
 * access of its own is a layer of its own — otherwise whoever may see the
 * others would have its value too. The rest are the article's.
 *
 * `template` is the type with everything it inherits.
 */
export function layersFor(
  template: Template,
  values: Values,
  texts: Record<string, string>,
  views: Record<string, string>,
  visibility: string,
): NewLayer[] {
  const layers: NewLayer[] = [];
  const own = template.properties.filter((f) => f.access && values[f.key] !== undefined);
  const common = Object.fromEntries(
    Object.entries(values).filter(([key]) => !own.some((f) => f.key === key)),
  );
  if (Object.keys(common).length > 0) {
    // The infobox's own access where its type gives it one.
    const box = template.infobox.access;
    layers.push({
      title: "",
      text: "",
      policy: box?.view ?? visibility,
      fields: common,
      section: PROPERTIES,
      edit: box?.edit ?? template.auth?.edit ?? "gm",
      parent: null,
    });
  }
  for (const field of own) {
    layers.push({
      title: "",
      text: "",
      policy: field.access!.view,
      fields: { [field.key]: values[field.key] },
      section: `${PROPERTIES}:${field.key}`,
      edit: field.access!.edit,
      parent: null,
    });
  }
  for (const block of template.blocks) {
    for (const part of block.parts) {
      layers.push({
        title: block.title,
        text: texts[part.key] ?? "",
        policy: views[part.key] ?? part.view,
        fields: {},
        section: part.key,
        edit: part.edit,
        parent: null,
      });
    }
  }
  return layers;
}

/**
 * The values a new document of this type starts with — its properties'
 * defaults, turned into what is stored: a number for a number, the ids of
 * the documents a link names.
 */
export function defaults(template: Template | undefined, articles: ArticleView[], templates: Template[]): Values {
  const resolve = linker(articles, templates);
  const out: Values = {};
  for (const field of template?.properties ?? []) {
    const written = (field.default ?? "").trim();
    if (!written) continue;
    // A number may say what it does to what picks it: `+1`, `-1`, `=19` — see `shared/Sheet`.
    const op = field.kind === "number" ? /^([+\-=])\s*(.+)$/.exec(written) : null;
    if (op) {
      if (Number.isFinite(Number(op[2]))) out[field.key] = `${op[1]}${Number(op[2])}`;
    } else if (field.kind === "number" || field.kind === "world-date") {
      const n = Number(written);
      if (Number.isFinite(n)) out[field.key] = n;
    } else if (field.kind === "reference" && written === "*") {
      // `= *`: every document of the type, by name — all skills, to start with.
      const ids = articles
        .filter((a) => isA(templates, a.articleType, field.articleType))
        .sort((a, b) => a.name.localeCompare(b.name))
        .map((a) => a.id);
      if (ids.length > 0) out[field.key] = field.multiple ? ids : ids[0];
    } else if (field.kind === "reference" && written === "last") {
      // `= last`: the last document of the type by name — a new session's previous one.
      const last = articles
        .filter((a) => isA(templates, a.articleType, field.articleType))
        .sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }))
        .pop();
      if (last) out[field.key] = field.multiple ? [last.id] : last.id;
    } else if (field.kind === "reference") {
      // Stored by id; a name that named nothing yet stays a name.
      const ids = written
        .split(",")
        .map((n) => n.trim())
        .filter(Boolean)
        .flatMap((n) => (n.startsWith("art:") ? [n] : (resolve({ of: "", name: n })?.id ?? [])));
      if (ids.length > 0) out[field.key] = field.multiple ? ids : ids[0];
    } else if (field.kind !== "image") {
      out[field.key] = written;
    }
  }
  return out;
}
