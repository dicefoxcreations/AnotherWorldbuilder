import type { GraphEdge } from "../../shared/Relations";
import { GRID, NODE_H, NODE_W, type Point } from "../../shared/GraphLayout";

/** Whether a line has someone at an end this reader cannot open — `buildGraph` names them `unknown:…`. */
export const isVeiled = (e: GraphEdge) => e.a.startsWith("unknown:") || e.b.startsWith("unknown:");

/** The id of the relation between two people — as `buildGraph` names it. */
export const pairId = (x: string, y: string) => (x < y ? `${x}|${y}` : `${y}|${x}`);

export type Route = {
  d: string;
  /** Points along the line, start to end — for labels and dragging. */
  samples: Point[];
};

/** `bottom 30` — the side, and how far along it, 0 to 1. A share written wrong is the middle. */
export function readSide(text: string): { side: string; along: number } {
  const [side, along] = text.split(" ");
  const share = Number(along) / 100;
  return { side, along: along !== undefined && Number.isFinite(share) ? Math.min(1, Math.max(0, share)) : 0.5 };
}

/** Where a line leaves a card: a side, and how far along it. */
export function anchor(box: Point, text: string): Point {
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

export const normal = (text: string): Point => {
  const side = readSide(text).side;
  return side === "top" ? { x: 0, y: -1 } : side === "bottom" ? { x: 0, y: 1 } : side === "left" ? { x: -1, y: 0 } : { x: 1, y: 0 };
};

/**
 * The direction out of a line at a point: at right angles to the line there,
 * towards `to` — along x or y. Where the line runs the way `to` lies, along it.
 */
export function across(samples: Point[], at: Point, to: Point): Point {
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

export function bezier(p0: Point, p1: Point, p2: Point, p3: Point): Route {
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
export function bestSide(from: Point, to: Point): string {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  if (Math.abs(dy) < NODE_H) return dx > 0 ? "right" : "left";
  return dy > 0 ? "bottom" : "top";
}

/**
 * A line from a point leaving in a direction to another, arriving in a
 * direction, in a shape.
 */
export function between(p: Point, np: Point, q: Point, nq: Point, shape: string): Route {
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
export function route(a: Point, b: Point, sides: [string, string], shape: string): Route {
  const [sa, sb] = [sides[0] || bestSide(a, b), sides[1] || bestSide(b, a)];
  return between(anchor(a, sa), normal(sa), anchor(b, sb), normal(sb), shape);
}

/** The point a share of the way along a line. */
export function along(samples: Point[], share: number): Point {
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
export function shareAt(samples: Point[], p: Point): number {
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
export const ALIGN = 8;

/** The point on a line at a given x — or y — nearest to `near`; none where the line does not get there. */
export function crossing(samples: Point[], axis: "x" | "y", v: number, near: Point): Point | null {
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
export function dockAt(card: Point, p: Point): string {
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
export function docks(card: Point): Point[] {
  const out: Point[] = [];
  for (let i = 0; i <= STEPS.long; i++)
    out.push({ x: card.x + (NODE_W * i) / STEPS.long, y: card.y }, { x: card.x + (NODE_W * i) / STEPS.long, y: card.y + NODE_H });
  for (let i = 1; i < STEPS.short; i++)
    out.push({ x: card.x, y: card.y + (NODE_H * i) / STEPS.short }, { x: card.x + NODE_W, y: card.y + (NODE_H * i) / STEPS.short });
  return out;
}
