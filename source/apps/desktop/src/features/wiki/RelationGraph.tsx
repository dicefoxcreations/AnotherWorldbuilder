import { useContext, useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import {
  api,
  type ArticleView,
  type GraphLine,
  type GraphNode,
  type GraphPerson,
  type Template,
  type WorldInfo,
} from "../../api/client";
import { CurrentDocument, useWorldData } from "../worlds/WorldData";
import { findType, propertyValues, subtypes, typeOf } from "../../shared/types";
import { imageUrl } from "../../shared/ui/WorldImage";
import {
  buildGraph,
  findRole,
  isRelation,
  relationFields,
  roleChoices,
  type Graph,
  type GraphEdge,
  type RelationEntry,
  type RelationField,
} from "../../shared/Relations";
import { GRID, NODE_H, NODE_W, stepsLayout, withPlaces, type Point } from "../../shared/GraphLayout";
import { ARROWS, COLOURS, SHAPES, SIDES, endPeople, saysSomething } from "../../shared/GraphText";
import { useSideBar } from "../../shared/ui/SideBar";
import { AccessSection } from "./Inspector";
import { defaults, layersFor, slugify, startTexts } from "./Article";
import { nameFrom, nextNumber } from "../templates/NamePattern";

/**
 * Changing a graph — only while editing. `onSet` sets relations, several
 * as one change: what `from` says about `to` in `property` — `[what to is
 * to from, what from is to to]`, or `null` to remove it. `onGraph` writes the graph's own text: where people stand,
 * how cards and lines look, what it shows — or `null`, and it is gone.
 */
export type GraphEdit = {
  /** Entries not saved yet, by document and property — they win over the stored ones. */
  overrides: Map<string, Map<string, RelationEntry[]>>;
  onSet: (changes: RelationChange[]) => Promise<void>;
  onGraph?: (next: GraphNode | null) => void;
};

export type RelationChange = { from: string; to: string; property: string; roles: [string, string] | null };

/** The colours of lines, chosen by the pair of roles where a line names none. */
const AUTO = COLOURS.filter((c) => c !== "grey");

/**
 * Colours for pairs of roles. Each pair starts from a colour of its own —
 * so the same pair looks the same in every graph — and moves on to the next
 * free one where another pair in this graph has it already.
 */
function coloursOfPairs(pairs: string[]): Map<string, string> {
  const out = new Map<string, string>();
  const taken = new Set<string>();
  for (const pair of [...new Set(pairs)].sort()) {
    let h = 0;
    for (const c of pair) h = (h * 31 + c.charCodeAt(0)) >>> 0;
    let i = h % AUTO.length;
    for (let tries = 0; tries < AUTO.length && taken.has(AUTO[i]); tries++) i = (i + 1) % AUTO.length;
    out.set(pair, AUTO[i]);
    taken.add(AUTO[i]);
  }
  return out;
}

/** What a graph shows, where it says nothing: everything. */
const shows = (g: GraphNode, what: string) => g.show.length === 0 || g.show.includes(what);

/** The id of the relation between two people — as `buildGraph` names it. */
const pairId = (x: string, y: string) => (x < y ? `${x}|${y}` : `${y}|${x}`);

/** A line as written, its ends by id: one person, or the two of a line. */
type Written = { key: string; ends: [string[], string[]]; line: GraphLine };

/** How a line's ends are told apart: by who is at them, in any order. */
const keyOf = (ends: string[][]) =>
  ends
    .map((e) => [...e].sort().join("+"))
    .sort()
    .join("|");

/**
 * A line with a line at an end — a child docked onto its parents' line, or
 * the parents' line joined to their sons'. It stands for every relation
 * from someone at one end to someone at the other.
 */
type Connector = Written & { covered: GraphEdge[] };

/**
 * People and the relations between them, on a surface.
 *
 * Every relation is a line between two cards, its roles beside whom they
 * describe. Where people stand is up to whoever edits: cards are put on a
 * grid by hand; whoever was not put anywhere stands in steps from the
 * middle, or beside a relative. A line may also run to or from another
 * line: a child docked onto its parents' line, or the parents' line joined
 * to their children's — every one at one end related to every one at the
 * other, drawn as one.
 *
 * While editing, what is clicked opens in the side bar: a card its card, a
 * line its relations, the free ground the graph.
 *
 * Only what this reader can open takes part. Someone they cannot open is
 * `???`: there is someone, but not who — and two of them are never drawn as
 * one, or the graph would tell what the data hides.
 */
export function RelationGraph({ query, edit }: { query: GraphNode; edit?: GraphEdit }) {
  const data = useWorldData();
  const current = useContext(CurrentDocument);
  if (!data) {
    return <p className="listing-placeholder small">Relations{query.of ? ` of ${query.of}` : ""}</p>;
  }
  return <Drawn query={query} edit={edit} current={current} {...data} />;
}

type Menu = { x: number; y: number; at: Point; person?: string; lines?: { edges: string[]; connector?: string } };
type Chosen = { graph?: true; person?: string; edges?: string[]; connector?: string };

function Drawn({
  query,
  edit,
  current,
  world,
  templates,
  articles,
  onPlace,
  onChanged,
}: {
  query: GraphNode;
  edit?: GraphEdit;
  current: string | null;
  world: WorldInfo;
  templates: Template[];
  articles: ArticleView[];
  onPlace: (o: { kind: "article"; id: string }) => void;
  onChanged: () => void;
}) {
  const [zoom, setZoom] = useState(1);
  const [chosen, setChosen] = useState<Chosen | null>(null);
  const [menu, setMenu] = useState<Menu | null>(null);
  const [error, setError] = useState("");
  const barId = `graph:${useId()}`;
  useSideBar(barId, !!chosen && !!edit, () => setChosen(null));

  const type = query.of
    ? (findType(templates, query.of) ?? templates.find((t) => t.name.toLowerCase() === query.of.toLowerCase()))
    : undefined;
  const byIdOrName = (v: string) => articles.find((a) => a.id === v || a.name.toLowerCase() === v.toLowerCase());
  const around = query.around === "self" ? (current ?? "") : query.around ? (byIdOrName(query.around)?.id ?? "") : "";

  // Who was put on the graph, by id — where, and how their card looks.
  const cards = useMemo(() => {
    const out = new Map<string, GraphPerson>();
    for (const p of query.people ?? []) {
      const a = byIdOrName(p.who);
      if (a) out.set(a.id, p);
    }
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query.people, articles]);
  const places = useMemo(() => new Map([...cards].map(([id, p]) => [id, { x: p.x, y: p.y }])), [cards]);

  // The lines as written, their ends by id.
  const written = useMemo(() => {
    const out: Written[] = [];
    for (const line of query.lines ?? []) {
      const ends = line.between.map((end) => endPeople(end).map((w) => byIdOrName(w)?.id ?? ""));
      if (ends.length !== 2 || ends.some((e) => e.length === 0 || e.some((id) => !id))) continue;
      out.push({ key: keyOf(ends), ends: ends as [string[], string[]], line });
    }
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query.lines, articles]);
  // How single lines between two people are drawn, by their relation.
  const settings = useMemo(() => {
    const out = new Map<string, { line: GraphLine; first: string }>();
    for (const w of written)
      if (w.ends[0].length === 1 && w.ends[1].length === 1) out.set(w.key, { line: w.line, first: w.ends[0][0] });
    return out;
  }, [written]);

  const roleName = (id: string) => articles.find((a) => a.id === id)?.name ?? "???";

  const graph = useMemo(
    () =>
      buildGraph(
        templates,
        articles,
        { of: type?.id ?? "", via: query.via, around, depth: query.depth || 1 },
        edit?.overrides,
        [...places.keys(), ...(around ? [around] : [])],
      ),
    [templates, articles, type?.id, query.via, around, query.depth, edit?.overrides, places],
  );

  // A first place for everyone: steps from the middle — or where set by hand.
  const drawing = useMemo(() => {
    const links = graph.edges.map((e) => [e.a, e.b] as [string, string]);
    const auto = stepsLayout(graph.nodes.map((n) => n.id), links, around);
    const related = new Map<string, string[]>();
    for (const [a, b] of links) {
      related.set(a, [...(related.get(a) ?? []), b]);
      related.set(b, [...(related.get(b) ?? []), a]);
    }
    // Someone this reader cannot open stands where the GM put them: the
    // stored card names them by id, which is all the reader learns.
    const all = new Map(places);
    const taken = new Set<string>();
    for (const n of graph.nodes) {
      if (n.article || !n.hidden || taken.has(n.hidden)) continue;
      const card = (query.people ?? []).find((p) => p.who === n.hidden);
      if (card) {
        all.set(n.id, { x: card.x, y: card.y });
        taken.add(n.hidden);
      }
    }
    return { at: withPlaces(auto, all, related), auto };
  }, [graph, around, places, query.people]);

  // Lines with a line at an end: the relations they stand for.
  const connectors = useMemo(() => {
    const out: Connector[] = [];
    for (const w of written) {
      if (w.ends[0].length === 1 && w.ends[1].length === 1) continue;
      const covered = w.ends[0].flatMap((x) =>
        w.ends[1].flatMap((y) => graph.edges.filter((e) => e.id === pairId(x, y))),
      );
      if (covered.length > 0) out.push({ ...w, covered });
    }
    return out;
  }, [written, graph]);

  const viaWanted = (f: RelationField) =>
    query.via.length === 0 || query.via.some((v) => v === f.key || v.toLowerCase() === f.label.toLowerCase());

  /** Who may be added: the documents of the graph's type, or of the relations' types. */
  const kinds = useMemo(() => {
    const roots = type
      ? [type.id]
      : [...new Set(templates.flatMap((t) => t.properties.filter(isRelation).filter(viaWanted).map((f) => f.articleType)))];
    const all = new Set<string>();
    const walk = (id: string) => {
      if (all.has(id)) return;
      all.add(id);
      subtypes(templates, id).forEach((t) => walk(t.id));
    };
    roots.forEach(walk);
    return [...all].flatMap((id) => findType(templates, id) ?? []);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [templates, type?.id, query.via]);

  // `Entf` removes what is chosen: a line, and the relations it stands for.
  const removeChosen = useRef<() => void>(() => {});
  useEffect(() => {
    if (!edit || !chosen || (!chosen.edges && !chosen.connector)) return;
    const key = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      if (e.key !== "Delete" || ["INPUT", "TEXTAREA", "SELECT"].includes(t.tagName) || t.isContentEditable) return;
      e.preventDefault();
      removeChosen.current();
    };
    document.addEventListener("keydown", key);
    return () => document.removeEventListener("keydown", key);
  }, [edit, chosen]);

  if (query.of && !type) return <p className="dead-link">There is no type “{query.of}”.</p>;
  if (query.around && !around) return <p className="dead-link">“{query.around}” is not here.</p>;

  const pair = (e: GraphEdge) => {
    const [x, y] = [e.atA.map(roleName).join(" / "), e.atB.map(roleName).join(" / ")];
    if (x && x === y) return x;
    return [x, y].filter(Boolean).sort().join(" – ");
  };
  const absorbed = new Set(connectors.flatMap((c) => c.covered.map((e) => e.id)));
  const own = graph.edges.filter((e) => pair(e) && !absorbed.has(e.id));
  const pairColours = coloursOfPairs(own.map(pair));
  /** A line's colour: its own, or its pair's. */
  const colour = (e: GraphEdge) => settings.get(e.id)?.line.color || pairColours.get(pair(e)) || "grey";
  const legend = [...pairColours.entries()].sort();

  const known = (id: string) => articles.some((a) => a.id === id);
  const run = (p: Promise<unknown>) => {
    setError("");
    p.catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)));
  };

  // A new relation goes into the first relation property either of them holds.
  const propertyFor = (from: string, to: string): [string, string, string] | null => {
    for (const [x, y] of [
      [from, to],
      [to, from],
    ]) {
      const a = articles.find((d) => d.id === x);
      const f = a && relationFields(templates, a.articleType).find(viaWanted);
      if (f) return [x, y, f.key];
    }
    return null;
  };
  /** What relates two where they are not yet — nothing where they are. */
  const relating = (x: string, y: string): RelationChange[] => {
    if (graph.edges.some((e) => e.id === pairId(x, y))) return [];
    const target = propertyFor(x, y);
    if (!target) throw new Error("Neither has a relation property here.");
    return [{ from: target[0], to: target[1], property: target[2], roles: ["", ""] }];
  };
  /** Removing relations, as changes. */
  const removing = (edges: GraphEdge[]): RelationChange[] =>
    edges.map((e) => ({ from: e.a, to: e.b, property: e.property, roles: null }));
  /** Setting what each of a relation is to the other. */
  const roling = (list: { edge: GraphEdge; atA: string; atB: string }[]): RelationChange[] =>
    list.map(({ edge, atA, atB }) => ({ from: edge.a, to: edge.b, property: edge.property, roles: [atB, atA] }));
  const connect = (from: string, to: string) => {
    if (from === to) return;
    // The new line, or the one already there, is chosen — its roles open.
    setChosen({ edges: [pairId(from, to)] });
    run((async () => edit!.onSet(relating(from, to)))());
  };

  // --- The graph's own text -------------------------------------------------
  const nameOf = (id: string) => articles.find((a) => a.id === id)?.name ?? "";
  /**
   * Everyone as they stand now, on the grid — by name, as written — with
   * their cards as set, and some changed.
   */
  const peopleWith = (changed: { id: string; name?: string; at?: Point; card?: Partial<GraphPerson> }[], without: string[] = []) => {
    const out = new Map<string, GraphPerson>();
    for (const n of graph.nodes) {
      if (!n.article || without.includes(n.id)) continue;
      const p = drawing.at.get(n.id);
      const had = cards.get(n.id);
      if (p) out.set(n.id, { color: "", sub: "", note: "", ...had, who: n.article.name, x: Math.round(p.x / GRID), y: Math.round(p.y / GRID) });
    }
    for (const c of changed) {
      const before = out.get(c.id) ?? { who: c.name ?? nameOf(c.id), x: 0, y: 0, color: "", sub: "", note: "" };
      const at = c.at ? { x: Math.round(c.at.x / GRID), y: Math.round(c.at.y / GRID) } : {};
      out.set(c.id, { ...before, ...at, ...c.card, who: c.name ?? before.who });
    }
    return [...out.values()];
  };
  const endName = (end: string[]) => end.map(nameOf).join(" & ");
  /** The lines as written — by name — with one replaced or removed. */
  const linesWith = (key: string | null, next: GraphLine | null, without: string[] = []): GraphLine[] => {
    const out: GraphLine[] = [];
    for (const w of written) {
      if (w.key === key || w.ends.some((e) => e.some((x) => without.includes(x)))) continue;
      out.push({ ...w.line, between: w.ends.map(endName) });
    }
    if (next && saysSomething(next)) out.push(next);
    return out;
  };
  const write = (patch: Partial<GraphNode>) => edit?.onGraph?.({ ...query, ...patch });
  const move = (id: string, to: Point) => write({ people: peopleWith([{ id, at: to }]), lines: linesWith(null, null) });
  const setCard = (id: string, card: Partial<GraphPerson>) =>
    write({ people: peopleWith([{ id, card }]), lines: linesWith(null, null) });
  const blank = (between: string[]): GraphLine => ({ between, color: "", sides: [], shape: "", arrow: "", label: "", dock: [], group: [], say: [] });
  /** How the line of `e` is drawn now — in the order it is written. */
  const lineOf = (e: GraphEdge): GraphLine => {
    const s = settings.get(e.id);
    if (s) return { ...s.line, between: [nameOf(s.first), nameOf(s.first === e.a ? e.b : e.a)] };
    return blank([nameOf(e.a), nameOf(e.b)]);
  };
  /** Changes how the line of `e` is drawn. `between` comes by id, when the canvas says which end is which. */
  const setLine = (e: GraphEdge, patch: Partial<GraphLine>) =>
    write({
      people: peopleWith([]),
      lines: linesWith(e.id, { ...lineOf(e), ...patch, ...(patch.between ? { between: patch.between.map(nameOf) } : {}) }),
    });
  /** How a connector is drawn now — by name, as written. */
  const connectorLine = (c: Connector): GraphLine => ({ ...c.line, between: c.ends.map(endName) });
  const setConnector = (c: Connector, patch: Partial<GraphLine>) =>
    write({ people: peopleWith([]), lines: linesWith(c.key, { ...connectorLine(c), ...patch }) });
  /** A connector given up: its relations stay, each drawn on its own. */
  const separate = (c: Connector) => {
    setChosen({ edges: c.covered.map((e) => e.id) });
    write({ people: peopleWith([]), lines: linesWith(c.key, null) });
  };
  /** Removes relations — and whatever line stood for them. */
  const removeRelations = (edges: GraphEdge[]) => {
    if (!edit) return;
    setChosen(null);
    run(edit.onSet(removing(edges)));
  };

  /**
   * A line from one end to the other, where an end is someone or a point on
   * the line between two: every relation from one end to the other comes
   * into being where it is missing, and they are drawn as one line. One
   * person to one person is an ordinary relation.
   */
  const join = (from: { person?: string; line?: GraphEdge; share?: number }, to: { person?: string; line?: GraphEdge; share?: number }) => {
    if (!edit) return;
    const endOf = (x: typeof from) => (x.person ? [x.person] : [x.line!.a, x.line!.b]);
    const [a, b] = [endOf(from), endOf(to)];
    if (a.some((x) => b.includes(x))) return;
    if (a.length === 1 && b.length === 1) return connect(a[0], b[0]);
    const share = (x: typeof from) => (x.line ? Math.round((x.share ?? 0.5) * 100) : 50);
    const dock = [share(from), share(to)].map((n) => (n === 50 ? "" : String(n)));
    const key = keyOf([a, b]);
    run(
      (async () => {
        // The line first: it stands as soon as its relations do.
        const changes = a.flatMap((x) => b.flatMap((y) => relating(x, y)));
        write({ people: peopleWith([]), lines: linesWith(key, { ...blank([endName(a), endName(b)]), dock }) });
        setChosen({ connector: key });
        await edit.onSet(changes);
      })(),
    );
  };

  /** Takes someone out of the tree: their lines to everyone in it, and their place. */
  const remove = (id: string) => {
    if (!edit) return;
    const lines = graph.edges.filter((e) => (e.a === id || e.b === id) && known(e.a) && known(e.b));
    setChosen(null);
    write({ people: peopleWith([], [id]), lines: linesWith(null, null, [id]) });
    run(edit.onSet(removing(lines)));
  };
  /** A free spot for someone new: right of everyone, on the top row. */
  const freeSpot = (): Point => {
    const xs = [...drawing.at.values()].map((p) => p.x);
    const ys = [...drawing.at.values()].map((p) => p.y);
    return xs.length === 0 ? { x: 0, y: 0 } : { x: Math.max(...xs) + NODE_W + 2 * GRID, y: Math.min(...ys) };
  };
  const add = (id: string, at: Point = freeSpot()) => write({ people: peopleWith([{ id, at }]), lines: linesWith(null, null) });
  /** A new document of a type — named as its type says — set where one clicked. */
  const create = (typeId: string, input: string, at: Point = freeSpot()) => {
    const template = typeOf(templates, typeId);
    if (!template) return;
    const pattern = template.namePattern || "%%input%%";
    const count = nextNumber(pattern, articles.filter((a) => a.articleType === typeId).map((a) => a.name));
    const name = nameFrom(pattern, input.trim(), count);
    if (!name) return;
    const visibility = template.auth?.view ?? "all";
    const layers = layersFor(template, defaults(template, articles, templates), startTexts(template), {}, visibility);
    run(
      api.articleCreate(world.id, typeId, slugify(name), name, visibility, layers).then((id) => {
        write({ people: peopleWith([{ id, name, at }]), lines: linesWith(null, null) });
        onChanged();
      }),
    );
  };
  const inGraph = new Set(graph.nodes.map((n) => n.id));
  /** Everyone back in steps from the middle — cards and line colours stay, line ends go back to auto. */
  const relayout = () =>
    write({
      people: [...cards.entries()].flatMap(([id, card]) => {
        const p = drawing.auto.get(id);
        return p ? [{ ...card, who: nameOf(id), x: Math.round(p.x / GRID), y: Math.round(p.y / GRID) }] : [];
      }),
      lines: linesWith(null, null).map((l) => ({ ...l, sides: [] })),
    });

  /** Ends as they stand: the upper first, each left to right. */
  const asTheyStand = (ends: [string[], string[]]): [string[], string[]] => {
    const at = (id: string) => drawing.at.get(id) ?? { x: 0, y: 0 };
    const sorted = ends.map((end) => [...end].sort((p, q) => at(p).x - at(q).x));
    const top = (end: string[]) => Math.min(...end.map((id) => at(id).y));
    return (top(sorted[0]) <= top(sorted[1]) ? [sorted[0], sorted[1]] : [sorted[1], sorted[0]]) as [string[], string[]];
  };
  const empty = graph.nodes.length === 0;
  const person = chosen?.person && known(chosen.person) ? chosen.person : null;
  const connector = connectors.find((c) => c.key === chosen?.connector) ?? null;
  const lines = graph.edges.filter((e) => (chosen?.edges ?? []).includes(e.id) && known(e.a) && known(e.b));
  removeChosen.current = () => removeRelations(connector ? connector.covered : lines);
  /** What someone is to the one in the middle. */
  const roleToMiddle = (id: string) => {
    const e = graph.edges.find((x) => (x.a === around && x.b === id) || (x.b === around && x.a === id));
    return e ? (e.a === id ? e.atA : e.atB).map(roleName).join(" / ") : "";
  };
  /** What stands under a name: as the card says, or the type where the graph shows types. */
  const subtitle = (a: ArticleView) => {
    const card = cards.get(a.id);
    const sub = card?.sub || (shows(query, "type") ? "type" : "none");
    if (sub === "type") return findType(templates, a.articleType)?.name ?? "";
    if (sub === "role") return roleToMiddle(a.id);
    if (sub === "note") return card?.note ?? "";
    return "";
  };
  // The page the graph stands on is a person only when it can hold relations.
  const pageIsPerson = !!current && relationFields(templates, articles.find((a) => a.id === current)?.articleType ?? "").length > 0;
  return (
    // Editing, the graph is chosen like every element in the text: outlined
    // on pointing, its settings in the side bar on a click.
    <div
      className={edit ? `relation-graph surface-card${chosen ? " chosen" : ""}` : "relation-graph"}
      data-label="Graph"
      // Around the drawing too — the legend, the margin — a click chooses the graph.
      onClick={(e) => edit && !(e.target as Element).closest("svg, button, select, input, a") && setChosen({ graph: true })}
    >
      {empty && !edit ? (
        <p className="small muted">No relations yet.</p>
      ) : (
        <Canvas
          at={drawing.at}
          connectors={connectors}
          absorbed={absorbed}
          settings={settings}
          cards={cards}
          zoom={zoom}
          graph={graph}
          centre={around}
          chosen={person}
          chosenEdges={chosen?.edges ?? []}
          chosenConnector={connector?.key ?? null}
          colour={colour}
          pair={pair}
          roleName={roleName}
          subtitle={subtitle}
          portraits={shows(query, "portrait")}
          roles={shows(query, "roles")}
          editing={!!edit}
          onPerson={(id) => (edit ? setChosen(id === person ? null : { person: id }) : onPlace({ kind: "article", id }))}
          onEdges={edit ? (ids) => setChosen({ edges: ids }) : undefined}
          onConnector={edit ? (key) => setChosen({ connector: key }) : undefined}
          onGround={edit ? () => setChosen({ graph: true }) : undefined}
          onJoin={edit ? join : undefined}
          onMove={edit?.onGraph ? move : undefined}
          onLine={edit?.onGraph ? setLine : undefined}
          onConnectorLine={edit?.onGraph ? setConnector : undefined}
          onMenu={edit ? setMenu : undefined}
        />
      )}
      <div className="graph-bar">
        {!empty && (
          <span className="segmented">
            <button className="compact" onClick={() => setZoom((z) => Math.max(0.4, z / 1.25))} title="Smaller">
              −
            </button>
            <button className="compact" onClick={() => setZoom((z) => Math.min(2.5, z * 1.25))} title="Larger">
              +
            </button>
          </span>
        )}
        {legend.length > 0 && (
          <ul className="graph-legend">
            {legend.map(([l, c]) => (
              <li key={l}>
                <span className={`swatch rel-${c}`} />
                {l}
              </li>
            ))}
          </ul>
        )}

      </div>
      {chosen && edit && (
        <SidePanel
          title={chosen.graph ? "Graph" : person ? nameOf(person) : "Relation"}
          error={error}
          onClose={() => setChosen(null)}
        >
          {chosen.graph ? (
            <>
            <AccessSection />
            <GraphSettings
              query={query}
              templates={templates}
              articles={articles}
              kinds={kinds}
              inGraph={inGraph}
              pageIsPerson={pageIsPerson}
              onChange={(patch) => write(patch)}
              onAdd={(id) => add(id)}
              onCreate={(typeId, name) => create(typeId, name)}
              onRelayout={relayout}
              onRemove={edit.onGraph ? () => edit.onGraph?.(null) : undefined}
            />
            <p className="small muted">Right-click to add someone. Drag a card to move it; drag from its dot to someone, or onto a line.</p>
            </>
          ) : person ? (
            <CardSettings
              card={cards.get(person)}
              hasMiddle={!!around && around !== person}
              onCard={edit.onGraph ? (patch) => setCard(person, patch) : undefined}
              onOpen={() => onPlace({ kind: "article", id: person })}
              onRemove={person !== around ? () => remove(person) : undefined}
            />
          ) : connector ? (
            <ConnectorCard
              connector={{ ...connector, ends: asTheyStand(connector.ends) }}
              written={connector.ends}
              line={connectorLine(connector)}
              articles={articles}
              templates={templates}
              onRoles={(list) => run(edit.onSet(roling(list)))}
              onLine={edit.onGraph ? (patch) => setConnector(connector, patch) : undefined}
              onSeparate={() => separate(connector)}
              onRemove={() => removeRelations(connector.covered)}
            />
          ) : (
            <>
              {lines.length === 0 && <p className="small muted">Nothing here yet — drag from a card's dot to someone.</p>}
              {lines.map((e) => (
                <RelationCard
                  key={e.id}
                  edge={e}
                  line={lineOf(e)}
                  articles={articles}
                  templates={templates}
                  onRoles={(atA, atB) => run(edit.onSet(roling([{ edge: e, atA, atB }])))}
                  onLine={edit.onGraph ? (patch) => setLine(e, patch) : undefined}
                  onRemove={() => removeRelations([e])}
                />
              ))}
            </>
          )}
        </SidePanel>
      )}
      {error && !chosen && <p className="graph-error small">{error}</p>}
      {menu && edit && (
        <GraphMenu
          menu={menu}
          kinds={kinds}
          articles={articles}
          inGraph={inGraph}
          centre={around}
          canPlace={!!edit.onGraph}
          onAdd={(id) => add(id, menu.at)}
          onCreate={(typeId, name) => create(typeId, name, menu.at)}
          onCard={(id) => setChosen({ person: id })}
          onRemove={remove}
          onLines={(l) => setChosen(l.connector ? { connector: l.connector } : { edges: l.edges })}
          onRemoveLines={(l) => removeRelations(graph.edges.filter((e) => l.edges.includes(e.id)))}
          onClose={() => setMenu(null)}
        />
      )}
    </div>
  );
}

