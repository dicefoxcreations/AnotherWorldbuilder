import { useEffect, useMemo, useRef, useState } from "react";
import { Icon } from "../../shared/ui/Icons";
import { api, type ArticleView, type BlockPart, type Caret, type DocumentText, type Property, type Template, type TemplateField, type WorldInfo, type BlockPlace, type TemplateBlock } from "../../api/client";
import { FoldBlock, SideColumn } from "../../shared/ui/BlockFold";
import { addSection, BlockSettings, moveBlock, ownerOf, type Arranging } from "./BlockSettings";
import { AccessLine, samePolicy } from "../../shared/ui/Visibility";
import { linker, linkIn, linksIn, linkText, linkTo, scanLinks } from "../../shared/Links";
import { anchorsIn, relinked, renameInText, withoutAnchor } from "../../shared/Anchors";
import { cutAtHeadings } from "../../shared/Sections";
import { type RemoteCaret } from "../../shared/editor/RemoteCarets";
import { type MarkKind, type Range } from "../../shared/Marks";
import { useSideBar } from "../../shared/ui/SideBar";
import { SectionAccess } from "./Inspector";
import { asRead, blockRank, lookOf } from "../../shared/BlockOrder";
import { propertyNamed, typeOf, articleById } from "../../shared/types";
import { RelationGraph } from "./RelationGraph";
import { CurrentDocument, ReadableParts, useWorldData } from "../worlds/WorldData";
import { RollScopeContext, type RollScope } from "../dice/DiceTray";
import { explain, lookupFor, sumOf, summedValues } from "../../shared/Sheet";
import { SheetEdit, ValuesProvider } from "./MarkdownText";
import { PropertyValue } from "./InfoboxForm";
import { ListEditing } from "./Listing";
import { type Run, sections } from "./CodeEditor";
import { newSession } from "./EditorBasics";
import { type SectionActions } from "./EditorInspectors";
import { CutSections, type Links, SectionBelow } from "./EditorTools";
import { PartEditor, mayEditPart } from "./PartEditor";
import { PropertyInput, boundIn, uniqueName } from "./PropertyInputs";
import { InfoboxEdit, OwnSectionSettings, useGraphEdit } from "./VisualSettings";

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
  // Who is not the GM arranges a section for this document alone.
  const arranging = (shown: TemplateBlock): Arranging => {
    const read = asRead(doc.blocks, type, article);
    return {
      shown,
      keys: read.map((b) => b.key),
      placeOf: (key) => read.find((b) => b.key === key)?.place ?? "flow",
      onLook: (look) => api.blockLookSet(world.id, article.id, shown.key, look).then(() => reload?.()),
      onOrder: (keys) => api.blockOrderSet(world.id, article.id, keys).then(() => reload?.()),
    };
  };
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
      parts: [{ key: `new-${stamp}-${i}`, text: s.text, view: part.view, edit: part.edit }],
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
      parts: [{ key, text: "", view: doc.access.view, edit: doc.access.edit }],
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
      const picked = articleById(all, id);
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
      const value = ids.flatMap((id) => articleById(all, id) ?? []).map((a) => linkTo(templates, a)).join(", ");
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
            {open ? <Icon name="chevron-down" /> : <Icon name="chevron-right" />} {r.name} <small className="small">{r.count}</small>
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
              <Icon name="close" />
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
    const typedBlock = type?.blocks.find((x) => x.key === written.key);
    const shape = typedBlock && lookOf(typedBlock, article);
    const block = shape ? { ...written, title: shape.title, style: shape.style, place: shape.place, width: shape.width, fold: shape.fold, heading: shape.heading, new: shape.new } : written;
    const own = !typed(block.key);
    // The GM sets up every section; a player the ones they may write in —
    // of a section the type sets out only who sees their part.
    const writable = block.parts.some((p) => mayEditPart(article, p.key));
    const pickable = (world.isGm || writable) && (own || !!ownerOf(templates, article.articleType, block.key));
    const below = !pickable || !world.isGm
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
        block.parts[0] && !(samePolicy(block.parts[0].view, doc.access.view) && samePolicy(block.parts[0].edit, doc.access.edit)) && (() => {
          const line = (
            <AccessLine
              world={world}
              view={samePolicy(block.parts[0].view, doc.access.view) ? undefined : block.parts[0].view}
              edit={samePolicy(block.parts[0].edit, doc.access.edit) ? undefined : block.parts[0].edit}
            />
          );
          // Changed in the section's settings, in the side bar.
          return pickable ? (
            <button className="access-button" onClick={(e) => (e.stopPropagation(), setPicked(block.key))} title="Who sees and changes it">
              {line}
            </button>
          ) : (
            line
          );
        })()
      }
      onMove={
        world.isGm && pickable && !own
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
              <CutSections.Provider value={own && world.isGm && pickable && p === block.parts.length - 1 ? (text, level) => cutSections(b, p, text, level) : null}>
              <PartEditor
                world={world}
                part={part}
                title={block.title}
                inherited={samePolicy(part.view, base)}
                sections={sectionActions}
                docAccess={doc.access}
                region={
                  // The first part's stands at the section's title.
                  p > 0 && !(samePolicy(part.view, doc.access.view) && samePolicy(part.edit, doc.access.edit))
                }
                removable={world.isGm && block.parts.length > 1}
                editor={!world.isGm && mayEditPart(article, part.key)}
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
  // In the order and the places the type and the document give them now —
  // a move in the side bar or by dragging shows at once.
  const rank = blockRank(type, article);
  const placeOf = (block: TemplateBlock) => {
    const typedBlock = type?.blocks.find((x) => x.key === block.key);
    return (typedBlock ? lookOf(typedBlock, article).place : block.place) ?? "flow";
  };
  const arranged = (where: BlockPlace) =>
    doc.blocks
      .map((block, b) => ({ block, b }))
      .filter(({ block }) => placeOf(block) === where)
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
          ofType={world.isGm}
          arrange={world.isGm ? undefined : arranging(lookOf(pickedBlock, article))}
          onClose={() => setPicked(null)}
          onAdded={setPicked}
          access={(() => {
            const b = doc.blocks.findIndex((x) => x.key === pickedBlock.key);
            return b < 0 ? null : <SectionAccess world={world} block={doc.blocks[b]} mayEdit={(key) => mayEditPart(article, key)} onPart={(p, patch) => setPart(b, p, patch)} />;
          })()}
        />
      )}
      {pickedOwn && (
        <OwnSectionSettings
          key={pickedOwn.key}
          block={pickedOwn}
          doc={doc}
          own={(key) => !typed(key)}
          arrange={world.isGm ? undefined : arranging(pickedOwn)}
          onDoc={onDoc}
          onBelow={() => ownSectionBelow(pickedOwn.key)}
          onClose={() => setPicked(null)}
        />
      )}
      {/* Opened from the head; folded, it takes no room. */}
      {propsOpen && (
        <section className="properties">
          <button className="fold ghost compact" onClick={() => onPropsOpen(false)} aria-expanded>
            <Icon name="chevron-down" /> Properties <small className="small">{doc.properties.length}</small>
          </button>
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
            <Icon name="add" /> Property
          </button>
        </section>
      )}

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
        onAccess={(a) => onDoc({ ...doc, infobox: samePolicy(a.view, doc.access.view) && samePolicy(a.edit, doc.access.edit) ? undefined : a })}
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
