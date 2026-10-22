import { useCallback, useEffect, useMemo, useRef, useState, type DragEvent, type ReactElement } from "react";
import { perLists } from "../../shared/Cache";
import { api, type ArticleView, type NavEntry, type Place, type Template, type WorldInfo } from "../../api/client";
import { documentOrder, findType, isA, pageOwner, parentField, parentIdOf, realTypes, ancestors, targetOf, typeOf } from "../../shared/types";
import { FoldChevron } from "../../shared/ui/BlockFold";
import { Inspector, InspectorSection } from "../wiki/Inspector";
import { useSideBar } from "../../shared/ui/SideBar";
import { SearchPick } from "../../shared/ui/SearchPick";

/**
 * The side bar on the left: the world's outline, as the GM sets it out —
 * types that unfold to their documents, single documents, groups of them.
 * Everyone sees the same entries in the same order; under a type each
 * reader sees the documents they can open. Folding is each device's own.
 *
 * The GM changes it here: ✎ makes the entries draggable, a click on one
 * opens its settings in the side bar on the right.
 */

const newId = () => (typeof crypto !== "undefined" && "randomUUID" in crypto ? crypto.randomUUID() : `${Date.now()}-${Math.random()}`).slice(0, 12);

/** Until the GM sets one out: the home page, then every type at the top. */
function outline(world: WorldInfo, templates: Template[]): NavEntry[] {
  const home: NavEntry[] = world.home ? [{ id: "home", kind: "document", target: world.home }] : [];
  return [...home, ...realTypes(templates).filter((t) => !t.supertype).map((t): NavEntry => ({ id: `type-${t.id}`, kind: "type", target: t.id }))];
}

/** A device's own memory of what is unfolded, per world. */
function useOpen(world: string): [ReadonlySet<string>, (key: string, open?: boolean) => void] {
  const key = `nav:${world}:open`;
  const [open, setOpen] = useState<ReadonlySet<string>>(() => {
    try {
      return new Set(JSON.parse(localStorage.getItem(key) ?? "[]") as string[]);
    } catch {
      return new Set();
    }
  });
  const toggle = useCallback(
    (id: string, to?: boolean) =>
      setOpen((was) => {
        const want = to ?? !was.has(id);
        if (want === was.has(id)) return was;
        const now = new Set(was);
        if (want) now.add(id);
        else now.delete(id);
        try {
          localStorage.setItem(key, JSON.stringify([...now]));
        } catch {
          // Not remembered — it folds all the same.
        }
        return now;
      }),
    [key],
  );
  return [open, toggle];
}

/** Every entry, walked depth first, with the groups above it. */
function* walk(entries: NavEntry[], above: NavEntry[] = []): Generator<[NavEntry, NavEntry[]]> {
  for (const e of entries) {
    yield [e, above];
    if (e.children) yield* walk(e.children, [...above, e]);
  }
}

/** The entries with `id` changed by `change` — or, giving `null`, taken out. */
function edit(entries: NavEntry[], id: string, change: (e: NavEntry) => NavEntry | null): NavEntry[] {
  return entries.flatMap((e) => {
    if (e.id === id) {
      const out = change(e);
      return out ? [out] : [];
    }
    return e.children ? [{ ...e, children: edit(e.children, id, change) }] : [e];
  });
}

type Drop = { id: string; where: "before" | "after" | "into" };

/** The entries with `moved` put before, after or into another. */
function move(entries: NavEntry[], moved: string, drop: Drop): NavEntry[] {
  let taken: NavEntry | null = null;
  const without = edit(entries, moved, (e) => {
    taken = e;
    return null;
  });
  if (!taken || moved === drop.id) return entries;
  const entry: NavEntry = taken;
  // Not into itself or what lies in it.
  for (const [e, above] of walk(without)) if (e.id === drop.id && above.some((a) => a.id === moved)) return entries;
  const place = (list: NavEntry[]): NavEntry[] =>
    list.flatMap((e) => {
      if (e.id === drop.id) {
        if (drop.where === "into") return [{ ...e, children: [...(e.children ?? []), entry] }];
        return drop.where === "before" ? [entry, e] : [e, entry];
      }
      return e.children ? [{ ...e, children: place(e.children) }] : [e];
    });
  return place(without);
}

