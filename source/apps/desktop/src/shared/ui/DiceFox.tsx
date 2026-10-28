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

/** A complete world by default; null repeats the approved layered build. */
export function DiceFox({ progress = 1, className = "" }: { progress?: number | null; className?: string }) {
  const id = `dicefox-${useId().replace(/[^a-zA-Z0-9_-]/g, "_")}`;
  const artwork = useMemo(() => ({ __html: content.replaceAll("__DICEFOX_ID__", id) }), [id]);
  const ref = useRef<SVGSVGElement>(null);
  const share = progress === null ? null : Number.isFinite(progress) ? clamp(progress) : 0;

  useLayoutEffect(() => {
    const root = ref.current!;
    const world = painter(root);
    const reduced = matchMedia("(prefers-reduced-motion: reduce)");
    let elapsed = 0, last = performance.now(), frame = 0;
    function paint(now: number) {
      const p = share ?? (reduced.matches ? 1 : Math.min(1, elapsed / BUILD));
      const opacity = share === null && !reduced.matches && elapsed > BUILD + HOLD
        ? 1 - (elapsed - BUILD - HOLD) / FADE : 1;
      world.paint(p, opacity);
      if (!reduced.matches) world.sway(now);
    }
    function tick(now: number) {
      if (share === null) elapsed = (elapsed + Math.min(80, now - last)) % (BUILD + HOLD + FADE);
      last = now;
      paint(now);
      frame = requestAnimationFrame(tick);
    }
    function resume() {
      cancelAnimationFrame(frame);
      last = performance.now();
      paint(last);
      const paused = reduced.matches || document.hidden;
      root.classList.toggle("dicefox-paused", paused);
      if (!paused && (share === null || share > .65)) frame = requestAnimationFrame(tick);
    }
    resume();
    reduced.addEventListener("change", resume);
    document.addEventListener("visibilitychange", resume);
    return () => {
      cancelAnimationFrame(frame);
      reduced.removeEventListener("change", resume);
      document.removeEventListener("visibilitychange", resume);
    };
  }, [share]);

  return <svg ref={ref} className={`dicefox${className ? ` ${className}` : ""}`}
    viewBox="0 0 1254 1254" aria-hidden="true" focusable="false"
    dangerouslySetInnerHTML={artwork} />;
}
