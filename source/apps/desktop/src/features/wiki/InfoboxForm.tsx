import { createContext, useContext, type ReactNode } from "react";
import type { ArticleView, Infobox, Place, Template, TemplateField } from "../../api/client";
import { linker, scanLinks } from "../../shared/Links";
import { documentsOf, picksOf, targetOf, type Pick } from "../../shared/types";
import { DocLink } from "./DocLink";
import { entriesOf } from "../../shared/Relations";
import { WorldImage } from "../../shared/ui/WorldImage";
import { ZoomImage } from "../../shared/ui/ImageZoom";
import { MultiPick } from "../../shared/ui/MultiPick";
import { OurMomentInput, WorldMomentPicker } from "../../shared/ui/DateInputs";
import { DEFAULT_CALENDAR, formatMoment, formatOurs } from "../../shared/Calendar";
import { useWorldData } from "../worlds/WorldData";
import { readNumber } from "../../shared/Sheet";

/**
 * How a number of the document shown came about — `15 + 1 Athlete` — by
 * property key; empty when it is just its own. See `shared/Sheet`.
 */
export const NumberHow = createContext<((key: string) => string) | null>(null);

/**
 * Everything that is picked is a document — of the field's type or one of
 * its subtypes, sorted by name. The document itself never offers itself: a
 * place does not lie in itself.
 */
type Context = {
  templates: Template[];
  /** The type of the article shown — a bare `parent` resolves against it. */
  articleType: string;
  /** The article shown, if it exists already. */
  selfId?: string;
};

function choices(field: TemplateField, articles: ArticleView[], ctx: Context): ArticleView[] {
  const target = targetOf(ctx.templates, ctx.articleType, field);
  if (!target) return [];
  return documentsOf(articles, ctx.templates, target).filter((a) => a.id !== ctx.selfId);
}

/**
 * The infobox of an article.
 *
 * The same component for reading and writing, so nothing looks different
 * while writing than afterwards. No grid: the properties it shows, drawn as
 * label-value pairs, with the image at the top.
 */
export function InfoboxForm({
  infobox,
  properties,
  values,
  articles,
  templates,
  articleType,
  selfId,
  editing,
  showEmpty,
  onValue,
  onPlace,
  onImage,
  access,
}: {
  /** Who sees and changes it — beside it where that differs from the document. */
  access?: ReactNode;
  infobox: Infobox;
  /** The type's properties, inherited ones included — what `bind=` names. */
  properties: TemplateField[];
  /** By property key. */
  values: Record<string, unknown>;
  /** All readable articles — for references. */
  articles: ArticleView[];
  templates: Template[];
  articleType: string;
  selfId?: string;
  editing?: boolean;
  /** Show empty fields too — for the preview in the TemplateEditor. */
  showEmpty?: boolean;
  onValue?: (key: string, value: unknown) => void;
  onPlace?: (o: Place) => void;
  /** Whoever may change the document chooses an image field's image here. */
  onImage?: (field: string) => void;
}) {
  // Only what the type lays out is shown. A property nothing binds is data
  // — for lists, filters and links — and never part of the page.
  const fields = infobox.show.flatMap((s) => {
    // The infobox keeps what it shows by key.
    const field = properties.find(
      (p) => p.key === s.property || p.label.toLowerCase() === s.property.toLowerCase(),
    );
    return field ? [{ ...field, label: s.label || field.label }] : [];
  });
  if (fields.length === 0 || infobox.align === "none") return null;

  // `auto` means: as wide as the content, capped by the stylesheet. A number
  // is the width in pixels. A fixed width nobody can justify no longer
  // exists — a box measuring 280 px around a 200 px image looked like an
  // accident.
  const number = Number(infobox.width);
  const style = Number.isFinite(number) && number > 0 ? { width: number } : undefined;
  const widthClass = style ? "fixed" : infobox.width;
  const ctx: Context = { templates, articleType, selfId };

  return (
    <aside className={`infobox ${infobox.align} ${widthClass}`} style={style}>
      {access && <div className="infobox-access">{access}</div>}
      <dl>
        {fields.map((field) => (
          <Line
            key={field.key}
            field={field}
            value={values[field.key]}
            articles={articles}
            ctx={ctx}
            editing={editing}
            showEmpty={showEmpty}
            onValue={onValue}
            onPlace={onPlace}
            onImage={onImage}
          />
        ))}
      </dl>
    </aside>
  );
}