/** How many documents a folder in the bar shows before the way to its list. */
const NAV_FIRST = 60;

/**
 * A list the bar shows, worked out once per list of documents — the bar is
 * drawn anew far more often than the documents change, see `shared/Cache`.
 */
const listsKnown = perLists((_a: ArticleView[], _t: Template[]) => new Map<string, ArticleView[]>());
function listed(articles: ArticleView[], templates: Template[], key: string, make: () => ArticleView[]): ArticleView[] {
  const known = listsKnown(articles, templates);
  let found = known.get(key);
  if (!found) known.set(key, (found = make()));
  return found;
}

export function NavBar({
  world,
  templates,
  articles,
  place,
  onPlace,
  editable,
  previewing,
  fresh,
  onError,
}: {
  world: WorldInfo;
  templates: Template[];
  articles: ArticleView[];
  place: Place;
  onPlace: (p: Place) => void;
  /** The GM, not previewing. */
  editable: boolean;
  /** The GM looks as a player would: what is only theirs stays away. */
  previewing: boolean;
  /** Counts up whenever the world changed — to load the entries again. */
  fresh: number;
  onError: (e: unknown) => void;
}) {
  const [saved, setSaved] = useState<NavEntry[] | null>(null);
  useEffect(() => {
    api.navigationLoad(world.id).then(setSaved).catch(onError);
  }, [world.id, fresh, onError]);
  const entries = useMemo(() => saved ?? outline(world, templates), [saved, world, templates]);
  const commit = (next: NavEntry[]) => {
    setSaved(next);
    api.navigationSet(world.id, next).catch((e) => {
      onError(e);
      api.navigationLoad(world.id).then(setSaved).catch(onError);
    });
  };

  const [open, toggle] = useOpen(world.id);
  const [editing, setEditing] = useState(false);
  const [chosen, setChosen] = useState<string | null>(null);
  useEffect(() => {
    if (!editable) setEditing(false);
  }, [editable]);
  useEffect(() => {
    if (!editing) setChosen(null);
  }, [editing]);
  const [query, setQuery] = useState("");
  const q = query.trim().toLowerCase();

  /** The documents an entry of a type holds: of it or below it, pages left out. */
  const docsOf = useCallback(
    (type: string) =>
      listed(articles, templates, `docs ${world.home} ${type}`, () =>
        articles
          .filter((a) => !a.pending && isA(templates, a.articleType, type) && !pageOwner(templates, world.home, a))
          .sort(documentOrder(templates, type, articles)),
      ),
    [articles, templates, world.home],
  );

  /**
   * Documents of other types that hang in this type's tree — a subclass
   * under its class: their parent field points at this type.
   */
  const hangingOf = useCallback(
    (type: string) => listed(articles, templates, `hanging ${world.home} ${type}`, () => {
      const kinds = realTypes(templates)
        .filter((t) => {
          if (isA(templates, t.id, type)) return false;
          const field = parentField(typeOf(templates, t.id));
          return !!field && isA(templates, targetOf(templates, t.id, field), type);
        })
        .map((t) => t.id);
      if (kinds.length === 0) return [];
      return articles
        .filter((a) => !a.pending && kinds.some((k) => isA(templates, a.articleType, k)) && !pageOwner(templates, world.home, a))
        .sort(documentOrder(templates, kinds[0], articles));
    }),
    [articles, templates, world.home],
  );

  /**
   * How closely a type's entry holds a document: 0 for its own type, one
   * more for each supertype between, further still for hanging below one of
   * it — `null` when it does not hold it at all.
   */
  const holding = (type: string, a: ArticleView): number | null => {
    if (a.articleType === type) return 0;
    const own = findType(templates, a.articleType);
    const up = own ? ancestors(templates, own).findIndex((t) => t.id === type) : -1;
    if (up >= 0) return up + 1;
    const seen = new Set<string>();
    let parent = parentIdOf(templates, a);
    while (parent && !seen.has(parent)) {
      seen.add(parent);
      const p = articles.find((x) => x.id === parent);
      if (!p) return null;
      if (isA(templates, p.articleType, type)) return 100;
      parent = parentIdOf(templates, p);
    }
    return null;
  };

  // Where one goes, the way there unfolds: its group, its type, its parents.
  useEffect(() => {
    if (place.kind !== "article") return;
    const a = articles.find((x) => x.id === place.id);
    if (!a) return;
    // The entry that holds it most closely: the document's own, else its
    // type's before a supertype's — a weapon under Weapons, not under Items.
    let best: { e: NavEntry; above: NavEntry[]; rank: number } | null = null;
    for (const [e, above] of walk(entries)) {
      const rank = e.kind === "document" ? (e.target === a.id ? -1 : null) : e.kind === "type" && e.target ? holding(e.target, a) : null;
      if (rank !== null && (!best || rank < best.rank)) best = { e, above, rank };
    }
    if (best) {
      const { e, above } = best;
      for (const g of above) toggle(g.id, true);
      if (e.kind === "type") {
        toggle(e.id, true);
        let parent = parentIdOf(templates, a);
        const seen = new Set<string>();
        while (parent && !seen.has(parent)) {
          seen.add(parent);
          toggle(`${e.id}/${parent}`, true);
          const p = articles.find((x) => x.id === parent);
          parent = p && parentIdOf(templates, p);
        }
      }
    }
    // Only on arriving somewhere — not whenever the documents load again.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [place]);

  const here = (p: Place) =>
    p.kind === place.kind && (p.kind === "types" || (p.kind === "list" && place.kind === "list" && p.articleType === place.articleType) || (p.kind === "article" && place.kind === "article" && p.id === place.id));
  /** A document's own place — the world's page is the world, a type's page its type. */
  const placeOf = (a: ArticleView): Place => {
    const owner = pageOwner(templates, world.home, a);
    if (owner?.kind === "world") return { kind: "types" };
    if (owner?.kind === "type") return { kind: "list", articleType: owner.type.id };
    return { kind: "article", id: a.id };
  };

  // --- Dragging, while editing -----------------------------------------
  const dragged = useRef<string | null>(null);
  const [drop, setDrop] = useState<Drop | null>(null);
  const dragProps = (e: NavEntry) =>
    editing
      ? {
          draggable: true,
          onDragStart: (ev: DragEvent) => {
            dragged.current = e.id;
            ev.dataTransfer.setData("text/x-nav", e.id);
            ev.dataTransfer.effectAllowed = "move";
          },
          onDragOver: (ev: DragEvent<HTMLElement>) => {
            if (!dragged.current) return;
            ev.preventDefault();
            const box = ev.currentTarget.getBoundingClientRect();
            const y = (ev.clientY - box.top) / box.height;
            // Into a group anything, into a type only a type: its subtypes.
            const moved = [...walk(entries)].find(([x]) => x.id === dragged.current)?.[0];
            const takes = e.kind === "group" || (e.kind === "type" && moved?.kind === "type");
            const where = takes && y > 0.3 && y < 0.7 ? "into" : y < 0.5 ? "before" : "after";
            if (drop?.id !== e.id || drop.where !== where) setDrop({ id: e.id, where });
          },
          onDragLeave: () => setDrop((d) => (d?.id === e.id ? null : d)),
          onDrop: (ev: DragEvent) => {
            ev.preventDefault();
            const from = dragged.current;
            dragged.current = null;
            setDrop(null);
            if (from && drop) commit(move(entries, from, drop));
          },
          onDragEnd: () => {
            dragged.current = null;
            setDrop(null);
          },
        }
      : {};

  // --- Drawing ---------------------------------------------------------
  const nameOf = (e: NavEntry) => {
    if (e.label) return e.label;
    if (e.kind === "type") return findType(templates, e.target ?? "")?.name ?? "???";
    if (e.kind === "document") return articles.find((a) => a.id === e.target)?.name ?? "???";
    return "Group";
  };
  const matches = (text: string) => !q || text.toLowerCase().includes(q);

  /** The documents under a type, as a tree where the type has one — with those hanging in it. */
  const docTree = (entry: NavEntry, docs: ArticleView[], hanging: ArticleView[], depth: number) => {
    const ids = new Set(docs.map((d) => d.id));
    // Only what hangs below a document here, however deep.
    const placed: ArticleView[] = [];
    for (let grew = true; grew; ) {
      grew = false;
      for (const h of hanging) {
        const p = parentIdOf(templates, h);
        if (!ids.has(h.id) && p && ids.has(p)) {
          ids.add(h.id);
          placed.push(h);
          grew = true;
        }
      }
    }
    const all = [...docs, ...placed];
    if (q) return capped(entry, all.filter((d) => matches(d.name)), depth, (d) => docRow(entry, d, [], depth));
    const below = new Map<string, ArticleView[]>();
    const roots: ArticleView[] = [];
    // Whose parent stands elsewhere is listed under it all the same — the
    // parent shown faintly: a player's people under the player.
    const outside = new Map<string, ArticleView[]>();
    for (const d of all) {
      const p = parentIdOf(templates, d);
      if (p && p !== d.id && ids.has(p)) below.set(p, [...(below.get(p) ?? []), d]);
      else if (p && p !== d.id && articles.some((a) => a.id === p)) outside.set(p, [...(outside.get(p) ?? []), d]);
      else roots.push(d);
    }
    const draw = (d: ArticleView, level: number, seen: Set<string>): ReactElement => {
      const kids = seen.has(d.id) ? [] : (below.get(d.id) ?? []);
      const inner = new Set(seen).add(d.id);
      return docRow(entry, d, kids.map((k) => draw(k, level + 1, inner)), level);
    };
    const context = [...outside.entries()]
      .map(([p, docs]) => [articles.find((a) => a.id === p)!, docs] as const)
      .sort(([a], [b]) => a.name.localeCompare(b.name))
      .map(([parent, docs]) => docRow(entry, parent, docs.map((d) => draw(d, depth + 1, new Set([parent.id]))), depth, true));
    return [...context, ...capped(entry, roots, depth, (d) => draw(d, depth, new Set()))];
  };

  /**
   * A long folder shows its first documents — and the one open now, wherever
   * it stands — and then the way to the type's own page, which lists them
   * all with a search. Thousands of rows in the bar would hold up the window.
   */
  const capped = (entry: NavEntry, docs: ArticleView[], depth: number, row: (d: ArticleView) => ReactElement): ReactElement[] => {
    if (docs.length <= NAV_FIRST) return docs.map(row);
    const first = docs.slice(0, NAV_FIRST);
    const now = docs.slice(NAV_FIRST).find((d) => here(placeOf(d)));
    const rest = docs.length - first.length - (now ? 1 : 0);
    const to: Place | null = entry.kind === "type" && entry.target ? { kind: "list", articleType: entry.target } : null;
    return [
      ...first.map(row),
      ...(now ? [row(now)] : []),
      <li key={`${entry.id}-more`}>
        <div className="nav-row nav-more" style={{ paddingLeft: depth * 14 }}>
          <span className="nav-fold" />
          <button className="nav-label" disabled={!to} onClick={() => to && onPlace(to)}>
            … {rest} more
          </button>
        </div>
      </li>,
    ];
  };

  /** A document's row — `context`: one that only stands above some of this entry's. */
  const docRow = (entry: NavEntry, d: ArticleView, kids: ReactElement[], depth: number, context = false) => {
    const key = `${entry.id}/${d.id}`;
    const unfolded = open.has(key);
    const p = placeOf(d);
    return (
      <li key={context ? `context-${d.id}` : d.id}>
        <div className={["nav-row", here(p) && !context ? "here" : "", context ? "context" : ""].filter(Boolean).join(" ")} style={{ paddingLeft: depth * 14 }}>
          {kids.length > 0 ? (
            <button className="nav-fold" aria-expanded={unfolded} onClick={() => toggle(key)}>
              <FoldChevron open={unfolded} />
            </button>
          ) : (
            <span className="nav-fold" />
          )}
          <button className="nav-label" onClick={() => onPlace(p)}>
            {d.name}
          </button>
        </div>
        {unfolded && kids.length > 0 && <ul>{kids}</ul>}
      </li>
    );
  };

  const entryRow = (e: NavEntry, depth: number): ReactElement | null => {
    if (previewing && e.gm) return null;
    const label = nameOf(e);
    const type = e.kind === "type" ? findType(templates, e.target ?? "") : undefined;
    const doc = e.kind === "document" ? articles.find((a) => a.id === e.target) : undefined;
    // What this reader cannot open does not stand there — only the GM,
    // setting it out, sees what is missing.
    if (!editing && ((e.kind === "type" && !type) || (e.kind === "document" && !doc))) return null;

    const icon = type?.icon ?? (doc && (doc.id === world.home ? world.icon : findType(templates, doc.articleType)?.icon));
    const docs = type ? docsOf(type.id) : [];
    const groupRows = (e.children ?? []).map((c) => entryRow(c, depth + 1)).filter((r) => r !== null);
    // A type's own documents: not those a type entry below it holds — in a group below it too.
    const elsewhere = [...walk(e.children ?? [])].filter(([c]) => c.kind === "type" && c.target).map(([c]) => c.target!);
    const own = (d: ArticleView) => !elsewhere.some((t) => isA(templates, d.articleType, t));
    const unfolded = !!q || open.has(e.id);
    // Only what is unfolded is worked out: thousands of rows a fold hides cost all the same.
    const docRows = !editing && type && unfolded ? docTree(e, docs.filter(own), hangingOf(type.id).filter(own), depth + 1) : [];
    if (q && !matches(label) && groupRows.length === 0 && docRows.length === 0) return null;
    if (!editing && e.kind === "group" && groupRows.length === 0) return null;

    const folds = e.kind === "group" || groupRows.length > 0 || (!editing && docs.length > 0);
    const p: Place | null = type ? { kind: "list", articleType: type.id } : doc ? placeOf(doc) : null;
    const pick = () => {
      if (editing) setChosen(e.id);
      else if (p) onPlace(p);
      else toggle(e.id);
    };
    const marks = [
      "nav-row",
      "nav-entry",
      e.kind === "group" ? "group" : "",
      p && here(p) ? "here" : "",
      editing && chosen === e.id ? "chosen" : "",
      drop?.id === e.id ? `drop-${drop.where}` : "",
      e.gm ? "gm" : "",
    ].filter(Boolean);
    return (
      <li key={e.id}>
        <div className={marks.join(" ")} style={{ paddingLeft: depth * 14 }} {...dragProps(e)}>
          {folds ? (
            <button className="nav-fold" aria-expanded={unfolded} onClick={() => toggle(e.id)}>
              <FoldChevron open={unfolded} />
            </button>
          ) : (
            <span className="nav-fold" />
          )}
          <button className="nav-label" onClick={pick} title={e.gm ? "Only for the GM" : undefined}>
            {icon && <span className="outline-icon">{icon}</span>}
            <span className="nav-name">{label}</span>
          </button>
          {type && !editing && <small className="nav-count">{docs.length}</small>}
        </div>
        {unfolded && (groupRows.length > 0 || docRows.length > 0) && (
          <ul>
            {groupRows}
            {docRows}
          </ul>
        )}
      </li>
    );
  };

  const add = (entry: NavEntry) => {
    commit([...entries, entry]);
    setChosen(entry.id);
  };
  const firstType = realTypes(templates).find((t) => !entries.some((e) => e.kind === "type" && e.target === t.id)) ?? realTypes(templates)[0];
  const chosenEntry = chosen ? [...walk(entries)].find(([e]) => e.id === chosen)?.[0] : undefined;

  return (
    <nav className={editing ? "nav-bar editing" : "nav-bar"} aria-label="Navigation">
      <div className="outline-head">
        <input className="nav-find" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Find" onKeyDown={(e) => e.key === "Escape" && setQuery("")} />
        {editable && (
          <button className={editing ? "ghost icon round on" : "ghost icon round"} title={editing ? "Done" : "Arrange"} onClick={() => setEditing((x) => !x)}>
            {editing ? "✓" : "✎"}
          </button>
        )}
      </div>
      <ul className="nav-tree">{entries.map((e) => entryRow(e, 0))}</ul>
      {editing && (
        <div className="nav-add">
          <button className="compact" disabled={!firstType} onClick={() => firstType && add({ id: newId(), kind: "type", target: firstType.id })}>
            ＋ Type
          </button>
          <button className="compact" disabled={articles.length === 0} onClick={() => add({ id: newId(), kind: "document", target: world.home || articles[0].id })}>
            ＋ Document
          </button>
          <button className="compact" onClick={() => add({ id: newId(), kind: "group", label: "Group" })}>
            ＋ Group
          </button>
        </div>
      )}
      {editing && chosenEntry && (
        <EntrySettings
          key={chosenEntry.id}
          entry={chosenEntry}
          templates={templates}
          articles={articles}
          fallback={chosenEntry.kind === "group" ? "" : nameOf({ ...chosenEntry, label: "" })}
          onChange={(next) => commit(edit(entries, chosenEntry.id, () => next))}
          onRemove={() => {
            commit(edit(entries, chosenEntry.id, () => null));
            setChosen(null);
          }}
          onClose={() => setChosen(null)}
        />
      )}
    </nav>
  );
}

/** An entry's settings, in the side bar on the right: who sees it, what it shows, its label. */
function EntrySettings({
  entry,
  templates,
  articles,
  fallback,
  onChange,
  onRemove,
  onClose,
}: {
  entry: NavEntry;
  templates: Template[];
  articles: ArticleView[];
  /** The name it carries without a label of its own. */
  fallback: string;
  onChange: (e: NavEntry) => void;
  onRemove: () => void;
  onClose: () => void;
}) {
  useSideBar("navigation", true, onClose);
  const [label, setLabel] = useState(entry.label ?? "");
  const keep = () => {
    const trimmed = label.trim();
    if (trimmed !== (entry.label ?? "") && (trimmed || entry.kind !== "group")) onChange({ ...entry, label: trimmed || undefined });
  };
  const types = realTypes(templates);
  const depth = (t: Template) => ancestors(templates, t).length;
  return (
    <Inspector title={entry.kind === "type" ? "Type" : entry.kind === "document" ? "Document" : "Group"} onClose={onClose}>
      <InspectorSection title="Access">
        <div className="segmented">
          <button className={entry.gm ? "" : "on"} onClick={() => onChange({ ...entry, gm: undefined })}>
            Everyone
          </button>
          <button className={entry.gm ? "on" : ""} onClick={() => onChange({ ...entry, gm: true })}>
            GM
          </button>
        </div>
      </InspectorSection>
      {entry.kind === "type" && (
        <InspectorSection title="Shows">
          <select value={entry.target} onChange={(e) => onChange({ ...entry, target: e.target.value })}>
            {types.map((t) => (
              <option key={t.id} value={t.id}>
                {"  ".repeat(depth(t))}
                {t.icon} {t.name}
              </option>
            ))}
          </select>
        </InspectorSection>
      )}
      {entry.kind === "document" && (
        <InspectorSection title="Shows">
          <SearchPick
            single
            items={articles.filter((a) => !a.pending).map((a) => ({ id: a.id, name: a.name, icon: findType(templates, a.articleType)?.icon }))}
            chosen={entry.target ? [entry.target] : []}
            onAdd={(id) => onChange({ ...entry, target: id })}
            onRemove={() => {}}
            placeholder="Document"
          />
        </InspectorSection>
      )}
      <InspectorSection title="Label">
        <input
          className="nav-label-field"
          value={label}
          placeholder={fallback}
          maxLength={80}
          onChange={(e) => setLabel(e.target.value)}
          onBlur={keep}
          onKeyDown={(e) => e.key === "Enter" && keep()}
        />
      </InspectorSection>
      <button className="danger inspector-remove" onClick={onRemove}>
        Remove
      </button>
    </Inspector>
  );
}
