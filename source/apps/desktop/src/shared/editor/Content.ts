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
          .map((p) => [block.title, block.style, p.text.trim(), p.view, p.edit, p.level]),
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