// --- Lines ------------------------------------------------------------------

type Route = {
  d: string;
  /** Points along the line, start to end — for labels and dragging. */
  samples: Point[];
};

/** `bottom 30` — the side, and how far along it, 0 to 1. */
function readSide(text: string): { side: string; along: number } {
  const [side, along] = text.split(" ");
  return { side, along: along === undefined ? 0.5 : Number(along) / 100 };
}

/** Where a line leaves a card: a side, and how far along it. */
function anchor(box: Point, text: string): Point {
  const { side, along } = readSide(text);
  switch (side) {
    case "top":
      return { x: box.x + NODE_W * along, y: box.y };
    case "bottom":
      return { x: box.x + NODE_W * along, y: box.y + NODE_H };
    case "left":
      return { x: box.x, y: box.y + NODE_H * along };
    default:
      return { x: box.x + NODE_W, y: box.y + NODE_H * along };
  }
}

const normal = (text: string): Point => {
  const side = readSide(text).side;
  return side === "top" ? { x: 0, y: -1 } : side === "bottom" ? { x: 0, y: 1 } : side === "left" ? { x: -1, y: 0 } : { x: 1, y: 0 };
};

/**
 * The direction out of a line at a point: at right angles to the line there,
 * towards `to` — along x or y. Where the line runs the way `to` lies, along it.
 */
