import { isMobile } from "../../shared/platform";
import { useMiddleDrag, useWheelZoom } from "../../shared/ui/PanZoom";
import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { type ArticleView, type GraphLine, type GraphPerson } from "../../api/client";
import { useWorldData } from "../worlds/WorldData";
import { propertyValues, typeOf } from "../../shared/types";
import { imageUrl } from "../../shared/ui/WorldImage";
import { type Graph, type GraphEdge } from "../../shared/Relations";
import { GRID, NODE_H, NODE_W, type Point } from "../../shared/GraphLayout";
import { ALIGN, type Route, across, along, anchor, bestSide, between, bezier, crossing, dockAt, docks, normal, route, shareAt, isVeiled, pairId } from "./GraphLines";
import { type Connector, type Menu } from "./RelationGraph";

/**
 * Zoom `z` by `k`, within bounds. Passing its own size it stops there, at
 * fitting the column — the wheel's small steps never miss it.
 */
export function zoomed(z: number, k: number): number {
  const next = Math.min(2.5, Math.max(0.4, z * k));
  return (z < 1 && next > 1) || (z > 1 && next < 1) || Math.abs(next - 1) < 1e-6 ? 1 : next;
}

/** Where a drag started or ends: someone, or a point on a line. */
type Spot = { person?: string; line?: GraphEdge; share?: number };

export type Drag =
  | { kind: "move"; id: string; grip: Point; to: Point; moved: boolean }
  | { kind: "link"; from: Spot; to: Point; over: string | null; line: { edge: GraphEdge; share: number } | null }
  | { kind: "label"; edge: GraphEdge; owner: string; samples: Point[]; share: number; from: number }
  | { kind: "end"; edge: GraphEdge; end: 0 | 1; card: string; to: Point }
  | { kind: "cend"; key: string; end: 0 | 1; card: string; to: Point }
  | { kind: "dock"; key: string; end: 0 | 1; samples: Point[]; share: number }
  | { kind: "ground" };

export function Canvas({
  at: laidOut,
  connectors,
  absorbed,
  settings,
  cards,
  zoom,
  onZoom,
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
  /** Zoomed by a factor: ctrl and the wheel. */
  onZoom: (k: number) => void;
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
  const frame = useRef<HTMLDivElement>(null);
  useMiddleDrag(frame);
  useWheelZoom(frame, svg, onZoom);
  const arrow = `arrow-${useId().replace(/[^\w-]/g, "")}`;
  const grid = `grid-${useId().replace(/[^\w-]/g, "")}`;
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
  const roleAt = (e: GraphEdge, id: string) => (isVeiled(e) ? "???" : (e.a === id ? e.atA : e.atB).map(roleName).join(" / "));
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
    <div ref={frame} className="graph-scroll">
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
      {editing && placed.length === 0 && <p className="graph-empty small muted">{isMobile ? "Long-press" : "Right-click"} to add someone.</p>}
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
  // As the canvas's own: React's ids hold signs a `url(#…)` need not take.
  const clip = `portrait-${useId().replace(/[^\w-]/g, "")}`;
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
        .map((w) => [...w][0]!.toUpperCase())
        .join("");

/** A name cut to `max` signs — whole ones, never half an emoji. */
export const short = (text: string, max: number) => {
  const signs = [...text];
  return signs.length > max ? `${signs.slice(0, max - 1).join("")}…` : text;
};
