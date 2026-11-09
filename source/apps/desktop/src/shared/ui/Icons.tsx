import { DiceFoxIcon, type DiceFoxIconName } from "./dicefox-icons/DiceFoxIcon";

/**
 * Line icons, drawn — not emoji: an emoji keeps its own colours, these take
 * the button's. DiceFox "Feine Kontur" (library icons/another-worldbuilder/
 * fine/v2, copied unchanged into `dicefox-icons/`); at 16–20 px the set
 * draws its plainer optical variant by itself.
 */
export function Icon({ name, size = 18, title }: { name: DiceFoxIconName; size?: number; title?: string }) {
  return <DiceFoxIcon name={name} size={size} mode="mono" title={title} />;
}

/** A folder: open it, or choose one. */
export const FolderIcon = () => <Icon name="folder" />;

/** Into another folder. */
export const MoveIcon = () => <Icon name="move" />;

/** A shared folder — one a cloud app keeps in step. */
export const CloudIcon = () => <Icon name="cloud" />;

/** A server: worlds kept there. */
export const ServerIcon = () => <Icon name="server" />;

/** Shared over the local network. */
export const NetworkIcon = () => <Icon name="network" />;

/** Who views something. */
export const EyeIcon = () => <Icon name="eye" />;

/** Who changes something. */
export const PenIcon = () => <Icon name="edit" />;

/** Copy to the clipboard. */
export const CopyIcon = () => <Icon name="copy" />;

/** Done. */
export const CheckIcon = () => <Icon name="check" />;

/** Save as a file. */
export const DownloadIcon = () => <Icon name="download" />;

/** The profile: back it up. */
export const KeyIcon = () => <Icon name="key" />;

/** Over the whole working area. */
export const MaximizeIcon = () => <Icon name="maximize" />;

/** Back into the page. */
export const MinimizeIcon = () => <Icon name="minimize" />;
