import { isMobile } from "../../shared/platform";
import { useContext, useEffect, useId, useMemo, useRef, useState } from "react";
import { api, type ArticleView, type GraphLine, type GraphNode, type GraphPerson, type Template, type WorldInfo } from "../../api/client";
import { CurrentDocument, useWorldData } from "../worlds/WorldData";
import { findType, isA, subtypes, typeOf, articleById } from "../../shared/types";
import { buildGraph, isRelation, relationFields, type GraphEdge, type RelationEntry, type RelationField } from "../../shared/Relations";
import { GRID, NODE_W, stepsLayout, withPlaces, type Point } from "../../shared/GraphLayout";
import { COLOURS, endPeople, saysSomething } from "../../shared/GraphText";
import { useSideBar } from "../../shared/ui/SideBar";
import { AccessSection } from "./Inspector";
import { defaults, layersFor, slugify, startTexts } from "./Article";
import { nameFrom, nextNumber } from "../templates/NamePattern";
import { Canvas, zoomed } from "./GraphCanvas";
import { isVeiled, pairId } from "./GraphLines";
import { CardSettings, ConnectorCard, GraphMenu, GraphSettings, RelationCard, SidePanel } from "./GraphEditing";

/**
 * Changing a graph — only while editing. `onSet` sets relations, several
 * as one change: what `from` says about `to` in `property` — `[what to is
 * to from, what from is to to]`, or `null` to remove it; with `was`, the
 * relation of those roles alone (none of them: one more), without, every
 * one between them. `onGraph` writes the graph's own text: where people stand,
 * how cards and lines look, what it shows — or `null`, and it is gone.
 */
export type GraphEdit = {
  /** Entries not saved yet, by document and property — they win over the stored ones. */
  overrides: Map<string, Map<string, RelationEntry[]>>;
  onSet: (changes: RelationChange[]) => Promise<void>;
  onGraph?: (next: GraphNode | null) => void;
};

export type RelationChange = { from: string; to: string; property: string; roles: [string, string] | null; was?: [string, string] };

/** One relation of a line: `[what A is, what B is]`, as `GraphEdge.pairs`. */
export type Pair = [string, string];

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
export type Connector = Written & { covered: GraphEdge[] };

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

export type Menu = { x: number; y: number; at: Point; person?: string; lines?: { edges: string[]; connector?: string } };
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
  // People by id, or by name as written — looked up, not searched for each card and line end.
  const byName = useMemo(() => {
    const out = new Map<string, ArticleView>();
    for (const a of articles) if (!out.has(a.name.toLowerCase())) out.set(a.name.toLowerCase(), a);
    return out;
  }, [articles]);
  const byIdOrName = (v: string) => articleById(articles, v) ?? byName.get(v.toLowerCase());
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

  const roleName = (id: string) => articleById(articles, id)?.name ?? "???";

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

  // Delete removes what is chosen: a line, and the relations it stands for.
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

  // A line to someone this reader cannot open tells nothing more than that
  // there is someone: grey, its roles ???, and no colour in the legend.
  const veiled = (e: GraphEdge) => isVeiled(e);
  const pair = (e: GraphEdge) => {
    if (veiled(e)) return "";
    const [x, y] = [e.atA.map(roleName).join(" / "), e.atB.map(roleName).join(" / ")];
    if (x && x === y) return x;
    return [x, y].filter(Boolean).sort().join(" – ");
  };
  const absorbed = new Set(connectors.flatMap((c) => c.covered.map((e) => e.id)));
  const own = graph.edges.filter((e) => pair(e) && !absorbed.has(e.id));
  const pairColours = coloursOfPairs(own.map(pair));
  /** A line's colour: its own, or its pair's. */
  const colour = (e: GraphEdge) => (veiled(e) ? "grey" : settings.get(e.id)?.line.color || pairColours.get(pair(e)) || "grey");
  const legend = [...pairColours.entries()].sort();

  const known = (id: string) => !!articleById(articles, id);
  const run = (p: Promise<unknown>) => {
    setError("");
    p.catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)));
  };

  // A new relation goes into the first relation property either of them
  // holds for the other's kind: a character's ties to an organization, its
  // relations to another character.
  const propertyFor = (from: string, to: string): [string, string, string] | null => {
    for (const [x, y] of [
      [from, to],
      [to, from],
    ]) {
      const a = articleById(articles, x);
      const b = articleById(articles, y);
      const f = a && b && relationFields(templates, a.articleType).find((f) => viaWanted(f) && isA(templates, b.articleType, f.articleType));
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
  /** One relation of a line removed — the others between them stay. */
  const removingOne = (edge: GraphEdge, was: Pair): RelationChange[] => [
    { from: edge.a, to: edge.b, property: edge.property, roles: null, was: [was[1], was[0]] },
  ];
  /** Setting what each of a relation is to the other — the one that `was` so; none was so: one more. */
  const roling = (list: { edge: GraphEdge; atA: string; atB: string; was: Pair }[]): RelationChange[] =>
    list.map(({ edge, atA, atB, was }) => ({ from: edge.a, to: edge.b, property: edge.property, roles: [atB, atA], was: [was[1], was[0]] }));
  const connect = (from: string, to: string) => {
    if (from === to) return;
    // The new line, or the one already there, is chosen — its roles open.
    setChosen({ edges: [pairId(from, to)] });
    run((async () => edit!.onSet(relating(from, to)))());
  };

  // --- The graph's own text -------------------------------------------------
  const nameOf = (id: string) => articleById(articles, id)?.name ?? "";
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
  const pageIsPerson = !!current && relationFields(templates, articleById(articles, current)?.articleType ?? "").length > 0;
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
          onZoom={(k) => setZoom((z) => zoomed(z, k))}
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
            <button className="compact" onClick={() => setZoom((z) => zoomed(z, 1 / 1.25))} title="Smaller">
              −
            </button>
            <button className="compact" onClick={() => setZoom((z) => zoomed(z, 1.25))} title="Larger">
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
            <p className="small muted">{isMobile ? "Long-press" : "Right-click"} to add someone. Drag a card to move it; drag from its dot to someone, or onto a line.</p>
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
                  onRoles={(atA, atB, was) => run(edit.onSet(roling([{ edge: e, atA, atB, was }])))}
                  onLine={edit.onGraph ? (patch) => setLine(e, patch) : undefined}
                  onRemove={(was) => (was ? run(edit.onSet(removingOne(e, was))) : removeRelations([e]))}
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
          templates={templates}
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
