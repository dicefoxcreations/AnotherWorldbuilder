import { useState } from "react";
import { api, type BlockFold, type BlockPlace, type BlockStyle, type Template, type TemplateBlock, type WorldInfo } from "../../api/client";
import { ancestors, findType, typeOf } from "../../shared/types";
import { useWorldData } from "../worlds/WorldData";
import { Inspector, InspectorSection } from "./Inspector";
import { MakesPick } from "./ListCard";

/** The type a block comes from: the document's own, or the supertype it inherits it from. */
export function ownerOf(templates: Template[], articleType: string, key: string): Template | undefined {
  const seen = new Set<string>();
  for (let t = templates.find((x) => x.id === articleType); t && !seen.has(t.id); t = templates.find((x) => x.id === t!.supertype)) {
    seen.add(t.id);
    if (t.blocks.some((b) => b.key === key)) return t;
  }
  return undefined;
}

/**
 * Moves a block before or after another one of the same document type —
 * and into the other's place (top, in text, bottom) when it stood elsewhere.
 *
 * Blocks of one type, with no order set along the way, move within that
 * type: for every type below it too. Otherwise the document's own type
 * orders them all, inherited ones included. Resolves with the types as
 * saved.
 */
export async function moveBlock(world: string, templates: Template[], articleType: string, key: string, to: string, after: boolean): Promise<Template[]> {
  const blocks = typeOf(templates, articleType)?.blocks ?? [];
  const moved = blocks.find((b) => b.key === key);
  const target = blocks.find((b) => b.key === to);
  const own = findType(templates, articleType);
  const from = ownerOf(templates, articleType, key);
  const into = ownerOf(templates, articleType, to);
  if (!moved || !target || !own || !from || !into || key === to) return templates;
  let all = templates;
  const fresh = (id: string) => all.find((t) => t.id === id)!;
  const place = target.place ?? "flow";
  if ((moved.place ?? "flow") !== place) {
    const owner = fresh(from.id);
    all = await api.templateSave(world, { ...owner, blocks: owner.blocks.map((b) => (b.key === key ? { ...b, place } : b)) });
  }
  const put = <T,>(list: T[], keyOf: (x: T) => string): T[] => {
    const rest = list.filter((x) => keyOf(x) !== key);
    const at = rest.findIndex((x) => keyOf(x) === to) + (after ? 1 : 0);
    const item = list.find((x) => keyOf(x) === key)!;
    return [...rest.slice(0, at), item, ...rest.slice(at)];
  };
  const ordered = [own, ...ancestors(templates, own)].some((t) => t.order?.length);
  if (from.id === into.id && !ordered) {
    const owner = fresh(from.id);
    return api.templateSave(world, { ...owner, blocks: put(owner.blocks, (b) => b.key) });
  }
  return api.templateSave(world, { ...fresh(own.id), order: put(blocks.map((b) => b.key), (k) => k) });
}

/** A new id, as Rust gives one (`template_text::new_id`): a ULID, in small letters. */
function newId(): string {
  const digits = "0123456789abcdefghjkmnpqrstvwxyz";
  let time = Date.now();
  let out = "";
  for (let i = 0; i < 10; i++) {
    out = digits[time % 32] + out;
    time = Math.floor(time / 32);
  }
  const random = crypto.getRandomValues(new Uint8Array(16));
  for (let i = 0; i < 16; i++) out += digits[random[i] % 32];
  return out;
}

/**
 * A new section for every document of a type, below another one — written
 * into the document's own type, where its documents read it, and put after
 * `after`. It takes the type's access, as `<block>` in its text would.
 * Resolves with the types as saved and the new block's key.
 */
export async function addSection(world: string, templates: Template[], articleType: string, after: string): Promise<{ all: Template[]; key: string }> {
  const own = findType(templates, articleType);
  if (!own) throw new Error("This document has no type to add a section to.");
  const below = typeOf(templates, articleType)?.blocks.find((b) => b.key === after);
  const key = newId();
  const access = own.auth ?? { view: "players", edit: "gm", level: "open" };
  const block: TemplateBlock = {
    key,
    title: "New section",
    style: "text",
    place: below?.place ?? "flow",
    fold: "open",
    heading: 1,
    parts: [{ key, text: "", view: access.view, edit: access.edit, level: access.level }],
  };
  let all = await api.templateSave(world, { ...own, blocks: [...own.blocks, block] });
  if (below) all = await moveBlock(world, all, articleType, key, after, true);
  return { all, key };
}

const PLACES: [BlockPlace, string][] = [
  ["top", "Top"],
  ["flow", "In text"],
  ["bottom", "Bottom"],
  ["side", "Side"],
];

const FOLDS: [BlockFold, string][] = [
  ["open", "Open"],
  ["folded", "Folded"],
  ["off", "Never"],
];

const STYLES: [BlockStyle, string][] = [
  ["text", "Text"],
  ["box", "Box"],
  ["quote", "Quote"],
  ["statblock", "Stat block"],
];

/**
 * The rows of a block's look in a side bar — size, style and place, then
 * folding and its create button — for a type's block and a document's own
 * section alike. Two halves, so the order can stand between them.
 */
