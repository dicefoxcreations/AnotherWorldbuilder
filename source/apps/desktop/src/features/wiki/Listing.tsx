import { createContext, Fragment, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { FoldChevron } from "../../shared/ui/BlockFold";
import { api, type ArticleView, type Node, type Template, type TemplateField } from "../../api/client";
import { linker, scanLinks } from "../../shared/Links";
import { CurrentDocument, useWorldData } from "../worlds/WorldData";
import { formulaIn, lookupFor, picksAlong, summedValues } from "../../shared/Sheet";
import { parse, reckon, type Scope } from "../../shared/Dice";
import { RollLabel, RollScopeContext } from "../dice/DiceTray";
import { Roll } from "./MarkdownText";
import { TallyMarks } from "../../shared/ui/TallyMarks";
import { PropertyValue } from "./InfoboxForm";
import { DocLink } from "./DocLink";
import {
  findType,
  idsIn,
  isA,
  isProperties,
  parentChain,
  picksFor,
  propertyNamed,
  propertyValues,
  targetOf,
  typeOf,
  orderBy,
  sortWritten,
  typeSort,
  documentOrder,
  type SortKey,
} from "../../shared/types";
import { relationsOf } from "../../shared/Relations";
import { WorldImage } from "../../shared/ui/WorldImage";
import { AddPick } from "../../shared/ui/MultiPick";
import { editOnOpen } from "./Document";
import { makeDocument } from "./Article";
import { OutlineLinks, openHit } from "./OutlineLinks";
import { asTree } from "../../shared/documentTree";
import { documentOutline, searchDocument, type TextHit } from "../../shared/documentText";
import { leadTo } from "../../shared/Anchors";

type ListingNode = Extract<Node, { kind: "listing" }>;

/** From how many entries a list has a search field — every list, so they all work alike. */
const SEARCH_FROM = 1;

/**
 * While a document is edited, a list from one of its properties picks into
 * the editor's text — saving writes it with everything else.
 */
export const ListEditing = createContext<{
  ids: (key: string) => string[];
  set: (key: string, ids: string[]) => void;
  /** A pick's own number, as the text says it — `with Proficiency`. */
  count: (key: string, id: string) => string;
  setCount: (key: string, id: string, n: string) => void;
} | null>(null);

/**
 * `<list type=Place …>` — documents of a type, drawn where the tag stands.
 *
 * The page decides what it shows: a world page lists its continents as
 * cards, a type page its members alphabetically in three columns. Which
 * documents take part is only known here — Rust cannot know what this
 * reader could decrypt, so nothing unreadable can slip into a list.
 */
export function Listing({ node }: { node: ListingNode }) {
  return node.choose ? <Filtered node={node} /> : <ListingOf node={node} />;
}

/**
 * `filter=Role`: a button for each value the listed documents have, and
 * the list shows only those with the one chosen — or all.
 */
function Filtered({ node }: { node: ListingNode }) {
  const data = useWorldData();
  const current = useContext(CurrentDocument);
  const [chosen, setChosen] = useState<string | null>(null);
  const label = node.choose ?? "";
  if (!data) return <ListingOf node={node} />;
  const { templates, articles } = data;
  const type = node.of ? typeByName(templates, node.of) : undefined;
  const values = [
    ...new Set(
      articles
        .filter((a) => a.articleType && (!type || isA(templates, a.articleType, type.id)))
        .filter((a) => node.filter.every(([f, w]) => matches(templates, articles, a, f, w === "self" ? (current ?? "") : w)))
        .flatMap((a) => shown(templates, articles, a, label).split(",").map((v) => v.trim()).filter(Boolean)),
    ),
  ].sort((a, b) => a.localeCompare(b, undefined, { numeric: true }));
  return (
    <>
      {values.length > 1 && (
        <div className="segmented listing-filter" title={label}>
          <button className={chosen === null ? "on" : ""} onClick={() => setChosen(null)}>
            All
          </button>
          {values.map((v) => (
            <button key={v} className={chosen === v ? "on" : ""} onClick={() => setChosen(v)}>
              {v}
            </button>
          ))}
        </div>
      )}
      <ListingOf node={chosen === null ? node : { ...node, filter: [...node.filter, [label, chosen]] }} />
    </>
  );
}

function ListingOf({ node }: { node: ListingNode }) {
  const data = useWorldData();
  const current = useContext(CurrentDocument);
  const editing = useContext(ListEditing);
  // A column clicked: sorted by it first, the list's own order after it.
  const [sorted, setSorted] = useState<SortKey | null>(null);
  // What a long list is searched for — by name and what it shows.
  const [needle, setNeedle] = useState("");
  // A delete asked for once: the second click on its × does it.
  const [asking, setAsking] = useState<string | null>(null);
  if (!data) {
    return (
      <p className="listing-placeholder small">
        List of {node.from || node.of || "all documents"}
        {node.style !== "list" ? ` as ${node.style}` : ""}
      </p>
    );
  }
  const { templates, articles, onPlace } = data;

  const type = node.of ? typeByName(templates, node.of) : undefined;
  if (node.of && !type) return <p className="dead-link">There is no type “{node.of}”.</p>;
  // `sort=`: the order it starts in and goes back to — else the type's.
  const own = node.sort?.length ? sortWritten(type && typeOf(templates, type.id), node.sort) : typeSort(templates, type?.id);
  const order = orderBy(templates, articles, sorted ? [sorted, ...own.filter((k) => k.key !== sorted.key)] : own);

  // `below=self`: what hangs below the document the list stands in.
  const belowWhat = node.below === "self" ? current ?? "" : node.below;
  const below = belowWhat
    ? articles.find((a) => a.id === belowWhat || a.name.toLowerCase() === belowWhat.toLowerCase())
    : undefined;
  if (node.below && !below) return <p className="dead-link">“{node.below}” is not here.</p>;

  // `from=`: what a property of this document picks, in its order — and
  // whoever may change the document adds and removes here with a click.
  // A path goes on through the picks: `from=Classes.Features` lists what
  // each class brings, and is only read.
  const self = node.from ? articles.find((a) => a.id === current) : undefined;
  const picker = self ? typeOf(templates, self.articleType) : undefined;
  const deep = /[.[]/.test(node.from);
  const along = deep && self ? picksAlong(templates, articles, self, node.from) : undefined;
  if (deep && self && !along) return <p className="dead-link">“{node.from}” picks no documents here.</p>;
  const field = node.from && !deep ? propertyNamed(picker, node.from) : undefined;
  if (node.from && !deep && (!field || field.kind !== "reference" || !field.multiple)) {
    return <p className="dead-link">“{node.from}” picks no documents here.</p>;
  }
  // What names are worth for each listed document: inside a pick its number
  // and the picker's values count too.
  const inPick = field && self ? (lookupFor(templates, articles, self).scopes?.(node.from) ?? []) : [];
  const scopeOf = (a: ArticleView): Scope => inPick.find((x) => x.pick.id === a.id) ?? { pick: a, look: lookupFor(templates, articles, a) };
  /** `when=`: only what the formula does not make 0. */
  const allowed = (s: Scope) => {
    if (!node.when?.trim()) return true;
    try {
      return reckon(parse(node.when, s.look.rules), s.look).total !== 0;
    } catch {
      return false;
    }
  };
  // `every`: each document of the type is there, always — none to add or take out.
  const every = field?.kind === "reference" && !!field.every;
  const picked =
    field && self
      ? every
        ? picksFor(templates, articles, self, field).map((p) => p.id)
        : editing
          ? editing.ids(field.key)
          : idsIn(self, field.key)
      : null;
  // A list that keeps only some is there to be read — picking happens elsewhere.
  const mayPick =
    !!field &&
    !!self &&
    !every &&
    !node.when?.trim() &&
    (!!editing || data.world.isGm || self.layers.some((l) => isProperties(l.section) && l.mayEdit));
  // While writing, each pick's own number is changed right in the list.
  const counter =
    editing && field?.kind === "reference" && field.count && !node.when?.trim()
      ? (a: ArticleView) => (
          <input
            className="pick-count"
            inputMode="decimal"
            size={Math.max(1, editing.count(field.key, a.id).length)}
            value={editing.count(field.key, a.id)}
            placeholder="0"
            title={`${field.count} — ${a.name}`}
            onChange={(e) => editing.setCount(field.key, a.id, e.target.value)}
          />
        )
      : undefined;
  const setPicked = (ids: string[]) => {
    if (!field || !self) return;
    if (editing) editing.set(field.key, ids);
    else api.referencesSet(data.world.id, self.id, field.key, ids).then(data.onChanged).catch(() => {});
  };

  const entries: Scope[] = (
    along ??
    (picked
      ? picked.flatMap((id) => articles.filter((a) => a.id === id))
      : articles.filter((a) => (type ? isA(templates, a.articleType, type.id) : a.articleType !== ""))
    ).map(scopeOf)
  )
    .filter(({ pick: a }) => !(picked || along) || !type || isA(templates, a.articleType, type.id))
    // `self` in a filter: the document the list stands in.
    .filter(({ pick: a }) => node.filter.every(([field, wanted]) => matches(templates, articles, a, field, wanted === "self" ? current ?? "" : wanted)))
    .filter(({ pick: a }) => !below || parentChain(templates, articles, a).some((p) => p?.id === below.id))
    .filter(allowed)
    .sort((a, b) => (picked || along ? 0 : order(a.pick, b.pick)));
  // A long list gets a search: its names and what it shows.
  const searchable = !node.count && node.style !== "inline" && entries.length >= SEARCH_FROM;
  const q = searchable ? needle.trim().toLowerCase() : "";
  // A tree — a book's chapters — is searched through its text as well; one
  // of the document it stands in, that document's own text too.
  const inText = node.style === "tree";
  const standsIn = inText && q && (node.below === "self" || node.filter.some(([, wanted]) => wanted === "self")) ? articles.find((a) => a.id === current) : undefined;
  const searched = standsIn && !entries.some((e) => e.pick.id === standsIn.id) ? [scopeOf(standsIn), ...entries] : entries;
  const hitsIn = new Map(inText && q ? searched.map(({ pick: a }) => [a.id, searchDocument(a, q)]) : []);
  const found = q
    ? searched.filter(
        ({ pick: a }) => !!hitsIn.get(a.id)?.length || [a.name, ...node.show.map((s) => shown(templates, articles, a, s))].some((t) => t.toLowerCase().includes(q)),
      )
    : entries;
  const docs = found.map((e) => e.pick);
  const search = searchable ? (
    <input
      className="search listing-search"
      value={needle}
      placeholder={inText ? "Search the text" : "Search"}
      onChange={(e) => setNeedle(e.target.value)}
      onKeyDown={(e) => e.key === "Escape" && setNeedle("")}
    />
  ) : null;

  // Taking one out, and the picker for the rest — only on a list from a property.
  const remove = (a: ArticleView) =>
    mayPick ? (
      // Not a button: on a card it stands inside one.
      <span
        role="button"
        tabIndex={0}
        className="pick-x"
        title={`Remove ${a.name}`}
        onClick={(e) => {
          e.stopPropagation();
          setPicked((picked ?? []).filter((id) => id !== a.id));
        }}
      >
        ×
      </span>
    ) : null;
  const target = field && self ? targetOf(templates, self.articleType, field) : "";
  const addable = mayPick
    ? articles
        .filter((a) => a.id !== self!.id && !(picked ?? []).includes(a.id) && target && isA(templates, a.articleType, target))
        .sort((a, b) => a.name.localeCompare(b.name))
    : [];
  const add =
    addable.length > 0 ? (
      <select
        className="pick-add listing-add"
        value=""
        title={`Add to ${field!.label}`}
        onChange={(e) => e.target.value && setPicked([...(picked ?? []), e.target.value])}
      >
        <option value="">＋</option>
        {addable.map((a) => (
          <option key={a.id} value={a.id}>
            {a.name}
          </option>
        ))}
      </select>
    ) : null;

  // A list of what names this document — `where="Owner=self"`: the GM adds
  // and takes away right here. `add=new` makes one naming this document (a
  // kill) and × deletes it; else ＋ picks one that is there (an item) and ×
  // lets go of it.
  const backType = type && !node.from && !node.count ? typeOf(templates, type.id) : undefined;
  const back =
    backType && current && node.filter.length === 1 && node.filter[0][1] === "self" ? propertyNamed(backType, node.filter[0][0]) : undefined;
  const here = back ? articles.find((a) => a.id === current) : undefined;
  const backTarget = back && type ? targetOf(templates, type.id, back) : "";
  const making = node.add === "new";
  const mayBack =
    !!back &&
    !!here &&
    back.kind === "reference" &&
    !!backTarget &&
    isA(templates, here.articleType, backTarget) &&
    data.world.isGm &&
    (making || !!back.multiple);
  const letGo = (a: ArticleView) =>
    mayBack ? (
      <span
        role="button"
        tabIndex={0}
        className={making && asking === a.id ? "pick-x asking" : "pick-x"}
        title={making ? `Delete ${a.name}` : `Remove ${a.name} from ${here!.name}`}
        onClick={(e) => {
          e.stopPropagation();
          if (making && asking !== a.id) return setAsking(a.id);
          setAsking(null);
          const done = making
            ? api.articleDelete(data.world.id, a.id, a.name)
            : api.referencesSet(data.world.id, a.id, back!.key, idsIn(a, back!.key).filter((id) => id !== current));
          done.then(data.onChanged).catch(() => {});
        }}
      >
        {making && asking === a.id ? "Delete?" : "×"}
      </span>
    ) : null;
  const makeOne = async () => {
    const base = `${here!.name} – ${type!.name}`;
    const taken = new Set(articles.map((a) => a.name));
    let name = base;
    for (let n = 2; taken.has(name); n++) name = `${base} (${n})`;
    const id = await makeDocument(data.world.id, templates, articles, backType!, name, { [back!.key]: back!.kind === "reference" && back!.multiple ? [current] : current });
    editOnOpen(id);
    data.onChanged();
    onPlace({ kind: "article", id });
  };
  const backAdd = !mayBack ? null : making ? (
    <button className="compact listing-add" title={`New ${type!.name} for ${here!.name}`} onClick={() => void makeOne().catch(() => {})}>
      ＋ {type!.name}
    </button>
  ) : (
    <AddPick
      title={`Add to ${here!.name}`}
      open={articles
        .filter((a) => isA(templates, a.articleType, type!.id) && !idsIn(a, back!.key).includes(current!))
        .sort((a, b) => a.name.localeCompare(b.name))}
      onAdd={(a) =>
        void api
          .referencesSet(data.world.id, a.id, back!.key, [...idsIn(a, back!.key), current!])
          .then(data.onChanged)
          .catch(() => {})
      }
    />
  );
  const adding = add ?? (backAdd ? <span className="listing-add-back">{backAdd}</span> : null);
  const taking = mayPick ? remove : letGo;

  // What only follows the picks — a path, a condition — is simply not there when empty.
  if (docs.length === 0 && q)
    return (
      <div className="listing-box">
        {search}
        <p className="small muted">Nothing found.</p>
      </div>
    );
  if (docs.length === 0)
    return adding ? (
      <div className="listing-box">
        <p className="small muted">Nothing here yet.</p>
        {adding}
      </div>
    ) : along || node.when?.trim() || node.style === "inline" ? null : (
      <p className="small muted">Nothing here yet.</p>
    );

  const open = (a: ArticleView) => onPlace({ kind: "article", id: a.id });
  if (inText && q) {
    // Each place the words stand, under its headings; a document found by its name alone, once.
    const hits = docs.flatMap((a): TextHit[] => (hitsIn.get(a.id)?.length ? hitsIn.get(a.id)! : [{ article: a, snippet: "" }]));
    return (
      <div className="listing-box">
        {search}
        <p className="small muted">
          {hits.length} {hits.length === 1 ? "match" : "matches"} in {docs.length} {docs.length === 1 ? "document" : "documents"}
        </p>
        <ul className="document-search-results">
          {hits.map((hit, i) => (
            <li key={`${hit.article.id}/${i}`}>
              <button className="wikilink" onClick={() => openHit(hit, q, current, () => open(hit.article))}>
                {[hit.article.name, ...(hit.path ?? (hit.section ? [hit.section] : []))].join(" › ")}
              </button>
              {hit.snippet && <p>{hit.snippet}</p>}
            </li>
          ))}
        </ul>
      </div>
    );
  }
  if (node.count) return <Tally node={node} docs={docs} templates={templates} articles={articles} type={type} open={open} />;
  // What follows the picks keeps their order; anything else sorts by a click on its column.
  const sorting = picked || along ? undefined : { sorted, onSort: setSorted };
  if (node.style === "inline")
    return (
      <Drawn node={node} docs={docs} scopes={entries} templates={templates} articles={articles} type={type} open={open} remove={taking} counter={counter} data={data} />
    );
  return (
    <div className={`listing-box${mayPick || mayBack ? " picking" : ""}`}>
      {search}
      <Drawn
        node={node}
        docs={docs}
        scopes={found}
        templates={templates}
        articles={articles}
        type={type}
        open={open}
        remove={taking}
        counter={counter}
        data={data}
        sorting={sorting}
      />
      {adding}
    </div>
  );
}

/** How many of a long list are drawn first, and how many more each time one scrolls near its end. */
const FIRST = 120;
const MORE = 240;

/**
 * A long list draws a first stretch, and more as one scrolls towards its
 * end: thousands of rows at once hold the window up for seconds.
 */
function useGrowing(total: number): [number, ReactNode] {
  const [limit, setLimit] = useState(FIRST);
  const spot = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = spot.current;
    if (!el || typeof IntersectionObserver === "undefined") return;
    const watch = new IntersectionObserver((seen) => seen.some((s) => s.isIntersecting) && setLimit((l) => l + MORE), { rootMargin: "800px" });
    watch.observe(el);
    return () => watch.disconnect();
  }, [limit, total]);
  const more =
    total > limit ? (
      <div ref={spot} className="listing-more small muted">
        {total - limit} more …
      </div>
    ) : null;
  return [Math.min(limit, total), more];
}

