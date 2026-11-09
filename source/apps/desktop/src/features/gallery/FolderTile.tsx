import { useState } from "react";
import { Icon } from "../../shared/ui/Icons";

/** A folder among the images: a click opens it, an image dropped on it goes in. */
export function FolderTile({
  name,
  count,
  onOpen,
  onDropImage,
}: {
  name: string;
  count: number;
  onOpen: () => void;
  onDropImage?: (image: string) => void;
}) {
  const [over, setOver] = useState(false);
  return (
    <button
      className={`gallery-tile folder-tile${over ? " over" : ""}`}
      onClick={onOpen}
      title={name}
      onDragOver={(e) => {
        if (!onDropImage) return;
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(e) => {
        e.preventDefault();
        setOver(false);
        const image = e.dataTransfer.getData("text/x-image");
        if (image) onDropImage?.(image);
      }}
    >
      <span className="folder-sign" aria-hidden>
        <Icon name="folder" />
      </span>
      <span className="image-name">{name}</span>
      <span className="tile-mark">{count}</span>
    </button>
  );
}
