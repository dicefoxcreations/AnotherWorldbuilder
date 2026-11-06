import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { WorldImage } from "./WorldImage";

/**
 * An image one reads: a click shows it as large as the window allows.
 *
 * Reading only looks — choosing another image belongs to editing. A click
 * anywhere or Escape closes it again.
 */
export function ZoomImage({ world, id, alt = "" }: { world: string; id: string; alt?: string }) {
  const [open, setOpen] = useState(false);
  useEffect(() => {
    if (!open) return;
    const close = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [open]);
  return (
    <>
      <WorldImage world={world} id={id} alt={alt} className="zoomable" onClick={() => setOpen(true)} />
      {open &&
        createPortal(
          <div className="image-zoom" onClick={() => setOpen(false)} role="dialog" aria-label={alt || "Image"}>
            <WorldImage world={world} id={id} alt={alt} />
          </div>,
          document.body,
        )}
    </>
  );
}