function across(samples: Point[], at: Point, to: Point): Point {
  const share = shareAt(samples, at);
  const a = along(samples, Math.max(0, share - 0.02));
  const b = along(samples, Math.min(1, share + 0.02));
  const flat = Math.abs(b.x - a.x) >= Math.abs(b.y - a.y);
  if (flat && to.y !== at.y) return { x: 0, y: to.y > at.y ? 1 : -1 };
  if (!flat && to.x !== at.x) return { x: to.x > at.x ? 1 : -1, y: 0 };
  return toward(at, to);
}

/** The direction from one point towards another — along x or y, whichever is longer. */
const toward = (p: Point, q: Point): Point =>
  Math.abs(q.y - p.y) >= Math.abs(q.x - p.x) ? { x: 0, y: q.y >= p.y ? 1 : -1 } : { x: q.x >= p.x ? 1 : -1, y: 0 };

function bezier(p0: Point, p1: Point, p2: Point, p3: Point): Route {
  const samples: Point[] = [];
  for (let i = 0; i <= 40; i++) {
    const t = i / 40;
    const u = 1 - t;
    samples.push({
      x: u * u * u * p0.x + 3 * u * u * t * p1.x + 3 * u * t * t * p2.x + t * t * t * p3.x,
      y: u * u * u * p0.y + 3 * u * u * t * p1.y + 3 * u * t * t * p2.y + t * t * t * p3.y,
    });
  }
  return { d: `M${p0.x},${p0.y} C${p1.x},${p1.y} ${p2.x},${p2.y} ${p3.x},${p3.y}`, samples };
}

/** The best side of a card towards a point — when a line names none. */
function bestSide(from: Point, to: Point): string {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  if (Math.abs(dy) < NODE_H) return dx > 0 ? "right" : "left";
  return dy > 0 ? "bottom" : "top";
}

/**
 * A line from a point leaving in a direction to another, arriving in a
 * direction, in a shape.
 */
function between(p: Point, np: Point, q: Point, nq: Point, shape: string): Route {
  if (shape === "straight") return { d: `M${p.x},${p.y} L${q.x},${q.y}`, samples: [p, q] };
  if (shape === "corner") {
    // Out a little, then at right angles to meet.
    const out = 2 * GRID;
    const p1 = { x: p.x + np.x * out, y: p.y + np.y * out };
    const q1 = { x: q.x + nq.x * out, y: q.y + nq.y * out };
    const elbow = np.x !== 0 ? { x: q1.x, y: p1.y } : { x: p1.x, y: q1.y };
    const pts = [p, p1, elbow, q1, q];
    return { d: `M${pts.map((v) => `${v.x},${v.y}`).join(" L")}`, samples: pts };
  }
  const reach = Math.max(40, Math.hypot(q.x - p.x, q.y - p.y) * 0.35);
  return bezier(p, { x: p.x + np.x * reach, y: p.y + np.y * reach }, { x: q.x + nq.x * reach, y: q.y + nq.y * reach }, q);
}

/** A line between two cards from chosen points, in a chosen shape. */
function route(a: Point, b: Point, sides: [string, string], shape: string): Route {
  const [sa, sb] = [sides[0] || bestSide(a, b), sides[1] || bestSide(b, a)];
  return between(anchor(a, sa), normal(sa), anchor(b, sb), normal(sb), shape);
}

/** The point a share of the way along a line. */
function along(samples: Point[], share: number): Point {
  const lengths = samples.slice(1).map((s, i) => Math.hypot(s.x - samples[i].x, s.y - samples[i].y));
  let left = lengths.reduce((s, l) => s + l, 0) * Math.min(1, Math.max(0, share));
  for (let i = 0; i < lengths.length; i++) {
    if (left <= lengths[i] || i === lengths.length - 1) {
      const t = lengths[i] ? left / lengths[i] : 0;
      return { x: samples[i].x + (samples[i + 1].x - samples[i].x) * t, y: samples[i].y + (samples[i + 1].y - samples[i].y) * t };
    }
    left -= lengths[i];
  }
  return samples[0];
}

/** How far along a line a point is nearest to it — 0 at the start, 1 at the end. */
function shareAt(samples: Point[], p: Point): number {
  const lengths = samples.slice(1).map((s, i) => Math.hypot(s.x - samples[i].x, s.y - samples[i].y));
  const total = lengths.reduce((s, l) => s + l, 0) || 1;
  let best = { d: Infinity, share: 0 };
  let walked = 0;
  for (let i = 0; i < lengths.length; i++) {
    for (let k = 0; k <= 10; k++) {
      const t = k / 10;
      const x = samples[i].x + (samples[i + 1].x - samples[i].x) * t;
      const y = samples[i].y + (samples[i + 1].y - samples[i].y) * t;
      const d = Math.hypot(x - p.x, y - p.y);
      if (d < best.d) best = { d, share: (walked + lengths[i] * t) / total };
    }
    walked += lengths[i];
  }
  return best.share;
}

