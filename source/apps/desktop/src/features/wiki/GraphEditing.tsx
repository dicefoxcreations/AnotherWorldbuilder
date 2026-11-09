import { Icon } from "../../shared/ui/Icons";
import { useInWindow } from "../../shared/ui/PopupMenu";
import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { type ArticleView, type GraphLine, type GraphNode, type GraphPerson, type Template } from "../../api/client";
import { findType, isA, articleById } from "../../shared/types";
import { findRole, isRelation, roleChoices, type GraphEdge, type RelationField } from "../../shared/Relations";
import { ARROWS, COLOURS, SHAPES, SIDES } from "../../shared/GraphText";
import { readSide } from "./GraphLines";
import { type Connector, type Menu, type Pair } from "./RelationGraph";

/**
 * The right-click menu of the surface: on a card, its card and taking it
 * out; on a line, its settings and removing it; on free ground, someone to
 * add — one who exists, or a new one.
 */
export function GraphMenu({
  menu,
  templates,
  kinds,
  articles,
  inGraph,
  centre,
  canPlace,
  onAdd,
  onCreate,
  onCard,
  onRemove,
  onLines,
  onRemoveLines,
  onClose,
}: {
  menu: Menu;
  templates: Template[];
  kinds: Template[];
  articles: ArticleView[];
  inGraph: Set<string>;
  centre: string;
  canPlace: boolean;
  onAdd: (id: string) => void;
  onCreate: (typeId: string, name: string) => void;
  onCard: (id: string) => void;
  onRemove: (id: string) => void;
  onLines: (l: NonNullable<Menu["lines"]>) => void;
  onRemoveLines: (l: NonNullable<Menu["lines"]>) => void;
  onClose: () => void;
}) {
  const [naming, setNaming] = useState<Template | null>(null);
  const [name, setName] = useState("");
  const [query, setQuery] = useState("");
  // Of which kind — empty: all of them.
  const [only, setOnly] = useState("");
  const box = useInWindow(menu.x, menu.y);
  const typeName = (id: string) => kinds.find((t) => t.id === id)?.name ?? "";
  // Who may still join, once something is typed: what begins with the
  // text first, then what holds it.
  const q = query.trim().toLowerCase();
  const found = !q
    ? []
    : articles
        .filter((a) => kinds.some((t) => t.id === a.articleType) && (!only || isA(templates, a.articleType, only)))
        .filter((a) => !inGraph.has(a.id) && a.name.toLowerCase().includes(q))
        .sort(
          (a, b) =>
            Number(!a.name.toLowerCase().startsWith(q)) - Number(!b.name.toLowerCase().startsWith(q)) ||
            a.name.localeCompare(b.name, undefined, { numeric: true }),
        )
        .slice(0, 40);
  useEffect(() => {
    const close = () => onClose();
    const key = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", key);
    };
  }, [onClose]);
  const pick = (f: () => void) => () => {
    f();
    onClose();
  };
  return createPortal(
    <div
      ref={box}
      className="brick-menu graph-menu"
      style={{ left: menu.x, top: menu.y }}
      onMouseDown={(e) => e.stopPropagation()}
      onContextMenu={(e) => e.preventDefault()}
    >
      {menu.lines ? (
        <div className="brick-group">
          <h5>{menu.lines.edges.length > 1 ? `${menu.lines.edges.length} relations` : "Relation"}</h5>
          <button className="ghost" onClick={pick(() => onLines(menu.lines!))}>
            Settings
          </button>
          <button className="ghost danger" onClick={pick(() => onRemoveLines(menu.lines!))}>
            Remove
          </button>
        </div>
      ) : menu.person ? (
        <div className="brick-group">
          <h5>{articleById(articles, menu.person)?.name}</h5>
          <button className="ghost" onClick={pick(() => onCard(menu.person!))}>
            Card
          </button>
          <button
            className="ghost danger"
            disabled={menu.person === centre}
            title="Takes them out, with their relations to everyone in this tree"
            onClick={pick(() => onRemove(menu.person!))}
          >
            Remove from tree
          </button>
        </div>
      ) : naming ? (
        <NewPerson
          kinds={kinds}
          type={naming}
          onType={setNaming}
          name={name}
          onName={setName}
          onCreate={() => {
            onCreate(naming.id, name);
            onClose();
          }}
        />
      ) : !canPlace ? (
        <div className="brick-group">
          <p className="small muted">Edit the document to add someone here.</p>
        </div>
      ) : (
        <>
          <div className="graph-menu-find">
            <input
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Find"
              onKeyDown={(e) => {
                if (e.key === "Enter" && found[0]) {
                  onAdd(found[0].id);
                  onClose();
                }
              }}
            />
            {kinds.length > 1 && (
              <select value={only} onChange={(e) => setOnly(e.target.value)} title="Of which kind">
                <option value="">All</option>
                {kinds.map((t) => (
                  <option key={t.id} value={t.id}>
                    {t.name}
                  </option>
                ))}
              </select>
            )}
          </div>
          {found.length > 0 && (
            <div className="brick-group">
              {found.map((a) => (
                <button key={a.id} className="ghost graph-hit" onClick={pick(() => onAdd(a.id))}>
                  <span className="graph-hit-name">{a.name}</span>
                  {!only && kinds.length > 1 && <span className="small muted">{typeName(a.articleType)}</span>}
                </button>
              ))}
            </div>
          )}
          {q && found.length === 0 && <p className="small muted">Nobody found.</p>}
          {/* A name typed into the search is the new one's name. */}
          <button
            className="ghost"
            onClick={() => {
              setName(query.trim());
              setNaming(kinds.find((t) => t.id === only) ?? kinds[0]);
            }}
          >
            <Icon name="add" /> New
          </button>
        </>
      )}
    </div>,
    document.body,
  );
}

