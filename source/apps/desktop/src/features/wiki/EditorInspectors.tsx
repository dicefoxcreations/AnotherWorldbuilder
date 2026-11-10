import { useMemo, useState } from "react";
import { type ArticleView } from "../../api/client";
import { linker, linkIn, linkText, linkTo, MARK, typeNames } from "../../shared/Links";
import { anchorName } from "../../shared/Anchors";
import { AccessRows, Inspector, InspectorSection } from "./Inspector";
import type { WorldInfo } from "../../api/client";
import { findType, isA, ancestors } from "../../shared/types";
import { type Links, namedLike } from "./EditorTools";
import { byText } from "../../shared/Collate";

/** What a section chosen by its anchor can be given: a new name, or none. */
export type SectionActions = {
  rename: (old: string, fresh: string) => Promise<void>;
  remove: (anchor: string) => void;
  /** How many other documents link to it. */
  linkedElsewhere: (anchor: string) => number;
  /** The anchors the document has. */
  taken: () => Set<string>;
};

/** A section in the side bar: its name to change, and taking it out. */
export function SectionInspector({ anchor, actions, onRenamed, onClose }: { anchor: string; actions: SectionActions; onRenamed: (fresh: string) => void; onClose: () => void }) {
  const [name, setName] = useState(anchor);
  const [problem, setProblem] = useState("");
  const [busy, setBusy] = useState(false);
  const elsewhere = actions.linkedElsewhere(anchor);
  const rename = async () => {
    const fresh = anchorName(name);
    if (!fresh || fresh === anchor) return setName(anchor);
    if (actions.taken().has(fresh)) return setProblem(`“${fresh}” is taken in this document.`);
    setBusy(true);
    setProblem("");
    try {
      await actions.rename(anchor, fresh);
      onRenamed(fresh);
    } catch (e) {
      setProblem(String(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Inspector title="Section" onClose={onClose}>
      <InspectorSection title="Name">
        <input
          value={name}
          disabled={busy}
          onChange={(e) => setName(e.target.value)}
          onBlur={() => void rename()}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
        />
        <small className="small muted">
          ^{anchorName(name) || anchor}
          {elsewhere > 0 && ` · linked from ${elsewhere} other ${elsewhere === 1 ? "document" : "documents"} — renamed there too`}
        </small>
        {problem && <small className="small error">{problem}</small>}
      </InspectorSection>
      <button
        className="danger compact inspector-remove"
        disabled={busy}
        title={elsewhere > 0 ? "Its links then lead to the document only" : undefined}
        onClick={() => {
          actions.remove(anchor);
          onClose();
        }}
      >
        Remove the section
      </button>
    </Inspector>
  );
}

/** Where what belongs to a heading ends: at the next heading as high or higher, or at the end. */
export function sectionEnd(text: string, start: number, level: number): number {
  const after = text.indexOf("\n", start);
  if (after < 0) return text.length;
  const next = new RegExp(`^#{1,${level}} `, "m").exec(text.slice(after + 1));
  return next ? after + 1 + next.index : text.length;
}

/** The heading the caret stands in, at `start` of `text`: its level and its words — or none. */
export function headingAt(text: string, start: number): { level: number; words: string } | null {
  const m = /^(#{1,6}) (.*)$/m.exec(text.slice(start, text.indexOf("\n", start) < 0 ? undefined : text.indexOf("\n", start)));
  return m ? { level: m[1].length, words: m[2].trim() } : null;
}

/**
 * A heading in the side bar: how high it stands — or plain text again. The
 * marks one would type for it (`## `) are not shown in the visual editor;
 * here they are set.
 */
export function HeadingInspector({
  world,
  level,
  words,
  onLevel,
  access,
  onAccess,
  onOwnAccess,
  onJoinAbove,
  onClose,
}: {
  world: WorldInfo;
  level: number;
  words: string;
  onLevel: (level: number) => void;
  /** Its section's access, where it is a part of its own — none: it shares its part's. */
  access: { view: string; edit: string } | null;
  onAccess: (patch: Partial<{ view: string; edit: string }>) => void;
  /** Makes the heading and what belongs to it a part of its own. */
  onOwnAccess?: () => void;
  /** Gives the section back to the text above it. */
  onJoinAbove?: () => void;
  onClose: () => void;
}) {
  return (
    <Inspector title={words ? `Heading · ${words}` : "Heading"} onClose={onClose}>
      {access ? (
        <InspectorSection title="Access — this section">
          <AccessRows world={world} value={access} viewOnly={!world.isGm} onChange={onAccess} />
          {onJoinAbove && (
            <button className="compact ghost" onClick={onJoinAbove}>
              Same access as above
            </button>
          )}
        </InspectorSection>
      ) : (
        onOwnAccess && (
          <InspectorSection title="Access">
            <button className="compact" onClick={onOwnAccess} title="This heading and what belongs to it, to the next heading as high">
              Own access from here
            </button>
          </InspectorSection>
        )
      )}
      <InspectorSection title="Level">
        <div className="segmented">
          {[1, 2, 3, 4, 5, 6].map((n) => (
            <button key={n} className={level === n ? "on" : ""} title={`As ${"#".repeat(n)} in the code`} onClick={() => onLevel(n)}>
              H{n}
            </button>
          ))}
        </div>
      </InspectorSection>
      <button className="compact inspector-remove" onClick={() => onLevel(0)}>
        Plain text again
      </button>
    </Inspector>
  );
}

/**
 * A link in the side bar: where it leads — another document found by its
 * name — and the text it shows in its place, empty for the document's own
 * name. Taken out, its text stays as plain words.
 */
export function LinkInspector({ raw, links, onChange, onClose }: { raw: string; links: Links; onChange: (raw: string) => void; onClose: () => void }) {
  const link = linkIn(raw.slice(MARK.length, -MARK.length));
  const resolve = useMemo(() => linker(links.all, links.templates), [links.all, links.templates]);
  const target = (link && resolve(link)) || null;
  const own = link && link.text !== link.name ? link.text : "";
  const [text, setText] = useState(own);
  const [find, setFind] = useState("");
  // Of which kind — empty: any. The kinds there are documents of, and those above them.
  const [only, setOnly] = useState("");
  const kinds = useMemo(() => {
    const ids = new Set<string>();
    for (const a of links.all) {
      const t = findType(links.templates, a.articleType);
      if (t) [t, ...ancestors(links.templates, t)].forEach((x) => ids.add(x.id));
    }
    return [...ids].flatMap((id) => findType(links.templates, id) ?? []).sort((a, b) => byText(a.name, b.name));
  }, [links.all, links.templates]);
  const among = only ? links.all.filter((a) => isA(links.templates, a.articleType, only)) : links.all;
  const found = find.trim() ? namedLike(among, find, links.selfId) : [];
  if (!link) return null;
  const write = (to: ArticleView | null, shown: string) =>
    to ? linkTo(links.templates, to, shown, link.property) : linkText(link.of, link.name, shown, link.property);
  const shownName = own || target?.name || link.name;
  return (
    <Inspector title="Link" onClose={onClose}>
      <InspectorSection title="Leads to">
        <span className={target ? "link-target" : "link-target muted"}>
          {target ? target.name : `${link.name} — not found`}
          {target && <span className="small muted"> · {typeNames(links.templates, target.articleType)[0]}</span>}
        </span>
        <div className="link-find">
          <input value={find} placeholder="Another document" onChange={(e) => setFind(e.target.value)} onKeyDown={(e) => e.key === "Enter" && found[0] && (onChange(write(found[0], text)), setFind(""))} />
          <select value={only} onChange={(e) => setOnly(e.target.value)} title="Of which kind">
            <option value="">All</option>
            {kinds.map((t) => (
              <option key={t.id} value={t.id}>
                {t.name}
              </option>
            ))}
          </select>
        </div>
        {found.map((a) => (
          <button key={a.id} className="ghost link-choice" onClick={() => (onChange(write(a, text)), setFind(""))}>
            <span>{a.name}</span>
            <span className="small muted">{typeNames(links.templates, a.articleType)[0]}</span>
          </button>
        ))}
        {find.trim() && found.length === 0 && <small className="small muted">Nothing found</small>}
      </InspectorSection>
      <InspectorSection title="Text">
        <input
          value={text}
          placeholder={target?.name ?? link.name}
          onChange={(e) => setText(e.target.value)}
          onBlur={() => text.trim() !== own && onChange(write(target, text))}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
        />
      </InspectorSection>
      <button className="danger compact inspector-remove" onClick={() => onChange(shownName)}>
        Remove the link
      </button>
    </Inspector>
  );
}