/** How near two ends must be to one line — across, or up and down — to be drawn exactly in it. */
const ALIGN = 8;

/** The point on a line at a given x — or y — nearest to `near`; none where the line does not get there. */
function crossing(samples: Point[], axis: "x" | "y", v: number, near: Point): Point | null {
  let best: Point | null = null;
  for (let i = 0; i + 1 < samples.length; i++) {
    const [s, t] = [samples[i], samples[i + 1]];
    if (s[axis] === t[axis] || (s[axis] - v) * (t[axis] - v) > 0) continue;
    const k = (v - s[axis]) / (t[axis] - s[axis]);
    const p = { x: s.x + (t.x - s.x) * k, y: s.y + (t.y - s.y) * k };
    if (!best || Math.hypot(p.x - near.x, p.y - near.y) < Math.hypot(best.x - near.x, best.y - near.y)) best = p;
  }
  return best;
}

/**
 * How finely a side is divided for docking: a tenth along the long sides,
 * a quarter along the short ones — the points stand about as far apart.
 */
const STEPS = { long: 10, short: 4 };

/**
 * The point on a card's edge nearest to the pointer — a side, and how far
 * along it in percent, on the side's own steps: `bottom 30`, `left 25`. The
 * middle is said by the side alone.
 */
function dockAt(card: Point, p: Point): string {
  const clamp = (v: number) => Math.min(1, Math.max(0, v));
  const options = [
    { side: "top", d: Math.abs(p.y - card.y), along: (p.x - card.x) / NODE_W, steps: STEPS.long },
    { side: "bottom", d: Math.abs(p.y - (card.y + NODE_H)), along: (p.x - card.x) / NODE_W, steps: STEPS.long },
    { side: "left", d: Math.abs(p.x - card.x), along: (p.y - card.y) / NODE_H, steps: STEPS.short },
    { side: "right", d: Math.abs(p.x - (card.x + NODE_W)), along: (p.y - card.y) / NODE_H, steps: STEPS.short },
  ].sort((x, y) => x.d - y.d);
  const o = options[0];
  const n = (Math.round(clamp(o.along) * o.steps) * 100) / o.steps;
  return n === 50 ? o.side : `${o.side} ${n}`;
}

/** The points a line can dock at around a card. */
function docks(card: Point): Point[] {
  const out: Point[] = [];
  for (let i = 0; i <= STEPS.long; i++)
    out.push({ x: card.x + (NODE_W * i) / STEPS.long, y: card.y }, { x: card.x + (NODE_W * i) / STEPS.long, y: card.y + NODE_H });
  for (let i = 1; i < STEPS.short; i++)
    out.push({ x: card.x, y: card.y + (NODE_H * i) / STEPS.short }, { x: card.x + NODE_W, y: card.y + (NODE_H * i) / STEPS.short });
  return out;
}

// --- Drawing ----------------------------------------------------------------

/** Where a drag started or ends: someone, or a point on a line. */
type Spot = { person?: string; line?: GraphEdge; share?: number };

type Drag =
  | { kind: "move"; id: string; grip: Point; to: Point; moved: boolean }
  | { kind: "link"; from: Spot; to: Point; over: string | null; line: { edge: GraphEdge; share: number } | null }
  | { kind: "label"; edge: GraphEdge; owner: string; samples: Point[]; share: number; from: number }
  | { kind: "end"; edge: GraphEdge; end: 0 | 1; card: string; to: Point }
  | { kind: "cend"; key: string; end: 0 | 1; card: string; to: Point }
  | { kind: "dock"; key: string; end: 0 | 1; samples: Point[]; share: number }
  | { kind: "ground" };

