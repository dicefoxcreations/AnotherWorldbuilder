/**
 * Comparing names as the language does — each collator made once:
 * `localeCompare` builds one on every call, and sorting thousands of names
 * calls it hundreds of thousands of times.
 */
export const byText = new Intl.Collator().compare;
/** `Session 9` before `Session 10`. */
export const byNumber = new Intl.Collator(undefined, { numeric: true }).compare;