function NewPerson({
  kinds,
  type,
  onType,
  name,
  onName,
  onCreate,
}: {
  kinds: Template[];
  type: Template;
  onType: (t: Template) => void;
  name: string;
  onName: (n: string) => void;
  onCreate: () => void;
}) {
  return (
    <form
      className="brick-group new-person"
      onSubmit={(e) => {
        e.preventDefault();
        if (name.trim()) onCreate();
      }}
    >
      <h5>New</h5>
      {kinds.length > 1 && (
        <select value={type.id} onChange={(e) => onType(kinds.find((t) => t.id === e.target.value) ?? type)}>
          {kinds.map((t) => (
            <option key={t.id} value={t.id}>
              {t.name}
            </option>
          ))}
        </select>
      )}
      <input autoFocus value={name} onChange={(e) => onName(e.target.value)} placeholder="Name" />
      <button className="primary compact" type="submit" disabled={!name.trim()}>
        Add
      </button>
    </form>
  );
}

/** The side bar — where versions stand — holding what was clicked. */
export function SidePanel({
  title,
  error,
  onClose,
  children,
}: {
  title: string;
  error: string;
  onClose: () => void;
  children: React.ReactNode;
}) {
  // Straight into the page's body: the side bar stands at the window's
  // right edge, whatever the graph sits in.
  return createPortal(
    <aside className="side-panel relation-panel">
      <div className="side-panel-head">
        <h3>{title}</h3>
        <button className="ghost icon round" title="Close" onClick={onClose}>
          <Icon name="close" />
        </button>
      </div>
      {children}
      {error && <p className="graph-error small">{error}</p>}
    </aside>,
    document.body,
  );
}

/** Colour swatches — `auto` and the named colours. */
function Swatches({ value, onPick, auto }: { value: string; onPick: (c: string) => void; auto: string }) {
  return (
    <div className="swatches" role="group" aria-label="Colour">
      <button className={value ? "swatch-button auto" : "swatch-button auto on"} title={auto} onClick={() => onPick("")}>
        auto
      </button>
      {COLOURS.map((c) => (
        <button key={c} className={`swatch-button rel-${c}${value === c ? " on" : ""}`} title={c} onClick={() => onPick(c)} />
      ))}
    </div>
  );
}