function Canvas({
  at: laidOut,
  connectors,
  absorbed,
  settings,
  cards,
  zoom,
  graph,
  centre,
  chosen,
  chosenEdges,
  chosenConnector,
  colour,
  pair,
  roleName,
  subtitle,
  portraits,
  roles,
  editing,
  onPerson,
  onEdges,
  onConnector,
  onGround,
  onJoin,
  onMove,
  onLine,
  onConnectorLine,
  onMenu,
}: {
  at: Map<string, Point>;
  connectors: Connector[];
  /** Relations drawn as part of a connector, not on their own. */
  absorbed: Set<string>;
  settings: Map<string, { line: GraphLine; first: string }>;
  cards: Map<string, GraphPerson>;
  zoom: number;
  graph: Graph;
  centre: string;
  chosen: string | null;
  chosenEdges: string[];
  chosenConnector: string | null;
  colour: (e: GraphEdge) => string;
  pair: (e: GraphEdge) => string;
  roleName: (id: string) => string;
  subtitle: (a: ArticleView) => string;
  portraits: boolean;
  /** Whether roles stand at the lines. */
  roles: boolean;
  editing: boolean;
  onPerson: (id: string) => void;
  /** While editing: lines clicked, by id. */
  onEdges?: (ids: string[]) => void;
  onConnector?: (key: string) => void;
  onGround?: () => void;
  /** A line drawn from someone or a line to someone or a line. */
  onJoin?: (from: Spot, to: Spot) => void;
  onMove?: (id: string, to: Point) => void;
  onLine?: (e: GraphEdge, patch: Partial<GraphLine>) => void;
  onConnectorLine?: (c: Connector, patch: Partial<GraphLine>) => void;
  onMenu?: (m: Menu) => void;
}) {
  const svg = useRef<SVGSVGElement>(null);
  const arrow = `arrow-${useId().replace(/:/g, "")}`;
  const grid = `grid-${useId().replace(/:/g, "")}`;
  const [drag, setDrag] = useState<Drag | null>(null);

  // While a card is moved it stands where the pointer is, on the grid.
  const snap = (v: number) => Math.round(v / GRID) * GRID;
  const at = new Map(laidOut);
  if (drag?.kind === "move" && drag.moved) at.set(drag.id, { x: snap(drag.to.x - drag.grip.x), y: snap(drag.to.y - drag.grip.y) });

  const placed = graph.nodes.filter((n) => at.has(n.id));

  // The surface is as large as what is drawn on it — cards, lines, arcs and
  // labels, measured — with a margin around: a narrow one to read, a wider
  // one to work in, with room to right-click. Before the first measuring the
  // cards alone stand in for it.
  const margin = editing ? 2 * GRID + NODE_H : GRID;
  const content = useRef<SVGGElement>(null);
  const xs = placed.map((n) => at.get(n.id)!.x);
  const ys = placed.map((n) => at.get(n.id)!.y);
  const [drawn, setDrawn] = useState(() => ({
    x: Math.min(0, ...xs),
    y: Math.min(0, ...ys) - NODE_H,
    w: Math.max(0, ...xs) - Math.min(0, ...xs) + NODE_W,
    h: Math.max(0, ...ys) - Math.min(0, ...ys) + 3 * NODE_H,
  }));
  useLayoutEffect(() => {
    // Not while something is on the move: the surface would shift under
    // the pointer. It fits again once it is put down.
    if (drag || !content.current || placed.length === 0) return;
    const b = content.current.getBBox();
    const near = (u: number, v: number) => Math.abs(u - v) < 1;
    if (!near(b.x, drawn.x) || !near(b.y, drawn.y) || !near(b.width, drawn.w) || !near(b.height, drawn.h))
      setDrawn({ x: b.x, y: b.y, w: b.width, h: b.height });
  });
  // While editing there is a surface to work on even with nobody on it yet.
  const least = editing ? { w: 760, h: 300 } : { w: 0, h: 0 };
  const width = Math.max(least.w, drawn.w + 2 * margin);
  const height = Math.max(least.h, drawn.h + 2 * margin);
  // What is drawn stands in the middle of a surface larger than it.
  const minX = placed.length === 0 ? -width / 2 : drawn.x - (width - drawn.w) / 2;
  const minY = placed.length === 0 ? -height / 2 : drawn.y - (height - drawn.h) / 2;

  const box = (id: string) => at.get(id)!;
  const mid = (id: string) => ({ x: box(id).x + NODE_W / 2, y: box(id).y + NODE_H / 2 });
  const roleAt = (e: GraphEdge, id: string) => (e.a === id ? e.atA : e.atB).map(roleName).join(" / ");
  const known = (id: string) => graph.nodes.some((n) => n.id === id && n.article);
  /** The side of `id`'s end as set — or, while it is dragged, where the pointer is. */
  const sideOf = (e: GraphEdge, id: string): string => {
    if (drag?.kind === "end" && drag.edge.id === e.id && drag.card === id) return dockAt(box(id), drag.to);
    const s = settings.get(e.id);
    if (!s) return "";
    return (s.first === id ? s.line.sides[0] : s.line.sides[1]) ?? "";
  };
  /** Opens the right-click menu of a line. */
  const lineMenu = (lines: NonNullable<Menu["lines"]>) => (e: React.MouseEvent) => {
    if (!onMenu) return;
    e.preventDefault();
    e.stopPropagation();
    onMenu({ x: e.clientX, y: e.clientY, at: { x: 0, y: 0 }, lines });
  };

  // --- Lines between two people: as set, or by default an arc over one row
  // and a curve between rows. Each role next to whom it describes; one word
  // in the middle when both say the same.
  const drawnApart = graph.edges.filter((e) => at.has(e.a) && at.has(e.b) && !absorbed.has(e.id));
  /** A line from `a` to `b` — the path runs that way, which is what its arrowheads go by. */
  const routeOf = (e: GraphEdge): { route: Route; a: string; b: string } => {
    const s = settings.get(e.id);
    // In the order the line is written: its sides go with its people.
    const [a, b] = s ? [s.first, s.first === e.a ? e.b : e.a] : box(e.a).x <= box(e.b).x ? [e.a, e.b] : [e.b, e.a];
    const sides: [string, string] = [sideOf(e, a), sideOf(e, b)];
    const shape = s?.line.shape ?? "";
    if (sides[0] || sides[1] || shape) return { route: route(box(a), box(b), sides, shape), a, b };
    const [pa, pb] = [box(a), box(b)];
    if (Math.abs(pa.y - pb.y) < NODE_H) {
      // On one row: an arc over it, off the middle of each card.
      const aLeft = pa.x <= pb.x;
      const xa = pa.x + NODE_W * (aLeft ? 0.72 : 0.28);
      const xb = pb.x + NODE_W * (aLeft ? 0.28 : 0.72);
      const lift = 40 + Math.min(60, Math.abs(xb - xa) * 0.08);
      return { route: bezier({ x: xa, y: pa.y }, { x: xa, y: pa.y - lift }, { x: xb, y: pb.y - lift }, { x: xb, y: pb.y }), a, b };
    }
    return { route: route(pa, pb, [pa.y < pb.y ? "bottom" : "top", pa.y < pb.y ? "top" : "bottom"], ""), a, b };
  };
  /** The line between two drawn on its own — or, without one, a stroke standing in for it. */
  const lineBetween = (x: string, y: string): { samples: Point[]; standIn: boolean } => {
    const e = drawnApart.find((f) => f.id === pairId(x, y));
    if (e) return { samples: routeOf(e).route.samples, standIn: false };
    const [p, q] = [box(x), box(y)];
    return { samples: [anchor(p, bestSide(p, q)), anchor(q, bestSide(q, p))], standIn: true };
  };
  const pill = (p: Point, text: string, key?: string) => (roles && text ? <Pill key={key} at={p} text={text} /> : null);
  // --- Connectors: a line whose end is someone, or a point on the line
  // between two. Every relation from one end to the other, drawn as one.
  const connectorEnds = (c: Connector) => {
    const centreOf = (end: string[]) => {
      const ps = end.map(mid);
      return { x: ps.reduce((s, p) => s + p.x, 0) / ps.length, y: ps.reduce((s, p) => s + p.y, 0) / ps.length };
    };
    const ends = c.ends.map((end, i) => {
      const other = centreOf(c.ends[1 - i]);
      if (end.length === 1) {
        const live = drag?.kind === "cend" && drag.key === c.key && drag.end === i ? dockAt(box(end[0]), drag.to) : "";
        const side = live || c.line.sides[i] || bestSide(box(end[0]), other);
        return { point: anchor(box(end[0]), side), out: normal(side), samples: null as Point[] | null, standIn: false };
      }
      const { samples, standIn } = lineBetween(end[0], end[1]);
      const live = drag?.kind === "dock" && drag.key === c.key && drag.end === i ? drag.share : null;
      const share = live ?? (c.line.dock[i] ? Number(c.line.dock[i]) / 100 : 0.5);
      return { point: along(samples, share), out: { x: 0, y: 0 }, samples, standIn };
    });
    // Nearly in line is meant to be in line: a point on a line slides along
    // it to stand straight above, below or beside the other end. Always the
    // second end first, so it looks the same while dragged and once put down.
    for (const i of [1, 0]) {
      const [me, other] = [ends[i], ends[1 - i]];
      if (!me.samples) continue;
      const p =
        Math.abs(me.point.x - other.point.x) < ALIGN
          ? crossing(me.samples, "x", other.point.x, me.point)
          : Math.abs(me.point.y - other.point.y) < ALIGN
            ? crossing(me.samples, "y", other.point.y, me.point)
            : null;
      if (p) {
        me.point = p;
        break;
      }
    }
    // From a line, a line leaves at right angles to it — a child drops from
    // its parents' line — on the side where the other end is.
    for (const [i, end] of ends.entries()) if (end.samples) end.out = across(end.samples, end.point, ends[1 - i].point);
    return ends;
  };
  const placedConnectors = connectors.filter((c) => c.ends.every((end) => end.every((id) => at.has(id))));
  const joined = placedConnectors.map((c) => {
    const [p, q] = connectorEnds(c);
    const r = between(p.point, p.out, q.point, q.out, c.line.shape);
    const ids = c.covered.map((e) => e.id);
    const on = chosenConnector === c.key;
    // What those at each end are to those at the other.
    // Left to right, as the people stand: each one's role, the roles once,
    // or a word of one's own — `Parents` for a father and a mother.
    const rolesOf = (end: string[], i: number) => {
      if (c.line.say[i]) return c.line.say[i];
      const each = [...end]
        .sort((x, y) => box(x).x - box(y).x)
        .map((x) => [...new Set(c.covered.filter((e) => e.a === x || e.b === x).map((e) => roleAt(e, x)))].filter(Boolean).join(" / "))
        .filter(Boolean);
      return (c.line.group[i] === "each" ? each : [...new Set(each)]).join(" / ");
    };
    const share = c.line.label && c.line.label !== "none" ? Number(c.line.label) / 100 : 0.2;
    const arrowSet = c.line.arrow;
    return (
      <g
        key={`c-${c.key}`}
        className={`edge joined rel-${c.line.color || "grey"}${on ? " chosen" : ""}`}
        onClick={onConnector ? () => onConnector(c.key) : undefined}
        onContextMenu={lineMenu({ edges: ids, connector: c.key })}
      >
        {[p, q].map((end, i) =>
          end.standIn && end.samples ? (
            <path key={i} d={`M${end.samples[0].x},${end.samples[0].y} L${end.samples[1].x},${end.samples[1].y}`} className="stand-in" />
          ) : null,
        )}
        <path
          d={r.d}
          markerEnd={arrowSet === "end" || arrowSet === "both" ? `url(#${arrow})` : undefined}
          markerStart={arrowSet === "start" || arrowSet === "both" ? `url(#${arrow})` : undefined}
        />
        {onConnector && <path d={r.d} className="hit" />}
        {[p, q].map((end, i) => (end.samples ? <circle key={i} cx={end.point.x} cy={end.point.y} r={4} className="join-point" /> : null))}
        {c.line.label !== "none" && pill(along(r.samples, share), rolesOf(c.ends[0], 0))}
        {c.line.label !== "none" && pill(along(r.samples, 1 - share), rolesOf(c.ends[1], 1))}
      </g>
    );
  });

  const lines = drawnApart.map((e) => {
    const { route: r, a, b } = routeOf(e);
    const s = settings.get(e.id)?.line;
    const [ra, rb] = [roleAt(e, a), roleAt(e, b)];
    const liveLabel = drag?.kind === "label" && drag.edge.id === e.id ? drag : null;
    // How far the roles stand from their people: as set, or a fifth.
    const share = liveLabel ? liveLabel.share : s?.label && s.label !== "none" ? Number(s.label) / 100 : 0.2;
    const pills =
      !roles || s?.label === "none"
        ? []
        : ra && ra === rb
          ? [{ at: along(r.samples, 0.5), text: ra, owner: "" }]
          : [
              { at: along(r.samples, share), text: ra, owner: a },
              { at: along(r.samples, 1 - share), text: rb, owner: b },
            ].filter((p) => p.text);
    const isChosen = chosenEdges.includes(e.id) || (drag?.kind === "link" && drag.line?.edge.id === e.id);
    // Arrowheads only where set: at the second (end), the first (start), both.
    const arrowSet = s?.arrow ?? "";
    const choose = onEdges ? () => onEdges([e.id]) : undefined;
    // The line, and apart from it its labels: those lie over every line,
    // also over one that docks right where a label stands. A click on
    // either chooses the line.
    return {
      line: (
        <g
          key={e.id}
          className={`edge rel-${colour(e)}${isChosen ? " chosen" : ""}`}
          onClick={choose}
          onContextMenu={lineMenu({ edges: [e.id] })}
        >
          <title>{pair(e)}</title>
          <path
            d={r.d}
            markerEnd={arrowSet === "end" || arrowSet === "both" ? `url(#${arrow})` : undefined}
            markerStart={arrowSet === "start" || arrowSet === "both" ? `url(#${arrow})` : undefined}
          />
          {onEdges && <path d={r.d} className="hit" />}
        </g>
      ),
      labels: pills.length > 0 && (
        <g key={e.id} className={`labels rel-${colour(e)}`} onClick={choose} onContextMenu={lineMenu({ edges: [e.id] })}>
        {pills.map((p, i) => (
          <Pill
            key={i}
            at={p.at}
            text={p.text}
            onGrab={
              onLine && p.owner
                ? (ev) => {
                    ev.stopPropagation();
                    svg.current?.setPointerCapture(ev.pointerId);
                    // Measured from whom the role describes.
                    const samples = p.owner === a ? r.samples : [...r.samples].reverse();
                    setDrag({ kind: "label", edge: e, owner: p.owner, samples, share, from: share });
                  }
                : undefined
            }
          />
        ))}
        </g>
      ),
    };
  });

  // --- Handles of what is chosen, over the cards: the ends of a line, to
  // drag along a card's edge or along the line they dock on; and the dot in
  // the middle of a line, to draw a new line from it.
  const handle = (key: string, p: Point, title: string, onDown: (ev: React.PointerEvent) => void, cls = "end-handle") => (
    <circle
      key={key}
      cx={p.x}
      cy={p.y}
      r={7}
      className={cls}
      onPointerDown={(ev) => {
        if (ev.button !== 0) return;
        ev.stopPropagation();
        svg.current?.setPointerCapture(ev.pointerId);
        onDown(ev);
      }}
    >
      <title>{title}</title>
    </circle>
  );
  const handles = editing
    ? [
        ...drawnApart
          .filter((e) => chosenEdges.includes(e.id))
          .flatMap((e) => {
            const { route: r, a, b } = routeOf(e);
            const ends = onLine
              ? [
                  handle(`${e.id}-0`, r.samples[0], "Drag along the card's edge", () =>
                    setDrag({ kind: "end", edge: e, end: 0, card: a, to: r.samples[0] }),
                  ),
                  handle(`${e.id}-1`, r.samples[r.samples.length - 1], "Drag along the card's edge", () =>
                    setDrag({ kind: "end", edge: e, end: 1, card: b, to: r.samples[r.samples.length - 1] }),
                  ),
                ]
              : [];
            const middle = along(r.samples, 0.5);
            return [
              ...ends,
              handle(
                `${e.id}-from`,
                middle,
                "Drag to someone or onto a line to connect",
                () => setDrag({ kind: "link", from: { line: e, share: 0.5 }, to: middle, over: null, line: null }),
                "line-handle",
              ),
            ];
          }),
        ...placedConnectors
          .filter((c) => c.key === chosenConnector && onConnectorLine)
          .flatMap((c) =>
            connectorEnds(c).map((end, i) =>
              end.samples
                ? handle(`${c.key}-dock-${i}`, end.point, "Drag along the line", () =>
                    setDrag({ kind: "dock", key: c.key, end: i as 0 | 1, samples: end.samples!, share: c.line.dock[i] ? Number(c.line.dock[i]) / 100 : 0.5 }),
                  )
                : handle(`${c.key}-end-${i}`, end.point, "Drag along the card's edge", () =>
                    setDrag({ kind: "cend", key: c.key, end: i as 0 | 1, card: c.ends[i][0], to: end.point }),
                  ),
            ),
          ),
      ]
    : [];
  // While an end is dragged along a card: where it can dock.
  const dragCard = drag?.kind === "end" || drag?.kind === "cend" ? drag.card : null;
  const guides = dragCard ? docks(box(dragCard)).map((p, i) => <circle key={i} cx={p.x} cy={p.y} r={2.5} className="dock" />) : null;

  /** Where the pointer is, in the drawing's coordinates. */
  const pointer = (e: { clientX: number; clientY: number }): Point => {
    const m = svg.current?.getScreenCTM();
    if (!m) return { x: 0, y: 0 };
    const p = new DOMPoint(e.clientX, e.clientY).matrixTransform(m.inverse());
    return { x: p.x, y: p.y };
  };
  const personAt = (p: Point) =>
    placed.find((n) => {
      const q = box(n.id);
      return n.article && p.x >= q.x && p.x <= q.x + NODE_W && p.y >= q.y && p.y <= q.y + NODE_H;
    })?.id ?? null;
  /** The line under the pointer, and how far along it — not one that touches `from`. */
  const lineAt = (p: Point, from: Spot): { edge: GraphEdge; share: number } | null => {
    if (!onJoin) return null;
    const touching = from.person ? [from.person] : from.line ? [from.line.a, from.line.b] : [];
    let best: { edge: GraphEdge; share: number; d: number } | null = null;
    for (const e of drawnApart) {
      if (touching.includes(e.a) || touching.includes(e.b) || !known(e.a) || !known(e.b)) continue;
      const { route: r } = routeOf(e);
      const share = shareAt(r.samples, p);
      const q = along(r.samples, share);
      const d = Math.hypot(q.x - p.x, q.y - p.y);
      if (d < 12 && (!best || d < best.d)) best = { edge: e, share: Math.min(0.95, Math.max(0.05, share)), d };
    }
    return best && { edge: best.edge, share: best.share };
  };
  const startOf = (from: Spot): Point =>
    from.person ? mid(from.person) : along(routeOf(from.line!).route.samples, from.share ?? 0.5);

  return (
    <div className="graph-scroll">
      <svg
        ref={svg}
        // At its own size it fits the column; zoomed, it is as large as asked and scrolls.
        className={`graph${editing ? " editing" : ""}${zoom === 1 ? " fit" : ""}`}
        width={width * zoom}
        height={height * zoom}
        viewBox={`${minX} ${minY} ${width} ${height}`}
        onPointerDown={(e) => {
          // On free ground: a click there opens the graph's settings.
          if (editing && e.button === 0 && (e.target === svg.current || (e.target as Element).classList?.contains("ground")))
            setDrag({ kind: "ground" });
        }}
        onPointerMove={(e) => {
          if (!drag) return;
          const p = pointer(e);
          if (drag.kind === "link") {
            const over = personAt(p);
            const touching = drag.from.person ? over === drag.from.person : over === drag.from.line?.a || over === drag.from.line?.b;
            setDrag({ ...drag, to: p, over: touching ? null : over, line: over ? null : lineAt(p, drag.from) });
          } else if (drag.kind === "dock") {
            setDrag({ ...drag, share: Math.min(0.98, Math.max(0.02, Math.round(shareAt(drag.samples, p) * 100) / 100)) });
          } else if (drag.kind === "move") {
            const start = { x: box(drag.id).x + drag.grip.x, y: box(drag.id).y + drag.grip.y };
            setDrag({ ...drag, to: p, moved: drag.moved || Math.hypot(p.x - start.x, p.y - start.y) > 6 });
          } else if (drag.kind === "label") {
            setDrag({ ...drag, share: Math.min(0.5, Math.max(0.05, Math.round(shareAt(drag.samples, p) * 20) / 20)) });
          } else if (drag.kind === "end" || drag.kind === "cend") setDrag({ ...drag, to: p });
        }}
        onPointerUp={(e) => {
          if (!drag) return;
          const p = pointer(e);
          setDrag(null);
          if (drag.kind === "ground") onGround?.();
          else if (drag.kind === "link") {
            const target = personAt(p);
            const to: Spot | null = target ? { person: target } : drag.line ? { line: drag.line.edge, share: drag.line.share } : null;
            if (to && !(drag.from.person && drag.from.person === to.person)) onJoin?.(drag.from, to);
            else if (!to && drag.from.line && Math.hypot(p.x - startOf(drag.from).x, p.y - startOf(drag.from).y) < 6)
              onEdges?.([drag.from.line.id]);
          } else if (drag.kind === "dock") {
            const c = connectors.find((x) => x.key === drag.key);
            const n = Math.round(drag.share * 100);
            if (c) {
              const dock = [c.line.dock[0] ?? "", c.line.dock[1] ?? ""];
              dock[drag.end] = n === 50 ? "" : String(n);
              onConnectorLine?.(c, { dock });
            }
          } else if (drag.kind === "cend") {
            const c = connectors.find((x) => x.key === drag.key);
            if (c) {
              const sides = [c.line.sides[0] ?? "", c.line.sides[1] ?? ""];
              sides[drag.end] = dockAt(box(drag.card), p);
              onConnectorLine?.(c, { sides });
            }
          } else if (drag.kind === "move") {
            if (!drag.moved) onPerson(drag.id);
            else onMove?.(drag.id, { x: snap(p.x - drag.grip.x), y: snap(p.y - drag.grip.y) });
          } else if (drag.kind === "label") {
            // Not moved: a click on the label, which chooses its line.
            if (drag.share === drag.from) onEdges?.([drag.edge.id]);
            else onLine?.(drag.edge, { label: String(Math.round(drag.share * 100)) });
          } else if (drag.kind === "end") {
            const s = settings.get(drag.edge.id);
            // In the order the line is written, or the dragged end first.
            const other = drag.edge.a === drag.card ? drag.edge.b : drag.edge.a;
            const [a, b] = s ? [s.first, s.first === drag.edge.a ? drag.edge.b : drag.edge.a] : [drag.card, other];
            const sides = [s?.line.sides[0] ?? "", s?.line.sides[1] ?? ""];
            sides[a === drag.card ? 0 : 1] = dockAt(box(drag.card), p);
            onLine?.(drag.edge, { sides, between: [a, b] });
          }
        }}
        onPointerCancel={() => setDrag(null)}
        onContextMenu={(e) => {
          e.preventDefault();
          if (!onMenu) return;
          const p = pointer(e);
          onMenu({ x: e.clientX, y: e.clientY, at: { x: snap(p.x - NODE_W / 2), y: snap(p.y - NODE_H / 2) }, person: personAt(p) ?? undefined });
        }}
      >
        <defs>
          <marker
            id={arrow}
            viewBox="0 0 10 10"
            refX="9"
            refY="5"
            // The same size however thick the line — a chosen line is drawn thicker.
            markerUnits="userSpaceOnUse"
            markerWidth="11"
            markerHeight="11"
            orient="auto-start-reverse"
          >
            <path d="M0,0 L10,5 L0,10 z" className="arrow" />
          </marker>
          <pattern id={grid} width={GRID} height={GRID} patternUnits="userSpaceOnUse">
            <circle cx={1} cy={1} r={1} className="grid-dot" />
          </pattern>
        </defs>
        {editing && <rect x={minX} y={minY} width={width} height={height} fill={`url(#${grid})`} className="ground" />}
        <g ref={content}>
          {lines.map((l) => l.line)}
          {joined}
          {lines.map((l) => l.labels)}
          {placed.map((n) => {
            const p = box(n.id);
            const name = n.article?.name ?? "???";
            const under = n.article ? subtitle(n.article) : "";
            const card = cards.get(n.id);
            const text = portraits ? NODE_H - 2 : 14;
            const cls = [
              "person",
              n.article ? "" : "unknown",
              card?.color ? `framed rel-${card.color}` : "",
              n.id === centre ? "centre" : "",
              n.id === chosen ? "chosen" : "",
              drag?.kind === "link" && drag.over === n.id ? "target" : "",
              drag?.kind === "move" && drag.id === n.id && drag.moved ? "moving" : "",
            ].join(" ");
            return (
              <g
                key={n.id}
                className={cls}
                transform={`translate(${p.x},${p.y})`}
                onPointerDown={(e) => {
                  if (!n.article || e.button !== 0) return;
                  e.preventDefault();
                  const q = pointer(e);
                  if (!editing) return;
                  // Followed beyond the surface's edge: it grows once the card is down.
                  svg.current?.setPointerCapture(e.pointerId);
                  setDrag({ kind: "move", id: n.id, grip: { x: q.x - p.x, y: q.y - p.y }, to: q, moved: false });
                }}
                onClick={() => !editing && n.article && onPerson(n.id)}
              >
                <title>{under ? `${name} — ${under}` : name}</title>
                <rect width={NODE_W} height={NODE_H} rx={10} />
                {/* The portrait's place: the picture once a character has one,
                    until then the initials. */}
                {portraits && <Portrait article={n.article} name={name} />}
                <text x={text} y={under ? NODE_H / 2 - 8 : NODE_H / 2} className="name">
                  {short(name, portraits ? 15 : 19)}
                </text>
                {under && (
                  <text x={text} y={NODE_H / 2 + 10} className="subtitle">
                    {short(under, portraits ? 20 : 24)}
                  </text>
                )}
                {/* While editing: the dot to draw a relation from. */}
                {editing && n.article && onJoin && (
                  <circle
                    cx={NODE_W}
                    cy={NODE_H / 2}
                    r={7}
                    className="handle"
                    onPointerDown={(e) => {
                      if (e.button !== 0) return;
                      e.stopPropagation();
                      e.preventDefault();
                      svg.current?.setPointerCapture(e.pointerId);
                      setDrag({ kind: "link", from: { person: n.id }, to: pointer(e), over: null, line: null });
                    }}
                  >
                    <title>Drag to someone, or onto a line</title>
                  </circle>
                )}
              </g>
            );
          })}
          {guides}
          {handles}
        </g>
        {drag?.kind === "link" && (
          <line x1={startOf(drag.from).x} y1={startOf(drag.from).y} x2={drag.to.x} y2={drag.to.y} className="drag-line" />
        )}
        {drag?.kind === "link" &&
          drag.line &&
          (() => {
            const p = along(routeOf(drag.line.edge).route.samples, drag.line.share);
            return <circle cx={p.x} cy={p.y} r={6} className="join-point target" />;
          })()}
      </svg>
      {editing && placed.length === 0 && <p className="graph-empty small muted">Right-click to add someone.</p>}
    </div>
  );
}

