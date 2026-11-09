import { useId, useLayoutEffect, useMemo, useRef } from "react";
import scene from "../../assets/dicefox/scene.svg?raw";
import fox from "../../assets/dicefox/00-fox-base.webp";
import paw from "../../assets/dicefox/paw-cutout.webp";
import earth from "../../assets/dicefox/20-earth.webp";
import grass from "../../assets/dicefox/30-grass.webp";
import mountains from "../../assets/dicefox/40-mountains.webp";
import river from "../../assets/dicefox/50-river.webp";
import trees from "../../assets/dicefox/60-trees.webp";
import clouds from "../../assets/dicefox/70-clouds.webp";
import castle from "../../assets/dicefox/80-castle.webp";
import "./DiceFox.css";

// Repository-owned SVG, exported unchanged from the approved v12 composition.
// Raster layers are bundled once; SVG references are unique for each instance.
const images: Record<string, string> = {
  "00_FOX_BASE": fox, PAW_CUTOUT: paw, "20_EARTH": earth, "30_GRASS": grass,
  "40_MOUNTAINS": mountains, "50_RIVER": river, "60_TREES": trees,
  "70_CLOUDS": clouds, "80_CASTLE": castle,
};
const content = scene.slice(scene.indexOf(">") + 1, scene.lastIndexOf("</svg>"))
  .replace(/__DICEFOX_ASSET_(\w+)__/g, (_, name: string) =>
    images[name].replaceAll("&", "&amp;").replaceAll('"', "&quot;"));

const stages = [
  ["earth", .035, .20], ["grass", .18, .35], ["mountains", .32, .50],
  ["river", .52, .70], ["trees", .65, .82], ["clouds", .77, .91],
  ["castle", .85, .96],
] as const;
const growth = [
  ["earth-chunk", .035, .018, .12, "earth"],
  ["grass-patch", .18, .012, .105, "grass"],
  ["mountain-form", .32, .024, .115, "vertical"],
  ["tree-form", .65, .009, .10, "vertical"],
  ["cloud-form", .77, .018, .10, "cloud"],
] as const;
const clamp = (x: number) => Math.min(1, Math.max(0, x));
const ease = (x: number) => { const p = clamp(x); return p * p * (3 - 2 * p); };
const BUILD = 18_000, HOLD = 8_000, FADE = 1_000;
// Following a progress: the whole build takes at least FOLLOW, so a step is
// grown into, not jumped to; the finished world stays SHOWN before onBuilt.
const FOLLOW = 2_400, SHOWN = 450, CREEP = 8_000;

function painter(root: SVGSVGElement) {
  const chunks = growth.map(([name, start, interval, duration, mode]) => ({
    elements: [...root.querySelectorAll<SVGGElement>(`.${name}`)], start, interval, duration, mode,
  }));
  const world = [...root.querySelectorAll<SVGGElement>(".world-layer")];
  const livingTrees = [...root.querySelectorAll<SVGGElement>(".tree-form")];
  return {
    paint(progress: number, opacity = 1) {
      for (const [name, start, end] of stages) {
        root.style.setProperty(`--${name}`, ease((progress - start) / (end - start)).toFixed(5));
      }
      for (const { elements, start, interval, duration, mode } of chunks) {
        elements.forEach((element, i) => {
          const p = ease((progress - start - i * interval) / duration);
          element.style.opacity = String(p);
          element.dataset.growth = String(p);
          element.style.transform = mode === "earth" ? `translateY(${(1 - p) * 26}px) scale(${.05 + p * .95})`
            : mode === "grass" ? `scale(${p})`
            : mode === "cloud" ? `translateX(${(1 - p) * -15}px) scale(${p})`
            : `scaleY(${p})`;
        });
      }
      for (const layer of world) layer.style.opacity = String(opacity);
    },
    sway(now: number) {
      livingTrees.forEach((element, i) => {
        const p = Number(element.dataset.growth);
        if (p > .99) element.style.transform = `scaleY(${p}) rotate(${Math.sin(now / 1700 + i * .8) * .35}deg)`;
      });
    },
  };
}

