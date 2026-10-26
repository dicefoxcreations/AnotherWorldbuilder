/**
 * Line icons, drawn — not emoji: an emoji keeps its own colours, these take
 * the button's. Same size and stroke everywhere.
 */

function Icon({ d }: { d: string[] }) {
  return (
    <svg
      viewBox="0 0 24 24"
      width="16"
      height="16"
      aria-hidden="true"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      {d.map((p) => (
        <path key={p} d={p} />
      ))}
    </svg>
  );
}

/** A folder: open it, or choose one. */
export const FolderIcon = () => (
  <Icon
    d={[
      "M6 14l1.45-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.55 6a2 2 0 0 1-1.94 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.93a2 2 0 0 1 1.66.9l.82 1.2a2 2 0 0 0 1.66.9H18a2 2 0 0 1 2 2v2",
    ]}
  />
);

/** Into another folder. */
export const MoveIcon = () => (
  <Icon
    d={[
      "M2 9V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H20a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2v-1",
      "M2 13h10",
      "m9 16 3-3-3-3",
    ]}
  />
);

/** Take out of a list — nothing is deleted. */
export const RemoveIcon = () => <Icon d={["M18 6 6 18", "M6 6l12 12"]} />;

/** Add to a list. */
export const AddIcon = () => <Icon d={["M12 5v14", "M5 12h14"]} />;

/** A shared folder — one a cloud app keeps in step. */
export const CloudIcon = () => <Icon d={["M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z"]} />;

/** A server: worlds kept there. */
export const ServerIcon = () => (
  <Icon d={["M4 3h16a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1Z", "M4 13h16a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1v-6a1 1 0 0 1 1-1Z", "M7 7h.01", "M7 17h.01"]} />
);

/** Shared over the local network. */
export const NetworkIcon = () => (
  <Icon
    d={[
      "M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20",
      "M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20",
      "M2 12h20",
    ]}
  />
);

/** Who views something. */
export const EyeIcon = () => (
  <Icon d={["M2.06 12.35a1 1 0 0 1 0-.7 10.75 10.75 0 0 1 19.88 0 1 1 0 0 1 0 .7 10.75 10.75 0 0 1-19.88 0", "M9 12a3 3 0 1 0 6 0a3 3 0 1 0-6 0"]} />
);

/** Who changes something. */
export const PenIcon = () => (
  <Icon d={["M21.17 6.81a1 1 0 0 0-3.98-3.99L3.84 16.17a2 2 0 0 0-.5.83l-1.32 4.35a.5.5 0 0 0 .62.63l4.36-1.33a2 2 0 0 0 .83-.5z", "m15 5 4 4"]} />
);

/** Copy to the clipboard. */
export const CopyIcon = () => (
  <Icon d={["M10 8h10a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H10a2 2 0 0 1-2-2V10a2 2 0 0 1 2-2Z", "M4 16a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2"]} />
);

/** Done. */
export const CheckIcon = () => <Icon d={["M20 6 9 17l-5-5"]} />;

/** Save as a file. */
export const DownloadIcon = () => <Icon d={["M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4", "m7 10 5 5 5-5", "M12 15V3"]} />;

/** Keys: back them up. */
export const KeyIcon = () => <Icon d={["M2 15.5a5.5 5.5 0 1 0 11 0a5.5 5.5 0 1 0-11 0", "m21 2-9.6 9.6", "m15.5 7.5 3 3L22 7l-3-3"]} />;

/** Over the whole working area. */
export const MaximizeIcon = () => (
  <Icon d={["M8 3H5a2 2 0 0 0-2 2v3", "M21 8V5a2 2 0 0 0-2-2h-3", "M3 16v3a2 2 0 0 0 2 2h3", "M16 21h3a2 2 0 0 0 2-2v-3"]} />
);

/** Back into the page. */
export const MinimizeIcon = () => (
  <Icon d={["M8 3v3a2 2 0 0 1-2 2H3", "M21 8h-3a2 2 0 0 1-2-2V3", "M3 16h3a2 2 0 0 1 2 2v3", "M16 21v-3a2 2 0 0 1 2-2h3"]} />
);
