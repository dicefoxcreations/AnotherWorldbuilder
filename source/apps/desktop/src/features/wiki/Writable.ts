import type { TemplateField, Writable } from "../../api/client";

/** Whether a property may be changed: a field of the type by its key, any other as the free ones. */
export const mayChangeProperty = (w: Writable, field: TemplateField | undefined): boolean =>
  w.whole || (field ? w.fields.includes(field.key) : w.free);

/** Whether a part of a section may be changed. */
export const mayChangePart = (w: Writable, key: string): boolean => w.whole || w.parts.includes(key);