/** A label on a line, on a plate of its own so lines do not run through it. */
function Pill({ at, text, onGrab }: { at: Point; text: string; onGrab?: (e: React.PointerEvent) => void }) {
  const width = text.length * 6.8 + 14;
  return (
    <g className={onGrab ? "pill movable" : "pill"} transform={`translate(${at.x},${at.y})`} onPointerDown={onGrab}>
      {onGrab && <title>Drag along the line</title>}
      <rect x={-width / 2} y={-10} width={width} height={20} rx={10} />
      <text className="role">{text}</text>
    </g>
  );
}

/**
 * A person's picture on their card — the first image field of their type —
 * in a circle; until there is one, their initials.
 */
function Portrait({ article, name }: { article: ArticleView | undefined; name: string }) {
  const data = useWorldData();
  const clip = useId();
  const field = article && data ? typeOf(data.templates, article.articleType)?.properties.find((f) => f.kind === "image") : undefined;
  const value = article && field ? propertyValues(article)[field.key] : undefined;
  const id = typeof value === "string" && value.startsWith("img:") ? value : null;
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    let gone = false;
    setUrl(null);
    if (id && data) imageUrl(data.world.id, id, true).then((u) => !gone && setUrl(u)).catch(() => {});
    return () => {
      gone = true;
    };
  }, [id, data?.world.id]);
  const r = NODE_H / 2 - 9;
  const c = NODE_H / 2;
  return (
    <>
      <circle cx={c} cy={c} r={r} className="portrait" />
      {url ? (
        <>
          <clipPath id={clip}>
            <circle cx={c} cy={c} r={r} />
          </clipPath>
          <image href={url} x={c - r} y={c - r} width={2 * r} height={2 * r} preserveAspectRatio="xMidYMin slice" clipPath={`url(#${clip})`} />
        </>
      ) : (
        <text x={c} y={c} className="initials">
          {initials(name)}
        </text>
      )}
    </>
  );
}

