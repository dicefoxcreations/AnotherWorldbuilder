import {
  useContext,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import type { ArticleView, Template, WorldInfo } from "../../api/client";
import { CurrentDocument, useWorldData } from "../../features/worlds/WorldData";
import { EyeIcon, PenIcon } from "./Icons";

/**
 * Visibility as a clickable property.
 *
 * A policy is an OR over terms: `players OR class:rogue`. Every term appears
 * as a bubble with ×. ＋ offers the basics; below them a field finds the rest
 * by name as one types — a player, a type, a document. What it does not
 * find, Enter takes as written: `class:rogue AND faction:guild`.
 *
 * The game master holds every key: they see and change everything anyway.
 * So *Everyone*, *All players* and *GM only* exclude each other, and whoever
 * is added by name comes to the GM, who always stays (`effective`).
 */

import { BASICS, effective, OR } from "../Policies";

export { effective, samePolicy, terms } from "../Policies";

/** A type's id as part of an attribute, as the core writes it (`own_view`). */
export const typeAttribute = (id: string) =>
  `type:${id.replace(/[^A-Za-z0-9._-]/g, "-")}`;


/**
 * What a term is called for humans.
 *
 * `short` applies on the section frame: there the mark shares the line with
 * the title, and “GM only” alone eats half a section.
 */
export function label(
  term: string,
  world: WorldInfo,
  short = false,
  around?: {
    self: string | null;
    articles: ArticleView[];
    templates: Template[];
  },
): string {
  if (term === "all") return "Everyone";
  if (term === "players") return short ? "Players" : "All players";
  if (term === "gm") return short ? "GM" : "GM only";
  // `self`: whoever is assigned to the document — saved as its `doc:…`.
  if (term === "self") return "Assigned";
  if (term.startsWith("doc:")) {
    const id = `art:${term.slice(4)}`;
    if (around?.self === id) return "Assigned";
    const named = around?.articles.find((a) => a.id === id);
    if (named) return named.name;
  }
  if (term.startsWith("type:")) {
    const type = around?.templates.find((t) => typeAttribute(t.id) === term);
    if (type) return `All ${type.name}`;
  }
  const member = world.members.find(
    (m) => `player:${m.name.toLowerCase()}` === term.toLowerCase(),
  );
  if (member) return member.name;
  return term;
}

export function VisibilityPicker({
  world,
  value,
  onChange,
  small,
  readOnly,
}: {
  world: WorldInfo;
  value: string;
  onChange: (fresh: string) => void;
  small?: boolean;
  /** Only to see who it is for — changed while editing. */
  readOnly?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [at, setAt] = useState(0);
  const [position, setPosition] = useState({ left: 0, top: 0 });
  const ref = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const parts = effective(value);
  const data = useWorldData();
  const self = useContext(CurrentDocument);
  const around = {
    self,
    articles: data?.articles ?? [],
    templates: data?.templates ?? [],
  };
  const own = self ? `doc:${self.replace(/^art:/, "")}` : null;

  // The menu hangs on the document, not in the section: otherwise it would
  // stay behind neighbours, because the mark opens a stacking context of its
  // own.
  useLayoutEffect(() => {
    if (!open || !buttonRef.current) return;
    const box = buttonRef.current.getBoundingClientRect();
    const width = 250;
    setPosition({
      left: Math.min(Math.max(8, box.left), window.innerWidth - width - 8),
      top: Math.min(box.bottom + 6, window.innerHeight - 40),
    });
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const click = (e: MouseEvent) => {
      const inMenu = menuRef.current?.contains(e.target as Node);
      const atButton = ref.current?.contains(e.target as Node);
      if (!inMenu && !atButton) setOpen(false);
    };
    const key = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", click);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("mousedown", click);
      document.removeEventListener("keydown", key);
    };
  }, [open]);

  const set = (fresh: string[]) => onChange(fresh.join(OR));
  // A basic stands alone; the GM stays — taking the rest away leaves the GM.
  const remove = (t: string) => set(BASICS.includes(t) ? ["gm"] : parts.filter((x) => x !== t));
  const add = (t: string) => {
    if (t) set(BASICS.includes(t) ? [t] : [...parts.filter((x) => !BASICS.includes(x) || x === "gm"), t].filter((x, i, a) => a.indexOf(x) === i));
    setOpen(false);
    setQuery("");
    setAt(0);
  };

  // The basics, always at hand. “Assigned” only in a document: whoever is
  // assigned to it (Options → Players → Assign).
  const basics: { value: string; text: string; hint: string }[] = [
    { value: "all", text: "Everyone", hint: "Everyone in the world" },
    {
      value: "players",
      text: "All players",
      hint: "Every player, not only the GM",
    },
    { value: "gm", text: "GM only", hint: "Only the game master" },
    ...(self
      ? [
          {
            value: "self",
            text: "Assigned",
            hint: "Whoever is assigned to this document — Options → Players → Assign",
          },
        ]
      : []),
  ].filter(
    (o) =>
      !(parts.length === 1 && parts[0] === o.value) &&
      !(o.value === "self" && (parts.includes("self") || (own && parts.includes(own)))),
  );
  // The rest, found by name: players, types, documents, other attributes.
  const rest: { value: string; text: string; kind: string }[] = [
    ...world.members
      .filter((m) => m.role !== "gm")
      .map((m) => ({
        value: `player:${m.name.toLowerCase()}`,
        text: m.name,
        kind: "Player",
      })),
    ...around.templates
      .filter((t) => !t.group)
      .map((t) => ({
        value: typeAttribute(t.id),
        text: `All ${t.name}`,
        kind: "Type",
      })),
    ...world.attributes
      .filter(
        (a) => !["all", "players", "gm"].includes(a) && !a.startsWith("type:"),
      )
      .map((a) =>
        a.startsWith("doc:")
          ? { value: a, text: label(a, world, false, around), kind: "Document" }
          : { value: a, text: a, kind: "Attribute" },
      ),
  ].filter(
    (o, i, all) =>
      !parts.includes(o.value) &&
      all.findIndex((x) => x.value === o.value) === i,
  );
  const q = query.trim().toLowerCase();
  const found = q
    ? rest
        .filter(
          (o) => o.text.toLowerCase().includes(q) || o.kind.toLowerCase() === q,
        )
        .slice(0, 30)
    : [];

  return (
    <div
      className={small ? "view-picker small-picker" : "view-picker"}
      ref={ref}
    >
      <div className="bubbles">
        {parts.map((t) => (
          <span
            key={t}
            // The GM cannot be taken away.
            className={`bubble ${t === "gm" ? "gm" : t === "all" ? "all" : ""}${readOnly || t === "gm" ? " plain" : ""}`}
            title={label(t, world, false, around)}
          >
            {t === "gm" && parts.length > 1 ? "GM" : label(t, world, small, around)}
            {!readOnly && t !== "gm" && (
              <button
                className="bubble-x"
                onClick={() => remove(t)}
                title="Remove"
                aria-label={`Remove ${label(t, world, false, around)}`}
              >
                ×
              </button>
            )}
          </span>
        ))}
        {!readOnly && (
          <button
            ref={buttonRef}
            className="bubble plus"
            onClick={() => setOpen((o) => !o)}
            title="Add"
          >
            ＋
          </button>
        )}
      </div>

      {open &&
        createPortal(
          <div
            className="view-menu"
            ref={menuRef}
            style={{ left: position.left, top: position.top }}
          >
            {basics.map((o) => (
              <button
                key={o.value}
                className="ghost"
                title={o.hint}
                onClick={() => add(o.value)}
              >
                {o.text}
              </button>
            ))}
            <input
              className="view-search"
              autoFocus
              value={query}
              placeholder="Player, type, document …"
              onChange={(e) => {
                setQuery(e.target.value);
                setAt(0);
              }}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown")
                  setAt((n) => Math.min(n + 1, found.length - 1));
                else if (e.key === "ArrowUp") setAt((n) => Math.max(n - 1, 0));
                // Nothing found: as written — an attribute, or `a AND b`.
                else if (e.key === "Enter")
                  add(found[at]?.value ?? query.trim());
                else return;
                e.preventDefault();
              }}
            />
            {found.length > 0 && (
              <div className="view-found">
                {found.map((o, i) => (
                  <button
                    key={o.value}
                    className={`ghost${i === at ? " on" : ""}`}
                    onClick={() => add(o.value)}
                  >
                    {o.text}
                    <small>{o.kind}</small>
                  </button>
                ))}
              </div>
            )}
            {q && found.length === 0 && (
              <p className="small muted">Enter takes it as written.</p>
            )}
          </div>,
          document.body,
        )}
    </div>
  );
}


/**
 * Who views and who changes something — beside a document's or a section's
 * title, after a hairline. Reading, only to know whom to ask; editing, to
 * change it in place.
 */
export function AccessLine({
  world,
  view,
  edit,
  onView,
  onEdit,
}: {
  world: WorldInfo;
  /** Left out: not shown — as the document's. */
  view?: string;
  edit?: string;
  onView?: (fresh: string) => void;
  onEdit?: (fresh: string) => void;
}) {
  // Unencrypted, no view or right counts — nothing to show.
  if (!world.encrypted) return null;
  return (
    <span className="head-access">
      {view !== undefined && (
        <>
          <span className="access-sign" title="Who views it">
            <EyeIcon />
          </span>
          <VisibilityPicker world={world} value={view} onChange={onView ?? (() => {})} small readOnly={!onView} />
        </>
      )}
      {edit !== undefined && (
        <>
          <span className="access-sign" title="Who changes it">
            <PenIcon />
          </span>
          <VisibilityPicker world={world} value={edit} onChange={onEdit ?? (() => {})} small readOnly={!onEdit} />
        </>
      )}
    </span>
  );
}
