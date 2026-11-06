import type { ReactNode } from "react";
import type { Template } from "../../api/client";
import { findType } from "../../shared/types";

/**
 * The icon a link to a document carries: its type's — unless the spot
 * already says what it is. A field of Character naming an NPC shows 👤;
 * a field of Rarity naming Rare shows nothing; in running text every link
 * shows its type.
 */
export function linkIcon(templates: Template[], articleType: string, context?: string): string | undefined {
  if (context && context === articleType) return undefined;
  return findType(templates, articleType)?.icon || undefined;
}

/** A link to a document, as read everywhere. */
export function DocLink({
  id,
  articleType,
  templates,
  context,
  anchor,
  onOpen,
  children,
}: {
  id: string;
  articleType: string;
  templates: Template[];
  /** The type the spot already names — a field's target, a list's type. */
  context?: string;
  /** A line of it (`^2f9e17`): resting on the link shows that part alone. */
  anchor?: string;
  onOpen: () => void;
  children: ReactNode;
}) {
  return (
    <button className="wikilink" data-preview={id} data-preview-anchor={anchor} data-icon={linkIcon(templates, articleType, context)} onClick={onOpen}>
      {children}
    </button>
  );
}