function Line({
  field,
  value,
  articles,
  ctx,
  editing,
  showEmpty,
  onValue,
  onPlace,
  onImage,
}: {
  field: TemplateField;
  value: unknown;
  articles: ArticleView[];
  ctx: Context;
  editing?: boolean;
  showEmpty?: boolean;
  onValue?: (key: string, value: unknown) => void;
  onPlace?: (o: Place) => void;
  onImage?: (field: string) => void;
}) {
  const world = useWorldData();
  // The image stands at the top without a label — it explains itself.
  if (field.kind === "image") {
    // A size from the type is a frame; without one the image has its own.
    const sized = !!(field.width && field.height);
    const style: React.CSSProperties = sized
      ? { aspectRatio: `${field.width} / ${field.height}`, width: `${field.width}px`, maxWidth: "100%" }
      : { width: "100%" };
    const id = typeof value === "string" && value.startsWith("img:") ? value : null;
    // Nothing to show and nobody to choose: no empty frame.
    if (!id && !onImage && !showEmpty) return null;
    const inner =
      id && world ? (
        onImage ? <WorldImage world={world.world.id} id={id} alt={field.label} /> : <ZoomImage world={world.world.id} id={id} alt={field.label} />
      ) : (
        <span className="image-placeholder">🖼</span>
      );
    const look = `infobox-image ${field.align}${sized ? "" : " natural"}`;
    return onImage ? (
      <button className={`${look} choosable`} style={style} onClick={() => onImage(field.key)} title="Choose an image">
        {inner}
      </button>
    ) : (
      <div className={look} style={style}>
        {inner}
      </div>
    );
  }

  // In the finished article an empty detail stays away — a line “Age: —”
  // tells nobody anything. In the template preview it is exactly the point.
  if (!editing && !showEmpty && isEmpty(value)) return null;

  return (
    <div className="infobox-row">
      {/* Without a label the key stands there — it stays empty in the text,
          but a nameless line in the box helps nobody. */}
      <dt>{field.label || field.key}</dt>
      <dd>
        {editing && onValue ? (
          <Input field={field} value={value} articles={articles} ctx={ctx} onValue={onValue} />
        ) : isEmpty(value) ? (
          <span className="muted">—</span>
        ) : (
          <Display field={field} value={value} articles={articles} ctx={ctx} onPlace={onPlace} />
        )}
      </dd>
    </div>
  );
}


/** An image field's image, inline — for `%%Name%%` in the text. */
function ImageValue({ value }: { value: unknown }) {
  const world = useWorldData();
  if (!world || typeof value !== "string" || !value.startsWith("img:")) return <span className="image-placeholder">🖼</span>;
  return <WorldImage world={world.world.id} id={value} className="inline-image" />;
}

export const isEmpty = (value: unknown) =>
  value === undefined || value === null || value === "" || (Array.isArray(value) && value.length === 0);

/**
 * A number as read: what it does to others in signs everyone reads — `+1`,
 * `−1`, `= 19`, `≥ 19`, `≤ 3`.
 */
function shownNumber(value: unknown): string {
  const n = readNumber(value);
  if (!n) return String(value);
  const sign = { "": "", "+": "+", "-": "−", "=": "= ", ">=": "≥ ", "<=": "≤ " }[n.op];
  return `${sign}${n.amount}`;
}

/** A property's value, read — for `%%Name%%` and elements with `bind=` in the text. */
export function PropertyValue({
  field,
  value,
  articles,
  templates,
  articleType,
  onPlace,
}: {
  field: TemplateField;
  value: unknown;
  articles: ArticleView[];
  templates: Template[];
  articleType: string;
  onPlace?: (o: Place) => void;
}) {
  if (isEmpty(value)) return <span className="muted">—</span>;
  if (field.kind === "image") return <ImageValue value={value} />;
  return (
    <Display
      field={field}
      value={value}
      articles={articles}
      ctx={{ templates, articleType }}
      onPlace={onPlace}
    />
  );
}

