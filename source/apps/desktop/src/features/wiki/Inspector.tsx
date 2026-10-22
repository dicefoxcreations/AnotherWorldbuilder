import { createContext, useContext, useId, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import type { BlockPart, TemplateBlock, WorldInfo } from "../../api/client";
import { attr, withAttr, type ImageAttrs } from "../../shared/ImageTag";
import { useSideBar } from "../../shared/ui/SideBar";
import { VisibilityPicker } from "../../shared/ui/Visibility";

/**
 * The inspector: what is chosen in the visual editor is set in the side
 * bar at the window's right edge — a part of the text, an image, a list,
 * a timeline, a calendar, a graph. Always in the same order: who may see
 * and change it on top (only for whoever may set that), then what it
 * shows, then its tag as written, then removing it. Nothing lies over the
 * text while one writes.
 */

/** The part of the text an element stands in — for the access on top. */
export type PartAccess = {
  world: WorldInfo;
  part: BlockPart;
  /** The section's title, to name the part by. */
  title: string;
  /** Nothing but one element stands in the part: its access is the part's. */
  alone: boolean;
  onPart: (p: Partial<BlockPart>) => void;
  /** Takes the whole part out — where there are others in its section. */
  onRemove?: () => void;
};

export const PartAccessContext = createContext<PartAccess | null>(null);

/** Moves an element past the paragraph or element above or below it — where there is one. */
export type PieceMove = { up?: () => void; down?: () => void };
export const PieceMoveContext = createContext<PieceMove | null>(null);

/** The side bar's panel: a head with what is chosen, and × to close it. */
export function Inspector({ title, onClose, children }: { title: ReactNode; onClose: () => void; children: ReactNode }) {
  return createPortal(
    <aside className="side-panel inspector" onMouseDown={(e) => e.stopPropagation()}>
      <div className="side-panel-head">
        <h3>{title}</h3>
        <button className="ghost icon round" title="Close" onClick={onClose}>
          ×
        </button>
      </div>
      {children}
    </aside>,
    document.body,
  );
}

/** A section of the inspector, with its small heading. */
export function InspectorSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="inspector-section">
      <h5>{title}</h5>
      {children}
    </section>
  );
}

const ACCESS = ["view", "edit", "level"] as const;

/** Who may see, who may change, how far it travels — three rows. */
export function AccessRows({
  world,
  value,
  onChange,
  readOnly,
}: {
  world: WorldInfo;
  value: { view: string; edit: string; level: string };
  onChange: (p: Partial<{ view: string; edit: string; level: string }>) => void;
  /** Only shown — set elsewhere. */
  readOnly?: boolean;
}) {
  return (
    <div className="inspector-access">
      <span className="small muted">View</span>
      <VisibilityPicker world={world} value={value.view} onChange={(view) => onChange({ view })} small readOnly={readOnly} />
      <span className="small muted">Change</span>
      <VisibilityPicker world={world} value={value.edit} onChange={(edit) => onChange({ edit })} small readOnly={readOnly} />
      <span className="small muted">Travels</span>
      <select className="level-picker" disabled={readOnly} value={value.level || "open"} onChange={(e) => onChange({ level: e.target.value })}>
        <option value="open">open</option>
        <option value="sealed">sealed</option>
        <option value="gm-only">GM only</option>
      </select>
    </div>
  );
}

/**
 * Who may see and change a section of this document. A section is a block
 * with one part or several — each part a layer with access of its own; one
 * part: its rows are the section's.
 */
export function SectionAccess({
  world,
  block,
  onPart,
}: {
  world: WorldInfo;
  block: TemplateBlock;
  onPart: (p: number, patch: Partial<{ view: string; edit: string; level: string }>) => void;
}) {
  if (!world.isGm || block.parts.length === 0) return null;
  return (
    <InspectorSection title="Access">
      {block.parts.map((part, p) => (
        <div key={part.key} className="section-access">
          {block.parts.length > 1 && <span className="small muted">Part {p + 1}</span>}
          <AccessRows world={world} value={part} onChange={(patch) => onPart(p, patch)} />
        </div>
      ))}
    </InspectorSection>
  );
}