/** One card: its frame, and what stands under the name. */
export function CardSettings({
  card,
  hasMiddle,
  onCard,
  onOpen,
  onRemove,
}: {
  card: GraphPerson | undefined;
  hasMiddle: boolean;
  onCard?: (patch: Partial<GraphPerson>) => void;
  onOpen: () => void;
  onRemove?: () => void;
}) {
  const [note, setNote] = useState(card?.note ?? "");
  const sub = card?.sub ?? "";
  return (
    <div className="card-settings">
      {onCard && (
        <>
          <div>
            <h5>Frame</h5>
            <Swatches value={card?.color ?? ""} onPick={(color) => onCard({ color })} auto="As usual" />
          </div>
          <label>
            <span>Under the name</span>
            <select value={sub} onChange={(e) => onCard({ sub: e.target.value })}>
              <option value="">As the graph says</option>
              <option value="type">Type</option>
              {hasMiddle && <option value="role">Role to the middle</option>}
              <option value="note">Own text</option>
              <option value="none">Nothing</option>
            </select>
          </label>
          {sub === "note" && (
            <input
              value={note}
              placeholder="Text under the name"
              onChange={(e) => setNote(e.target.value)}
              onBlur={() => note !== (card?.note ?? "") && onCard({ note })}
              onKeyDown={(e) => e.key === "Enter" && onCard({ note })}
            />
          )}
        </>
      )}
      <button className="compact" onClick={onOpen}>
        Open page
      </button>
      {onRemove && (
        <button className="compact danger" onClick={onRemove} title="Takes them out, with their relations to everyone in this tree">
          Remove from tree
        </button>
      )}
    </div>
  );
}

/** The roles a relation may have: those of its property's role type. */
function rolesFor(templates: Template[], articles: ArticleView[], e: GraphEdge) {
  const field = templates.flatMap((t) => t.properties).find((f): f is RelationField => isRelation(f) && f.key === e.property);
  return field ? roleChoices(templates, articles, field) : [];
}

/** One relation: both roles, and how its line is drawn. */
export function RelationCard({
  edge: e,
  line,
  articles,
  templates,
  onRoles,
  onLine,
  onRemove,
}: {
  edge: GraphEdge;
  line: GraphLine;
  articles: ArticleView[];
  templates: Template[];
  /** A relation's roles set — the one that `was` so; none was so: one more. */
  onRoles: (atA: string, atB: string, was: Pair) => void;
  onLine?: (patch: Partial<GraphLine>) => void;
  /** The relation that `was` so removed — without, every one between them. */
  onRemove: (was?: Pair) => void;
}) {
  const name = (id: string) => articleById(articles, id)?.name ?? "???";
  const roles = rolesFor(templates, articles, e);
  const pairs: Pair[] = e.pairs.length > 0 ? e.pairs : [["", ""]];
  const pick = (pair: Pair, end: 0 | 1) => (
    <label>
      <span>{name(end === 0 ? e.a : e.b)} is</span>
      <select
        value={findRole(roles, pair[end])?.id ?? ""}
        onChange={(x) => (end === 0 ? onRoles(x.target.value, pair[1], pair) : onRoles(pair[0], x.target.value, pair))}
      >
        <option value="">—</option>
        {roles.map((r) => (
          <option key={r.id} value={r.id}>
            {r.name}
          </option>
        ))}
      </select>
    </label>
  );
  return (
    <section className="relation-card">
      <div className="relation-card-head">
        <strong>
          {name(e.a)} – {name(e.b)}
        </strong>
        <button className="bubble-x" title={pairs.length > 1 ? "Remove these relations" : "Remove the relation"} onClick={() => onRemove()}>
          <Icon name="close" />
        </button>
      </div>
      {pairs.map((pair) => (
        <div key={pair.join("|")} className={pairs.length > 1 ? "relation-pair several" : "relation-pair"}>
          {pairs.length > 1 && (
            <button className="bubble-x" title="Remove this relation" onClick={() => onRemove(pair)}>
              <Icon name="close" />
            </button>
          )}
          {pick(pair, 0)}
          {pick(pair, 1)}
        </div>
      ))}
      {!pairs.some((p) => !p[0] && !p[1]) && (
        <button className="compact" onClick={() => onRoles("", "", ["", ""])}>
          <Icon name="add" /> Another relation
        </button>
      )}
      {onLine && <LineSettings line={line} sides={[true, true]} onLine={onLine} />}
    </section>
  );
}

/**
 * A line between ends that may be lines, by end: for each person there what
 * they are to those at the other end — one choice sets it towards all of
 * them — and for an end of several, what its label says.
 */