export function ShapeRows({ block, onPatch, part }: { block: TemplateBlock; onPatch: (p: Partial<TemplateBlock>) => void; part: "look" | "more" }) {
  const save = onPatch;
  if (part === "more")
    return (
      <>
        <span className="small muted">Folds</span>
        <div className="segmented">
          {FOLDS.map(([v, t]) => (
            <button key={v} className={(block.fold ?? "open") === v ? "on" : ""} onClick={() => save({ fold: v })}>
              {t}
            </button>
          ))}
        </div>
        <span className="small muted" title="A ＋ beside the title makes one">New</span>
        <MakesPick of={block.new ?? []} onChange={(names) => save({ new: names })} />
      </>
    );
  return (
    <>
      <span className="small muted">Heading</span>
      <div className="segmented">
        {[1, 2, 3, 4].map((n) => (
          <button
            key={n}
            className={(block.heading ?? 1) === n ? "on" : ""}
            title={n === 1 ? "As # in the text" : n === 4 ? "Text size" : `As ${"#".repeat(n)} in the text`}
            onClick={() => save({ heading: n })}
          >
            H{n}
          </button>
        ))}
      </div>
      <span className="small muted">Style</span>
      <select value={block.style} onChange={(e) => save({ style: e.target.value as BlockStyle })}>
        {STYLES.map(([v, t]) => (
          <option key={v} value={v}>
            {t}
          </option>
        ))}
      </select>
      <span className="small muted">Place</span>
      <div className="segmented">
        {PLACES.map(([v, t]) => (
          <button key={v} className={(block.place ?? "flow") === v ? "on" : ""} onClick={() => save({ place: v })}>
            {t}
          </button>
        ))}
      </div>
      {block.place === "side" && (
        <>
          <span className="small muted">Width</span>
          <span className="width-field">
            <input
              type="number"
              min={160}
              max={800}
              step={10}
              value={block.width || ""}
              placeholder="320"
              onChange={(e) => save({ width: Math.max(0, Math.round(Number(e.target.value) || 0)) })}
            />
            <span className="small muted">px</span>
          </span>
        </>
      )}
    </>
  );
}

/**
 * A block chosen in the visual editor: its title and its size, style, place
 * and folding.
 * They are its type's — for every document of the type — and are saved to
 * the type at once. The game master's.
 */
export function BlockSettings({
  world,
  templates,
  articleType,
  block,
  onClose,
  onAdded,
  access,
}: {
  world: WorldInfo;
  templates: Template[];
  /** The document's type — where to look for the block's. */
  articleType: string;
  block: TemplateBlock;
  onClose: () => void;
  /** A new section was made below this one — to choose it next. */
  onAdded?: (key: string) => void;
  /** Who sees and changes it in this document — first, above what the type says. */
  access?: React.ReactNode;
}) {
  const data = useWorldData();
  const owner = ownerOf(templates, articleType, block.key);
  const [title, setTitle] = useState(block.title);
  const [failed, setFailed] = useState("");
  if (!owner) return null;
  const save = (patch: Partial<TemplateBlock>) => {
    setFailed("");
    api
      .templateSave(world.id, { ...owner, blocks: owner.blocks.map((b) => (b.key === block.key ? { ...b, ...patch } : b)) })
      .then((all) => data?.onTemplates?.(all))
      .catch((e) => setFailed(String(e)));
  };
  const commitTitle = () => title.trim() !== block.title && save({ title: title.trim() });
  // Its neighbours where it stands — on top, in the text or below.
  const here = (typeOf(templates, articleType)?.blocks ?? []).filter((b) => (b.place ?? "flow") === (block.place ?? "flow"));
  const at = here.findIndex((b) => b.key === block.key);
  const move = (to: TemplateBlock, after: boolean) => {
    setFailed("");
    moveBlock(world.id, templates, articleType, block.key, to.key, after)
      .then((all) => data?.onTemplates?.(all))
      .catch((e) => setFailed(String(e)));
  };
  return (
    <Inspector title={`Section · ${block.title || "untitled"}`} onClose={onClose}>
      {access}
      <InspectorSection title={`Type · ${owner.name}`}>
        <div className="inspector-access block-settings">
          <span className="small muted">Title</span>
          <input
            value={title}
            placeholder="none"
            onChange={(e) => setTitle(e.target.value)}
            onBlur={commitTitle}
            onKeyDown={(e) => e.key === "Enter" && commitTitle()}
          />
          <ShapeRows block={block} onPatch={save} part="look" />
          <span className="small muted">Order</span>
          <div className="segmented">
            <button disabled={at <= 0} title="Before the one above" onClick={() => move(here[at - 1], false)}>
              ↑ Up
            </button>
            <button disabled={at < 0 || at >= here.length - 1} title="After the one below" onClick={() => move(here[at + 1], true)}>
              ↓ Down
            </button>
          </div>
          <ShapeRows block={block} onPatch={save} part="more" />
        </div>
        <button
          className="compact"
          title={`A section for every ${findType(templates, articleType)?.name ?? "document of this type"}`}
          onClick={() => {
            setFailed("");
            addSection(world.id, templates, articleType, block.key)
              .then(({ all, key }) => {
                data?.onTemplates?.(all);
                data?.onChanged();
                onAdded?.(key);
              })
              .catch((e) => setFailed(String(e)));
          }}
        >
          ＋ Section below
        </button>
        {failed && <p className="small error">{failed}</p>}
      </InspectorSection>
    </Inspector>
  );
}
