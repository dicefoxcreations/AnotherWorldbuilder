import { readImageLine, readTagLine, type ImageAttrs } from "./ImageTag";
import type { GraphLine, GraphNode, LabelAt, MapAccess, MapNode } from "../api/client";
import { tableAt } from "./TableText";

/**
 * A graph as text — the same notation as `aw_model::markdown::Graph`:
 *
 * ```text
 * <graph around=self depth=2>
 * <person who=Tim at=12,6>
 * <line between="Tim, Simon" color=amber sides="bottom, top" shape=curve label=30>
 * </graph>
 * ```
 *
 * The visual editor shows a graph as a surface, not as code. To do that it
 * cuts a part's text into pieces — text, graph, text — and writes a graph
 * back when it changes on the surface.
 */

export const COLOURS = ["teal", "amber", "blue", "rose", "green", "violet", "red", "cyan", "grey"] as const;
/** The markers a pin on a map may be — as `aw_model::markdown::MARKERS`; drawn in `ui/MapIcons`. */
export const MARKERS = [
  "pin", "door", "ladder", "stairs", "chest", "skull", "castle", "tower", "house", "camp",
  "cave", "ship", "anchor", "tree", "mountain", "flag", "star", "sword", "eye", "question",
] as const;
export const SIDES = ["top", "right", "bottom", "left"] as const;
export const SHAPES = ["curve", "straight", "corner"] as const;
export const ARROWS = ["end", "start", "both"] as const;
export const SHOWN = ["portrait", "type", "roles"] as const;
export const SUBS = ["type", "role", "note", "none"] as const;

const is = (list: readonly string[], v: string) => list.includes(v);

/** `bottom` or `bottom 30` — a side, and how far along it; 50 is said by leaving it out. */
export function side(text: string): string {
  const [s, along, more] = text.trim().split(/\s+/);
  if (!s || !is(SIDES, s) || more !== undefined) return "";
  if (along === undefined) return s;
  const n = Number(along);
  if (!/^\d+$/.test(along) || n > 100) return s;
  return n === 50 ? s : `${s} ${n}`;
}

export type Piece =
  | { kind: "text"; text: string }
  | { kind: "graph"; text: string; graph: GraphNode }
  | { kind: "image"; text: string; image: ImageAttrs }
  | { kind: "list"; text: string; list: ImageAttrs }
  | { kind: "timeline"; text: string; timeline: ImageAttrs }
  | { kind: "calendar"; text: string; calendar: ImageAttrs }
  | { kind: "create"; text: string; create: ImageAttrs }
  | { kind: "abilities"; text: string; abilities: ImageAttrs }
  | { kind: "view"; text: string; view: ImageAttrs }
  | { kind: "embed"; text: string; embed: ImageAttrs }
  | { kind: "map"; text: string; map: MapNode }
  | { kind: "table"; text: string };

