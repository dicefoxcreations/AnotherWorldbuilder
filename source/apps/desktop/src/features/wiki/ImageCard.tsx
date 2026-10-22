import { useState } from "react";
import type { WorldInfo } from "../../api/client";
import { attr, readImageLine, withAttr, writeImageLine, type ImageAttrs } from "../../shared/ImageTag";
import { ImagePicker } from "../../shared/ui/ImagePicker";
import { WorldImage } from "../../shared/ui/WorldImage";
import { AccessSection, InspectorSection, Surface, TagSection } from "./Inspector";

const ALIGN = [
  { id: "left", name: "Left" },
  { id: "center", name: "Center" },
  { id: "right", name: "Right" },
];

/** How an image stands in the flow of the text: its place, its size, whether the text runs beside it. */
export function imageSurface(image: ImageAttrs): string {
  const align = attr(image, "align") || "center";
  const sized = attr(image, "width") || attr(image, "size");
  const wraps = align === "center" || attr(image, "wrap") !== "no";
  return `image ${align}${sized ? " sized" : ""}${wraps ? "" : " nowrap"}`;
}

/**
 * An image in the visual editor: the picture itself and its caption, as
 * read. A click chooses it; the side bar then shows who may see it, where
 * it stands and how wide, what the others see in its place, and its tag.
 * Chosen, a click on the picture picks another.
 */
export function ImageCard({
  world,
  image,
  ownPart,
  onChange,
}: {
  world: WorldInfo;
  image: ImageAttrs;
  /** The image is a part of its own, with access of its own: the others see the stand-in. */
  ownPart: boolean;
  /** `null` takes the image out. */
  onChange: (next: ImageAttrs | null) => void;
}) {
  const [picking, setPicking] = useState<"src" | "hidden" | null>(null);
  const set = (name: string, value: string) => onChange(withAttr(image, name, value));
  const src = attr(image, "src");
  const standIn = attr(image, "for") !== "";
  const align = attr(image, "align") || "center";
  const wraps = align !== "center" && attr(image, "wrap") !== "no";
  const width = attr(image, "width") || attr(image, "size").split(/[x×X]/)[0] || "";
  // Only some see it: the others may see something else in its place.
  const hidden = ownPart || attr(image, "view") !== "";
  const inspector = standIn ? (
    <p className="small muted">What the others see in place of an image hidden from them. It changes with that image.</p>
  ) : (
    <>
      <AccessSection attrs={image} onAttrs={onChange} />
      <InspectorSection title="Image">
        <button className="compact" onClick={() => setPicking("src")}>
          Choose another
        </button>
        <div className="segmented">
          {ALIGN.map((a) => (
            <button key={a.id} className={align === a.id ? "on" : ""} onClick={() => set("align", a.id)}>
              {a.name}
            </button>
          ))}
        </div>
        <button
          className={`compact${wraps ? " on" : ""}`}
          disabled={align === "center"}
          title={align === "center" ? "A centred image stands on its own" : "Text runs beside the image"}
          onClick={() => set("wrap", wraps ? "no" : "")}
        >
          Text beside
        </button>
        <label className="image-card-width" title="Width in pixels — empty: as wide as there is room">
          <input
            type="number"
            min={40}
            max={4000}
            value={width}
            placeholder="Width"
            onChange={(e) => onChange(withAttr(withAttr(image, "size", ""), "width", e.target.value))}
          />
          px
        </label>
      </InspectorSection>
      {hidden && (
        <InspectorSection title="Others see">
          <div className="image-card-hidden">
            <button className="stand-in" onClick={() => setPicking("hidden")} title="Choose what the others see">
              {attr(image, "hidden") ? <WorldImage world={world.id} id={attr(image, "hidden")} preview /> : <span className="image-placeholder unknown">?</span>}
            </button>
            <input value={attr(image, "hidden-caption")} placeholder="Their caption" onChange={(e) => set("hidden-caption", e.target.value)} />
          </div>
        </InspectorSection>
      )}
      <TagSection code={writeImageLine(image)} onCode={(code) => onChange(code === null ? null : (readImageLine(code) ?? image))} />
      <button className="danger compact inspector-remove" onClick={() => onChange(null)}>
        Remove the image
      </button>
    </>
  );
  return (
    <Surface label={standIn ? "Stand-in" : "Image"} className={`image-card ${align}${width ? " sized" : ""}${wraps ? "" : " nowrap"}`} inspector={inspector}>
      <figure className="image-card-figure" onClick={(e) => e.stopPropagation()}>
        <button
          className="image-card-picture"
          onClick={() => !standIn && setPicking("src")}
          title={standIn ? "What the others see in place of a hidden image" : "Choose another image"}
          style={width ? { width: Number(width), maxWidth: "100%" } : undefined}
        >
          {src.startsWith("img:") ? <WorldImage world={world.id} id={src} /> : <span className={`image-placeholder ${standIn ? "unknown" : ""}`}>{standIn ? "?" : "🖼"}</span>}
        </button>
        {standIn ? (
          <figcaption className="small muted">{attr(image, "caption") || "Hidden image"}</figcaption>
        ) : (
          <input className="image-card-caption" value={attr(image, "caption")} placeholder="Caption" onChange={(e) => set("caption", e.target.value)} />
        )}
      </figure>
      {picking && (
        <ImagePicker
          world={world.id}
          onChosen={(id) => {
            set(picking, id);
            setPicking(null);
          }}
          onRemove={picking === "hidden" && attr(image, "hidden") ? () => (set("hidden", ""), setPicking(null)) : undefined}
          onCancel={() => setPicking(null)}
        />
      )}
    </Surface>
  );
}
