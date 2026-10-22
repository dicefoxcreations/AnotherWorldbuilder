import { useContext, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { ArticleView, MapAccess, MapArea, MapNode, MapPin, Place, WorldInfo } from "../../api/client";
import { COLOURS, firstMap, joinPins, MARKERS } from "../../shared/GraphText";
import { findType } from "../../shared/types";
import { ImagePicker } from "../../shared/ui/ImagePicker";
import { MarkerIcon } from "../../shared/ui/MapIcons";
import { PopupMenu, type MenuItem } from "../../shared/ui/PopupMenu";
import { SearchPick } from "../../shared/ui/SearchPick";
import { imageUrl } from "../../shared/ui/WorldImage";
import { useWorldData } from "../worlds/WorldData";
import { AccessRows, InspectorSection, PartAccessContext, Surface } from "./Inspector";

/**
 * A map: a picture moved and zoomed like a map on the web — dragged, the
 * wheel, the buttons — with markers and regions on it.
 *
 * A marker lies where it lies on the picture, in tenths of a percent, so it
 * stays in place at every zoom; only its sign keeps its size. A region is a
 * shape around a part of the picture — a country on a continent — that
 * lights up, border and all, under the pointer. Both may lead to a
 * document. Where that document has a map of its own, a click goes there in
 * place — a stair to the cellar, a country to its own map — and ‹ comes
 * back; otherwise the document opens. Resting on either shows its document,
 * as on any link.
 *
 * While writing (`onMap`), a click chooses a marker or a region, a drag
 * moves a marker or a region's corner; “＋ Marker” and a click sets a new
 * one, “＋ Region” and a click at each corner draws one — a click on its
 * first corner, or ✓, closes it.
 */

/** What a level of the map is: its map, and the document it belongs to. */
type Level = { map: MapNode; id?: string; name?: string };

/** What is chosen while writing. */
export type Chosen = { kind: "pin" | "area"; i: number } | null;

/** How far a map zooms out and in, relative to the picture fitting in. */
const ZOOM_OUT = 0.5;
const ZOOM_IN = 16;

type Drag = {
  kind: "pan" | "pin" | "corner";
  /** A marker, or the region whose corner. */
  i?: number;
  corner?: number;
  sx: number;
  sy: number;
  vx: number;
  vy: number;
  moved: boolean;
  /** Where it was grabbed, from its point — and where it is now. */
  ox?: number;
  oy?: number;
  at?: { x: number; y: number };
  /** A click without a move: what it means. */
  click?: () => void;
};

/** Where on a region's border a corner fits best: after which corner, nearest to the point. */
function nearestEdge(points: [number, number][], x: number, y: number): number {
  let best = 0;
  let far = Infinity;
  points.forEach(([ax, ay], i) => {
    const [bx, by] = points[(i + 1) % points.length];
    const dx = bx - ax;
    const dy = by - ay;
    const t = Math.max(0, Math.min(1, ((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy || 1)));
    const d = Math.hypot(ax + t * dx - x, ay + t * dy - y);
    if (d < far) (far = d), (best = i);
  });
  return best;
}

/** A menu where the pointer was: what it holds. */
type Menu = { x: number; y: number; title: string; items: MenuItem[] };

/** The middle of a region, for its name. */
const middle = (points: [number, number][]) => [
  points.reduce((s, p) => s + p[0], 0) / points.length,
  points.reduce((s, p) => s + p[1], 0) / points.length,
];

export function MapView({
  world,
  map,
  articles,
  onPlace,
  onMap,
  chosen,
  onChoose,
}: {
  world: string;
  map: MapNode;
  articles: ArticleView[];
  onPlace: (o: Place) => void;
  /** While writing: the map changed — a marker or a region set or moved. */
  onMap?: (next: MapNode) => void;
  chosen?: Chosen;
  onChoose?: (c: Chosen) => void;
}) {
  // Levels gone through: the map as written, then those it led to.
  const [levels, setLevels] = useState<Level[]>([]);
  const level: Level = levels.length ? levels[levels.length - 1] : { map };
  const writing = !!onMap && levels.length === 0;
  const box = useRef<HTMLDivElement>(null);
  // The picture's address, with the source it is of: until the next level's
  // has come, the last one's must not stand in for it — its size would fit
  // the view to the wrong picture.
  const [loaded, setLoaded] = useState<{ src: string; url: string } | null>(null);
  const url = loaded?.src === level.map.src ? loaded.url : null;
  const [size, setSize] = useState<{ w: number; h: number } | null>(null);
  const [view, setView] = useState<{ x: number; y: number; s: number } | null>(null);
  const [mode, setMode] = useState<"" | "pin" | "area">("");
  // A region being drawn, corner by corner.
  const [draft, setDraft] = useState<[number, number][]>([]);
  // The region under the pointer, and where the pointer is — its name goes there, above all else.
  const [hovered, setHovered] = useState<{ i: number; x: number; y: number } | null>(null);
  const [menu, setMenu] = useState<Menu | null>(null);
  // What is being dragged, where it is meanwhile.
  const [moving, setMoving] = useState<{ kind: "pin" | "corner"; i: number; corner?: number; x: number; y: number } | null>(null);
  const drag = useRef<Drag | null>(null);

  useEffect(() => {
    let gone = false;
    setLoaded(null);
    setSize(null);
    setView(null);
    if (level.map.src.startsWith("img:"))
      imageUrl(world, level.map.src, false)
        .then((u) => !gone && setLoaded({ src: level.map.src, url: u }))
        .catch(() => {});
    return () => {
      gone = true;
    };
  }, [world, level.map.src]);

  /** The picture fitting in, centred. */
  const fit = () => {
    const el = box.current;
    if (!el || !size) return null;
    const s = Math.min(el.clientWidth / size.w, el.clientHeight / size.h);
    return { s, x: (el.clientWidth - size.w * s) / 2, y: (el.clientHeight - size.h * s) / 2 };
  };
  useLayoutEffect(() => {
    if (size && !view) setView(fit());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [size]);

  /** Zooms by `k` around a point of the box — the point stays under the pointer. */
  const zoom = (k: number, px?: number, py?: number) => {
    const base = fit();
    if (!view || !base || !box.current) return;
    const s = Math.min(base.s * ZOOM_IN, Math.max(base.s * ZOOM_OUT, view.s * k));
    const cx = px ?? box.current.clientWidth / 2;
    const cy = py ?? box.current.clientHeight / 2;
    const f = s / view.s;
    setView({ s, x: cx - (cx - view.x) * f, y: cy - (cy - view.y) * f });
  };

  // The wheel zooms — and the page does not scroll meanwhile.
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const wheel = (e: WheelEvent) => {
      e.preventDefault();
      const r = el.getBoundingClientRect();
      zoom(Math.exp(-e.deltaY * 0.0015), e.clientX - r.left, e.clientY - r.top);
    };
    el.addEventListener("wheel", wheel, { passive: false });
    return () => el.removeEventListener("wheel", wheel);
  });

  // Escape gives up a region being drawn.
  useEffect(() => {
    if (!draft.length && mode !== "area") return;
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") (setDraft([]), setMode(""));
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [draft.length, mode]);

  /** A point of the box as a place on the picture, in tenths of a percent. */
  const onPicture = (clientX: number, clientY: number) => {
    const r = box.current!.getBoundingClientRect();
    const x = ((clientX - r.left - view!.x) / view!.s / size!.w) * 1000;
    const y = ((clientY - r.top - view!.y) / view!.s / size!.h) * 1000;
    return { x: Math.round(Math.min(1000, Math.max(0, x))), y: Math.round(Math.min(1000, Math.max(0, y))) };
  };

  /** A document's map in place, where it has one; otherwise the document. */
  const go = (to: string) => {
    const doc = articles.find((a) => a.id === to);
    if (!doc) return;
    const there = joinPins(doc.layers.map((l) => ({ text: l.text ?? "", policy: l.policy }))).map(firstMap).find(Boolean);
    if (there?.src) setLevels((ls) => [...ls, { map: there, id: doc.id, name: doc.name }]);
    else onPlace({ kind: "article", id: doc.id });
  };

  const removePin = (i: number) => {
    onMap!({ ...level.map, pins: level.map.pins.filter((_, j) => j !== i) });
    onChoose?.(null);
  };
  const removeArea = (i: number) => {
    onMap!({ ...level.map, areas: level.map.areas.filter((_, j) => j !== i) });
    onChoose?.(null);
  };
  /** A corner more on a region's border, after corner `after`. */
  const addCorner = (i: number, after: number, at: { x: number; y: number }) =>
    onMap!({
      ...level.map,
      areas: level.map.areas.map((r, j) => (j === i ? { ...r, points: [...r.points.slice(0, after + 1), [at.x, at.y], ...r.points.slice(after + 1)] } : r)),
    });
  const removeCorner = (i: number, k: number) =>
    onMap!({ ...level.map, areas: level.map.areas.map((r, j) => (j === i ? { ...r, points: r.points.filter((_, n) => n !== k) } : r)) });
  /** A right click while writing: a menu for what is under the pointer. */
  const menuFor = (e: React.MouseEvent, title: string, items: MenuItem[]) => {
    e.preventDefault();
    e.stopPropagation();
    if (!writing) return;
    setMenu({ x: e.clientX, y: e.clientY, title, items });
  };
  /** Where the pointer is in the box — for the name of a region under it. */
  const inBox = (e: React.PointerEvent) => {
    const r = box.current!.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };

  /** The region drawn so far, closed: a region of its own, chosen. */
  const close = () => {
    if (draft.length < 3) return;
    onMap!({ ...level.map, areas: [...level.map.areas, { points: draft, label: "", to: "", color: "" }] });
    onChoose?.({ kind: "area", i: level.map.areas.length });
    setDraft([]);
    setMode("");
  };

  /** A press: on the picture, a marker, a region or a corner — what a drag and a click then do. */
  const press = (e: React.PointerEvent, kind: Drag["kind"], click?: () => void, i?: number, corner?: number, from?: { x: number; y: number }) => {
    if (!view || e.button !== 0) return;
    e.stopPropagation();
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    const grabbed = kind !== "pan" && size ? onPicture(e.clientX, e.clientY) : undefined;
    drag.current = {
      kind,
      i,
      corner,
      sx: e.clientX,
      sy: e.clientY,
      vx: view.x,
      vy: view.y,
      moved: false,
      ox: grabbed && from ? grabbed.x - from.x : 0,
      oy: grabbed && from ? grabbed.y - from.y : 0,
      click,
    };
  };
  const move = (e: React.PointerEvent) => {
    const d = drag.current;
    if (!d || !view) return;
    const dx = e.clientX - d.sx;
    const dy = e.clientY - d.sy;
    if (Math.abs(dx) + Math.abs(dy) > 3) d.moved = true;
    if (!d.moved) return;
    if (d.kind === "pan") return setView({ ...view, x: d.vx + dx, y: d.vy + dy });
    const at = onPicture(e.clientX, e.clientY);
    d.at = { x: Math.min(1000, Math.max(0, at.x - (d.ox ?? 0))), y: Math.min(1000, Math.max(0, at.y - (d.oy ?? 0))) };
    setMoving({ kind: d.kind, i: d.i!, corner: d.corner, ...d.at });
  };
  const release = (e: React.PointerEvent) => {
    const d = drag.current;
    drag.current = null;
    setMoving(null);
    if (!d) return;
    e.stopPropagation();
    if (!d.moved) return d.click?.();
    const at = d.at;
    if (!at || d.i === undefined) return;
    if (d.kind === "pin") onMap!({ ...level.map, pins: level.map.pins.map((p, j) => (j === d.i ? { ...p, x: at.x, y: at.y } : p)) });
    if (d.kind === "corner")
      onMap!({
        ...level.map,
        areas: level.map.areas.map((r, j) => (j === d.i ? { ...r, points: r.points.map((p, k) => (k === d.corner ? [at.x, at.y] : p)) } : r)),
      });
  };

  /** A click on the picture itself: a marker set, a corner drawn — or nothing chosen. */
  const onBackground = (e: React.PointerEvent) =>
    press(e, "pan", () => {
      if (!writing || !size) return;
      const at = onPicture(e.clientX, e.clientY);
      if (mode === "pin") {
        onMap!({ ...level.map, pins: [...level.map.pins, { ...at, label: "", to: "", color: "" }] });
        onChoose?.({ kind: "pin", i: level.map.pins.length });
        setMode("");
      } else if (mode === "area") setDraft((d) => [...d, [at.x, at.y]]);
      else onChoose?.(null);
    });

  const pins = level.map.pins.map((p, i) => (moving?.kind === "pin" && moving.i === i ? { ...p, x: moving.x, y: moving.y } : p));
  const areas = level.map.areas.map((r, i) =>
    moving?.kind === "corner" && moving.i === i
      ? { ...r, points: r.points.map((p, k): [number, number] => (k === moving.corner ? [moving.x, moving.y] : p)) }
      : r,
  );
  const chosenArea = writing && chosen?.kind === "area" ? areas[chosen.i] : undefined;
  const counter = view ? `scale(${1 / view.s})` : undefined;
  return (
    <div className={`map-view${writing ? " writing" : ""}${mode ? ` adding-${mode}` : ""}`} style={{ height: level.map.height }}>
      <div
        ref={box}
        className="map-box"
        onPointerDown={onBackground}
        onPointerMove={move}
        onPointerUp={release}
        onDoubleClick={(e) => {
          if (writing) return;
          const r = box.current!.getBoundingClientRect();
          zoom(2, e.clientX - r.left, e.clientY - r.top);
        }}
      >
        {!level.map.src.startsWith("img:") ? (
          <span className="map-empty small muted">{writing ? "Choose a picture in the side bar." : "No picture yet."}</span>
        ) : !url ? (
          <span className="map-empty small muted">🗺</span>
        ) : (
          <div
            // Each level its own picture: loaded anew, even where it is the same.
            key={`${levels.length}:${level.map.src}`}
            className="map-layer"
            style={
              view && size
                ? { width: size.w, height: size.h, transform: `translate(${view.x}px, ${view.y}px) scale(${view.s})` }
                : { visibility: "hidden" }
            }
          >
            <img src={url} alt="" draggable={false} onLoad={(e) => setSize({ w: e.currentTarget.naturalWidth, h: e.currentTarget.naturalHeight })} />
            {view && (
              <svg className="map-areas" viewBox="0 0 1000 1000" preserveAspectRatio="none">
                {areas.map((r, i) => (
                  <polygon
                    key={i}
                    className={`map-area${r.color ? ` rel-${r.color}` : ""}${r.to ? " leads" : ""}${writing && chosen?.kind === "area" && chosen.i === i ? " chosen" : ""}${r.view ? " secret" : ""}`}
                    points={r.points.map((p) => p.join(",")).join(" ")}
                    onPointerEnter={(e) => setHovered({ i, ...inBox(e) })}
                    onPointerLeave={() => setHovered((h) => (h?.i === i ? null : h))}
                    onContextMenu={(e) => {
                      const at = onPicture(e.clientX, e.clientY);
                      menuFor(e, r.label || "Region", [
                        { label: "Add a corner here", onClick: () => (addCorner(i, nearestEdge(r.points, at.x, at.y), at), onChoose?.({ kind: "area", i })) },
                        { label: "Remove the region", danger: true, onClick: () => removeArea(i) },
                      ]);
                    }}
                    onPointerDown={(e) =>
                      // While drawing, a click inside a region is a corner like any other.
                      mode
                        ? onBackground(e)
                        : press(e, "pan", () => (writing ? onChoose?.({ kind: "area", i }) : r.to && go(r.to)))
                    }
                    onPointerMove={(e) => (setHovered({ i, ...inBox(e) }), move(e))}
                    onPointerUp={release}
                  />
                ))}
                {draft.length > 0 && <polyline className="map-draft" points={draft.map((p) => p.join(",")).join(" ")} />}
              </svg>
            )}
            {view &&
              areas.map((r, i) => {
                const [x, y] = middle(r.points);
                const shown = r.label && writing && chosen?.kind === "area" && chosen.i === i && hovered?.i !== i;
                return shown ? (
                  <div key={`l${i}`} className="map-area-label" style={{ left: `${x / 10}%`, top: `${y / 10}%`, transform: counter }}>
                    <span>{r.label}</span>
                  </div>
                ) : null;
              })}
            {/* The corners of the region chosen, to drag; those of one being drawn. */}
            {view &&
              chosenArea?.points.map((p, k) => (
                <div
                  key={`c${k}`}
                  className="map-corner"
                  style={{ left: `${p[0] / 10}%`, top: `${p[1] / 10}%`, transform: counter }}
                  title="Drag to move — right click to remove"
                  onPointerDown={(e) => press(e, "corner", undefined, chosen!.i, k, { x: p[0], y: p[1] })}
                  onPointerMove={move}
                  onPointerUp={release}
                  onContextMenu={(e) =>
                    menuFor(e, "Corner", [
                      {
                        label: "Remove the corner",
                        danger: true,
                        disabled: chosenArea.points.length <= 3,
                        title: chosenArea.points.length <= 3 ? "A region has three corners at least" : undefined,
                        onClick: () => removeCorner(chosen!.i, k),
                      },
                    ])
                  }
                />
              ))}
            {/* Between two corners: drag to make a corner there — the border finer. */}
            {view &&
              chosenArea?.points.map((p, k) => {
                const q = chosenArea.points[(k + 1) % chosenArea.points.length];
                const mid = { x: Math.round((p[0] + q[0]) / 2), y: Math.round((p[1] + q[1]) / 2) };
                return (
                  <div
                    key={`m${k}`}
                    className="map-corner between"
                    title="Drag to add a corner"
                    style={{ left: `${mid.x / 10}%`, top: `${mid.y / 10}%`, transform: counter }}
                    onPointerDown={(e) => {
                      addCorner(chosen!.i, k, mid);
                      press(e, "corner", undefined, chosen!.i, k + 1, mid);
                    }}
                    onPointerMove={move}
                    onPointerUp={release}
                  />
                );
              })}
            {view &&
              draft.map((p, k) => (
                <div
                  key={`d${k}`}
                  className={`map-corner draft${k === 0 && draft.length >= 3 ? " closes" : ""}`}
                  style={{ left: `${p[0] / 10}%`, top: `${p[1] / 10}%`, transform: counter }}
                  title={k === 0 && draft.length >= 3 ? "Close the region" : undefined}
                  onPointerDown={(e) => press(e, "pan", k === 0 ? close : undefined)}
                  onPointerMove={move}
                  onPointerUp={release}
                />
              ))}
            {view &&
              pins.map((p, i) => (
                <div
                  key={i}
                  className={`map-pin${p.icon ? " icon" : ""}${p.color ? ` rel-${p.color}` : ""}${writing && chosen?.kind === "pin" && chosen.i === i ? " chosen" : ""}${p.to ? " leads" : ""}${p.view ? " secret" : ""} label-${p.label_at ?? "right"}${p.label_hover ? " label-hover" : ""}`}
                  style={{ left: `${p.x / 10}%`, top: `${p.y / 10}%`, transform: counter }}
                  title={p.label}
                  onPointerDown={(e) =>
                    // While setting or drawing, a click on a marker counts as one on the picture.
                    mode
                      ? onBackground(e)
                      : writing
                        ? press(e, "pin", () => onChoose?.({ kind: "pin", i }), i, undefined, { x: p.x, y: p.y })
                        : press(e, "pan", () => p.to && go(p.to))
                  }
                  onPointerMove={move}
                  onPointerUp={release}
                  onContextMenu={(e) => menuFor(e, p.label || "Marker", [{ label: "Remove the marker", danger: true, onClick: () => removePin(i) }])}
                >
                  {p.icon ? (
                    <span className="map-pin-badge">
                      <MarkerIcon name={p.icon} size={15} />
                    </span>
                  ) : (
                    <span className="map-pin-mark" />
                  )}
                  {p.label && <span className="map-pin-label">{p.label}</span>}
                </div>
              ))}
          </div>
        )}
      </div>
      {hovered && areas[hovered.i]?.label && !drag.current && (
        <div className={`map-hover-label${hovered.y < 48 ? " below" : ""}`} style={{ left: hovered.x, top: hovered.y }}>
          {areas[hovered.i].label}
        </div>
      )}
      {menu && <PopupMenu x={menu.x} y={menu.y} title={menu.title} items={menu.items} onClose={() => setMenu(null)} />}
      <div className="map-controls">
        {levels.length > 0 && (
          <button className="compact" title="Back" onClick={() => setLevels((ls) => ls.slice(0, -1))}>
            ‹ {levels.length > 1 ? levels[levels.length - 2].name : "Back"}
          </button>
        )}
        {level.name && (
          <button className="ghost compact map-level" title={`Open ${level.name}`} onClick={() => level.id && onPlace({ kind: "article", id: level.id })}>
            {level.name}
          </button>
        )}
        <span className="map-spacer" />
        {writing && (
          <>
            <button
              className={`compact${mode === "pin" ? " on" : ""}`}
              title="Then click where it goes"
              onClick={() => (setDraft([]), setMode((m) => (m === "pin" ? "" : "pin")))}
            >
              ＋ Marker
            </button>
            {mode === "area" && draft.length >= 3 ? (
              <button className="compact on" title="Close the region" onClick={close}>
                ✓ Region
              </button>
            ) : (
              <button
                className={`compact${mode === "area" ? " on" : ""}`}
                title="Then click at each corner, the first again to close it"
                onClick={() => (setDraft([]), setMode((m) => (m === "area" ? "" : "area")))}
              >
                ＋ Region
              </button>
            )}
          </>
        )}
        <button className="compact" title="Zoom in" onClick={() => zoom(1.5)}>
          +
        </button>
        <button className="compact" title="Zoom out" onClick={() => zoom(1 / 1.5)}>
          −
        </button>
        <button className="compact" title="All of it" onClick={() => setView(fit())}>
          ⤢
        </button>
      </div>
    </div>
  );
}

/** Where a pin's name may stand while always shown: the sign, and what it means. */
const SIDES: ["left" | "top" | "right" | "bottom", string, string][] = [
  ["left", "←", "Left"],
  ["top", "↑", "Above"],
  ["right", "→", "Right"],
  ["bottom", "↓", "Below"],
];

/**
 * Who sees a marker or region — the GM's. As the map by default; set apart,
 * only those it names have it at all: saving puts it into a part of its own
 * (see `aw_model::document`, “Secret pins”). Revealing is giving it wider
 * access, step by step — to one character, to all players, back to the map's.
 */
function ItemAccess({ item, onChange }: { item: MapAccess; onChange: (next: MapAccess) => void }) {
  const at = useContext(PartAccessContext);
  if (!at || !at.world.isGm) return null;
  if (!item.view)
    return (
      <InspectorSection title="Access">
        <p className="small muted">Sees it: whoever sees the map.</p>
        <button className="compact" onClick={() => onChange({ view: "gm", edit: "gm" })}>
          GM only
        </button>
      </InspectorSection>
    );
  const value = { view: item.view, edit: item.edit || at.part.edit, level: item.level || "open" };
  return (
    <InspectorSection title="Access — its own">
      <AccessRows
        world={at.world}
        value={value}
        onChange={(p) => {
          const next = { ...value, ...p };
          onChange({ view: next.view, edit: next.edit, level: next.level === "open" ? undefined : next.level });
        }}
      />
      <button className="compact ghost" onClick={() => onChange({ view: undefined, edit: undefined, level: undefined })}>
        As the map
      </button>
    </InspectorSection>
  );
}

/** A map as it is read. */
export function MapBlock({ map }: { map: MapNode }) {
  const data = useWorldData();
  if (!data) return <p className="listing-placeholder small">Map</p>;
  return <MapView world={data.world.id} map={map} articles={data.articles} onPlace={data.onPlace} />;
}

/**
 * A map in the visual editor — drawn as read, its markers and regions set
 * and moved on it. The side bar holds the one chosen — its name, where it
 * leads, its sign and colour — and the list of all of them; below, the
 * picture and the height.
 */
export function MapCard({ world, map, onChange }: { world: WorldInfo; map: MapNode; onChange: (next: MapNode | null) => void }) {
  const data = useWorldData();
  const [chosen, setChosen] = useState<Chosen>(null);
  const [picking, setPicking] = useState(false);
  const [menu, setMenu] = useState<Menu | null>(null);
  /** Right click on an entry of the list: take it out. */
  const listMenu = (e: React.MouseEvent, kind: "pin" | "area", i: number, title: string) => {
    e.preventDefault();
    setMenu({
      x: e.clientX,
      y: e.clientY,
      title,
      items: [
        {
          label: kind === "pin" ? "Remove the marker" : "Remove the region",
          danger: true,
          onClick: () => {
            onChange(kind === "pin" ? { ...map, pins: map.pins.filter((_, j) => j !== i) } : { ...map, areas: map.areas.filter((_, j) => j !== i) });
            setChosen(null);
          },
        },
      ],
    });
  };
  // Made secret or revealed, a marker moves among the others when saved (its
  // part lies elsewhere) — the side bar keeps to it by where it lies and its name.
  const sign = (c: Chosen) =>
    c?.kind === "pin" ? JSON.stringify(map.pins[c.i] && [map.pins[c.i].x, map.pins[c.i].y, map.pins[c.i].label]) : c ? JSON.stringify(map.areas[c.i] && [map.areas[c.i].points, map.areas[c.i].label]) : "";
  const kept = useRef<{ chosen: Chosen; sign: string } | null>(null);
  useEffect(() => {
    if (!chosen) {
      kept.current = null;
      return;
    }
    const now = sign(chosen);
    // Chosen anew, or the same one still where it was: nothing to follow.
    if (!kept.current || kept.current.chosen !== chosen || now === kept.current.sign) {
      kept.current = { chosen, sign: now };
      return;
    }
    const list = chosen.kind === "pin" ? map.pins : map.areas;
    const found = list.findIndex((_, i) => sign({ kind: chosen.kind, i }) === kept.current!.sign);
    if (found >= 0 && found !== chosen.i) {
      const next = { kind: chosen.kind, i: found };
      kept.current = { chosen: next, sign: kept.current.sign };
      setChosen(next);
    } else kept.current = { chosen, sign: now };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [map, chosen]);
  const pin = chosen?.kind === "pin" ? map.pins[chosen.i] : undefined;
  const area = chosen?.kind === "area" ? map.areas[chosen.i] : undefined;
  const setPin = (next: Partial<MapPin>) => chosen && onChange({ ...map, pins: map.pins.map((p, i) => (i === chosen.i ? { ...p, ...next } : p)) });
  const setArea = (next: Partial<MapArea>) => chosen && onChange({ ...map, areas: map.areas.map((r, i) => (i === chosen.i ? { ...r, ...next } : r)) });
  const documents = (data?.articles ?? [])
    .map((a) => ({ id: a.id, name: a.name, icon: findType(data!.templates, a.articleType)?.icon }))
    .sort((a, b) => a.name.localeCompare(b.name));
  // While writing, `to` is a document's name as the code has it — or an id, once picked.
  const docOf = (to: string) => {
    const all = data?.articles ?? [];
    return all.find((a) => a.id === to) ?? all.find((a) => a.name.toLowerCase() === to.trim().toLowerCase());
  };
  const nameOf = (to: string) => docOf(to)?.name;
  const item = pin ?? area;
  const set = pin ? setPin : setArea;
  const inspector = (
    <>
      {/* The map has its section's access; a marker or region chosen may have its own. */}
      {item && <ItemAccess item={item} onChange={set} />}
      {item && (
        <InspectorSection title={pin ? "Marker" : "Region"}>
          <input value={item.label} placeholder="Name" onChange={(e) => set({ label: e.target.value })} />
          {pin && (
            <div className="map-label-at">
              <div className="segmented">
                <button className={!pin.label_hover ? "on" : ""} onClick={() => setPin({ label_hover: undefined })}>
                  Always
                </button>
                <button className={pin.label_hover ? "on" : ""} onClick={() => setPin({ label_hover: true })}>
                  On hover
                </button>
              </div>
              <div className="segmented" title="Where the name stands">
                {SIDES.map(([side, sign, title]) => (
                  <button
                    key={side}
                    title={title}
                    className={(pin.label_at ?? "right") === side ? "on" : ""}
                    onClick={() => setPin({ label_at: side === "right" ? undefined : side })}
                  >
                    {sign}
                  </button>
                ))}
              </div>
            </div>
          )}
          <SearchPick
            items={documents}
            chosen={item.to ? [docOf(item.to)?.id ?? item.to] : []}
            onAdd={(id) => set({ to: id })}
            onRemove={() => set({ to: "" })}
            placeholder="Leads to…"
            single
          />
          {pin && (
            <div className="map-markers">
              {MARKERS.map((m) => (
                <button
                  key={m}
                  className={`map-marker-choice${(pin.icon || "pin") === m ? " on" : ""}`}
                  title={m}
                  onClick={() => setPin({ icon: m === "pin" ? undefined : m })}
                >
                  <MarkerIcon name={m} size={16} />
                </button>
              ))}
            </div>
          )}
          <div className="map-colours">
            {["", ...COLOURS].map((c) => (
              <button
                key={c || "none"}
                className={`map-colour${c ? ` rel-${c}` : ""}${item.color === c ? " on" : ""}`}
                title={c || "As usual"}
                onClick={() => set({ color: c })}
              />
            ))}
          </div>
          <button
            className="danger compact"
            onClick={() => {
              onChange(pin ? { ...map, pins: map.pins.filter((_, i) => i !== chosen!.i) } : { ...map, areas: map.areas.filter((_, i) => i !== chosen!.i) });
              setChosen(null);
            }}
          >
            Remove
          </button>
        </InspectorSection>
      )}
      <InspectorSection title={`On the map · ${map.pins.length + map.areas.length}`}>
        <ul className="map-list">
          {map.pins.map((p, i) => (
            <li key={`p${i}`}>
              <button
                className={`map-list-item${chosen?.kind === "pin" && chosen.i === i ? " on" : ""}${p.color ? ` rel-${p.color}` : ""}`}
                onClick={() => setChosen({ kind: "pin", i })}
                onContextMenu={(e) => listMenu(e, "pin", i, p.label || "Marker")}
              >
                <span className="map-list-sign">
                  <MarkerIcon name={p.icon || "pin"} size={14} />
                </span>
                <span className="map-list-name">{p.label || <em className="muted">Unnamed</em>}</span>
                {p.view && <span className="map-list-secret" title={`Sees: ${p.view}`}>◌</span>}
                {p.to && <span className="small muted">→ {nameOf(p.to) ?? "?"}</span>}
              </button>
            </li>
          ))}
          {map.areas.map((r, i) => (
            <li key={`a${i}`}>
              <button
                className={`map-list-item${chosen?.kind === "area" && chosen.i === i ? " on" : ""}${r.color ? ` rel-${r.color}` : ""}`}
                onClick={() => setChosen({ kind: "area", i })}
                onContextMenu={(e) => listMenu(e, "area", i, r.label || "Region")}
              >
                <span className="map-list-sign area" />
                <span className="map-list-name">{r.label || <em className="muted">Unnamed region</em>}</span>
                {r.view && <span className="map-list-secret" title={`Sees: ${r.view}`}>◌</span>}
                {r.to && <span className="small muted">→ {nameOf(r.to) ?? "?"}</span>}
              </button>
            </li>
          ))}
        </ul>
      </InspectorSection>
      <InspectorSection title="Map">
        <button className="compact" onClick={() => setPicking(true)}>
          {map.src ? "Choose another picture" : "Choose a picture"}
        </button>
        {map.areas.length > 1 && (
          <button
            className="compact"
            title="Each region its own colour — set one by itself above"
            onClick={() => onChange({ ...map, areas: map.areas.map((r, i) => ({ ...r, color: COLOURS[i % (COLOURS.length - 1)] })) })}
          >
            Colour each region
          </button>
        )}
        <label className="image-card-width" title="How tall the map stands in the page">
          <input
            type="number"
            min={160}
            max={2000}
            value={map.height}
            onChange={(e) => onChange({ ...map, height: Math.min(2000, Math.max(160, Number(e.target.value) || 480)) })}
          />
          px
        </label>
      </InspectorSection>
      <button className="danger compact inspector-remove" onClick={() => onChange(null)}>
        Remove the map
      </button>
      {menu && <PopupMenu x={menu.x} y={menu.y} title={menu.title} items={menu.items} onClose={() => setMenu(null)} />}
    </>
  );
  return (
    <Surface label="Map" className="map-card" inspector={inspector}>
      <MapView
        world={world.id}
        map={map}
        articles={data?.articles ?? []}
        onPlace={data?.onPlace ?? (() => {})}
        onMap={onChange}
        chosen={chosen}
        onChoose={setChosen}
      />
      {picking && (
        <ImagePicker
          world={world.id}
          onChosen={(id) => {
            onChange({ ...map, src: id });
            setPicking(false);
          }}
          onCancel={() => setPicking(false)}
        />
      )}
    </Surface>
  );
}