const initials = (name: string) =>
  name === "???"
    ? "?"
    : name
        .split(/\s+/)
        .filter(Boolean)
        .slice(0, 2)
        .map((w) => w[0]!.toUpperCase())
        .join("");

const short = (text: string, max: number) => (text.length > max ? `${text.slice(0, max - 1)}…` : text);

// --- Editing ----------------------------------------------------------------

/**
 * The right-click menu of the surface: on a card, its card and taking it
 * out; on a line, its settings and removing it; on free ground, someone to
 * add — one who exists, or a new one.
 */
function GraphMenu({
  menu,
  kinds,
  articles,
  inGraph,
  centre,
  canPlace,
  onAdd,
  onCreate,
  onCard,
  onRemove,
  onLines,
  onRemoveLines,
  onClose,
}: {
  menu: Menu;
  kinds: Template[];
  articles: ArticleView[];
  inGraph: Set<string>;
  centre: string;
  canPlace: boolean;
  onAdd: (id: string) => void;
  onCreate: (typeId: string, name: string) => void;
  onCard: (id: string) => void;
  onRemove: (id: string) => void;
  onLines: (l: NonNullable<Menu["lines"]>) => void;
  onRemoveLines: (l: NonNullable<Menu["lines"]>) => void;
  onClose: () => void;
}) {
  const [naming, setNaming] = useState<Template | null>(null);
  const [name, setName] = useState("");
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
  const pick = (f: () => void) => () => {
    f();
    onClose();
  };
  return createPortal(
    <div
      className="brick-menu graph-menu"
      style={{ left: Math.min(menu.x, window.innerWidth - 260), top: Math.min(menu.y, window.innerHeight - 340) }}
      onMouseDown={(e) => e.stopPropagation()}
      onContextMenu={(e) => e.preventDefault()}
    >
      {menu.lines ? (
        <div className="brick-group">
          <h5>{menu.lines.edges.length > 1 ? `${menu.lines.edges.length} relations` : "Relation"}</h5>
          <button className="ghost" onClick={pick(() => onLines(menu.lines!))}>
            Settings
          </button>
          <button className="ghost danger" onClick={pick(() => onRemoveLines(menu.lines!))}>
            Remove
          </button>
        </div>
      ) : menu.person ? (
        <div className="brick-group">
          <h5>{articles.find((a) => a.id === menu.person)?.name}</h5>
          <button className="ghost" onClick={pick(() => onCard(menu.person!))}>
            Card
          </button>
          <button
            className="ghost danger"
            disabled={menu.person === centre}
            title="Takes them out, with their relations to everyone in this tree"
            onClick={pick(() => onRemove(menu.person!))}
          >
            Remove from tree
          </button>
        </div>
      ) : naming ? (
        <NewPerson
          type={naming}
          name={name}
          onName={setName}
          onCreate={() => {
            onCreate(naming.id, name);
            onClose();
          }}
        />
      ) : !canPlace ? (
        <div className="brick-group">
          <p className="small muted">Edit the document to add someone here.</p>
        </div>
      ) : (
        kinds.map((t) => {
          const choices = articles
            .filter((a) => a.articleType === t.id && !inGraph.has(a.id))
            .sort((a, b) => a.name.localeCompare(b.name));
          return (
            <div key={t.id} className="brick-group">
              <h5>{t.name}</h5>
              <button className="ghost" onClick={() => setNaming(t)}>
                ＋ New {t.name}
              </button>
              {choices.map((a) => (
                <button key={a.id} className="ghost" onClick={pick(() => onAdd(a.id))}>
                  {a.name}
                </button>
              ))}
            </div>
          );
        })
      )}
    </div>,
    document.body,
  );
}

function NewPerson({
  type,
  name,
  onName,
  onCreate,
}: {
  type: Template;
  name: string;
  onName: (n: string) => void;
  onCreate: () => void;
}) {
  return (
    <form
      className="brick-group new-person"
      onSubmit={(e) => {
        e.preventDefault();
        if (name.trim()) onCreate();
      }}
    >
      <h5>New {type.name}</h5>
      <input autoFocus value={name} onChange={(e) => onName(e.target.value)} placeholder="Name" />
      <button className="primary compact" type="submit" disabled={!name.trim()}>
        Add
      </button>
    </form>
  );
}

/** The side bar — where versions stand — holding what was clicked. */
function SidePanel({
  title,
  error,
  onClose,
  children,
}: {
  title: string;
  error: string;
  onClose: () => void;
  children: React.ReactNode;
}) {
  // Straight into the page's body: the side bar stands at the window's
  // right edge, whatever the graph sits in.
  return createPortal(
    <aside className="side-panel relation-panel">
      <div className="side-panel-head">
        <h3>{title}</h3>
        <button className="ghost icon round" title="Close" onClick={onClose}>
          ×
        </button>
      </div>
      {children}
      {error && <p className="graph-error small">{error}</p>}
    </aside>,
    document.body,
  );
}

/** Colour swatches — `auto` and the named colours. */
function Swatches({ value, onPick, auto }: { value: string; onPick: (c: string) => void; auto: string }) {
  return (
    <div className="swatches" role="group" aria-label="Colour">
      <button className={value ? "swatch-button auto" : "swatch-button auto on"} title={auto} onClick={() => onPick("")}>
        auto
      </button>
      {COLOURS.map((c) => (
        <button key={c} className={`swatch-button rel-${c}${value === c ? " on" : ""}`} title={c} onClick={() => onPick(c)} />
      ))}
    </div>
  );
}

/** One card: its frame, and what stands under the name. */
function CardSettings({
  card,
  hasMiddle,
  onCard,
  onOpen,
  onRemove,
}: {
  card: GraphPerson | undefined;
  hasMiddle: boolean;
  onCard?: (patch: Partial<GraphPerson>) => void;
  onOpen: () => void;
  onRemove?: () => void;
}) {
  const [note, setNote] = useState(card?.note ?? "");
  const sub = card?.sub ?? "";
  return (
    <div className="card-settings">
      {onCard && (
        <>
          <div>
            <h5>Frame</h5>
            <Swatches value={card?.color ?? ""} onPick={(color) => onCard({ color })} auto="As usual" />
          </div>
          <label>
            <span>Under the name</span>
            <select value={sub} onChange={(e) => onCard({ sub: e.target.value })}>
              <option value="">As the graph says</option>
              <option value="type">Type</option>
              {hasMiddle && <option value="role">Role to the middle</option>}
              <option value="note">Own text</option>
              <option value="none">Nothing</option>
            </select>
          </label>
          {sub === "note" && (
            <input
              value={note}
              placeholder="Text under the name"
              onChange={(e) => setNote(e.target.value)}
              onBlur={() => note !== (card?.note ?? "") && onCard({ note })}
              onKeyDown={(e) => e.key === "Enter" && onCard({ note })}
            />
          )}
        </>
      )}
      <button className="compact" onClick={onOpen}>
        Open page
      </button>
      {onRemove && (
        <button className="compact danger" onClick={onRemove} title="Takes them out, with their relations to everyone in this tree">
          Remove from tree
        </button>
      )}
    </div>
  );
}

/** The roles a relation may have: those of its property's role type. */
function rolesFor(templates: Template[], articles: ArticleView[], e: GraphEdge) {
  const field = templates.flatMap((t) => t.properties).find((f): f is RelationField => isRelation(f) && f.key === e.property);
  return field ? roleChoices(templates, articles, field) : [];
}

/** One relation: both roles, and how its line is drawn. */
function RelationCard({
  edge: e,
  line,
  articles,
  templates,
  onRoles,
  onLine,
  onRemove,
}: {
  edge: GraphEdge;
  line: GraphLine;
  articles: ArticleView[];
  templates: Template[];
  onRoles: (atA: string, atB: string) => void;
  onLine?: (patch: Partial<GraphLine>) => void;
  onRemove: () => void;
}) {
  const name = (id: string) => articles.find((a) => a.id === id)?.name ?? "???";
  const roles = rolesFor(templates, articles, e);
  const roleAt = (id: string) => (id === e.a ? e.atA[0] : e.atB[0]) ?? "";
  const set = (id: string, role: string) => (id === e.a ? onRoles(role, e.atB[0] ?? "") : onRoles(e.atA[0] ?? "", role));
  const pick = (id: string) => (
    <label>
      <span>{name(id)} is</span>
      <select value={findRole(roles, roleAt(id))?.id ?? ""} onChange={(x) => set(id, x.target.value)}>
        <option value="">—</option>
        {roles.map((r) => (
          <option key={r.id} value={r.id}>
            {r.name}
          </option>
        ))}
      </select>
    </label>
  );
  return (
    <section className="relation-card">
      <div className="relation-card-head">
        <strong>
          {name(e.a)} – {name(e.b)}
        </strong>
        <button className="bubble-x" title="Remove the relation" onClick={onRemove}>
          ×
        </button>
      </div>
      {pick(e.a)}
      {pick(e.b)}
      {onLine && <LineSettings line={line} sides={[true, true]} onLine={onLine} />}
    </section>
  );
}

/**
 * A line between ends that may be lines, by end: for each person there what
 * they are to those at the other end — one choice sets it towards all of
 * them — and for an end of several, what its label says.
 */
function ConnectorCard({
  connector: c,
  written,
  line,
  articles,
  templates,
  onRoles,
  onLine,
  onSeparate,
  onRemove,
}: {
  connector: Connector;
  /** The ends in the order the line is written — `group` and `say` go by it. */
  written: [string[], string[]];
  line: GraphLine;
  articles: ArticleView[];
  templates: Template[];
  onRoles: (list: { edge: GraphEdge; atA: string; atB: string }[]) => void;
  onLine?: (patch: Partial<GraphLine>) => void;
  onSeparate: () => void;
  onRemove: () => void;
}) {
  const name = (id: string) => articles.find((a) => a.id === id)?.name ?? "???";
  const names = (end: string[]) => end.map(name).join(" & ");
  const roles = c.covered.length ? rolesFor(templates, articles, c.covered[0]) : [];
  const row = (x: string) => {
    const mine = c.covered.filter((e) => e.a === x || e.b === x);
    const now = [...new Set(mine.map((e) => (e.a === x ? e.atA[0] : e.atB[0]) ?? ""))];
    // One choice for all — or `mixed`, where they differ.
    const value = now.length === 1 ? (findRole(roles, now[0])?.id ?? "") : "mixed";
    return (
      <label key={x}>
        <span>{name(x)}</span>
        <select
          value={value}
          onChange={(ev) => {
            const role = ev.target.value;
            if (role === "mixed") return;
            onRoles(
              mine.map((e) => ({
                edge: e,
                atA: e.a === x ? role : (e.atA[0] ?? ""),
                atB: e.b === x ? role : (e.atB[0] ?? ""),
              })),
            );
          }}
        >
          {value === "mixed" && <option value="mixed">(differs)</option>}
          <option value="">—</option>
          {roles.map((r) => (
            <option key={r.id} value={r.id}>
              {r.name}
            </option>
          ))}
        </select>
      </label>
    );
  };
  return (
    <section className="relation-card">
      <div className="relation-card-head">
        <strong>
          {names(c.ends[0])} – {names(c.ends[1])}
        </strong>
        <button className="bubble-x" title="Remove these relations" onClick={onRemove}>
          ×
        </button>
      </div>
      {c.ends.map((end, k) => {
        // Its place as written: the ends here are as they stand.
        const i = written.findIndex((w) => w.length === end.length && w.every((x) => end.includes(x))) as 0 | 1;
        return (
          <div key={end.join("+")} className="connector-end">
            <h5>
              {names(end)} <span className="muted">to {names(c.ends[1 - k])}</span>
            </h5>
            {end.map(row)}
            {onLine && end.length > 1 && <GroupLabel line={line} end={i} onLine={onLine} />}
          </div>
        );
      })}
      {onLine && <LineSettings line={line} sides={[c.ends[0].length === 1, c.ends[1].length === 1]} onLine={onLine} />}
      <button className="compact" onClick={onSeparate} title="Each relation drawn as a line of its own">
        Separate
      </button>
    </section>
  );
}