/** The documents of a list, drawn the way it asks for. */
function Drawn({
  node,
  docs,
  scopes,
  templates,
  articles,
  type,
  open,
  remove,
  counter,
  data,
  sorting,
}: {
  node: ListingNode;
  /** A table sorts by a click on a column's head. */
  sorting?: { sorted: SortKey | null; onSort: (k: SortKey | null) => void };
  docs: ArticleView[];
  scopes: Scope[];
  /** While writing: a field for a pick's own number. */
  counter?: (a: ArticleView) => React.ReactNode;
  templates: Template[];
  articles: ArticleView[];
  type: Template | undefined;
  open: (a: ArticleView) => void;
  remove: (a: ArticleView) => React.ReactNode;
  data: NonNullable<ReturnType<typeof useWorldData>>;
}) {
  const { onPlace } = data;
  const [limit, more] = useGrowing(docs.length);
  const first = docs.slice(0, limit);

  if (node.style === "cards") {
    return (
      <>
      <div className="listing-cards">
        {first.map((a) => (
          <button key={a.id} className="listing-card" onClick={() => open(a)}>
            {wantsImage(templates, a, node.show) && (
              <span className="tile-image">
                {cardImage(templates, a, node.show) && data.world ? (
                  <WorldImage world={data.world.id} id={cardImage(templates, a, node.show)!} preview alt={a.name} />
                ) : (
                  <span className="image-placeholder">{findType(templates, a.articleType)?.icon || "📄"}</span>
                )}
              </span>
            )}
            <strong>{a.name}{remove(a)}</strong>
            {node.show
              .filter((s) => s.toLowerCase() !== "image" && fieldFor(templates, a, s)?.kind !== "image")
              .map((s) => {
                const text = shown(templates, articles, a, s);
                return text ? (
                  <span key={s} className="field">
                    <span className="field-name">{fieldFor(templates, a, s)?.label ?? s}</span>
                    {text}
                  </span>
                ) : null;
              })}
          </button>
        ))}
      </div>
      {more}
      </>
    );
  }

  if (node.style === "table") {
    // The columns are properties of the listed type — by key, as stored.
    const listed = type ? typeOf(templates, type.id) : undefined;
    // `Type` is no property: it sorts by the name of each document's type.
    const columns = node.show.map((s) => {
      const field = propertyNamed(listed, s);
      return { key: s, label: field?.label ?? headOf(s), by: field?.key ?? (s.toLowerCase() === "type" ? "type" : undefined) };
    });
    return (
      <div className="listing-scroll">
      <table className="listing-table">
        <thead>
          <tr>
            <th>
              <SortHead label="Name" by="name" sorting={sorting} />
            </th>
            {columns.map((c) => (
              <th key={c.key}>{c.by ? <SortHead label={c.label} by={c.by} sorting={sorting} /> : c.label}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {first.map((a, i) => (
            <tr key={`${i}-${a.id}`}>
              <td>
                <DocLink id={a.id} articleType={a.articleType} templates={templates} context={type?.id} onOpen={() => open(a)}>
                  {a.name}
                </DocLink>
                {counter?.(a)}
                {remove(a)}
              </td>
              {columns.map((c) => {
                const field = fieldFor(templates, a, c.key);
                const worth = reckonedCell(scopes[i], c.key, field);
                return (
                  <td key={c.key}>
                    {worth !== undefined ? (
                      worth
                    ) : field ? (
                      <PropertyValue
                        field={field}
                        value={summedValues(templates, articles, a)[field.key]}
                        articles={articles}
                        templates={templates}
                        articleType={a.articleType}
                        onPlace={onPlace}
                      />
                    ) : (
                      shown(templates, articles, a, c.key)
                    )}
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
      {more}
      </div>
    );
  }

  // In the line, by commas: `**Skills** Stealth +6, Perception +2`.
  if (node.style === "inline") {
    return (
      <span className="listing-inline">
        {docs.map((a, i) => (
          <Fragment key={`${i}-${a.id}`}>
            {i > 0 && ", "}
            <DocLink id={a.id} articleType={a.articleType} templates={templates} context={type?.id} onOpen={() => open(a)}>
              {a.name}
            </DocLink>
            {counter?.(a)}
            {node.show
              .filter((col) => !isMarks(col))
              .map((col) => {
                const worth = reckonedCell(scopes[i], col, fieldFor(templates, a, col)) ?? shown(templates, articles, a, col);
                return worth === "" || worth === undefined ? null : <Fragment key={col}> {worth}</Fragment>;
              })}
          </Fragment>
        ))}
      </span>
    );
  }

  if (node.style === "tree") {
    return <Tree node={node} docs={docs} templates={templates} articles={articles} type={type} open={open} remove={remove} data={data} />;
  }

  return (
    <>
    <ul className={`listing${node.show.length ? " with-values" : ""}`} style={{ columnCount: node.columns }}>
      {first.map((a, i) => (
        <li key={`${i}-${a.id}`}>
          {/* While writing, the number itself takes the place of its marks. */}
          {counter
            ? counter(a)
            : node.show.filter(isMarks).map((col) => <Fragment key={col}>{reckonedCell(scopes[i], col, undefined)}</Fragment>)}
          <DocLink id={a.id} articleType={a.articleType} templates={templates} context={type?.id} onOpen={() => open(a)}>
            {a.name}
          </DocLink>
          {remove(a)}
          {node.show.filter((col) => !isMarks(col)).map((col) => {
            const worth = reckonedCell(scopes[i], col, fieldFor(templates, a, col)) ?? shown(templates, articles, a, col);
            return worth === "" || worth === undefined ? null : (
              <span key={col} className="listing-value">
                {worth}
              </span>
            );
          })}
        </li>
      ))}
    </ul>
    {more}
    </>
  );
}

/**
 * A tree of documents: each with its picture — or its type's sign — and how
 * many lie below it. The first level stands open, deeper branches fold;
 * an arrow opens them.
 */
function Tree({
  node,
  docs,
  templates,
  articles,
  type,
  open,
  remove,
  data,
}: {
  node: ListingNode;
  docs: ArticleView[];
  templates: Template[];
  articles: ArticleView[];
  type: Template | undefined;
  open: (a: ArticleView) => void;
  remove: (a: ArticleView) => React.ReactNode;
  data: NonNullable<ReturnType<typeof useWorldData>>;
}) {
  // What the reader opened or closed; the rest as it starts.
  const [turned, setTurned] = useState<ReadonlySet<string>>(new Set());
  const current = useContext(CurrentDocument);
  const flat = asTree(templates, docs);
  const below = new Map<string, ArticleView[]>();
  const roots: ArticleView[] = [];
  const stack: ArticleView[] = [];
  for (const { article, depth } of flat) {
    stack.length = depth;
    if (depth === 0) roots.push(article);
    else below.set(stack[depth - 1].id, [...(below.get(stack[depth - 1].id) ?? []), article]);
    stack.push(article);
  }
  const count = (a: ArticleView): number => (below.get(a.id) ?? []).reduce((n, c) => n + 1 + count(c), 0);
  const values = node.show.filter((s) => s.toLowerCase() !== "image" && fieldFor(templates, docs[0], s)?.kind !== "image");
  const draw = (a: ArticleView, depth: number): ReactNode => {
    const kids = below.get(a.id) ?? [];
    // A chapter without chapters below it: its headings fold out instead, folded at first.
    const outline = kids.length === 0 ? documentOutline(a) : [];
    const folds = kids.length > 0 || outline.length > 0;
    const openNow = folds && (kids.length > 0 && depth === 0) !== turned.has(a.id);
    const picture = cardImage(templates, a, ["image"]);
    return (
      <li key={a.id}>
        <div className="tree-row">
          {folds ? (
            <button
              className="tree-fold"
              aria-expanded={openNow}
              title={openNow ? "Fold" : "Open"}
              onClick={() =>
                setTurned((was) => {
                  const now = new Set(was);
                  if (!now.delete(a.id)) now.add(a.id);
                  return now;
                })
              }
            >
              <FoldChevron open={openNow} />
            </button>
          ) : (
            <span className="tree-fold" />
          )}
          {picture ? (
            <WorldImage world={data.world.id} id={picture} preview className="tree-picture" alt="" />
          ) : (
            <span className="tree-picture tree-sign">{findType(templates, a.articleType)?.icon || "📄"}</span>
          )}
          <DocLink id={a.id} articleType={a.articleType} templates={templates} context={a.articleType} onOpen={() => open(a)}>
            {a.name}
          </DocLink>
          {remove(a)}
          {folds && <span className="tree-count">{kids.length ? count(a) : outline.length}</span>}
          {values.map((col) => {
            const text = shown(templates, articles, a, col);
            return text ? (
              <span key={col} className="listing-value">
                {text}
              </span>
            ) : null;
          })}
        </div>
        {openNow && (
          <ul>
            {kids.map((k) => draw(k, depth + 1))}
            <OutlineLinks entries={outline} onOpen={(anchor) => leadTo(a.id, anchor, current, () => open(a))} />
          </ul>
        )}
      </li>
    );
  };
  void type;
  return <ul className="listing-tree">{roots.map((r) => draw(r, 0))}</ul>;
}

/**
 * A column's head that sorts by it: once up, again down. The name's head
 * carries the way back to the list's own order.
 */
function SortHead({
  label,
  by,
  sorting,
  first = "up",
}: {
  label: string;
  by: string;
  sorting?: { sorted: SortKey | null; onSort: (k: SortKey | null) => void };
  /** Numbers start with the most. */
  first?: "up" | "down";
}) {
  if (!sorting) return <>{label}</>;
  const on = sorting.sorted?.key === by;
  const desc = on ? !sorting.sorted!.desc : first === "down";
  return (
    <span className="sort-head">
      <button className={on ? "on" : ""} title={`Sort by ${label}`} onClick={() => sorting.onSort({ key: by, desc })}>
        {label}
        {on && <span className="sort-way">{sorting.sorted!.desc ? " ▼" : " ▲"}</span>}
      </button>
      {by === "name" && sorting.sorted && (
        <button className="sort-reset" title="Back to the list's order" onClick={() => sorting.onSort(null)}>
          ↺
        </button>
      )}
    </span>
  );
}

/**
 * `count=Killer by=Kind`: a row for each value of a property and how many of
 * the listed documents have it — with `by`, split by another property's
 * values too. Most first; a click on a head sorts by it.
 */
function Tally({
  node,
  docs,
  templates,
  articles,
  type,
  open,
}: {
  node: ListingNode;
  docs: ArticleView[];
  templates: Template[];
  articles: ArticleView[];
  type: Template | undefined;
  open: (a: ArticleView) => void;
}) {
  const [sorted, setSorted] = useState<SortKey | null>(null);
  const listed = type ? typeOf(templates, type.id) : undefined;
  const field = propertyNamed(listed, node.count ?? "");
  const split = node.by ? propertyNamed(listed, node.by) : undefined;
  if (!field) return <p className="dead-link">“{node.count}” is no property of {type?.name ?? "these documents"}.</p>;
  if (node.by && !split) return <p className="dead-link">“{node.by}” is no property of {type?.name ?? "these documents"}.</p>;
  const byId = new Map(articles.map((a) => [a.id, a]));
  // A document a reader cannot open is not counted under its name: who it
  // is stays as closed as the document.
  const valuesOf = (a: ArticleView, f: TemplateField): string[] =>
    f.kind === "reference"
      ? idsIn(a, f.key).filter((id) => byId.has(id))
      : shown(templates, articles, a, f.key)
          .split(",")
          .map((v) => v.trim())
          .filter(Boolean);
  const nameOf = (v: string) => byId.get(v)?.name ?? v;
  // The columns in the order their own kind keeps: Boss, Major, Minion.
  const parts = split ? [...new Set(docs.flatMap((a) => valuesOf(a, split)))] : [];
  if (split?.kind === "reference") {
    parts.sort((x, y) => {
      const [p, q] = [byId.get(x)!, byId.get(y)!];
      return p.articleType === q.articleType ? documentOrder(templates, p.articleType, articles)(p, q) : nameOf(x).localeCompare(nameOf(y));
    });
  } else parts.sort((x, y) => x.localeCompare(y, undefined, { numeric: true }));
  const rows = new Map<string, { total: number; of: Map<string, number> }>();
  for (const a of docs) {
    const within = split ? valuesOf(a, split) : [];
    for (const v of valuesOf(a, field)) {
      const row = rows.get(v) ?? { total: 0, of: new Map() };
      row.total += 1;
      for (const w of within) row.of.set(w, (row.of.get(w) ?? 0) + 1);
      rows.set(v, row);
    }
  }
  const numberOf = (v: string, key: string) => (key === "total" ? rows.get(v)!.total : (rows.get(v)!.of.get(key) ?? 0));
  const byNumbers = (keys: string[]) => (x: string, y: string) => {
    for (const k of keys) {
      const d = numberOf(y, k) - numberOf(x, k);
      if (d !== 0) return d;
    }
    return 0;
  };
  const byName = (x: string, y: string) => nameOf(x).localeCompare(nameOf(y), undefined, { numeric: true });
  const usual = (x: string, y: string) => byNumbers(["total", ...parts])(x, y) || byName(x, y);
  const order = !sorted
    ? usual
    : sorted.key === "name"
      ? (x: string, y: string) => (sorted.desc ? -1 : 1) * byName(x, y)
      : (x: string, y: string) => (sorted.desc ? 1 : -1) * byNumbers([sorted.key])(x, y) || usual(x, y);
  const shownRows = [...rows.keys()].sort(order);
  const sorting = { sorted, onSort: setSorted };
  const sum = (key: string) => shownRows.reduce((n, v) => n + numberOf(v, key), 0);
  if (shownRows.length === 0) return <p className="small muted">Nothing here yet.</p>;
  return (
    <div className="listing-scroll">
    <table className="listing-table tally">
      <thead>
        <tr>
          <th>
            <SortHead label={field.label} by="name" sorting={sorting} />
          </th>
          {parts.map((p) => (
            <th key={p} className="number">
              <SortHead label={nameOf(p)} by={p} sorting={sorting} first="down" />
            </th>
          ))}
          <th className="number">
            <SortHead label="Total" by="total" sorting={sorting} first="down" />
          </th>
        </tr>
      </thead>
      <tbody>
        {shownRows.map((v) => {
          const doc = byId.get(v);
          return (
            <tr key={v}>
              <td>
                {doc ? (
                  <DocLink id={doc.id} articleType={doc.articleType} templates={templates} context={field ? targetOf(templates, type?.id ?? "", field) : undefined} onOpen={() => open(doc)}>
                    {doc.name}
                  </DocLink>
                ) : (
                  v
                )}
              </td>
              {parts.map((p) => (
                <td key={p} className="number">
                  {numberOf(v, p) || ""}
                </td>
              ))}
              <td className="number">
                <strong>{numberOf(v, "total")}</strong>
              </td>
            </tr>
          );
        })}
      </tbody>
      <tfoot>
        <tr>
          <td>Total</td>
          {parts.map((p) => (
            <td key={p} className="number">
              {sum(p)}
            </td>
          ))}
          <td className="number">
            <strong>{sum("total")}</strong>
          </td>
        </tr>
      </tfoot>
    </table>
    </div>
  );
}

/** `●Proficiency`: a number shown as so many marks — before the name in a list. */
const MARKS = /^([^\p{L}\p{N}[\s"'])\s*(.+)$/u;
const isMarks = (col: string) => MARKS.test(col);

/** A column's head: `[[+ Bonus]]` is headed `Bonus`, `●Proficiency` `Proficiency`. */
const headOf = (col: string) => formulaIn(col)?.replace(/^\s*[+-]\s*/, "").trim() ?? MARKS.exec(col)?.[2] ?? col;

/**
 * A cell reckoned inside the listed pick: a roll in `[[…]]`, a number — the
 * pick's own (`Proficiency`) or one of its properties, reckoned for whoever
 * picked it. `undefined`: nothing to reckon here, show it as stored.
 */
function reckonedCell(scope: Scope | undefined, col: string, field: ReturnType<typeof fieldFor>): ReactNode | undefined {
  if (!scope) return undefined;
  const marks = MARKS.exec(col);
  if (marks) {
    const n = Math.max(0, scope.look(marks[2]) ?? 0);
    return <TallyMarks sign={marks[1]} n={n} className="listing-marks" title={`${marks[2]} ${n}`} />;
  }
  const formula = formulaIn(col);
  if (formula !== undefined) return <ScopedRoll scope={scope} formula={formula} />;
  if (field && field.kind !== "number") return undefined;
  const n = scope.look(col);
  return n === undefined ? undefined : String(n);
}

/** A roll as it stands in text — inside the pick, named by it. */
function ScopedRoll({ scope, formula }: { scope: Scope; formula: string }) {
  const outer = useContext(RollScopeContext);
  return (
    <RollScopeContext.Provider
      value={{ document: outer?.document ?? scope.pick.name, lookup: scope.look, explain: () => "", numbers: [], secret: outer?.secret ?? false }}
    >
      <RollLabel.Provider value={scope.pick.name}>
        <Roll formula={formula} text="" />
      </RollLabel.Provider>
    </RollScopeContext.Provider>
  );
}

/** Finds what a link points at — by id, as text stores it, or by name. */
const resolveIn = (all: Template[], articles: ArticleView[]) => linker(articles, all);

/** A type by the name it has in the text — or by its id. */
const typeByName = (all: Template[], name: string) =>
  all.find((t) => t.name.toLowerCase() === name.toLowerCase()) ?? findType(all, name);

/** The property of a document's type with this name. */
const fieldFor = (all: Template[], article: ArticleView, label: string) =>
  propertyNamed(typeOf(all, article.articleType), label);

/** What a field holds, as text — references by the names of their targets. */
function shown(all: Template[], articles: ArticleView[], article: ArticleView, label: string): string {
  if (label.toLowerCase() === "type") return findType(all, article.articleType)?.name ?? "";
  const field = fieldFor(all, article, label);
  // A free property: its text, its links as the text they show.
  if (!field)
    return scanLinks(freeValue(article, label))
      .map((p) => (typeof p === "string" ? p : p.text !== p.name ? p.text : (resolveIn(all, articles)(p)?.name ?? "???")))
      .join("");
  if (field.kind === "relation") {
    const name = (id: string) => articles.find((a) => a.id === id)?.name ?? "???";
    return relationsOf(article, field.key)
      .map((e) => (e.role ? `${name(e.with)} (${name(e.role)})` : name(e.with)))
      .join(", ");
  }
  if (field.kind === "number") {
    const n = summedValues(all, articles, article)[field.key];
    return n === undefined ? "" : String(n);
  }
  const values = idsIn(article, field.key);
  if (field.kind !== "reference") return values.join(", ");
  return values.map((id) => articles.find((a) => a.id === id)?.name ?? "???").join(", ");
}

/**
 * `where="Role=NPC"` — does one of the field's values match? `Owner=*`:
 * has it any value at all.
 *
 * `where="Lies in="` asks for an empty field: the top level of a tree.
 */
function matches(
  all: Template[],
  articles: ArticleView[],
  article: ArticleView,
  label: string,
  wanted: string,
): boolean {
  if (wanted === "" || wanted === "*") {
    const field = fieldFor(all, article, label);
    const empty = field ? idsIn(article, field.key).length === 0 : freeValue(article, label) === "";
    return wanted === "" ? empty : !empty;
  }
  // A document is named by its id, as text stores it — or, where it named
  // nothing when written, still by its name.
  const field = fieldFor(all, article, label);
  if (field?.kind === "reference" && idsIn(article, field.key).includes(wanted)) return true;
  const text = shown(all, articles, article, label);
  return text
    .split(",")
    .map((v) => v.trim().toLowerCase())
    .includes(wanted.toLowerCase());
}

/** A free property's value, by its name in any case. */
function freeValue(article: ArticleView, name: string): string {
  for (const layer of article.layers) {
    for (const [key, value] of Object.entries(layer.fields ?? {})) {
      if (key.toLowerCase() === name.toLowerCase() && typeof value === "string") return value;
    }
  }
  return "";
}

/**
 * The picture on a card: the image field it names — or, for `image`, the
 * type's first image field. `null` while that field is empty.
 */
function cardImage(all: Template[], article: ArticleView, show: string[]): string | null {
  const named = show.map((s) => fieldFor(all, article, s)).find((f) => f?.kind === "image");
  const field = named ?? (show.some((s) => s.toLowerCase() === "image") ? typeOf(all, article.articleType)?.properties.find((f) => f.kind === "image") : undefined);
  const value = field ? propertyValues(article)[field.key] : undefined;
  return typeof value === "string" && value.startsWith("img:") ? value : null;
}

/** A card shows a picture when asked for `image` or for an image field. */
const wantsImage = (all: Template[], article: ArticleView, show: string[]) =>
  show.some((s) => s.toLowerCase() === "image" || fieldFor(all, article, s)?.kind === "image");