/**
 * A complete world by default; null repeats the approved layered build. With
 * `onBuilt` it grows from nothing after `progress`, never faster than FOLLOW,
 * and creeps on while a step takes long — then, at 1, it finishes the world,
 * holds it a moment and calls `onBuilt`.
 */
export function DiceFox({ progress = 1, className = "", onBuilt }: { progress?: number | null; className?: string; onBuilt?: () => void }) {
  const id = `dicefox-${useId().replace(/[^a-zA-Z0-9_-]/g, "_")}`;
  const artwork = useMemo(() => ({ __html: content.replaceAll("__DICEFOX_ID__", id) }), [id]);
  const ref = useRef<SVGSVGElement>(null);
  const share = progress === null ? null : Number.isFinite(progress) ? clamp(progress) : 0;
  // What is drawn, kept across steps; a plain logo starts where it stands.
  const shown = useRef<number | null>(null);
  const built = useRef(onBuilt);
  built.current = onBuilt;
  const following = !!onBuilt;

  useLayoutEffect(() => {
    const root = ref.current!;
    const world = painter(root);
    const reduced = matchMedia("(prefers-reduced-motion: reduce)");
    let elapsed = 0, last = performance.now(), frame = 0, whole = 0, told = false;
    const since = last;
    if (share !== null && (shown.current === null || !following)) shown.current = following ? 0 : share;
    function tell() {
      if (told) return;
      told = true;
      built.current?.();
    }
    // Where it heads: the step reached, and a little beyond while it lasts.
    function goal(now: number) {
      if (share === null || share >= 1) return 1;
      return Math.min(.95, share + (.95 - share) * (1 - Math.exp(-(now - since) / CREEP)) * .5);
    }
    function paint(now: number) {
      const p = share === null ? (reduced.matches ? 1 : Math.min(1, elapsed / BUILD)) : shown.current ?? share;
      const opacity = share === null && !reduced.matches && elapsed > BUILD + HOLD
        ? 1 - (elapsed - BUILD - HOLD) / FADE : 1;
      world.paint(p, opacity);
      if (!reduced.matches) world.sway(now);
    }
    function tick(now: number) {
      const step = Math.min(80, now - last);
      if (share === null) elapsed = (elapsed + step) % (BUILD + HOLD + FADE);
      else if (following) {
        const at = shown.current ?? 0;
        shown.current = Math.min(Math.max(at, goal(now)), at + step / FOLLOW);
      }
      last = now;
      paint(now);
      if (following && share === 1 && (shown.current ?? 0) >= 1) {
        whole ||= now;
        if (now - whole >= SHOWN) tell();
      }
      frame = requestAnimationFrame(tick);
    }
    function resume() {
      cancelAnimationFrame(frame);
      last = performance.now();
      paint(last);
      const paused = reduced.matches || document.hidden;
      root.classList.toggle("dicefox-paused", paused);
      // Not drawn meanwhile: nothing to wait for once it is there.
      if (paused && following && share === 1) {
        shown.current = 1;
        paint(last);
        tell();
      }
      if (!paused && (share === null || following || share > .65)) frame = requestAnimationFrame(tick);
    }
    resume();
    reduced.addEventListener("change", resume);
    document.addEventListener("visibilitychange", resume);
    return () => {
      cancelAnimationFrame(frame);
      reduced.removeEventListener("change", resume);
      document.removeEventListener("visibilitychange", resume);
    };
  }, [share, following]);

  return <svg ref={ref} className={`dicefox${className ? ` ${className}` : ""}`}
    viewBox="0 0 1254 1254" aria-hidden="true" focusable="false"
    dangerouslySetInnerHTML={artwork} />;
}