/**
 * What the label of an end of several says: each one's role — `Son / Son` —
 * the roles once — `Son` — or a word of one's own: `Sons`.
 */
function GroupLabel({ line, end, onLine }: { line: GraphLine; end: 0 | 1; onLine: (patch: Partial<GraphLine>) => void }) {
  const said = line.say[end] ?? "";
  const [own, setOwn] = useState(said !== "");
  const [text, setText] = useState(said);
  const mode = own || said ? "own" : line.group[end] === "each" ? "each" : "";
  const set = (group: string, say: string) => {
    const pair = (list: string[], v: string) => {
      const next = [list[0] ?? "", list[1] ?? ""];
      next[end] = v;
      return next.every((x) => !x) ? [] : next;
    };
    onLine({ group: pair(line.group, group), say: pair(line.say, say.replace(/,/g, " ").trim()) });
  };
  return (
    <div className="group-label">
      <span className="small muted">Label</span>
      <div className="segmented" role="group" aria-label="Label">
        {[
          ["each", "Separate"],
          ["", "Combined"],
          ["own", "Own text"],
        ].map(([v, title]) => (
          <button
            key={v}
            className={mode === v ? "on" : ""}
            onClick={() => {
              setOwn(v === "own");
              if (v !== "own") set(v, "");
            }}
          >
            {title}
          </button>
        ))}
      </div>
      {mode === "own" && (
        <input
          value={text}
          autoFocus={!said}
          onChange={(e) => setText(e.target.value)}
          onBlur={() => text.trim() !== said && set("", text)}
          onKeyDown={(e) => e.key === "Enter" && set("", text)}
        />
      )}
    </div>
  );
}

/** How a line is drawn: colour, shape, arrowheads, where it leaves each card, where its roles stand. */
function LineSettings({
  line,
  sides: canSide,
  onLine,
}: {
  line: GraphLine;
  /** Which ends are cards — only those have a side. */
  sides: [boolean, boolean];
  onLine: (patch: Partial<GraphLine>) => void;
}) {
  const [first, second] = line.between;
  const side = (i: 0 | 1) => {
    const { side: s } = readSide(line.sides[i] || "x");
    return (
      <label>
        <span>{line.between[i]}</span>
        <select
          value={SIDES.includes(s as (typeof SIDES)[number]) ? line.sides[i] : ""}
          onChange={(x) => {
            const sides = [line.sides[0] ?? "", line.sides[1] ?? ""];
            sides[i] = x.target.value;
            onLine({ sides });
          }}
        >
          <option value="">auto</option>
          {SIDES.map((v) => (
            <option key={v} value={v}>
              {v}
            </option>
          ))}
          {line.sides[i] && !SIDES.includes(line.sides[i] as (typeof SIDES)[number]) && <option value={line.sides[i]}>{line.sides[i]} %</option>}
        </select>
      </label>
    );
  };
  const label = line.label === "none" ? "none" : line.label ? "set" : "";
  const arrowNames: Record<(typeof ARROWS)[number], string> = {
    end: `→ ${second}`,
    start: `→ ${first}`,
    both: "both ends",
  };
  return (
    <div className="line-settings">
      <h5>Line</h5>
      <Swatches value={line.color} onPick={(color) => onLine({ color })} auto="By the roles" />
      <div className="segmented" role="group" aria-label="Shape">
        {["", ...SHAPES].map((s) => (
          <button key={s} className={(line.shape ?? "") === s ? "on" : ""} onClick={() => onLine({ shape: s })}>
            {s || "auto"}
          </button>
        ))}
      </div>
      <label>
        <span>Arrowheads</span>
        <select value={line.arrow} onChange={(x) => onLine({ arrow: x.target.value })}>
          <option value="">none</option>
          {ARROWS.map((a) => (
            <option key={a} value={a}>
              {arrowNames[a]}
            </option>
          ))}
        </select>
      </label>
      {(canSide[0] || canSide[1]) && (
        <div className="line-sides">
          {canSide[0] ? side(0) : <span />}
          {canSide[1] ? side(1) : <span />}
        </div>
      )}
      <p className="small muted">Choose the line, then drag its ends. Delete removes it.</p>
      <label>
        <span>Roles</span>
        <select
          value={label}
          onChange={(x) => onLine({ label: x.target.value === "none" ? "none" : x.target.value === "set" ? line.label || "35" : "" })}
        >
          <option value="">beside each end</option>
          <option value="set">further along</option>
          <option value="none">hidden</option>
        </select>
      </label>
      {label === "set" && (
        <input type="range" min={5} max={50} step={5} value={Number(line.label)} onChange={(x) => onLine({ label: x.target.value })} />
      )}
    </div>
  );
}

/**
 * The graph as a whole: someone to add, what cards and lines show, and
 * whether it fills itself.
 */
function GraphSettings({
  query,
  templates,
  articles,
  kinds,
  inGraph,
  pageIsPerson,
  onChange,
  onAdd,
  onCreate,
  onRelayout,
  onRemove,
}: {
  query: GraphNode;
  templates: Template[];
  articles: ArticleView[];
  kinds: Template[];
  inGraph: Set<string>;
  /** Whether the page the graph stands on is a person — then “this page” can be the middle. */
  pageIsPerson: boolean;
  onChange: (patch: Partial<GraphNode>) => void;
  onAdd: (id: string) => void;
  onCreate: (typeId: string, name: string) => void;
  onRelayout: () => void;
  onRemove?: () => void;
}) {
  const [naming, setNaming] = useState<Template | null>(null);
  const [name, setName] = useState("");
  const people = articles.filter((a) => kinds.some((t) => t.id === a.articleType)).sort((a, b) => a.name.localeCompare(b.name));
  const addable = people.filter((a) => !inGraph.has(a.id));
  const relations = [...new Map(templates.flatMap((t) => t.properties.filter(isRelation)).map((f) => [f.label, f])).values()];

  // How it fills itself, in one choice.
  const mode = query.around ? "around" : query.of ? "type" : "";
  const middle =
    query.around === "self" ? "self" : (articles.find((a) => a.id === query.around || a.name === query.around)?.name ?? "");
  const shown = (what: string) => query.show.length === 0 || query.show.includes(what);
  const flip = (what: string) => {
    const all = ["portrait", "type", "roles"];
    const now = all.filter(shown);
    const next = now.includes(what) ? now.filter((w) => w !== what) : [...now, what];
    onChange({ show: next.length === all.length ? [] : next });
  };
  return (
    <div className="graph-settings">
      <section>
        <h5>Add someone</h5>
        {naming ? (
          <NewPerson
            type={naming}
            name={name}
            onName={setName}
            onCreate={() => {
              onCreate(naming.id, name);
              setNaming(null);
              setName("");
            }}
          />
        ) : (
          <select
            value=""
            onChange={(e) => {
              const v = e.target.value;
              if (v.startsWith("new:")) setNaming(kinds.find((k) => k.id === v.slice(4)) ?? null);
              else if (v) onAdd(v);
            }}
          >
            <option value="">＋ Person</option>
            {addable.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
            {kinds.map((t) => (
              <option key={t.id} value={`new:${t.id}`}>
                New {t.name} …
              </option>
            ))}
          </select>
        )}
      </section>

      <section>
        <h5>Show</h5>
        <div className="chips">
          <button className={shown("portrait") ? "chip on" : "chip"} onClick={() => flip("portrait")}>
            Portraits
          </button>
          <button className={shown("type") ? "chip on" : "chip"} onClick={() => flip("type")}>
            Types under names
          </button>
          <button className={shown("roles") ? "chip on" : "chip"} onClick={() => flip("roles")}>
            Roles at lines
          </button>
        </div>
      </section>

      <section>
        <h5>Fill in by itself</h5>
        <select
          value={mode}
          onChange={(e) => {
            const v = e.target.value;
            if (v === "around") onChange({ around: pageIsPerson ? "self" : (people[0]?.name ?? ""), of: "" });
            else if (v === "type") onChange({ around: "", of: kinds[0]?.name ?? "" });
            else onChange({ around: "", of: "" });
          }}
        >
          <option value="">No — only who I add</option>
          <option value="around">Relatives of someone</option>
          <option value="type">Everyone of a type</option>
        </select>
        {mode === "around" && (
          <>
            <label>
              <span>Relatives of</span>
              <select value={middle} onChange={(e) => onChange({ around: e.target.value })}>
                {pageIsPerson && <option value="self">this page</option>}
                {people.map((p) => (
                  <option key={p.id} value={p.name}>
                    {p.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span>How far</span>
              <select value={query.depth || 1} onChange={(e) => onChange({ depth: Number(e.target.value) })}>
                <option value={1}>Direct relations</option>
                <option value={2}>… and theirs</option>
                <option value={3}>… three steps</option>
                <option value={4}>… four steps</option>
                <option value={5}>… five steps</option>
              </select>
            </label>
          </>
        )}
        {mode === "type" && (
          <select value={findType(templates, query.of)?.name ?? query.of} onChange={(e) => onChange({ of: e.target.value })}>
            {kinds.map((t) => (
              <option key={t.id} value={t.name}>
                {t.name}
              </option>
            ))}
          </select>
        )}
        <p className="small muted">Whoever you add stays, either way.</p>
      </section>

      {relations.length > 1 && (
        <section>
          <h5>Relations drawn</h5>
          <div className="chips">
            {relations.map((f) => {
              const on = query.via.length === 0 || query.via.some((v) => v === f.key || v.toLowerCase() === f.label.toLowerCase());
              return (
                <button
                  key={f.key}
                  className={on ? "chip on" : "chip"}
                  onClick={() => {
                    const all = query.via.length === 0 ? relations.map((r) => r.label) : query.via;
                    const next = on ? all.filter((v) => v !== f.key && v.toLowerCase() !== f.label.toLowerCase()) : [...all, f.label];
                    onChange({ via: next.length === relations.length ? [] : next });
                  }}
                >
                  {f.label}
                </button>
              );
            })}
          </div>
        </section>
      )}

      <section className="graph-actions">
        {(query.people.length > 0 || query.lines.length > 0) && (
          <button className="compact" onClick={onRelayout}>
            Lay out anew
          </button>
        )}
        {onRemove && (
          <button className="compact danger" onClick={onRemove}>
            Remove graph
          </button>
        )}
      </section>
    </div>
  );
}
