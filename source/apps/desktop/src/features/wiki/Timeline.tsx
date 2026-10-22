import { useContext, useState } from "react";
import type { ArticleView, Node, Template } from "../../api/client";
import { formatMoment, splitMoment, toYmd, yearText, moonsAt } from "../../shared/Calendar";
import { shownName } from "../../shared/editor/FlowText";
import { linker, linkIn } from "../../shared/Links";
import { documentOrder, findType, isA, isProperties, propertyValues, typeOf } from "../../shared/types";
import { MoonsOf } from "../../shared/ui/MoonPhase";
import { WorldImage } from "../../shared/ui/WorldImage";
import { CurrentDocument, useWorldData, type WorldData } from "../worlds/WorldData";

type TimelineNode = Extract<Node, { kind: "timeline" }>;

/** One thing on the timeline: a document, at one of its dates. */
export type Entry = { article: ArticleView; type: Template | undefined; at: number; until: number | null; label: string; image: string | null; excerpt: string };

/**
 * A colour of its own for each type — by its place among the world's types,
 * a golden angle apart, so neighbours never look alike. The same in every
 * timeline.
 */
export const hueOf = (all: Template[], id: string) => {
  const i = all.findIndex((t) => t.id === id);
  return i < 0 ? 200 : Math.round((172 + i * 137.508) % 360);
};

/**
 * The first words of a document, without the marks one writes — its links
 * by the names they show. From the first of the parts named in `from` it
 * has, by title, or a text property so named (a kill's *How*); where it has
 * none of them, or none are named, from its first text; `none`: nothing.
 */
