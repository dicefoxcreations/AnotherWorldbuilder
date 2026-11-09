import { useMemo, useRef, useState } from "react";
import { Icon } from "../../shared/ui/Icons";
import { api, type ArticleView, type Access, type DocumentText, type GraphNode, type Property, type Template, type WorldInfo, type TemplateBlock } from "../../api/client";
import { ShapeRows, type Arranging } from "./BlockSettings";
import { AccessLine } from "../../shared/ui/Visibility";
import { linker, typeNames } from "../../shared/Links";
import { useSideBar } from "../../shared/ui/SideBar";
import { AccessRows, Inspector, InspectorSection, SectionAccess } from "./Inspector";
import { propertyNamed, propertyValues, articleById } from "../../shared/types";
import { WorldImage } from "../../shared/ui/WorldImage";
import { findRole, readRelations, relationFields, relationsOf, roleChoices, writeRelations, type RelationEntry, type WrittenRelation } from "../../shared/Relations";
import { type GraphEdit } from "./RelationGraph";
import { graphText, pieces as graphPieces } from "../../shared/GraphText";
import { useWorldData } from "../worlds/WorldData";
import { ImagePicker } from "../../shared/ui/ImagePicker";
import { type Links } from "./EditorTools";
import { PropertyInput } from "./PropertyInputs";

/**
 * A section of the document's own, chosen by its title: its name, where it
 * stands among the document's own sections, a new one below, or out. They are
 * the document's — the type's sections are set in `BlockSettings`.
 */
export function OwnSectionSettings({
  block,
  doc,
  own,
  arrange,
  onDoc,
  onBelow,
  onClose,
}: {
  block: TemplateBlock;
  doc: DocumentText;
  own: (key: string) => boolean;
  /** Who is not the GM moves it among all the document's sections. */
  arrange?: Arranging;
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
          {/* Where it stands among the others, a new one, none: the GM's. */}
          {!world?.isGm && arrange && (() => {
            const here = arrange.keys.filter((k) => arrange.placeOf(k) === arrange.placeOf(block.key));
            const i = here.indexOf(block.key);
            const swap = (other: string) => {
              const next = [...arrange.keys];
              const [a, b] = [next.indexOf(block.key), next.indexOf(other)];
              [next[a], next[b]] = [next[b], next[a]];
              void arrange.onOrder(next);
            };
            return (
              <>
                <span className="small muted">Order</span>
                <div className="segmented">
                  <button disabled={i <= 0} onClick={() => swap(here[i - 1])}>
                    <Icon name="up" /> Up
                  </button>
                  <button disabled={i < 0 || i >= here.length - 1} onClick={() => swap(here[i + 1])}>
                    <Icon name="down" /> Down
                  </button>
                </div>
              </>
            );
          })()}
          {world?.isGm && (
            <>
              <span className="small muted">Order</span>
              <div className="segmented">
                <button disabled={at <= 0} onClick={() => swap(mine[at - 1])}>
                  <Icon name="up" /> Up
                </button>
                <button disabled={at < 0 || at >= mine.length - 1} onClick={() => swap(mine[at + 1])}>
                  <Icon name="down" /> Down
                </button>
              </div>
            </>
          )}
          <ShapeRows block={block} onPatch={patch} part="more" />
        </div>
        {world?.isGm && (
          <>
            <button className="compact" onClick={onBelow}>
              <Icon name="add" /> Section below
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
          </>
        )}
      </InspectorSection>
    </Inspector>
  );
}

/**
 * The infobox while editing — where it stands and as it looks when read,
 * its values changed in place. An image field is no text: choosing one
 * writes it right away, as on the page.
 */
export function InfoboxEdit({
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
  // Who sees and changes it: in the side bar, the GM's to set.
  const [inspecting, setInspecting] = useState(false);
  useSideBar(`infobox:${article.id}`, inspecting, () => setInspecting(false));
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
          {id ? <WorldImage world={world.id} id={id} alt={label} /> : <span className="image-placeholder"><Icon name="image" /></span>}
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
  // Anywhere on the box but on what one fills in or follows — and not in
  // a menu one of them opened elsewhere on the page.
  const pick = (e: React.MouseEvent<HTMLElement>) => {
    const at = e.target as Element;
    if (!e.currentTarget.contains(at) || at.closest("input, select, textarea, button, a, label, [contenteditable], [role=button], .view-picker")) return;
    setInspecting(true);
  };
  return (
    <aside className={`infobox ${box.align} ${style ? "fixed" : box.width} editing${world.isGm ? " pickable" : ""}${inspecting ? " inspected" : ""}`} style={style} onClick={world.isGm ? pick : undefined}>
      {(world.isGm || access !== doc.access) && (
        <div className="infobox-access">
          {world.isGm ? (
            <button className={`access-button${inspecting ? " on" : ""}`} onClick={() => setInspecting(true)} title="Who sees and changes it">
              <AccessLine world={world} view={access.view} edit={access.edit} />
            </button>
          ) : (
            <AccessLine world={world} view={access.view} edit={access.edit} />
          )}
        </div>
      )}
      {inspecting && (
        <Inspector title="Infobox" onClose={() => setInspecting(false)}>
          <InspectorSection title="Access">
            <AccessRows world={world} value={access} onChange={(patch) => onAccess({ ...access, ...patch })} />
          </InspectorSection>
        </Inspector>
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
export function useGraphEdit(
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
      const locked = relationsOf(article, f.key).filter((e) => !!!articleById(all, e.with));
      mine.set(f.key, [...written, ...locked]);
    }
    return new Map([[article.id, mine]]);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [doc.properties, all, templates, article]);

  const name = (id: string) => articleById(all, id)?.name ?? "";
  /**
   * Changes what this document's text says about `other`: the relation of
   * the roles it `was` — none of them: one more — without, every one.
   */
  const writeOwn = (other: string, property: string, roles: [string, string] | null, was?: [string, string]) => {
    const label = templates.flatMap((t) => t.properties).find((f) => f.key === property)?.label ?? "";
    const field = own.find((f) => f.key === property) ?? own.find((f) => f.label.toLowerCase() === label.toLowerCase());
    const target = articleById(all, other);
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
    const choices = roleChoices(templates, all, field);
    const idOf = (role: string) => findRole(choices, role)?.id ?? "";
    const meant = (r: WrittenRelation) =>
      resolve(r.link)?.id === other && (!was || (idOf(r.role) === was[0] && idOf(r.ownRole) === was[1]));
    const at = rows.findIndex(meant);
    const kept = rows.filter((r) => !meant(r));
    if (entry) kept.splice(at >= 0 ? at : kept.length, 0, entry);
    // The same relation twice says nothing more.
    const seen = new Set<string>();
    const value = writeRelations(
      kept.filter((r) => {
        const key = `${resolve(r.link)?.id ?? r.link.name}|${idOf(r.role)}|${idOf(r.ownRole)}`;
        if (seen.has(key)) return false;
        seen.add(key);
        return true;
      }),
    );
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
          ({ from, to, property, roles, was }) =>
            !(from === article.id && writeOwn(to, property, roles, was)) &&
            !(to === article.id && writeOwn(from, property, roles && [roles[1], roles[0]], was && [was[1], was[0]])),
        );
        if (rest.length === 0) return;
        await api.relationsSet(
          world.id,
          rest.map((c) => ({ article: c.from, property: c.property, with: c.to, roles: c.roles, was: c.was })),
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
