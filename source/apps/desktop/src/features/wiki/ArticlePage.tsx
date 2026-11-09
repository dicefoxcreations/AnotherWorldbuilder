import { useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import {
  api,
  PROPERTIES,
  type ArticleView,
  type LayerView,
  type Node,
  type Place,
  type Template,
  type BlockPlace,
  type TemplateBlock,
  type TemplateField,
  type WorldInfo,
} from "../../api/client";
import { ArticleHead } from "./ArticleHead";
import { DeleteButton, DeleteDialog } from "../../shared/ui/Delete";
import { headingOf, MarkdownText, useMarkdowns, type TopFolds, type ValueOf } from "./MarkdownText";
import { AccessLine, RegionAccess, samePolicy } from "../../shared/ui/Visibility";
import { InfoboxForm, NumberHow, PropertyValue } from "./InfoboxForm";
import { linker, linksIn, type Resolve } from "../../shared/Links";
import { findType, idsIn, propertyNamed, isProperties, pageOwner, permits, typeOf, articleById } from "../../shared/types";
import { explain, lookupFor, sumOf, summedValues } from "../../shared/Sheet";
import { RollScopeContext, type RollScope } from "../dice/DiceTray";
import { CurrentDocument, ReadableParts, useWorldData } from "../worlds/WorldData";
import { FoldBlock, SideColumn, useFoldedHeadings } from "../../shared/ui/BlockFold";
import { blockRank, lookOf } from "../../shared/BlockOrder";
import { sectioned } from "../../shared/Sections";
import { joinPins } from "../../shared/GraphText";
import { linkIcon } from "./DocLink";

/**
 * Everything that points at an article, grouped by the field it points with.
 *
 * Every reference field has a reverse direction: whoever names Chicago as
 * “Home” shows up at Chicago under “Home of” — a session naming Tim among
 * its characters under “Characters of”, not under “Characters”, which would
 * promise characters. The parent field is the tree, so its reverse is called
 * what it is — what lies below. Only what this reader may see takes part.
 */
function incoming(
  templates: Template[],
  all: ArticleView[],
  article: ArticleView,
): { title: string; articles: ArticleView[] }[] {
  const groups = new Map<string, ArticleView[]>();
  const resolve = linker(all, templates);
  const pointsHere = (text: string) => linksIn(text).some((l) => resolve(l)?.id === article.id);
  const add = (title: string, a: ArticleView) => {
    const list = groups.get(title) ?? [];
    if (!list.some((x) => x.id === a.id)) groups.set(title, [...list, a]);
  };

  for (const a of all) {
    if (a.id === article.id) continue;
    const type = typeOf(templates, a.articleType);
    for (const field of type?.properties ?? []) {
      if (field.kind !== "reference") continue;
      if (idsIn(a, field.key).includes(article.id))
        add(field.parent ? "Below" : field.label ? `${field.label} of` : "Referenced by", a);
    }
    // Free properties name documents by `@@Type:Name@@`, like the text does.
    const known = new Set((type?.properties ?? []).map((f) => f.key));
    for (const layer of a.layers) {
      for (const [name, value] of Object.entries(layer.fields ?? {})) {
        if (!known.has(name) && typeof value === "string" && pointsHere(value)) add(`${name} of`, a);
      }
    }
    if (a.layers.some((l) => pointsHere(l.text))) add("Mentioned in", a);
  }

  return [...groups.entries()]
    .sort(([x], [y]) => (x === "Below" ? -1 : y === "Below" ? 1 : x.localeCompare(y)))
    .map(([title, articles]) => ({
      title,
      articles: articles.sort((x, y) => x.name.localeCompare(y.name)),
    }));
}

/** How many of one kind show before the rest are asked for. */
const BACKLINKS_SHOWN = 24;

/**
 * One group of what points here, by type — the type's name and how many.
 * Two of the same name tell what they belong to: a chapter its book. A long
 * kind shows its first and opens the rest on asking.
 */
function Backlinks({
  title,
  articles,
  templates,
  all,
  onPlace,
}: {
  title: string;
  articles: ArticleView[];
  templates: Template[];
  all: ArticleView[];
  onPlace: (o: Place) => void;
}) {
  const [open, setOpen] = useState<string[]>([]);
  const kinds = useMemo(() => {
    const by = new Map<string, ArticleView[]>();
    for (const a of articles) by.set(a.articleType, [...(by.get(a.articleType) ?? []), a]);
    return [...by].map(([type, list]) => ({ type, name: typeOf(templates, type)?.name ?? "Page", list })).sort((x, y) => x.name.localeCompare(y.name));
  }, [articles, templates]);
  /** What a document belongs to: the first document its references name. */
  const above = (a: ArticleView) => {
    for (const field of typeOf(templates, a.articleType)?.properties ?? []) {
      if (field.kind !== "reference") continue;
      const id = idsIn(a, field.key)[0];
      const name = id && articleById(all, id)?.name;
      if (name) return name;
    }
    return "";
  };
  return (
    <div>
      <h3>{title}</h3>
      {kinds.map(({ type, name, list }) => {
        const twice = new Set(list.filter((a, i) => list.findIndex((b) => b.name === a.name) !== i).map((a) => a.name));
        const shown = open.includes(type) ? list : list.slice(0, BACKLINKS_SHOWN);
        return (
          <div key={type} className="backlink-kind">
            {kinds.length > 1 && (
              <div className="small muted">
                {name} · {list.length}
              </div>
            )}
            <ul className="chips">
              {shown.map((a) => {
                const of = twice.has(a.name) ? above(a) : "";
                return (
                  <li key={a.id}>
                    <button className="chip" data-icon={linkIcon(templates, a.articleType)} onClick={() => onPlace({ kind: "article", id: a.id })}>
                      {a.name}
                      {of && <span className="chip-note"> · {of}</span>}
                    </button>
                  </li>
                );
              })}
              {shown.length < list.length && (
                <li>
                  <button className="chip more" onClick={() => setOpen([...open, type])}>
                    {list.length - shown.length} more
                  </button>
                </li>
              )}
            </ul>
          </div>
        );
      })}
    </div>
  );
}

/**
 * An article, readable.
 *
 * The infobox at its position, below or beside it the blocks as flowing
 * text. A block this reader may not see does not appear at all — the text
 * closes up. That is not a hidden display: its holder never sent it here.
 */
export function ArticlePage({
  world,
  article,
  templates,
  all,
  onPlace,
  actions,
  glance = false,
  headless = false,
  section = "",
}: {
  world: WorldInfo;
  article: ArticleView;
  templates: Template[];
  all: ArticleView[];
  onPlace: (o: Place) => void;
  /** What the owner of a page adds to its head — a type's “New” button. */
  actions?: ReactNode;
  /**
   * Only to look at — the page as the link preview shows it: no access to
   * change, no deleting, no backlinks.
   */
  glance?: boolean;
  /** Without its head — drawn into another page, under that page's heading for it. */
  headless?: boolean;
  /**
   * That section alone, by its title — as it stands on the page: its look,
   * the document's values, its rolls; on top the document's name, leading
   * to it, unless `headless`. A creature's stat block in a book.
   */
  section?: string;
}) {
  const template = typeOf(templates, article.articleType);
  const resolve = useMemo(() => linker(all, templates), [all, templates]);
  // Out of the index, its texts not read yet: asked for now, before the rest.
  const reading = useWorldData();
  useEffect(() => {
    if (article.partial) reading?.want?.(article.id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [article.partial, article.id]);
  const readableParts = useMemo(() => new Set(article.layers.map((l) => l.section)), [article]);
  const hasInfobox = article.layers.some((l) => isProperties(l.section));
  // Sections the template does not know — the text of a page, which has no
  // type, or blocks whose template changed. They are content all the same.
  // Each block's parts as the document has them — `story`, `story-2`, … —
  // those the type sets out and those written in the document alone (an
  // element with access of its own, pins set apart), in their order.
  const partsOf = useMemo(() => {
    const keys = (template?.blocks ?? []).map((b) => b.key);
    const out = new Map<string, LayerView[]>(keys.map((k) => [k, []]));
    for (const l of article.layers) {
      const owner = keys.includes(l.section) ? l.section : keys.find((k) => partIndex(l.section, k) !== null);
      if (owner !== undefined) out.get(owner)!.push(l);
    }
    for (const [k, list] of out) list.sort((a, b) => partIndex(a.section, k)! - partIndex(b.section, k)!);
    return out;
  }, [article, template]);
  const known = new Set([PROPERTIES, ...[...partsOf.values()].flat().map((l) => l.section)]);
  // The rest by block too: `page`, `page-2` … are the parts of one section.
  const loose = useMemo(() => {
    const rest = article.layers.filter((l) => !known.has(l.section) && !isProperties(l.section));
    const sections = new Set(rest.map((l) => l.section));
    const groups = new Map<string, LayerView[]>();
    for (const l of rest) {
      const base = [...sections].find((k) => k !== l.section && partIndex(l.section, k) !== null) ?? l.section;
      groups.set(base, [...(groups.get(base) ?? []), l]);
    }
    for (const [k, list] of groups) list.sort((a, b) => partIndex(a.section, k)! - partIndex(b.section, k)!);
    // In the order the document keeps for them; those it does not name after.
    const order = article.order ?? [];
    const rank = (k: string) => (order.includes(k) ? order.indexOf(k) : order.length);
    return [...groups]
      .map(([section, layers]) => ({ section, layers }))
      .map((g, i) => ({ ...g, i }))
      .sort((a, b) => rank(a.section) - rank(b.section) || a.i - b.i);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [article, partsOf]);
  // A page wears its owner's icon: the world's, or its type's.
  const owner = pageOwner(templates, world.home, article);
  const icon =
    owner?.kind === "world"
      ? world.icon
      : owner?.kind === "type"
        ? owner.type.icon || "📄"
        : template?.icon || "📄";
  const missing = article.layerTotal - article.layers.length;
  const groups = useMemo(() => (glance ? [] : incoming(templates, all, article)), [glance, templates, all, article]);

  // What `%%Name%%` and an element with `bind=` show: a declared property by
  // its kind, a free one as the text it is.
  // Numbers with what feats and the like add to them — see `shared/Sheet`.
  const values = summedValues(templates, all, article);
  const numberHow = (key: string) => {
    const field = template?.properties.find((p) => p.key === key);
    return field ? explain(sumOf(templates, all, article, field.label)) : "";
  };
  const rolls: RollScope = useMemo(
    () => ({
      document: article.name,
      lookup: lookupFor(templates, all, article),
      explain: (name) => explain(sumOf(templates, all, article, name)),
      numbers: (template?.properties ?? []).filter((p) => p.kind === "number").map((p) => p.label),
      secret: false,
    }),
    [templates, all, article, template],
  );
  const valueOf: ValueOf = (name) => {
    const declared = propertyNamed(template, name);
    const key = declared?.key ?? Object.keys(values).find((k) => k.toLowerCase() === name.trim().toLowerCase());
    // `%%Name%%`: the document's own name, where no property is called so.
    if (key === undefined && name.trim().toLowerCase() === "name") return <>{article.name}</>;
    const field: TemplateField = declared ?? { key: name, label: name, kind: "text", multiline: false };
    return (
      <PropertyValue
        field={field}
        value={key === undefined ? undefined : values[key]}
        articles={all}
        templates={templates}
        articleType={article.articleType}
        onPlace={onPlace}
      />
    );
  };

  // A block of the type, as far as this reader can open it. A part this
  // reader could not open has no layer here — then it is missing, and the
  // text closes up. Nor does a part with nothing written in it: a wiki shows
  // no empty headings.
  // Beside a section's title: only what differs from the document — who views, who changes.
  const differs = (part: { policy: string; edit: string }) => {
    const view = samePolicy(part.policy, article.visibility) ? undefined : part.policy;
    const edit = samePolicy(part.edit, article.edit) ? undefined : part.edit;
    return view === undefined && edit === undefined ? null : <AccessLine world={world} view={view} edit={edit} />;
  };
  // At the top of a region with access of its own: whom it is for.
  const region = (part: { policy: string; edit: string }) =>
    samePolicy(part.policy, article.visibility) && samePolicy(part.edit, article.edit) ? null : (
      <RegionAccess world={world} view={part.policy} edit={part.edit} docView={article.visibility} docEdit={article.edit} />
    );
  // A player — or the GM seeing as one — finds the sections they may write
  // in, empty as they are; the GM's own page shows none.
  const previewing = !!reading?.previewing;
  const offered = (block: TemplateBlock) =>
    !glance &&
    (!world.isGm || previewing) &&
    (!!article.startable?.includes(block.key) || (partsOf.get(block.key) ?? []).some((l) => l.mayEdit));
  const drawBlock = (block: TemplateBlock) => {
    if (article.partial) return null;
    const parts = (partsOf.get(block.key) ?? []).filter((l) => l.text.trim() !== "");
    if (parts.length === 0)
      return offered(block) ? (
        <FoldBlock key={block.key} article={article.id} block={block}>
          <p className="small muted empty-section">Nothing written yet</p>
        </FoldBlock>
      ) : null;
    return (
      <FoldBlock
        key={block.key}
        article={article.id}
        block={block}
        access={!glance && differs(parts[0])}
      >
        <BlockText
          article={article.id}
          block={block.key}
          layers={parts}
          base={article.visibility}
          access={glance ? undefined : region}
          resolve={resolve}
          values={valueOf}
          onPlace={onPlace}
        />
      </FoldBlock>
    );
  };

  // The sections where they stand, in the document's order: its type's as
  // they look in it, its own as their first part says.
  const rank = blockRank(template, article);
  const shaped = (template?.blocks ?? []).map((b) => lookOf(b, article));
  const drawPlace = (where: BlockPlace) =>
    [
      ...shaped.filter((b) => (b.place ?? "flow") === where).map((b) => ({ key: b.key, drawn: drawBlock(b) })),
      ...drawOwn(where),
    ]
      .sort((x, y) => rank(x.key) - rank(y.key))
      .map((x) => x.drawn);

  // The document's own sections where they stand: how each looks is in its first part.
  function drawOwn(where: BlockPlace) {
    return (article.partial ? [] : loose)
      .filter(({ layers }) => layers.some((l) => l.text.trim() !== ""))
      .map(({ section, layers }) => ({ section, layers, block: ownBlock(section, layers[0]) }))
      .filter(({ block }) => (block.place ?? "flow") === where)
      .map(({ section, layers, block }) => ({ key: section, drawn: (
        <FoldBlock
          key={section}
          article={article.id}
          block={block}
          source={layers.map((l) => l.text).join("\n")}
          access={!glance && differs(layers[0])}
        >
          <BlockText article={article.id} block={section} layers={layers} base={article.visibility} access={glance ? undefined : region} resolve={resolve} values={valueOf} onPlace={onPlace} />
        </FoldBlock>
      ) }));
  }

  // The one section asked for: the document's name on top in place of the
  // section's title — as a printed stat block has it.
  const only = section.trim().toLowerCase();
  const alone = only
    ? [
        ...(template?.blocks ?? [])
          .filter((b) => b.title.trim().toLowerCase() === only)
          .map((block) => ({ key: block.key, block, layers: partsOf.get(block.key) ?? [] })),
        ...loose
          .map(({ section: key, layers }) => ({ key, layers, block: ownBlock(key, layers[0]) }))
          .filter(({ block }) => block.title.trim().toLowerCase() === only),
      ]
        .map(({ key, block, layers }) => ({ key, block, layers: layers.filter((l) => l.text.trim() !== "") }))
        .filter(({ layers }) => layers.length > 0)
        .map(({ key, block, layers }, i) => (
          <FoldBlock key={key} article={article.id} block={{ ...block, title: "" }}>
            {i === 0 && !headless && (
              <h2 className="markdown-head section-name">
                <button className="embed-name" data-preview={article.id} onClick={() => onPlace({ kind: "article", id: article.id })}>
                  {article.name}
                </button>
              </h2>
            )}
            <BlockText article={article.id} block={key} layers={layers} base={article.visibility} resolve={resolve} values={valueOf} onPlace={onPlace} />
          </FoldBlock>
        ))
    : null;

  return (
    <CurrentDocument.Provider value={article.id}>
    <ReadableParts.Provider value={readableParts}>
    <RollScopeContext.Provider value={rolls}>
    <NumberHow.Provider value={numberHow}>
    {alone && article.partial ? (
      <p className="small muted">Loading</p>
    ) : alone ? (
      <div className="article-section" data-document={article.id}>{alone}</div>
    ) : (
    <article className="article-page" data-document={article.id}>
      {!headless && (
        <ArticleHead
          icon={icon}
          title={article.name}
          type={
            <>
              {template && (
                <button
                  className="chip"
                  onClick={() => onPlace({ kind: "list", articleType: article.articleType })}
                >
                  {template.name}
                </button>
              )}
              {missing > 0 && (
                <span className="small">
                  {missing} of {article.layerTotal} closed
                </span>
              )}
            </>
          }
          marks={
            <>
              {actions}
              {/* Who views and who changes it — to know whom to ask; changed while editing. */}
              {!glance && article.visibility && <AccessLine world={world} view={article.visibility} edit={article.edit} />}
            </>
          }
        />
      )}

      {drawPlace("top")}

      <div className={`article-body align-${template?.infobox.align ?? "none"}`}>
        {template && hasInfobox && (
          <InfoboxForm
            infobox={template.infobox}
            properties={template.properties}
            values={values}
            articles={all}
            templates={templates}
            articleType={article.articleType}
            selfId={article.id}
            onPlace={onPlace}
            access={(() => {
              const box = article.layers.find((l) => l.section === PROPERTIES);
              return box ? differs(box) : null;
            })()}
          />
        )}

        <div className="flowing-text">
          {article.partial && <p className="small muted">Loading</p>}
          {drawPlace("flow")}
        </div>
        <SideColumn>
          {drawPlace("side")}
        </SideColumn>
      </div>

      {drawPlace("bottom")}

      {!glance && (groups.length > 0 || reading?.loading) && (
        <section className="backlinks">
          {groups.map((g) => (
            <Backlinks key={g.title} title={g.title} articles={g.articles} templates={templates} all={all} onPlace={onPlace} />
          ))}
          {/* What the texts name comes as they are read. */}
          {reading?.loading && <p className="small muted">Still reading the world</p>}
          <p className="small">
            Computed from what is readable on this device — not from an index that would know
            more than you.
          </p>
        </section>
      )}
    </article>
    )}
    </NumberHow.Provider>
    </RollScopeContext.Provider>
    </ReadableParts.Provider>
    </CurrentDocument.Provider>
  );
}

/** Which part of a block a section is: `story-3` is part 2 of `story` — as `part_index` in Rust. */
function partIndex(section: string, block: string): number | null {
  if (section === block) return 0;
  const rest = section.startsWith(`${block}-`) ? section.slice(block.length + 1) : "";
  const n = /^\d+$/.test(rest) ? Number(rest) : 0;
  return n >= 2 ? n - 1 : null;
}

/**
 * The text of a block, read: its parts one after another. Its headings fold
 * what follows them, across the parts — a part with all it holds folded
 * away goes too.
 */
export function BlockText({
  article,
  block,
  layers,
  access,
  ...shown
}: {
  article: string;
  block: string;
  layers: LayerView[];
  base: string;
  /** Who views and changes a part, where it differs from the document — the first part's stands at the title. */
  access?: (part: LayerView) => ReactNode;
  resolve: Resolve;
  /** The document's values — none in a page drawn into another. */
  values?: ValueOf;
  onPlace: (o: Place) => void;
}) {
  // Pins set apart for some readers stand on their map again.
  const texts = useMemo(() => joinPins(layers), [layers]);
  const trees = useMarkdowns(texts);
  const [folded, toggle] = useFoldedHeadings(article, block, texts.join("\n"));
  const folds: TopFolds[] | null = useMemo(() => {
    if (!trees) return null;
    const placed = sectioned(
      // An embedded document heads a section of its own, as a third-level heading.
      trees.map((nodes) => nodes.map((n) => ({ heading: headingOf(n), ends: n.kind === "embed" ? 3 : undefined }))),
      folded,
    );
    return placed.map((part) =>
      part.map(({ hidden, head }) => ({
        hidden,
        fold: head === undefined ? undefined : { folded: folded.has(head), toggle: () => toggle(head) },
      })),
    );
    // `toggle` is the same each time, as far as it matters: it reads no state.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [trees, folded]);
  return (
    <>
      {layers.map((layer, i) =>
        texts[i] === "" || (folds && folds[i].length > 0 && folds[i].every((n) => n.hidden)) ? null : (
          <PartDisplay key={layer.id} layer={{ ...layer, text: texts[i] }} nodes={trees?.[i]} folds={folds?.[i]} access={i > 0 ? access?.(layer) : null} first={i === 0} {...shown} />
        ),
      )}
    </>
  );
}

/**
 * A part of a block, read.
 *
 * A part is exactly one layer, stored with its own rule of who sees it.
 * Changing it happens in the edit mode; here it is only shown, marked
 * when it is narrower than the default — after the section's first part:
 * the first one's access stands at the section's title already, and a frame
 * around it too would read as a section inside the section.
 */
function PartDisplay({
  layer,
  nodes,
  folds,
  access,
  base,
  first = false,
  resolve,
  values,
  onPlace,
}: {
  layer: LayerView;
  /** Its text split already, and what its headings fold — see `BlockText`. */
  nodes?: Node[];
  folds?: TopFolds;
  access?: ReactNode;
  /** Who sees the document as a whole — a part seen by exactly them is not set apart. */
  base: string;
  /** The section's first part: its access is the section's, at its title. */
  first?: boolean;
  resolve: Resolve;
  values?: ValueOf;
  onPlace: (o: Place) => void;
}) {
  // Guarded means: narrower than the document itself — as the editor marks it.
  const guarded = !first && layer.policy !== base;
  // What is rolled in a part only the GM sees stays with the GM.
  const scope = useContext(RollScopeContext);
  const secret = layer.policy.trim().toLowerCase() === "gm";
  const rolls = useMemo(() => (scope && secret ? { ...scope, secret } : scope), [scope, secret]);
  return (
    <RollScopeContext.Provider value={rolls}>
    <div
      className={`block-part${guarded ? " guarded" : ""}`}
    >
      {access}
      <MarkdownText text={layer.text} nodes={nodes} folds={folds} resolve={resolve} value={values} onPlace={onPlace} />
    </div>
    </RollScopeContext.Provider>
  );
}

/** A section of the document's own as a block: its title and how it looks, from its first part. */
export function ownBlock(key: string, first: { title: string; fields: unknown }): TemplateBlock {
  const shape = ((first.fields as { shape?: Partial<TemplateBlock> } | null)?.shape ?? {}) as Partial<TemplateBlock>;
  return {
    key,
    title: first.title,
    style: shape.style ?? "text",
    place: shape.place ?? "flow",
    width: shape.width ?? 0,
    fold: shape.fold ?? "open",
    heading: shape.heading ?? 1,
    new: shape.new ?? [],
    parts: [],
  };
}

/**
 * Deleting a document — while editing only, never beside what one reads.
 * The world's and a type's page belong to them: they go only with their owner.
 */
export function DeleteDocument({
  world,
  article,
  templates,
  onPlace,
  onChanged,
  onError,
}: {
  world: WorldInfo;
  article: ArticleView;
  templates: Template[];
  onPlace: (o: Place) => void;
  onChanged: () => void;
  onError: (e: unknown) => void;
}) {
  const [remove, setRemove] = useState(false);
  // Whoever may edit it may delete it — into the trash, for the GM to bring back.
  if (!(world.isGm || permits(article.edit, world.attributes)) || pageOwner(templates, world.home, article)) return null;
  return (
    <>
      <DeleteButton what={article.name} onClick={() => setRemove(true)} />
      {remove && (
        <DeleteDialog
          title={`Delete ${article.name}`}
          name={article.name}
          warning="It goes to the trash, with every one of its parts. The GM can bring it back."
          trash
          onCancel={() => setRemove(false)}
          onDelete={(confirmation) =>
            api
              .articleDelete(world.id, article.id, confirmation)
              .then(() => {
                setRemove(false);
                onPlace(findType(templates, article.articleType) ? { kind: "list", articleType: article.articleType } : { kind: "types" });
                onChanged();
              })
              .catch(onError)
          }
        />
      )}
    </>
  );
}

