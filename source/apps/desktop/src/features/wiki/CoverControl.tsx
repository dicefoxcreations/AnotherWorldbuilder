import type { Cover } from "../../api/client";

/**
 * A document's cover in the side bar, the GM's to set: the alias those who
 * may not open it know it by, and whether links lead them to it. Links
 * written meanwhile keep the alias, also once the document is revealed.
 */
export function CoverRows({ name, cover, onCover }: { name: string; cover?: Cover; onCover: (next: Cover | undefined) => void }) {
  const linkable = !!cover && cover.access.view !== "gm";
  // Neither an alias nor links: no cover at all.
  const set = (alias: string, links: boolean) =>
    onCover(alias.trim() || links ? { alias, access: { view: links ? "all" : "gm", edit: cover?.access.edit ?? "gm" } } : undefined);
  return (
    <div className="cover-rows">
      <label className="field">
        <span className="small muted">Alias</span>
        <input value={cover?.alias ?? ""} placeholder={name} onChange={(e) => set(e.target.value, linkable)} />
      </label>
      <label className="switch-row">
        <input type="checkbox" checked={linkable} onChange={(e) => set(cover?.alias ?? "", e.target.checked)} />
        <span>Linkable</span>
      </label>
    </div>
  );
}

/** What the head says of a cover — nothing where there is none. */
export const coverLabel = (cover?: Cover) => (!cover ? "" : cover.alias ? `Alias · ${cover.alias}` : "Linkable");
