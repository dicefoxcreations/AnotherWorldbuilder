import type { Access, Template } from "../api/client";

/**
 * Changing access without doubling the text.
 *
 * The template's article has an access of its own — in the text, `view=`
 * and `edit=` on `<article>`. Whoever changes the article's permission means
 * this one. So it is changed, and everything that said nothing of its own so
 * far keeps following it: otherwise every click would leave one more
 * `view=` in the text that says the same thing again.
 */

export const OPEN: Access = { view: "all", edit: "all" };

/** Whether this access restricts nothing at all — then no bracket is needed. */
export const isOpen = (r: Access) =>
  r.view === "all" && r.edit === "all";

export const sameAccess = (a: Access, b: Access) =>
  a.view === b.view && a.edit === b.edit;

/** The template's bracket — without a statement of its own: everyone may do everything. */
export const bracket = (template: Template): Access => template.auth ?? OPEN;

/**
 * Changes the bracket around the whole article.
 *
 * Everything that matched it exactly before still matches it afterwards —
 * it said nothing of its own, after all. What differs stays untouched.
 */
export function articleAccess(template: Template, part: Partial<Access>): Template {
  const old = bracket(template);
  const fresh = { ...old, ...part };
  const follows = (r: Access) => sameAccess(r, old);

  return {
    ...template,
    auth: isOpen(fresh) ? null : fresh,
    blocks: template.blocks.map((b) => ({
      ...b,
      parts: b.parts.map((t) => (follows(t) ? { ...t, ...fresh } : t)),
    })),
  };
}
