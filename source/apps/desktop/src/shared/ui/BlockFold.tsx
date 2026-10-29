import { createContext, useContext, useEffect, useState, type DragEvent, type ReactNode } from "react";
import type { TemplateBlock } from "../../api/client";
import { CreateButton } from "../../features/wiki/CreateButton";
import { anchorsIn, LEAD_EVENT, pendingAnchor } from "../Anchors";

/** What a block carries while it is dragged: its key. */
const DRAGGED = "application/x-aw-block";

/** Everything open, whatever the reader folded — where a part of a page is looked for. */
export const Unfolded = createContext(false);

/**
 * Whether a text holds the anchor asked for in its document — then it opens,
 * whatever the reader folded, until they fold it again. `source`: the text.
 */
function useAnchorOpens(article: string, source?: string): [boolean, () => void] {
  const holds = (anchor: string | null) => !!anchor && !!source && anchorsIn(source).some((mark) => mark.anchor === anchor);
  const asked = holds(pendingAnchor(article));
  const [opened, setOpened] = useState<string | null>(null);
  useEffect(() => {
    if (asked) setOpened(article);
    const lead = (e: Event) => {
      const { doc, anchor } = (e as CustomEvent<{ doc: string; anchor: string }>).detail;
      if (doc === article && holds(anchor)) setOpened(article);
    };
    document.addEventListener(LEAD_EVENT, lead);
    return () => document.removeEventListener(LEAD_EVENT, lead);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [article, source, asked]);
  return [asked || opened === article, () => setOpened(null)];
}

/**
 * Whether a block of a document is folded — for this reader, on this
 * device. The type says how it starts (`fold=folded`); what the reader did last
 * is remembered, and where nothing can be remembered it starts as the type
 * says.
 */
export function useFold(article: string, block: string, initial: boolean, source?: string): [boolean, () => void] {
  const open = useContext(Unfolded);
  const [revealed, unreveal] = useAnchorOpens(article, source);
  // What the reader did counts until the type says otherwise.
  const key = `fold:${article}:${block}:${initial ? "folded" : "open"}`;
  const [folded, setFolded] = useState(() => {
    try {
      const kept = localStorage.getItem(key);
      return kept === null ? initial : kept === "1";
    } catch {
      return initial;
    }
  });
  const toggle = () => {
    unreveal();
    setFolded((was) => {
      // Opened for an anchor: it was open, the reader folds it.
      if (revealed) was = false;
      try {
        localStorage.setItem(key, was ? "0" : "1");
      } catch {
        // Not remembered — it folds all the same.
      }
      return !was;
    });
  };
  return [folded && !open && !revealed, toggle];
}

/**
 * Which headings of a block's text are folded — for this reader, on this
 * device, as `useFold` remembers a block. A heading starts open.
 */
export function useFoldedHeadings(article: string, block: string, source?: string): [ReadonlySet<string>, (head: string) => void] {
  const open = useContext(Unfolded);
  const [revealed, unreveal] = useAnchorOpens(article, source);
  const key = `fold:${article}:${block}:headings`;
  const [folded, setFolded] = useState<ReadonlySet<string>>(() => {
    try {
      return new Set(JSON.parse(localStorage.getItem(key) ?? "[]") as string[]);
    } catch {
      return new Set();
    }
  });
  const toggle = (head: string) => {
    unreveal();
    setFolded((was) => {
      const now = new Set(revealed ? [] : was);
      if (!now.delete(head)) now.add(head);
      try {
        localStorage.setItem(key, JSON.stringify([...now]));
      } catch {
        // Not remembered — it folds all the same.
      }
      return now;
    });
  };
  return [open || revealed ? NONE : folded, toggle];
}

const NONE: ReadonlySet<string> = new Set();

/**
 * The arrow before what folds — a block's title, a heading in the text. It
 * stands in the margin, so the title stands where the text does.
 */
export function FoldChevron({ open }: { open: boolean }) {
  return (
    <svg className={`head-chevron${open ? " open" : ""}`} viewBox="0 0 16 16" aria-hidden>
      <path d="M6 3.5 10.5 8 6 12.5" />
    </svg>
  );
}

/**
 * The column at the side of a page: the blocks placed `side`, beside the
 * text, staying in view while the page scrolls. Not there without them.
 */
export function SideColumn({ children }: { children: ReactNode }) {
  const has = (Array.isArray(children) ? children.flat(2) : [children]).some((c) => c !== null && c !== undefined && c !== false);
  return has ? <aside className="side-column">{children}</aside> : null;
}

/**
 * A block of a document — read or written alike. Its title is a heading,
 * as large as its type says (`heading=1` to `4`). Its title folds it, unless
 * its type says `fold=off`; `fold=folded` starts it folded. One placed `top`
 * or `bottom` stands above or below the infobox and the text, across the
 * page; one at the `side` in a column of its own, and folds away sideways.
 *
 * While writing (`onPick`), a click on the title chooses the block — its
 * settings open in the side bar — and the arrow before it folds it. A block
 * without a title shows a mark to choose it by, while the pointer is over it.
 * With `onMove`, the title can be dragged onto another block: dropped on its
 * upper half it stands before it, on the lower half after it.
 */
export function FoldBlock({
  article,
  block,
  onPick,
  picked,
  onMove,
  access,
  children,
  source,
}: {
  article: string;
  block: TemplateBlock;
  /** Who views and changes it — right of the title, after a hairline. */
  access?: ReactNode;
  onPick?: () => void;
  picked?: boolean;
  /** Another block was dropped here — `key` goes before or after this one. */
  onMove?: (key: string, after: boolean) => void;
  children: ReactNode;
  /** Its text: it opens when an anchor in it is asked for. */
  source?: string;
}) {
  const fold = block.fold ?? "open";
  const [folded, toggle] = useFold(article, block.key, fold === "folded", source);
  const folds = !!block.title && fold !== "off";
  const shut = folds && folded;
  const sign = folds && <FoldChevron open={!shut} />;
  const place = block.place === "top" || block.place === "bottom" || block.place === "side" ? ` at-${block.place}` : "";
  // At the side it stands as wide as it says — folded, a strip with its title.
  const width = block.place === "side" && block.width && !shut ? { width: block.width } : undefined;
  const [drop, setDrop] = useState<"before" | "after" | null>(null);
  // Caught on the way down, so the text inside does not take the block as
  // something dropped into it.
  const side = (e: DragEvent<HTMLElement>) => {
    const box = e.currentTarget.getBoundingClientRect();
    return e.clientY > box.top + box.height / 2 ? "after" : "before";
  };
  const over = (e: DragEvent<HTMLElement>) => {
    if (!onMove || !e.dataTransfer.types.includes(DRAGGED)) return;
    e.preventDefault();
    e.stopPropagation();
    setDrop(side(e));
  };
  const dropped = (e: DragEvent<HTMLElement>) => {
    if (!onMove || !e.dataTransfer.types.includes(DRAGGED)) return;
    e.preventDefault();
    e.stopPropagation();
    const key = e.dataTransfer.getData(DRAGGED);
    setDrop(null);
    if (key && key !== block.key) onMove(key, side(e) === "after");
  };
  return (
    <section
      className={`block ${block.style}${place}${shut ? " folded" : ""}${picked ? " picked" : ""}${drop ? ` drop-${drop}` : ""}`}
      style={width}
      onDragOverCapture={over}
      onDragLeave={(e) => !e.currentTarget.contains(e.relatedTarget as Node | null) && setDrop(null)}
      onDropCapture={dropped}
    >
      {(block.title || onPick) && (
        <div className={`block-mark head-${block.heading ?? 1}${!block.title ? " untitled" : ""}`}>
          {onPick ? (
            <>
              {folds && (
                <button className="head-fold" aria-expanded={!shut} title={shut ? "Open" : "Fold"} onClick={toggle}>
                  {sign}
                </button>
              )}
              <button
                className="block-title block-fold"
                onClick={onPick}
                draggable={!!onMove}
                title={onMove ? "Settings — drag to move" : undefined}
                onDragStart={(e) => {
                  e.dataTransfer.setData(DRAGGED, block.key);
                  e.dataTransfer.effectAllowed = "move";
                }}
              >
                {block.title || "Block"}
              </button>
            </>
          ) : folds ? (
            <button className="block-title block-fold" aria-expanded={!shut} onClick={toggle}>
              {sign}
              {block.title}
            </button>
          ) : (
            <span className="block-title">{block.title}</span>
          )}
          {/* Beside its title: a button that makes documents (`new=`). */}
          {!!block.new?.length && <CreateButton of={block.new} label="＋" still={!!onPick} small />}
          {access && (
            <>
              <span className="meta-line" />
              {access}
            </>
          )}
        </div>
      )}
      {!shut && children}
    </section>
  );
}
