import type { NavEntry, Template, WorldInfo } from "../api/client";
import { isA, parentField, realTypes, targetOf, typeOf } from "./types";

/**
 * Until the GM sets one out: the home page, then every type at the top —
 * but those kept out of the outline, which other documents only point to.
 */
export function defaultNavigation(world: Pick<WorldInfo, "home">, templates: Template[]): NavEntry[] {
  const home: NavEntry[] = world.home ? [{ id: "home", kind: "document", target: world.home }] : [];
  const types = realTypes(templates).filter((t) => !t.supertype && !t.navigationHidden);
  return [...home, ...types.map((t): NavEntry => ({ id: `type-${t.id}`, kind: "type", target: t.id }))];
}

/**
 * The types whose documents hang in `type`'s tree — a subclass under its
 * class: their parent field points at it. Not those the outline lists in a
 * place of their own, a player's people under "NPCs of the players": shown
 * there, they would stand twice.
 */
export function hangingKinds(templates: Template[], type: string, entries: NavEntry[]): string[] {
  const own = new Set<string>();
  const walk = (list: NavEntry[]) => list.forEach((e) => (e.kind === "type" && e.target && own.add(e.target), walk(e.children ?? [])));
  walk(entries);
  return realTypes(templates)
    .filter((t) => {
      if (isA(templates, t.id, type) || own.has(t.id)) return false;
      const field = parentField(typeOf(templates, t.id));
      return !!field && isA(templates, targetOf(templates, t.id, field), type);
    })
    .map((t) => t.id);
}