/**
 * Who may see and change what is chosen — the game master's.
 *
 * An element with access of its own carries it on its tag, and becomes a
 * part of its own when saved. One that stands alone in its part has the
 * part's. One among text shares its part's — that changes everything in
 * the part; “Own access” gives it its own.
 */
export function AccessSection({ attrs, onAttrs }: { attrs?: ImageAttrs; onAttrs?: (next: ImageAttrs) => void }) {
  const at = useContext(PartAccessContext);
  if (!at || !at.world.isGm) return null;
  const { world, part, onPart } = at;
  const own = !!attrs && ACCESS.some((n) => attr(attrs, n) !== "");
  if (own && attrs && onAttrs) {
    const value = { view: attr(attrs, "view") || part.view, edit: attr(attrs, "edit") || part.edit, level: attr(attrs, "level") || "open" };
    return (
      <InspectorSection title="Access — its own">
        <AccessRows
          world={world}
          value={value}
          onChange={(p) => onAttrs(Object.entries({ ...value, ...p }).reduce((a, [n, v]) => withAttr(a, n, n === "level" && v === "open" ? "" : v), attrs))}
        />
        <button className="compact ghost" onClick={() => onAttrs(ACCESS.reduce((a, n) => withAttr(a, n, ""), attrs))}>
          Same as its part
        </button>
      </InspectorSection>
    );
  }
  // A part of a section with several: its own access, set here.
  if (!attrs) {
    return (
      <InspectorSection title="Access — this part">
        <AccessRows world={world} value={part} onChange={onPart} />
      </InspectorSection>
    );
  }
  // An element has the access of the section it stands in — set there, so
  // nothing of it here; only “Own access”, where it can have its own.
  if (at.alone || !attrs || !onAttrs) return null;
  return (
    <InspectorSection title="Access">
      <button
        className="compact"
        onClick={() => onAttrs(withAttr(withAttr(withAttr(attrs, "view", part.view), "edit", part.edit), "level", part.level === "open" ? "" : part.level))}
      >
        Own access
      </button>
    </InspectorSection>
  );
}

/** Its tag as written — to change by hand; Enter takes it. */
export function TagSection({ code, onCode }: { code: string; onCode: (next: string | null) => void }) {
  const [text, setText] = useState(code);
  const [was, setWas] = useState(code);
  if (code !== was) {
    setWas(code);
    setText(code);
  }
  return (
    <InspectorSection title="Tag">
      <textarea
        className="inspector-code"
        rows={Math.min(6, Math.ceil(text.length / 34) + 1)}
        value={text}
        spellCheck={false}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            onCode(text.trim() || null);
          } else if (e.key === "Escape") setText(code);
        }}
        onBlur={() => text !== code && onCode(text.trim() || null)}
      />
    </InspectorSection>
  );
}

/**
 * An element standing in the text, chosen by a click: outlined while chosen,
 * its inspector in the side bar. The first click only chooses it — links and
 * buttons in it work from the second on.
 */
export function Surface({
  label,
  className = "",
  inspector,
  children,
}: {
  label: string;
  className?: string;
  /** What the side bar shows while it is chosen. */
  inspector: ReactNode;
  children: ReactNode;
}) {
  const id = `surface:${useId()}`;
  const [chosen, setChosen] = useState(false);
  const move = useContext(PieceMoveContext);
  useSideBar(id, chosen, () => setChosen(false));
  return (
    <div
      className={`surface-card${chosen ? " chosen" : ""} ${className}`}
      data-label={label}
      onClickCapture={(e) => {
        if (chosen) return;
        e.preventDefault();
        e.stopPropagation();
        setChosen(true);
      }}
    >
      {children}
      {chosen && (
        <Inspector title={label} onClose={() => setChosen(false)}>
          {move && (
            <InspectorSection title="Order">
              <div className="segmented">
                <button disabled={!move.up} title="Above what stands before it" onClick={move.up}>
                  ↑ Up
                </button>
                <button disabled={!move.down} title="Below what follows it" onClick={move.down}>
                  ↓ Down
                </button>
              </div>
            </InspectorSection>
          )}
          {inspector}
        </Inspector>
      )}
    </div>
  );
}
