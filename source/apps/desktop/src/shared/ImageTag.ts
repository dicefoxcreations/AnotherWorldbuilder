/**
 * An `<image …>` on a line of its own, as the visual side changes it: its
 * attributes in order, so writing it back changes only what was changed.
 * The same attributes as Rust reads (`markdown.rs`, `document.rs`).
 */
export type ImageAttrs = [string, string][];

/** The attributes of a line holding nothing but a `<name …>` tag, or `null` if the line is none. */
export function readTagLine(name: string, line: string): ImageAttrs | null {
  const m = new RegExp(`^\\s*<${name}(\\s[^<>]*)?>\\s*$`).exec(line);
  if (!m) return null;
  return [...(m[1] ?? "").matchAll(/([\w-]+)(?:=(?:"([^"]*)"|([^\s>]+)))?/g)].map((a) => [a[1].toLowerCase(), a[2] ?? a[3] ?? ""]);
}

/** The attributes of an image line, or `null` if the line is none. */
export const readImageLine = (line: string) => readTagLine("image", line);

export const attr = (a: ImageAttrs, name: string) => a.find(([n]) => n === name)?.[1] ?? "";

/** With `name` set — in its place, or at the end — or taken out when empty. */
export function withAttr(a: ImageAttrs, name: string, value: string): ImageAttrs {
  if (!value) return a.filter(([n]) => n !== name);
  return a.some(([n]) => n === name) ? a.map(([n, v]) => [n, n === name ? value : v]) : [...a, [name, value]];
}

// Unquoted only what Markdown still reads as a tag: no `"'=<>` or backtick.
const quoted = (v: string) => (v !== "" && /^[^\s"'=<>`]+$/.test(v) ? v : `"${v.replace(/"/g, "'")}"`);

export function writeImageLine(a: ImageAttrs): string {
  return `<image${a.map(([n, v]) => (v === "" && n !== "caption" ? ` ${n}` : ` ${n}=${quoted(v)}`)).join("")}>`;
}

/** A `<name …>` line, written back from its attributes. */
export function writeTagLine(name: string, a: ImageAttrs): string {
  return `<${name}${a.map(([n, v]) => (v === "" ? ` ${n}` : ` ${n}=${quoted(v)}`)).join("")}>`;
}

export const writeListLine = (a: ImageAttrs) => writeTagLine("list", a);

/** An image as it goes into a text — with what the gallery says of it: its access and stand-in. */
export function imageTag(id: string, image?: { defaults?: { view?: string; edit?: string; hidden?: string; hiddenCaption?: string } }): string {
  const q = (v: string) => (/^[^\s"<>]+$/.test(v) ? v : `"${v.replace(/"/g, "'")}"`);
  const d = image?.defaults;
  let tag = `<image src=${id} align=right caption="«»"`;
  if (d?.view) tag += ` view=${q(d.view)} edit=${q(d.edit || "gm")}`;
  if (d?.view && d.hidden) tag += ` hidden=${d.hidden}`;
  if (d?.view && d.hiddenCaption) tag += ` hidden-caption=${q(d.hiddenCaption)}`;
  return tag + ">";
}