/** `name=value` and `name="value with spaces"`. */
function attributes(tag: string): Map<string, string> {
  return new Map([...tag.matchAll(/([\w-]+)=(?:"([^"]*)"|([^\s>]+))/g)].map((a) => [a[1], (a[2] ?? a[3]).trim()]));
}

const listed = (value = "") =>
  value
    .split(",")
    .map((v) => v.trim())
    .filter(Boolean);

/** Whether a line opens a graph: `<graph …>` on a line of its own. */
const opens = (line: string) => /^<graph(\s[^<>]*)?>$/.test(line.trim());

export function readGraph(tag: string, inside: string[]): GraphNode {
  const a = attributes(tag);
  const g: GraphNode = {
    of: a.get("type") ?? "",
    via: listed(a.get("via")),
    around: a.get("around") ?? "",
    depth: Math.min(5, Math.max(1, Number(a.get("depth")) || 1)),
    people: [],
    lines: [],
    show: listed(a.get("show")).filter((v) => is(SHOWN, v)),
  };
  for (const raw of inside) {
    const line = raw.trim();
    const name = /^<(\w+)/.exec(line)?.[1]?.toLowerCase();
    const at = attributes(line);
    if (name === "person") {
      const [x, y] = (at.get("at") ?? "").split(",").map((n) => Number(n.trim()));
      const who = (at.get("who") ?? "").trim();
      const color = at.get("color") ?? "";
      const sub = at.get("sub") ?? "";
      if (who && Number.isInteger(x) && Number.isInteger(y))
        g.people.push({
          who,
          x,
          y,
          color: is(COLOURS, color) ? color : "",
          sub: is(SUBS, sub) ? sub : "",
          note: at.get("note") ?? "",
        });
    } else if (name === "line") {
      // Each end someone, or two joined by `&`.
      const between = (at.get("between") ?? "").split(",").map((end) => listed(end.replace(/&/g, ",")).join(" & "));
      if (between.length !== 2 || !between.every((end) => end && end.split(" & ").length <= 2)) continue;
      const dock = (at.get("dock") ?? "").split(",").map((d) => {
        const n = Number(d.trim());
        return /^\d+$/.test(d.trim()) && n <= 100 && n !== 50 ? String(n) : "";
      });
      const color = at.get("color") ?? "";
      const shape = at.get("shape") ?? "";
      const label = at.get("label") ?? "";
      const sides = (at.get("sides") ?? "").split(",").map(side);
      const arrow = at.get("arrow") ?? "";
      const group = (at.get("group") ?? "").split(",").map((v) => (v.trim() === "each" ? "each" : ""));
      const say = (at.get("say") ?? "").split(",").map((v) => v.trim());
      g.lines.push({
        between,
        color: (COLOURS as readonly string[]).includes(color) ? color : "",
        sides: sides.every((s) => !s) ? [] : sides,
        shape: is(SHAPES, shape) ? shape : "",
        arrow: is(ARROWS, arrow) ? arrow : "",
        dock: dock.every((d) => !d) ? [] : dock,
        group: group.every((v) => !v) ? [] : group,
        say: say.every((v) => !v) ? [] : say,
        label: label === "none" || (/^\d+$/.test(label) && Number(label) <= 100) ? label : "",
      });
    }
  }
  return g;
}

/**
 * A part's text cut into text and graphs. Joined again, the pieces give the
 * text exactly as it was.
 */
export function pieces(text: string): Piece[] {
  const lines = text.split(/(?<=\n)/);
  const out: Piece[] = [];
  let plain = "";
  for (let i = 0; i < lines.length; ) {
    // An image is a surface of its own too, a line long.
    const image = readImageLine(lines[i]);
    if (image) {
      if (plain) out.push({ kind: "text", text: plain });
      plain = "";
      out.push({ kind: "image", text: lines[i], image });
      i++;
      continue;
    }
    // A timeline, the same.
    const timeline = readTagLine("timeline", lines[i]);
    if (timeline) {
      if (plain) out.push({ kind: "text", text: plain });
      plain = "";
      out.push({ kind: "timeline", text: lines[i], timeline });
      i++;
      continue;
    }
    // And a calendar.
    const calendar = readTagLine("calendar", lines[i]);
    if (calendar) {
      if (plain) out.push({ kind: "text", text: plain });
      plain = "";
      out.push({ kind: "calendar", text: lines[i], calendar });
      i++;
      continue;
    }
    // A create button.
    const create = readTagLine("create", lines[i]);
    if (create) {
      if (plain) out.push({ kind: "text", text: plain });
      plain = "";
      out.push({ kind: "create", text: lines[i], create });
      i++;
      continue;
    }
    // An ability row.
    const abilities = readTagLine("abilities", lines[i]);
    if (abilities) {
      if (plain) out.push({ kind: "text", text: plain });
      plain = "";
      out.push({ kind: "abilities", text: lines[i], abilities });
      i++;
      continue;
    }
    // A group's layout, drawn.
    const view = readTagLine("view", lines[i]);
    if (view) {
      if (plain) out.push({ kind: "text", text: plain });
      plain = "";
      out.push({ kind: "view", text: lines[i], view });
      i++;
      continue;
    }
    // Another document's text.
    const embed = readTagLine("embed", lines[i]);
    if (embed) {
      if (plain) out.push({ kind: "text", text: plain });
      plain = "";
      out.push({ kind: "embed", text: lines[i], embed });
      i++;
      continue;
    }
    // A list too: what it shows is drawn, not its tag.
    const list = readTagLine("list", lines[i]);
    if (list) {
      if (plain) out.push({ kind: "text", text: plain });
      plain = "";
      out.push({ kind: "list", text: lines[i], list });
      i++;
      continue;
    }
    // A map: from `<map …>` to `</map>`, its pins between.
    if (opensMap(lines[i])) {
      if (plain) out.push({ kind: "text", text: plain });
      plain = "";
      const close = lines.slice(i + 1).findIndex((l) => l.trim().toLowerCase() === "</map>");
      const end = close < 0 ? i + 1 : i + 2 + close;
      const inside = close < 0 ? [] : lines.slice(i + 1, i + 1 + close);
      out.push({ kind: "map", text: lines.slice(i, end).join(""), map: readMap(lines[i], inside) });
      i = end;
      continue;
    }
    // A table: its head, its rule and its rows.
    const rows = tableAt(lines, i);
    if (rows > 0) {
      if (plain) out.push({ kind: "text", text: plain });
      plain = "";
      out.push({ kind: "table", text: lines.slice(i, i + rows).join("") });
      i += rows;
      continue;
    }
    if (!opens(lines[i])) {
      plain += lines[i++];
      continue;
    }
    if (plain) out.push({ kind: "text", text: plain });
    plain = "";
    const close = lines.slice(i + 1).findIndex((l) => l.trim().toLowerCase() === "</graph>");
    const end = close < 0 ? i + 1 : i + 2 + close;
    const inside = close < 0 ? [] : lines.slice(i + 1, i + 1 + close);
    out.push({ kind: "graph", text: lines.slice(i, end).join(""), graph: readGraph(lines[i], inside) });
    i = end;
  }
  if (plain) out.push({ kind: "text", text: plain });
  return out;
}

/** Writes an attribute value — in quotes where needed: Markdown reads a tag only when an unquoted value holds none of `"'=<>` and backtick. */
const value = (v: string) => (v === "" || /[\s"'=<>`]/.test(v) ? `"${v.replace(/"/g, "")}"` : v);

function lineTag(l: GraphLine): string {
  const parts = [`between=${value(l.between.join(", "))}`];
  if (l.color) parts.push(`color=${l.color}`);
  if (l.sides.some(Boolean)) parts.push(`sides=${value(l.sides.join(", "))}`);
  if (l.shape) parts.push(`shape=${l.shape}`);
  if (l.arrow) parts.push(`arrow=${l.arrow}`);
  if (l.dock.some(Boolean)) parts.push(`dock=${value(l.dock.join(", "))}`);
  if (l.group.some(Boolean)) parts.push(`group=${value(l.group.join(", "))}`);
  if (l.say.some(Boolean)) parts.push(`say=${value(l.say.join(", "))}`);
  if (l.label) parts.push(`label=${l.label}`);
  return `<line ${parts.join(" ")}>`;
}

/** Whether a line says anything beyond the usual. */
/** Whether a line is more than between two people, drawn as usual: it says how, or one of its ends is a line. */
export const saysSomething = (l: GraphLine) =>
  !!(l.color || l.sides.some(Boolean) || l.shape || l.arrow || l.label || l.between.some((end) => end.includes("&")));

/** The people of an end: `Anna & Franklin` → both. */
export const endPeople = (end: string) => end.split("&").map((w) => w.trim()).filter(Boolean);

/** A graph as it is written — always ending in a line break. */
export function graphText(g: GraphNode): string {
  const attrs: string[] = [];
  if (g.of) attrs.push(`type=${value(g.of)}`);
  if (g.via.length) attrs.push(`via=${value(g.via.join(", "))}`);
  if (g.around) attrs.push(`around=${value(g.around)}`);
  if (g.depth && g.depth !== 1) attrs.push(`depth=${g.depth}`);
  if (g.show.length) attrs.push(`show=${value(g.show.join(", "))}`);
  const lines = [
    `<graph${attrs.length ? " " + attrs.join(" ") : ""}>`,
    ...g.people.map((p) => {
      const parts = [`who=${value(p.who)}`, `at=${p.x},${p.y}`];
      if (p.color) parts.push(`color=${p.color}`);
      if (p.sub) parts.push(`sub=${p.sub}`);
      if (p.note) parts.push(`note=${value(p.note)}`);
      return `<person ${parts.join(" ")}>`;
    }),
    ...g.lines.filter(saysSomething).map(lineTag),
    "</graph>",
  ];
  return lines.join("\n") + "\n";
}

/** Whether a line opens a map: `<map …>` on a line of its own. */
const opensMap = (line: string) => /^<map(\s[^<>]*)?>$/.test(line.trim());

/** How tall a map stands without saying — as `aw_model::markdown::MAP_HEIGHT`. */
export const MAP_HEIGHT = 480;

/** Where a pin's name may stand besides right of it — as `aw_model::markdown::LABEL_AT`. */
export const LABEL_AT = ["left", "top", "bottom"] as const;

/** A pin's or region's own access, as it stands on its tag. */
function accessOf(at: Map<string, string>): MapAccess {
  const out: MapAccess = {};
  for (const n of ["view", "edit", "level"] as const) if (at.get(n)) out[n] = at.get(n);
  return out;
}

const accessText = (a: MapAccess) => (["view", "edit", "level"] as const).filter((n) => a[n]).map((n) => `${n}=${value(a[n]!)}`);

/** Reads a map: its tag, and the pins on the lines before `</map>`. Forgiving: what does not read is left out. */
export function readMap(tag: string, inside: string[]): MapNode {
  const a = attributes(tag);
  const height = Number(a.get("height"));
  const tenths = (v: string | undefined) => {
    const n = Number((v ?? "").trim());
    return v !== undefined && v.trim() !== "" && Number.isFinite(n) ? Math.min(1000, Math.max(0, Math.round(n * 10))) : undefined;
  };
  const pins: MapNode["pins"] = [];
  const areas: MapNode["areas"] = [];
  for (const raw of inside) {
    const line = raw.trim();
    if (/^<area\b/i.test(line)) {
      const at = attributes(line);
      const points = (at.get("at") ?? "").split(/\s+/).filter(Boolean).map((p) => p.split(",").map(tenths));
      if (points.length < 3 || points.some((p) => p.length !== 2 || p[0] === undefined || p[1] === undefined)) continue;
      const color = at.get("color") ?? "";
      areas.push({ points: points as [number, number][], label: at.get("label") ?? "", to: at.get("to") ?? "", color: is(COLOURS, color) ? color : "", ...accessOf(at) });
      continue;
    }
    if (!/^<pin\b/i.test(line)) continue;
    const at = attributes(line);
    const [x, y] = (at.get("at") ?? "").split(",");
    const px = tenths(x);
    const py = tenths(y);
    if (px === undefined || py === undefined) continue;
    const color = at.get("color") ?? "";
    const icon = at.get("icon") ?? "";
    const labelWords = (at.get("label-at") ?? "").split(/\s+/);
    const labelAt = labelWords.find((w) => is(LABEL_AT, w)) ?? "";
    pins.push({
      x: px,
      y: py,
      label: at.get("label") ?? "",
      to: at.get("to") ?? "",
      color: is(COLOURS, color) ? color : "",
      ...(is(MARKERS, icon) && icon !== "pin" ? { icon } : {}),
      ...(labelAt ? { label_at: labelAt as LabelAt } : {}),
      ...(labelWords.includes("hover") ? { label_hover: true } : {}),
      ...accessOf(at),
    });
  }
  return { src: a.get("src") ?? "", height: Number.isFinite(height) && height > 0 ? Math.min(2000, Math.max(160, height)) : MAP_HEIGHT, pins, areas };
}

/** A map as it is written — without the last line break. */
export function mapText(m: MapNode): string {
  const attrs = [`src=${value(m.src)}`];
  if (m.height !== MAP_HEIGHT) attrs.push(`height=${m.height}`);
  const at = (n: number) => String(Math.round(n) / 10);
  return [
    `<map ${attrs.join(" ")}>`,
    ...m.pins.map((p) => {
      const parts = [`at=${at(p.x)},${at(p.y)}`];
      if (p.label) parts.push(`label=${value(p.label)}`);
      if (p.to) parts.push(`to=${value(p.to)}`);
      if (p.color) parts.push(`color=${p.color}`);
      if (p.icon && p.icon !== "pin") parts.push(`icon=${p.icon}`);
      const labelAt = [p.label_at, p.label_hover && "hover"].filter(Boolean).join(" ");
      if (labelAt) parts.push(`label-at=${value(labelAt)}`);
      return `<pin ${[...parts, ...accessText(p)].join(" ")}>`;
    }),
    ...m.areas.map((r) => {
      const parts = [`at="${r.points.map(([x, y]) => `${at(x)},${at(y)}`).join(" ")}"`];
      if (r.label) parts.push(`label=${value(r.label)}`);
      if (r.to) parts.push(`to=${value(r.to)}`);
      if (r.color) parts.push(`color=${r.color}`);
      return `<area ${[...parts, ...accessText(r)].join(" ")}>`;
    }),
    "</map>",
  ].join("\n");
}

/**
 * Puts the pins set apart back onto their maps — for reading. A part that
 * holds nothing but `<pins for=img:…>` (see `aw_model::document`, “Secret
 * pins”) gives its pins to the first map with that picture, each marked with
 * who sees it; the part itself is then empty. What this reader cannot open
 * never came: those pins are simply not there.
 */
export function joinPins(layers: { text: string; policy: string }[]): string[] {
  const texts = layers.map((l) => l.text);
  const found: { src: string; lines: string[] }[] = [];
  layers.forEach((layer, i) => {
    const sections = pinsSections(layer.text);
    if (!sections) return;
    texts[i] = "";
    for (const [src, pins] of sections) {
      const lines = pins.map((p) => p.replace(/\s*\/?>$/, ` view=${value(layer.policy)}>`));
      const known = found.find((f) => f.src === src);
      if (known) known.lines.push(...lines);
      else found.push({ src, lines });
    }
  });
  for (const { src, lines } of found) {
    for (let i = 0; i < texts.length; i++) {
      const rows = texts[i].split("\n");
      let inside = false;
      const end = rows.findIndex((row) => {
        if (opensMap(row)) inside = (attributes(row).get("src") ?? "") === src;
        else if (inside && row.trim().toLowerCase() === "</map>") return true;
        return false;
      });
      if (end < 0) continue;
      texts[i] = [...rows.slice(0, end), ...lines, ...rows.slice(end)].join("\n");
      break;
    }
  }
  return texts;
}

/** A part's `<pins for=…>` sections — when it holds nothing else. */
function pinsSections(text: string): [string, string[]][] | null {
  const out: [string, string[]][] = [];
  let open = false;
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (!line) continue;
    if (!open && /^<pins(\s[^<>]*)?>$/i.test(line)) {
      out.push([attributes(line).get("for") ?? "", []]);
      open = true;
    } else if (open && /^<\/pins>$/i.test(line)) open = false;
    else if (open && /^<(pin|area)\b[^<>]*>$/i.test(line)) out[out.length - 1][1].push(line);
    else return null;
  }
  return out.length && !open ? out : null;
}

/** The first map in a text — a document's, to go to its level. */
export function firstMap(text: string): MapNode | undefined {
  const found = pieces(text).find((p) => p.kind === "map");
  return found?.kind === "map" ? found.map : undefined;
}
