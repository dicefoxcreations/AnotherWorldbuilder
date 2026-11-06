/**
 * NamePattern: how the name of a new article comes about.
 *
 * The template holds `<name>` or `<name="Session log %%count%%">`. There are
 * two placeholders: `%%input%%` is what the user types, `%%count%%` counts the
 * articles of this type up. A pattern without `%%input%%` needs no input any
 * more — the name is then already settled.
 *
 * The name is built here and only here: Rust merely stores it. The pattern
 * is checked in Rust, though, when the template is read — that is where a
 * `%%number%%` is noticed that would otherwise stand in the name literally.
 */

export const INPUT = "%%input%%";
export const COUNT = "%%count%%";

/** Whether this pattern still needs something from the user. */
export const needsInput = (pattern: string) => pattern.includes(INPUT);

/** Fills in input and counter. */
export function nameFrom(pattern: string, input: string, count: string): string {
  return pattern.split(INPUT).join(input).split(COUNT).join(count).trim();
}

/**
 * The next free number for `%%count%%`.
 *
 * What counts is not how many there are but which number is the highest:
 * whoever deletes “Session log 3” should not hand out the 3 a second time —
 * two articles of the same name would be indistinguishable in every list.
 * The number keeps the width of the highest one: after “Session 035” comes
 * “Session 036”.
 */
export function nextNumber(pattern: string, names: string[]): string {
  if (!pattern.includes(COUNT)) return "1";

  // The pattern becomes a search expression: literal stays literal, %%input%%
  // may be anything, %%count%% is the number sought.
  const raw = pattern
    .split(COUNT)
    .map((part) =>
      part
        .split(INPUT)
        .map((s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
        .join("[\\s\\S]*?"),
    )
    .join("(\\d+)");
  let expression: RegExp;
  try {
    expression = new RegExp(`^${raw}$`);
  } catch {
    return String(names.length + 1);
  }

  let highest = 0;
  let width = 1;
  for (const name of names) {
    const hit = expression.exec(name.trim());
    if (!hit) continue;
    for (const group of hit.slice(1)) {
      const number = Number.parseInt(group, 10);
      if (Number.isFinite(number) && number > highest) {
        highest = number;
        width = group.length;
      }
    }
  }
  return String(highest + 1).padStart(width, "0");
}

/** Fills in only the counter — and leaves spaces at the edges alone. */
export const fillCount = (text: string, count: string) => text.split(COUNT).join(count);

/**
 * What the pattern looks like in a preview.
 *
 * `%%input%%` becomes the word the user will type later — in the builder a
 * placeholder stands there so the frame around it stays visible.
 */
export const patternPreview = (pattern: string, placeholder = "…") =>
  nameFrom(pattern, placeholder, "1");