export function ConnectorCard({
  connector: c,
  written,
  line,
  articles,
  templates,
  onRoles,
  onLine,
  onSeparate,
  onRemove,
}: {
  connector: Connector;
  /** The ends in the order the line is written — `group` and `say` go by it. */
  written: [string[], string[]];
  line: GraphLine;
  articles: ArticleView[];
  templates: Template[];
  onRoles: (list: { edge: GraphEdge; atA: string; atB: string; was: Pair }[]) => void;
  onLine?: (patch: Partial<GraphLine>) => void;
  onSeparate: () => void;
  onRemove: () => void;
}) {
  const name = (id: string) => articleById(articles, id)?.name ?? "???";
  const names = (end: string[]) => end.map(name).join(" & ");
  const roles = c.covered.length ? rolesFor(templates, articles, c.covered[0]) : [];
  const row = (x: string) => {
    const mine = c.covered.filter((e) => e.a === x || e.b === x);
    // Each line by its first relation; more of them are set apart.
    const first = (e: GraphEdge): Pair => e.pairs[0] ?? ["", ""];
    const now = [...new Set(mine.map((e) => first(e)[e.a === x ? 0 : 1]))];
    // One choice for all — or `mixed`, where they differ.
    const value = now.length === 1 ? (findRole(roles, now[0])?.id ?? "") : "mixed";
    return (
      <label key={x}>
        <span>{name(x)}</span>
        <select
          value={value}
          onChange={(ev) => {
            const role = ev.target.value;
            if (role === "mixed") return;
            onRoles(
              mine.map((e) => ({
                edge: e,
                atA: e.a === x ? role : first(e)[0],
                atB: e.b === x ? role : first(e)[1],
                was: first(e),
              })),
            );
          }}
        >
          {value === "mixed" && <option value="mixed">(differs)</option>}
          <option value="">—</option>
          {roles.map((r) => (
            <option key={r.id} value={r.id}>
              {r.name}
            </option>
          ))}
        </select>
      </label>
    );
  };
  return (
    <section className="relation-card">
      <div className="relation-card-head">
        <strong>
          {names(c.ends[0])} – {names(c.ends[1])}
        </strong>
        <button className="bubble-x" title="Remove these relations" onClick={onRemove}>
          <Icon name="close" />
        </button>
      </div>
      {c.ends.map((end, k) => {
        // Its place as written: the ends here are as they stand.
        const i = written.findIndex((w) => w.length === end.length && w.every((x) => end.includes(x))) as 0 | 1;
        return (
          <div key={end.join("+")} className="connector-end">
            <h5>
              {names(end)} <span className="muted">to {names(c.ends[1 - k])}</span>
            </h5>
            {end.map(row)}
            {onLine && end.length > 1 && <GroupLabel line={line} end={i} onLine={onLine} />}
          </div>
        );
      })}
      {onLine && <LineSettings line={line} sides={[c.ends[0].length === 1, c.ends[1].length === 1]} onLine={onLine} />}
      <button className="compact" onClick={onSeparate} title="Each relation drawn as a line of its own">
        Separate
      </button>
    </section>
  );
}

/**
 * What the label of an end of several says: each one's role — `Son / Son` —
 * the roles once — `Son` — or a word of one's own: `Sons`.
 */
function GroupLabel({ line, end, onLine }: { line: GraphLine; end: 0 | 1; onLine: (patch: Partial<GraphLine>) => void }) {
  const said = line.say[end] ?? "";
  const [own, setOwn] = useState(said !== "");
  const [text, setText] = useState(said);
  const mode = own || said ? "own" : line.group[end] === "each" ? "each" : "";
  const set = (group: string, say: string) => {
    const pair = (list: string[], v: string) => {
      const next = [list[0] ?? "", list[1] ?? ""];
      next[end] = v;
      return next.every((x) => !x) ? [] : next;
    };
    onLine({ group: pair(line.group, group), say: pair(line.say, say.replace(/,/g, " ").trim()) });
  };
  return (
    <div className="group-label">
      <span className="small muted">Label</span>
      <div className="segmented" role="group" aria-label="Label">
        {[
          ["each", "Separate"],
          ["", "Combined"],
          ["own", "Own text"],
        ].map(([v, title]) => (
          <button
            key={v}
            className={mode === v ? "on" : ""}
            onClick={() => {
              setOwn(v === "own");
              if (v !== "own") set(v, "");
            }}
          >
            {title}
          </button>
        ))}
      </div>
      {mode === "own" && (
        <input
          value={text}
          autoFocus={!said}
          onChange={(e) => setText(e.target.value)}
          onBlur={() => text.trim() !== said && set("", text)}
          onKeyDown={(e) => e.key === "Enter" && set("", text)}
        />
      )}
    </div>
  );
}

