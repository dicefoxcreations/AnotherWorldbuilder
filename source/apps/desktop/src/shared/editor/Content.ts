import { rebase } from "./Merge";
import type { DocumentText } from "../../api/client";

/**
 * Whether two readings of a document say the same — ignoring how the code
 * is laid out, and parts with nothing in them yet, which are not stored.
 */
export function sameContent(a: DocumentText, b: DocumentText): boolean {
  const meaning = (d: DocumentText) =>
    JSON.stringify({
      name: d.name.trim(),
      access: d.access,
      properties: d.properties
        .filter((p) => p.value.trim())
        .map((p) => [p.name.toLowerCase(), p.value.trim()])
        .sort(),
      blocks: d.blocks.flatMap((block) =>
        block.parts
          .filter((p) => p.text.trim())
          .map((p) => [block.title, block.style, p.text.trim(), p.view, p.edit]),
      ),
    });
  return meaning(a) === meaning(b);
}

/**
 * The saved reading, with the text of each part as it was typed where it
 * says the same: a space or an empty line at the end, which the store does
 * not keep, stays while one goes on typing.
 */
export function keepTyped(saved: DocumentText, typed: DocumentText | null): DocumentText {
  if (!typed || typed.blocks.length !== saved.blocks.length) return saved;
  return {
    ...saved,
    blocks: saved.blocks.map((block, i) => {
      const was = typed.blocks[i].parts;
      if (was.length !== block.parts.length) return block;
      return { ...block, parts: block.parts.map((p, j) => (was[j].text.trim() === p.text.trim() ? { ...p, text: was[j].text } : p)) };
    }),
  };
}

/**
 * What came from others (`theirs`) with what one typed since `base` on top
 * — part by part, property by property. None where one's own changes
 * reach beyond the parts the others' reading has (a part added here): then
 * the code is merged instead.
 */
export function mergeTyped(base: DocumentText, ours: DocumentText, theirs: DocumentText): DocumentText | null {
  const parts = (d: DocumentText) => new Map(d.blocks.flatMap((b) => b.parts.map((p) => [p.key, p.text] as const)));
  const [was, mine, other] = [parts(base), parts(ours), parts(theirs)];
  for (const [key, text] of mine) if (!other.has(key) && text !== was.get(key)) return null;
  const props = (d: DocumentText) => new Map(d.properties.map((p) => [p.name, p.value] as const));
  const [pWas, pMine] = [props(base), props(ours)];
  for (const [name, value] of pMine) if (!theirs.properties.some((p) => p.name === name) && value !== pWas.get(name)) return null;
  const merged = (b: string | undefined, o: string | undefined, t: string) => (b === undefined || o === undefined ? t : rebase(b, o, t));
  return {
    ...theirs,
    name: rebase(base.name, ours.name, theirs.name),
    properties: theirs.properties.map((p) => ({ ...p, value: merged(pWas.get(p.name), pMine.get(p.name), p.value) })),
    blocks: theirs.blocks.map((block) => ({
      ...block,
      parts: block.parts.map((p) => ({ ...p, text: merged(was.get(p.key), mine.get(p.key), p.text) })),
    })),
  };
}