function Display({
  field,
  value,
  articles,
  ctx,
  onPlace,
}: {
  field: TemplateField;
  value: unknown;
  articles: ArticleView[];
  ctx: Context;
  onPlace?: (o: Place) => void;
}) {
  const world = useWorldData();
  const how = useContext(NumberHow);
  if (field.kind === "number") {
    const why = how?.(field.key) ?? "";
    return (
      <span className={why ? "summed" : undefined} title={why || undefined}>
        {shownNumber(value)}
        {field.unit ? ` ${field.unit}` : ""}
      </span>
    );
  }
  if (field.kind === "relation") {
    // One line each: who, and what they are to this document. Whom this
    // reader cannot open stays ??? — that there is someone is no secret.
    const name = (id: string) => articles.find((a) => a.id === id)?.name ?? "???";
    return (
      <span className="relations">
        {entriesOf(value).map((e, i) => {
          const target = articles.find((a) => a.id === e.with);
          return (
            <span key={i} className="relation">
              {target && onPlace ? (
                <DocLink id={target.id} articleType={target.articleType} templates={ctx.templates} context={field.articleType} onOpen={() => onPlace({ kind: "article", id: target.id })}>
                  {target.name}
                </DocLink>
              ) : (
                <span className="dead-link">{target?.name ?? "???"}</span>
              )}
              {e.role && <span className="relation-role">{name(e.role)}</span>}
            </span>
          );
        })}
      </span>
    );
  }
  if (field.kind === "reference") {
    return (
      <span className="references">
        {picksOf(value).map(({ id, count }) => {
          // Every value is a document, so every value is a link — even “male”.
          // Whoever writes about it later finds the page already linked.
          const target = articles.find((a) => a.id === id);
          const link =
            target && onPlace ? (
              <DocLink id={id} articleType={target.articleType} templates={ctx.templates} context={targetOf(ctx.templates, ctx.articleType, field)} onOpen={() => onPlace({ kind: "article", id })}>
                {target.name}
              </DocLink>
            ) : (
              // Unreadable target: it exists, but not for this reader.
              <span className="dead-link">{target?.name ?? "???"}</span>
            );
          // A pick's own number stands after it: Fighter 3.
          return field.count && count !== undefined ? (
            <span key={id} className="counted">
              {link} {count}
            </span>
          ) : (
            <span key={id}>{link}</span>
          );
        })}
      </span>
    );
  }

  if (field.kind === "world-date" && typeof value === "number") return <>{formatMoment(world?.world.calendar ?? DEFAULT_CALENDAR, value)}</>;
  if (field.kind === "date-time" && typeof value === "string") return <>{formatOurs(value)}</>;
  const shown = Array.isArray(value) ? value.join(", ") : String(value ?? "");
  // Text may hold links — stored by id, shown by name.
  const resolve = linker(articles, ctx.templates);
  return (
    <>
      {scanLinks(shown).map((p, i) => {
        if (typeof p === "string") return <span key={i}>{p}</span>;
        const target = resolve(p);
        const text = p.text !== p.name ? p.text : (target?.name ?? "???");
        return target && onPlace ? (
          <DocLink key={i} id={target.id} articleType={target.articleType} templates={ctx.templates} onOpen={() => onPlace({ kind: "article", id: target.id })}>
            {text}
          </DocLink>
        ) : (
          <span key={i} className="dead-link">
            {text}
          </span>
        );
      })}
    </>
  );
}

function Input({
  field,
  value,
  articles,
  ctx,
  onValue,
}: {
  field: TemplateField;
  value: unknown;
  articles: ArticleView[];
  ctx: Context;
  onValue: (key: string, value: unknown) => void;
}) {
  const set = (w: unknown) => onValue(field.key, w);
  const world = useWorldData();

  switch (field.kind) {
    case "reference": {
      // What points here is set where it points from.
      if (field.from) return <span className="muted">—</span>;
      const targets = choices(field, articles, ctx);
      if (field.multiple) {
        const chosen = picksOf(value);
        // Stored as ids — with its own number where a pick has one.
        const store = (picks: Pick[]) => set(picks.map((p) => (p.count === undefined ? p.id : { with: p.id, count: p.count })));
        return (
          <MultiPick
            chosen={chosen.map((p) => ({ key: p.id, name: articles.find((a) => a.id === p.id)?.name ?? "???", count: p.count === undefined ? "" : String(p.count) }))}
            open={targets.filter((z) => !chosen.some((p) => p.id === z.id))}
            onRemove={(i) => store(chosen.filter((_, j) => j !== i))}
            onAdd={(z) => store([...chosen, { id: z.id }])}
            count={field.count}
            onCount={(i, n) => {
              const number = Number(n.replace(",", "."));
              store(chosen.map((p, j) => (j !== i ? p : n.trim() === "" || !Number.isFinite(number) ? { id: p.id } : { id: p.id, count: number })));
            }}
          />
        );
      }
      return (
        <select value={String(value ?? "")} onChange={(e) => set(e.target.value || undefined)}>
          <option value="">— none —</option>
          {targets.map((z) => (
            <option key={z.id} value={z.id}>
              {z.name}
            </option>
          ))}
        </select>
      );
    }
    case "relation":
      // Relations are set on the document's page, where both sides are known.
      return <span className="muted">—</span>;
    case "number":
      return (
        <input
          type="number"
          value={String(value ?? "")}
          onChange={(e) => set(e.target.value === "" ? undefined : Number(e.target.value))}
        />
      );
    case "world-date":
      return (
        <WorldMomentPicker
          calendar={world?.world.calendar ?? DEFAULT_CALENDAR}
          value={typeof value === "number" ? value : null}
          onValue={(v) => set(v === null ? undefined : v)}
        />
      );
    case "date-time":
      return <OurMomentInput value={typeof value === "string" ? value : ""} onValue={(v) => set(v || undefined)} />;
    case "text":
      return field.multiline ? (
        <textarea
          className="grows"
          value={String(value ?? "")}
          onChange={(e) => set(e.target.value)}
          rows={3}
        />
      ) : (
        <input value={String(value ?? "")} onChange={(e) => set(e.target.value)} />
      );
    default:
      return <input value={String(value ?? "")} onChange={(e) => set(e.target.value)} />;
  }
}