/** How a line is drawn: colour, shape, arrowheads, where it leaves each card, where its roles stand. */
function LineSettings({
  line,
  sides: canSide,
  onLine,
}: {
  line: GraphLine;
  /** Which ends are cards — only those have a side. */
  sides: [boolean, boolean];
  onLine: (patch: Partial<GraphLine>) => void;
}) {
  const [first, second] = line.between;
  const side = (i: 0 | 1) => {
    const { side: s } = readSide(line.sides[i] || "x");
    return (
      <label>
        <span>{line.between[i]}</span>
        <select
          value={SIDES.includes(s as (typeof SIDES)[number]) ? line.sides[i] : ""}
          onChange={(x) => {
            const sides = [line.sides[0] ?? "", line.sides[1] ?? ""];
            sides[i] = x.target.value;
            onLine({ sides });
          }}
        >
          <option value="">auto</option>
          {SIDES.map((v) => (
            <option key={v} value={v}>
              {v}
            </option>
          ))}
          {line.sides[i] && !SIDES.includes(line.sides[i] as (typeof SIDES)[number]) && <option value={line.sides[i]}>{line.sides[i]} %</option>}
        </select>
      </label>
    );
  };
  const label = line.label === "none" ? "none" : line.label ? "set" : "";
  const arrowNames: Record<(typeof ARROWS)[number], string> = {
    end: `→ ${second}`,
    start: `→ ${first}`,
    both: "both ends",
  };
  return (
    <div className="line-settings">
      <h5>Line</h5>
      <Swatches value={line.color} onPick={(color) => onLine({ color })} auto="By the roles" />
      <div className="segmented" role="group" aria-label="Shape">
        {["", ...SHAPES].map((s) => (
          <button key={s} className={(line.shape ?? "") === s ? "on" : ""} onClick={() => onLine({ shape: s })}>
            {s || "auto"}
          </button>
        ))}
      </div>
      <label>
        <span>Arrowheads</span>
        <select value={line.arrow} onChange={(x) => onLine({ arrow: x.target.value })}>
          <option value="">none</option>
          {ARROWS.map((a) => (
            <option key={a} value={a}>
              {arrowNames[a]}
            </option>
          ))}
        </select>
      </label>
      {(canSide[0] || canSide[1]) && (
        <div className="line-sides">
          {canSide[0] ? side(0) : <span />}
          {canSide[1] ? side(1) : <span />}
        </div>
      )}
      <p className="small muted">Choose the line, then drag its ends. Delete removes it.</p>
      <label>
        <span>Roles</span>
        <select
          value={label}
          onChange={(x) => onLine({ label: x.target.value === "none" ? "none" : x.target.value === "set" ? line.label || "35" : "" })}
        >
          <option value="">beside each end</option>
          <option value="set">further along</option>
          <option value="none">hidden</option>
        </select>
      </label>
      {label === "set" && (
        <input type="range" min={5} max={50} step={5} value={Number(line.label)} onChange={(x) => onLine({ label: x.target.value })} />
      )}
    </div>
  );
}

/**
 * The graph as a whole: someone to add, what cards and lines show, and
 * whether it fills itself.
 */