function excerpt(article: ArticleView, type: Template | undefined, resolve: ReturnType<typeof linker>, from: string[]): string {
  const parts = article.layers.filter((l) => !isProperties(l.section) && l.text.trim());
  const wanted = from.map((f) => f.toLowerCase());
  if (wanted.includes("none")) return "";
  const values = propertyValues(article);
  const named = wanted
    .map((w) => {
      const layer = parts.find((l) => l.title.toLowerCase() === w);
      if (layer) return layer.text;
      const field = type?.properties.find((f) => f.label.toLowerCase() === w && f.kind === "text");
      const value = field ? values[field.key] : undefined;
      return typeof value === "string" && value.trim() ? value : undefined;
    })
    .find((t) => t !== undefined);
  const text = named ?? parts[0]?.text ?? "";
  const plain = text
    .split("\n")
    .filter((l) => !/^\s*</.test(l))
    .join(" ")
    .replace(/@@(?:(?!@@).)+@@/g, (m) => {
      const link = linkIn(m.slice(2, -2));
      return link ? (link.text !== link.name ? link.text : (resolve(link)?.name ?? shownName(m))) : m;
    })
    .replace(/%%[^%]+%%/g, "")
    .replace(/[*_`#>]+/g, "")
    .replace(/^\s*[-+]\s/g, "")
    .replace(/\s+/g, " ")
    .trim();
  return plain.length > 170 ? `${plain.slice(0, 168).replace(/\s\S*$/, "")} …` : plain;
}

/**
 * Whether a document picks another in one of its properties — a session its
 * players, an event who took part — or is it. Relations do not count: that
 * someone is a friend does not put their birth into one's life.
 */
function names(article: ArticleView, id: string): boolean {
  if (article.id === id) return true;
  return article.layers.some(
    (l) => isProperties(l.section) && Object.values(l.fields ?? {}).some((v) =>
        // A pick, also one with its own number — not a relation, which is no pick.
        Array.isArray(v) ? v.some((x) => x === id || (!!x && typeof x === "object" && "count" in x && (x as { with?: unknown }).with === id)) : v === id,
      ),
  );
}

/** What a timeline or a calendar asks for. */
export type Dated = { of: string[]; show: string[]; around: string; until: string; text: string[] };

/** The documents one picks in its properties, in their order. */
function pickedIds(article: ArticleView): string[] {
  return Object.values(propertyValues(article)).flatMap((v) =>
    (Array.isArray(v) ? v : [v]).flatMap((x) =>
      typeof x === "string" && x.startsWith("art:") ? [x] : x && typeof x === "object" && typeof (x as { with?: unknown }).with === "string" ? [(x as { with: string }).with] : [],
    ),
  );
}

/**
 * Every date of the documents asked for, oldest first: what a timeline
 * draws and a calendar puts on its days. A string where the question names
 * what is not there.
 *
 * A document without a date of its own stands on the first date of what it
 * picks — a kill on its session's. On one day the first type named leads:
 * what picks one of its documents stands right after it, in the order the
 * types are named — a session, its events, its kills.
 */
export function datedEntries(data: WorldData, current: string | null, node: Dated): Entry[] | string {
  const { templates, articles } = data;
  const wanted = node.of.map((n) => templates.find((t) => t.id === n || t.name.toLowerCase() === n.toLowerCase()));
  if (wanted.some((t) => !t)) return `There is no type “${node.of[wanted.findIndex((t) => !t)]}”.`;
  const around = node.around === "self" ? current : node.around ? articles.find((a) => a.id === node.around || a.name === node.around)?.id : null;
  if (node.around && !around) return `“${node.around}” is not here.`;
  const show = node.show.map((s) => s.toLowerCase());
  // The ends of spans, by comma — one for each type that has one.
  const until = node.until.split(",").map((u) => u.trim().toLowerCase()).filter(Boolean);
  const resolve = linker(articles, templates);
  const byId = new Map(articles.map((a) => [a.id, a]));
  const orders = new Map<string, (a: ArticleView, b: ArticleView) => number>();
  const orderOf = (type: string) => {
    if (!orders.has(type)) orders.set(type, documentOrder(templates, type, articles));
    return orders.get(type)!;
  };
  const isEnd = (f: { label: string; key: string }) => until.includes(f.label.toLowerCase()) || until.includes(f.key.toLowerCase());
  const datesOf = (a: ArticleView) => (typeOf(templates, a.articleType)?.properties ?? []).filter((f) => f.kind === "world-date" && !isEnd(f));
  // The first date of the first document it picks that has one.
  const borrowed = (a: ArticleView): number | null => {
    for (const id of pickedIds(a)) {
      const other = byId.get(id);
      const values = other ? propertyValues(other) : {};
      const at = other ? datesOf(other).map((f) => values[f.key]).find((v) => typeof v === "number") : undefined;
      if (typeof at === "number") return at;
    }
    return null;
  };
  const lead = wanted[0];
  const rank = (a: ArticleView) => {
    const i = wanted.findIndex((t) => isA(templates, a.articleType, t!.id));
    return i < 0 ? wanted.length : i;
  };
  // The document of the leading type an entry belongs to: itself, or the first it picks.
  const anchorOf = (a: ArticleView) =>
    !lead ? undefined : isA(templates, a.articleType, lead.id) ? a : pickedIds(a).map((id) => byId.get(id)).find((o) => o && isA(templates, o.articleType, lead.id));

  return articles
    .filter((a) => a.articleType && (wanted.length === 0 || wanted.some((t) => isA(templates, a.articleType, t!.id))))
    .filter((a) => !around || names(a, around))
    .flatMap((a) => {
      const type = typeOf(templates, a.articleType);
      const values = propertyValues(a);
      // The end of a span is no moment of its own.
      const end = (type?.properties ?? []).find((f) => f.kind === "world-date" && isEnd(f));
      const dates = datesOf(a);
      const picture = (type?.properties ?? []).find((f) => f.kind === "image");
      const image = picture && typeof values[picture.key] === "string" ? (values[picture.key] as string) : null;
      const endAt = end && typeof values[end.key] === "number" ? (values[end.key] as number) : null;
      const entry = (at: number, label: string): Entry => ({ article: a, type: findType(templates, a.articleType), at, until: endAt, label, image, excerpt: excerpt(a, type, resolve, node.text) });
      if (dates.length === 0) {
        const at = borrowed(a);
        return at === null ? [] : [entry(at, "")];
      }
      return dates
        .filter((f) => show.length === 0 || show.includes(f.label.toLowerCase()) || show.includes(f.key))
        .flatMap((f) => (typeof values[f.key] === "number" ? [entry(values[f.key] as number, dates.length > 1 ? f.label : "")] : []));
    })
    .sort((x, y) => {
      if (x.at !== y.at) return x.at - y.at;
      const [p, q] = [anchorOf(x.article), anchorOf(y.article)];
      if (p && q && p !== q) return orderOf(p.articleType)(p, q);
      const d = rank(x.article) - rank(y.article);
      if (d !== 0) return d;
      return x.article.articleType === y.article.articleType ? orderOf(x.article.articleType)(x.article, y.article) : x.article.name.localeCompare(y.article.name);
    });
}

/**
 * `<timeline …>` — what happened when, along the world's calendar.
 *
 * Every date a document has is a moment on it: a birth, a death, a session,
 * an event. The years stand on the axis, and each new day under them, each entry beside it — with its
 * date, its type's colour, a picture and its first words; where the world
 * has moons, how they stood. A new era is named where it begins. A click on
 * a type above shows or hides its entries.
 */
export function Timeline({ node }: { node: TimelineNode }) {
  const data = useWorldData();
  const current = useContext(CurrentDocument);
  const [hidden, setHidden] = useState<Set<string>>(new Set());
  if (!data) return <p className="listing-placeholder small">Timeline{node.of.length ? ` of ${node.of.join(", ")}` : ""}</p>;
  const { templates, world, onPlace } = data;
  const calendar = world.calendar;
  const entries = datedEntries(data, current, node);
  if (typeof entries === "string") return <p className="dead-link">{entries}</p>;

  const types = [...new Map(entries.map((e) => [e.type?.id ?? "", e.type])).values()];
  const shown = entries.filter((e) => !hidden.has(e.type?.id ?? ""));
  if (entries.length === 0) return <p className="small muted">Nothing dated here yet.</p>;

  // Eras are named only where the entries reach over more than one.
  const eras = new Set(shown.map((e) => toYmd(calendar, splitMoment(e.at)[0]).index)).size > 1;
  let lastYear: string | null = null;
  let lastEra: number | null = null;
  let lastDay: number | null = null;
  let side = 0;
  return (
    <div className="timeline">
      {types.length > 1 && (
        <div className="timeline-filter">
          {types.map((t) => {
            const id = t?.id ?? "";
            const off = hidden.has(id);
            return (
              <button
                key={id}
                className={`chip${off ? "" : " on"}`}
                style={{ ["--hue" as string]: hueOf(templates, id) }}
                onClick={() => setHidden((h) => (off ? new Set([...h].filter((x) => x !== id)) : new Set([...h, id])))}
              >
                <span className="timeline-swatch" />
                {t?.icon} {t?.name ?? "—"}
              </button>
            );
          })}
        </div>
      )}
      <ol className="timeline-axis">
        {shown.map((e) => {
          const [day] = splitMoment(e.at);
          const ymd = toYmd(calendar, day);
          const yearShown = yearText(ymd.era, ymd.year);
          const era =
            eras && ymd.index !== lastEra ? (
              <li key={`era${e.article.id}${e.at}`} className="timeline-era">
                <span>{ymd.era.name}</span>
              </li>
            ) : null;
          const year =
            yearShown !== lastYear || era ? (
              <li key={`y${e.article.id}${e.at}`} className="timeline-year">
                <span>{yearShown}</span>
              </li>
            ) : null;
          const dayMark =
            day !== lastDay ? (
              <li key={`d${e.article.id}${e.at}`} className="timeline-day">
                <span>
                  {ymd.day} {ymd.era.months[ymd.month - 1]?.name ?? ""}
                </span>
              </li>
            ) : null;
          lastEra = ymd.index;
          lastYear = yearShown;
          lastDay = day;
          const hue = hueOf(templates, e.type?.id ?? "");
          return [
            era,
            year,
            dayMark,
            <li key={`${e.article.id}${e.at}${e.label}`} className={`timeline-entry ${side++ % 2 ? "right" : "left"}`} style={{ ["--hue" as string]: hue }}>
              <span className="timeline-dot" />
              <button className="timeline-card" onClick={() => onPlace({ kind: "article", id: e.article.id })}>
                {e.image && <WorldImage world={world.id} id={e.image} preview className="timeline-image" alt="" />}
                <span className="timeline-text">
                  <span className="timeline-date">
                    {formatMoment(calendar, e.at)}
                    {e.until !== null && <span className="timeline-span"> – {formatMoment(calendar, e.until)}</span>}
                    {e.label && <span className="timeline-label"> · {e.label}</span>}
                    <MoonsOf moons={moonsAt(calendar, day)} day={day} size={11} />
                  </span>
                  <strong>
                    <span className="timeline-icon">{e.type?.icon}</span> {e.article.name}
                  </strong>
                  {e.excerpt && <span className="timeline-excerpt">{e.excerpt}</span>}
                </span>
              </button>
            </li>,
          ];
        })}
      </ol>
    </div>
  );
}
