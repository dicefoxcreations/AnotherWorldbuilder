import { useContext, useMemo, useState, type ReactNode } from "react";
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
import { findType, idsIn, propertyNamed, isProperties, pageOwner, typeOf } from "../../shared/types";
import { explain, lookupFor, sumOf, summedValues } from "../../shared/Sheet";
import { RollScopeContext, type RollScope } from "../dice/DiceTray";
import { CurrentDocument, ReadableParts } from "../worlds/WorldData";
import { FoldBlock, SideColumn, useFoldedHeadings } from "../../shared/ui/BlockFold";
import { sectioned } from "../../shared/Sections";
import { joinPins } from "../../shared/GraphText";
import { linkIcon } from "./DocLink";

/**
 * Everything that points at an article, grouped by the field it points with.
 *
 * Every reference field has a reverse direction: whoever names Chicago as
 * “Home” shows up at Chicago under “Home”. The parent field is the tree, so
 * its reverse is called what it is — what lies below. Only what this reader
 * can decrypt takes part.
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
        add(field.parent ? "Below" : field.label || "Referenced by", a);
    }
    // Free properties name documents by `@@Type:Name@@`, like the text does.
    const known = new Set((type?.properties ?? []).map((f) => f.key));
    for (const layer of a.layers) {
      for (const [name, value] of Object.entries(layer.fields ?? {})) {
        if (!known.has(name) && typeof value === "string" && pointsHere(value)) add(name, a);
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

/**
 * An article, readable.
 *
 * The infobox at its position, below or beside it the blocks as flowing
 * text. A block this reader could not decrypt does not appear at all — the
 * text closes up. That is not a hidden display: the bytes cannot be opened on
 * this device.
 */
export function ArticlePage({
  world,
  article,
  templates,
  all,
  onPlace,
  onChanged,
  onError,
  actions,
  glance = false,
}: {
  world: WorldInfo;
  article: ArticleView;
  templates: Template[];
  all: ArticleView[];
  onPlace: (o: Place) => void;
  onChanged: () => void;
  onError: (e: unknown) => void;
  /** What the owner of a page adds to its head — a type's “New” button. */
  actions?: ReactNode;
  /**
   * Only to look at — the page as the link preview shows it: no access to
   * change, no deleting, no backlinks.
   */
  glance?: boolean;
}) {
  const template = typeOf(templates, article.articleType);
  const resolve = useMemo(() => linker(all, templates), [all, templates]);
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
  const drawBlock = (block: TemplateBlock) => {
    const parts = (partsOf.get(block.key) ?? []).filter((l) => l.text.trim() !== "");
    if (parts.length === 0) return null;
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

  // The document's own sections where they stand: how each looks is in its first part.
  const drawOwn = (where: BlockPlace) =>
    loose
      .filter(({ layers }) => layers.some((l) => l.text.trim() !== ""))
      .map(({ section, layers }) => ({ section, layers, block: ownBlock(section, layers[0]) }))
      .filter(({ block }) => (block.place ?? "flow") === where)
      .map(({ section, layers, block }) => (
        <FoldBlock
          key={section}
          article={article.id}
          block={block}
          access={!glance && differs(layers[0])}
        >
          <BlockText article={article.id} block={section} layers={layers} base={article.visibility} access={glance ? undefined : region} resolve={resolve} values={valueOf} onPlace={onPlace} />
        </FoldBlock>
      ));

  return (
    <CurrentDocument.Provider value={article.id}>
    <ReadableParts.Provider value={readableParts}>
    <RollScopeContext.Provider value={rolls}>
    <NumberHow.Provider value={numberHow}>
    <article className="article-page">
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
            {article.pending && (
              <span className="chip waiting" title="Yours until the GM's app has made it — then everyone's it may be.">
                Waits for the GM
              </span>
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

      {(template?.blocks ?? []).filter((b) => b.place === "top").map(drawBlock)}
      {drawOwn("top")}

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
          {(template?.blocks ?? []).filter((b) => (b.place ?? "flow") === "flow").map(drawBlock)}
          {drawOwn("flow")}
        </div>
        <SideColumn>
          {(template?.blocks ?? []).filter((b) => b.place === "side").map(drawBlock)}
          {drawOwn("side")}
        </SideColumn>
      </div>

      {(template?.blocks ?? []).filter((b) => b.place === "bottom").map(drawBlock)}
      {drawOwn("bottom")}

      {!glance && groups.length > 0 && (
        <section className="backlinks">
          {groups.map((g) => (
            <div key={g.title}>
              <h3>{g.title}</h3>
              <ul className="chips">
                {g.articles.map((a) => (
                  <li key={a.id}>
                    <button className="chip" data-icon={linkIcon(templates, a.articleType)} onClick={() => onPlace({ kind: "article", id: a.id })}>
                      {a.name}
                    </button>
                  </li>
                ))}
              </ul>
            </div>
          ))}
          <p className="small">
            Computed from what is readable on this device — not from an index that would know
            more than you.
          </p>
        </section>
      )}
    </article>
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
  const [folded, toggle] = useFoldedHeadings(article, block);
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
          <PartDisplay key={layer.id} layer={{ ...layer, text: texts[i] }} nodes={trees?.[i]} folds={folds?.[i]} access={i > 0 ? access?.(layer) : null} {...shown} />
        ),
      )}
    </>
  );
}

/**
 * A part of a block, read.
 *
 * A part is exactly one layer — an encrypted package of its own with its own
 * view. Changing it happens in the edit mode; here it is only shown, marked
 * when it is narrower than the default.
 */
function PartDisplay({
  layer,
  nodes,
  folds,
  access,
  base,
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
  resolve: Resolve;
  values?: ValueOf;
  onPlace: (o: Place) => void;
}) {
  // Guarded means: narrower than the document itself — as the editor marks it.
  const guarded = layer.policy !== base || layer.level !== "open";
  // What is rolled in a part only the GM sees stays with the GM.
  const scope = useContext(RollScopeContext);
  const secret = layer.level === "gm-only" || layer.policy.trim().toLowerCase() === "gm";
  const rolls = useMemo(() => (scope && secret ? { ...scope, secret } : scope), [scope, secret]);
  return (
    <RollScopeContext.Provider value={rolls}>
    <div
      className={`block-part${guarded ? " guarded" : ""}${
        layer.level !== "open" ? ` level-${layer.level}` : ""
      }`}
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
  if (!world.isGm || pageOwner(templates, world.home, article)) return null;
  return (
    <>
      <DeleteButton what={article.name} onClick={() => setRemove(true)} />
      {remove && (
        <DeleteDialog
          title={`Delete ${article.name}`}
          name={article.name}
          warning="The article goes with every one of its parts — including the ones you cannot see right now. This cannot be undone."
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

