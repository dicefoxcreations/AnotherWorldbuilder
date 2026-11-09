import { useState } from "react";
import type { OutlineEntry, TextHit } from "../../shared/documentText";
import { leadTo } from "../../shared/Anchors";
import { FoldChevron } from "../../shared/ui/BlockFold";
import { FIND_EVENT } from "../../shared/ui/QuickOpen";

/**
 * Opens a place the words were found: at its heading, if it has an anchor,
 * and the find bar on the words there — it does not scroll again where the
 * anchor already did.
 */
export function openHit(hit: TextHit | undefined, words: string, current: string | null, open: () => void) {
  const doc = hit?.article.id;
  if (doc && hit.anchor) leadTo(doc, hit.anchor, current, open);
  else open();
  if (hit && words) document.dispatchEvent(new CustomEvent(FIND_EVENT, { detail: { words, doc, anchor: hit.anchor, scroll: !hit.anchor } }));
}

/**
 * The headings of a document one can jump to, below its row in a tree or in
 * the side bar — each folds what is below it, starting folded.
 */
export function OutlineLinks({
  entries,
  onOpen,
  navigation = false,
  depth = 0,
}: {
  entries: OutlineEntry[];
  onOpen: (anchor: string) => void;
  /** In the side bar: its rows, indented by `depth`. */
  navigation?: boolean;
  depth?: number;
}) {
  const [opened, setOpened] = useState<ReadonlySet<string>>(new Set());
  const toggle = (anchor: string) =>
    setOpened((was) => {
      const now = new Set(was);
      if (!now.delete(anchor)) now.add(anchor);
      return now;
    });
  const fold = navigation ? "nav-fold" : "tree-fold";
  const draw = (entry: OutlineEntry, level: number) => {
    const open = opened.has(entry.anchor);
    const below = entry.children.length > 0;
    return (
      <li key={entry.anchor}>
        <div className={navigation ? "nav-row nav-section" : "tree-row document-section"} style={navigation ? { paddingLeft: level * 14 } : undefined}>
          {below ? (
            <button className={fold} aria-expanded={open} onClick={() => toggle(entry.anchor)}>
              <FoldChevron open={open} />
            </button>
          ) : (
            <span className={fold} />
          )}
          <button className={navigation ? "nav-label" : "wikilink"} onClick={() => onOpen(entry.anchor)}>
            {entry.label}
          </button>
        </div>
        {open && below && <ul>{entry.children.map((child) => draw(child, level + 1))}</ul>}
      </li>
    );
  };
  return <>{entries.map((entry) => draw(entry, depth))}</>;
}
