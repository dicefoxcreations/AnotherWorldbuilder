/**
 * Where the people of a relation graph stand when nobody set them by hand:
 * in rows, by steps from the one in the middle — they on top, whom they name
 * below, and so on. Without a middle, the most related one starts.
 *
 * This is only a first place. Cards are moved by hand and stay where they
 * were put; the graph knows nothing of generations and needs not.
 *
 * Small and synchronous on purpose — a graph has dozens of people, not
 * thousands, and a layout library would be the largest part of the app.
 */

export type Point = { x: number; y: number };

/** The size of a person's card: portrait, name, a line beneath. */
export const NODE_W = 172;
export const NODE_H = 54;
/** People set by hand snap to this grid. */
export const GRID = 20;
export const ROW = 160;
const GAP = 60;

export function stepsLayout(ids: string[], links: [string, string][], centre = ""): Map<string, Point> {
  const known = new Set(ids);
  const neighbours = new Map<string, string[]>();
  for (const [a, b] of links) {
    if (a === b || !known.has(a) || !known.has(b)) continue;
    if (!neighbours.get(a)?.includes(b)) neighbours.set(a, [...(neighbours.get(a) ?? []), b]);
    if (!neighbours.get(b)?.includes(a)) neighbours.set(b, [...(neighbours.get(b) ?? []), a]);
  }
  const mostRelated = (list: string[]) =>
    [...list].sort((a, b) => (neighbours.get(b)?.length ?? 0) - (neighbours.get(a)?.length ?? 0))[0];

  // Steps, breadth first. A group nobody reaches starts from its most
  // related member, on the top row.
  const level = new Map<string, number>();
  for (let start = centre && known.has(centre) ? centre : mostRelated(ids); start; ) {
    level.set(start, 0);
    let queue = [start];
    while (queue.length > 0) {
      const after: string[] = [];
      for (const from of queue)
        for (const n of neighbours.get(from) ?? []) {
          if (level.has(n)) continue;
          level.set(n, level.get(from)! + 1);
          after.push(n);
        }
      queue = after;
    }
    start = mostRelated(ids.filter((id) => !level.has(id)));
  }

  const rows: string[][] = [];
  for (const id of ids) (rows[level.get(id) ?? 0] ??= []).push(id);
  // Within a row: under the middle of whom one is related to above.
  const x = new Map<string, number>();
  rows.forEach((row, r) => {
    if (!row) return;
    const want = (id: string) => {
      const above = (neighbours.get(id) ?? []).filter((n) => (level.get(n) ?? 0) === r - 1 && x.has(n));
      return above.length ? above.reduce((s, n) => s + x.get(n)!, 0) / above.length : Infinity;
    };
    const sorted = [...row].sort((a, b) => want(a) - want(b));
    const width = sorted.length * (NODE_W + GAP) - GAP;
    const wishes = sorted.map(want).filter(Number.isFinite);
    const centreOfWishes = wishes.length ? wishes.reduce((s, v) => s + v, 0) / wishes.length + NODE_W / 2 : width / 2;
    sorted.forEach((id, i) => x.set(id, centreOfWishes - width / 2 + i * (NODE_W + GAP)));
  });

  const at = new Map<string, Point>();
  for (const id of ids) at.set(id, { x: Math.round(x.get(id)! / GRID) * GRID, y: (level.get(id) ?? 0) * ROW });
  return at;
}

/**
 * People where they were set by hand, on the grid — and whoever was not set
 * on a free spot beside a relative who was. Without anyone set by hand the
 * tree stays as laid out.
 */
export function withPlaces(
  auto: Map<string, Point>,
  places: Map<string, Point>,
  related: Map<string, string[]>,
): Map<string, Point> {
  if (places.size === 0) return auto;
  const at = new Map<string, Point>();
  for (const [id, p] of places) if (auto.has(id)) at.set(id, { x: p.x * GRID, y: p.y * GRID });
  const free = (p: Point) =>
    [...at.values()].every(
      (q) => Math.abs(q.x - p.x) >= NODE_W + GRID || Math.abs(q.y - p.y) >= NODE_H + GRID,
    );
  const snap = (v: number) => Math.round(v / GRID) * GRID;
  // Those with a placed relative first: they have somewhere to go.
  const rest = [...auto.keys()]
    .filter((id) => !at.has(id))
    .sort((a, b) => countPlaced(b) - countPlaced(a));
  function countPlaced(id: string) {
    return (related.get(id) ?? []).filter((r) => at.has(r)).length;
  }
  for (const id of rest) {
    const near = (related.get(id) ?? []).map((r) => at.get(r)).find(Boolean);
    const xs = [...at.values()].map((p) => p.x);
    const ys = [...at.values()].map((p) => p.y);
    const base = near ?? { x: xs.length ? Math.min(...xs) : 0, y: ys.length ? Math.max(...ys) + ROW : 0 };
    const steps = [
      { x: NODE_W + 3 * GRID, y: 0 },
      { x: 0, y: ROW },
      { x: -(NODE_W + 3 * GRID), y: 0 },
      { x: 0, y: -ROW },
    ];
    let spot: Point | undefined;
    for (let ring = near ? 1 : 0; !spot && ring < 40; ring++) {
      for (const s of steps) {
        const p = { x: snap(base.x + s.x * Math.max(1, ring)), y: snap(base.y + (near ? s.y : 0)) };
        if (free(p)) {
          spot = p;
          break;
        }
      }
    }
    at.set(id, spot ?? { x: snap(base.x), y: snap(base.y + ROW) });
  }
  return at;
}
