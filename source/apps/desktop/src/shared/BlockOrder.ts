import type { ArticleView, BlockPlace, Template, TemplateBlock } from "../api/client";

/** Where a place stands when read: on top, in the text, beside it, below — as `document::place_rank`. */
export const PLACE_RANK: Record<BlockPlace, number> = { top: 0, flow: 1, side: 2, bottom: 3 };

/** A type's section as it looks in this document: its own look over the type's — see `document::Look`. */
export function lookOf(block: TemplateBlock, article: ArticleView | undefined): TemplateBlock {
  const look = article?.looks?.[block.key];
  return look ? { ...block, ...look } : block;
}

/**
 * Where each section stands among a document's, as `document::blocks_of`
 * orders them: in the order the document keeps, where that holds its type's
 * sections too — otherwise the type's first, its own after them in theirs.
 * One the order does not know stands last.
 */
export function blockRank(type: Template | undefined, article: ArticleView | undefined): (key: string) => number {
  const typed = (type?.blocks ?? []).map((b) => b.key);
  const order = article?.order ?? [];
  const list = order.some((k) => typed.includes(k)) ? order : [...typed, ...order];
  return (key) => {
    const i = list.indexOf(key);
    return i < 0 ? Infinity : i;
  };
}

/**
 * A document's blocks as they are read — its type's looking as they do in
 * it, all of them by place and in its order. As `document::blocks_of`.
 */
export function asRead(blocks: TemplateBlock[], type: Template | undefined, article: ArticleView): TemplateBlock[] {
  const rank = blockRank(type, article);
  return blocks
    .map((written) => {
      const typedBlock = type?.blocks.find((x) => x.key === written.key);
      if (!typedBlock) return written;
      const { title, style, place, fold, heading, width } = lookOf(typedBlock, article);
      return { ...written, title, style, place, fold, heading, width };
    })
    .map((block, b) => ({ block, b }))
    .sort((x, y) => PLACE_RANK[x.block.place ?? "flow"] - PLACE_RANK[y.block.place ?? "flow"] || rank(x.block.key) - rank(y.block.key) || x.b - y.b)
    .map(({ block }) => block);
}
