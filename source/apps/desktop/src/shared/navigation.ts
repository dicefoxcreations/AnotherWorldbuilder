import type { NavEntry, Template, WorldInfo } from "../api/client";
import { realTypes } from "./types";

/**
 * Until the GM sets one out: the home page, then every type at the top —
 * but those kept out of the outline, which other documents only point to.
 */
export function defaultNavigation(world: Pick<WorldInfo, "home">, templates: Template[]): NavEntry[] {
  const home: NavEntry[] = world.home ? [{ id: "home", kind: "document", target: world.home }] : [];
  const types = realTypes(templates).filter((t) => !t.supertype && !t.navigationHidden);
  return [...home, ...types.map((t): NavEntry => ({ id: `type-${t.id}`, kind: "type", target: t.id }))];
}