export function GraphSettings({
  query,
  templates,
  articles,
  kinds,
  inGraph,
  pageIsPerson,
  onChange,
  onAdd,
  onCreate,
  onRelayout,
  onRemove,
}: {
  query: GraphNode;
  templates: Template[];
  articles: ArticleView[];
  kinds: Template[];
  inGraph: Set<string>;
  /** Whether the page the graph stands on is a person — then “this page” can be the middle. */
  pageIsPerson: boolean;
  onChange: (patch: Partial<GraphNode>) => void;
  onAdd: (id: string) => void;
  onCreate: (typeId: string, name: string) => void;
  onRelayout: () => void;
  onRemove?: () => void;
}) {
  const [naming, setNaming] = useState<Template | null>(null);
  const [name, setName] = useState("");
  const people = articles.filter((a) => kinds.some((t) => t.id === a.articleType)).sort((a, b) => a.name.localeCompare(b.name));
  const addable = people.filter((a) => !inGraph.has(a.id));
  const relations = [...new Map(templates.flatMap((t) => t.properties.filter(isRelation)).map((f) => [f.label, f])).values()];

  // How it fills itself, in one choice.
  const mode = query.around ? "around" : query.of ? "type" : "";
  const middle =
    query.around === "self" ? "self" : (articles.find((a) => a.id === query.around || a.name === query.around)?.name ?? "");
  const shown = (what: string) => query.show.length === 0 || query.show.includes(what);
  const flip = (what: string) => {
    const all = ["portrait", "type", "roles"];
    const now = all.filter(shown);
    const next = now.includes(what) ? now.filter((w) => w !== what) : [...now, what];
    onChange({ show: next.length === all.length ? [] : next });
  };
  return (
    <div className="graph-settings">
      <section>
        <h5>Add someone</h5>
        {naming ? (
          <NewPerson
            kinds={[naming]}
            type={naming}
            onType={setNaming}
            name={name}
            onName={setName}
            onCreate={() => {
              onCreate(naming.id, name);
              setNaming(null);
              setName("");
            }}
          />
        ) : (
          <select
            value=""
            onChange={(e) => {
              const v = e.target.value;
              if (v.startsWith("new:")) setNaming(kinds.find((k) => k.id === v.slice(4)) ?? null);
              else if (v) onAdd(v);
            }}
          >
            <option value="">＋ Person</option>
            {addable.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
            {kinds.map((t) => (
              <option key={t.id} value={`new:${t.id}`}>
                New {t.name} …
              </option>
            ))}
          </select>
        )}
      </section>

      <section>
        <h5>Show</h5>
        <div className="chips">
          <button className={shown("portrait") ? "chip on" : "chip"} onClick={() => flip("portrait")}>
            Portraits
          </button>
          <button className={shown("type") ? "chip on" : "chip"} onClick={() => flip("type")}>
            Types under names
          </button>
          <button className={shown("roles") ? "chip on" : "chip"} onClick={() => flip("roles")}>
            Roles at lines
          </button>
        </div>
      </section>

      <section>
        <h5>Fill in by itself</h5>
        <select
          value={mode}
          onChange={(e) => {
            const v = e.target.value;
            if (v === "around") onChange({ around: pageIsPerson ? "self" : (people[0]?.name ?? ""), of: "" });
            else if (v === "type") onChange({ around: "", of: kinds[0]?.name ?? "" });
            else onChange({ around: "", of: "" });
          }}
        >
          <option value="">No — only who I add</option>
          <option value="around">Relatives of someone</option>
          <option value="type">Everyone of a type</option>
        </select>
        {mode === "around" && (
          <>
            <label>
              <span>Relatives of</span>
              <select value={middle} onChange={(e) => onChange({ around: e.target.value })}>
                {pageIsPerson && <option value="self">this page</option>}
                {people.map((p) => (
                  <option key={p.id} value={p.name}>
                    {p.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span>How far</span>
              <select value={query.depth || 1} onChange={(e) => onChange({ depth: Number(e.target.value) })}>
                <option value={1}>Direct relations</option>
                <option value={2}>… and theirs</option>
                <option value={3}>… three steps</option>
                <option value={4}>… four steps</option>
                <option value={5}>… five steps</option>
              </select>
            </label>
          </>
        )}
        {mode === "type" && (
          <select value={findType(templates, query.of)?.name ?? query.of} onChange={(e) => onChange({ of: e.target.value })}>
            {kinds.map((t) => (
              <option key={t.id} value={t.name}>
                {t.name}
              </option>
            ))}
          </select>
        )}
        <p className="small muted">Whoever you add stays, either way.</p>
      </section>

      {relations.length > 1 && (
        <section>
          <h5>Relations drawn</h5>
          <div className="chips">
            {relations.map((f) => {
              const on = query.via.length === 0 || query.via.some((v) => v === f.key || v.toLowerCase() === f.label.toLowerCase());
              return (
                <button
                  key={f.key}
                  className={on ? "chip on" : "chip"}
                  onClick={() => {
                    const all = query.via.length === 0 ? relations.map((r) => r.label) : query.via;
                    const next = on ? all.filter((v) => v !== f.key && v.toLowerCase() !== f.label.toLowerCase()) : [...all, f.label];
                    onChange({ via: next.length === relations.length ? [] : next });
                  }}
                >
                  {f.label}
                </button>
              );
            })}
          </div>
        </section>
      )}

      <section className="graph-actions">
        {(query.people.length > 0 || query.lines.length > 0) && (
          <button className="compact" onClick={onRelayout}>
            Lay out anew
          </button>
        )}
        {onRemove && (
          <button className="compact danger" onClick={onRemove}>
            Remove graph
          </button>
        )}
      </section>
    </div>
  );
}
