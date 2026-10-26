import { createContext, Fragment, useContext, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { api, type Node, type Place, type Rule, type Template } from "../../api/client";
import { idOf, type Resolve } from "../../shared/Links";
import { anchorDrawn, leadTo } from "../../shared/Anchors";
import { Listing, ListEditing } from "./Listing";
import { MapBlock } from "./MapView";
import { TallyMarks } from "../../shared/ui/TallyMarks";
import { FoldChevron } from "../../shared/ui/BlockFold";
import { RelationGraph } from "./RelationGraph";
import { MonthView } from "./MonthView";
import { Timeline } from "./Timeline";
import { CreateButton } from "./CreateButton";
import { Embed } from "./Embed";
import { DocLink } from "./DocLink";
import { PropertyValue } from "./InfoboxForm";
import { CurrentDocument, ReadableParts, useWorldData } from "../worlds/WorldData";
import { ZoomImage } from "../../shared/ui/ImageZoom";
import { effective, propertyNamed, takesIn, typeOf } from "../../shared/types";
import { summedValues } from "../../shared/Sheet";
import { MARK, type MarkKind } from "../../shared/Marks";
import { RollLabel, RollScopeContext, useDice } from "../dice/DiceTray";
import { checks, checkShown, hasDice, isCheck, isJudged, parse, parseRoll, reckon, signed, written, type Ast, type Lookup } from "../../shared/Dice";

/**
 * Showing Markdown — from the tree Rust delivers.
 *
 * There is deliberately no `dangerouslySetInnerHTML` here and no Markdown
 * parser either: Rust splits the text, this file builds React elements from
 * it. What Rust does not know as a node cannot come into being here as
 * markup at all — so a foreign world author gets no script into the
 * application.
 */

/** Shows a property of the document being drawn, by name. */
export type ValueOf = (name: string) => ReactNode;

const Values = createContext<ValueOf | null>(null);

/** What `%%Name%%` shows in text drawn outside `MarkdownText` — the editor's surfaces. */
export const ValuesProvider = Values.Provider;

/**
 * While writing: a score's own value to change, beside the tile that shows
 * what it comes to — the editor gives it; reading has none.
 */
export const SheetEdit = createContext<((name: string) => ReactNode) | null>(null);

/** The groups whose layouts are being drawn — a layout that shows itself stops. */
const ViewChain = createContext<string[]>([]);

/** A heading of the text that folds what follows it — see `shared/Sections`. */
export type HeadingFold = { folded: boolean; toggle: () => void };

/** Per node at the top of the text: hidden under a folded heading, or a heading that folds. */
export type TopFolds = { hidden: boolean; fold?: HeadingFold }[];

export function MarkdownText({
  text,
  nodes,
  folds,
  resolve,
  value,
  onPlace,
}: {
  text: string;
  /** The text already split — by whoever needed its headings first. */
  nodes?: Node[];
  /** What its headings fold. */
  folds?: TopFolds;
  /** Finds what an `@@Type:Name@@` points at — see `linker`. */
  resolve: Resolve;
  /** What `%%Name%%` and an element with `bind=` show. Without it they
   *  stand there as written — in a template, say, which has no values. */
  value?: ValueOf;
  onPlace: (o: Place) => void;
}) {
  const read = useMarkdown(nodes ? null : text);
  return (
    <Values.Provider value={value ?? null}>
      <div className="markdown">{draw(nodes ?? read, resolve, onPlace, folds)}</div>
    </Values.Provider>
  );
}

/**
 * Lets Rust split the text.
 *
 * While typing, the last valid tree stays, so the preview does not flicker
 * on every keystroke.
 */
export function useMarkdown(text: string | null): Node[] {
  const [node, setNode] = useState<Node[]>([]);
  useEffect(() => {
    if (text === null) return;
    let cancelled = false;
    api
      .markdownRead(text)
      .then((fresh) => {
        if (!cancelled) setNode(fresh);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [text]);
  return node;
}

/** What changed against another version, drawn in the text — see `shared/Marks`. */
type Mark = { kind: "mark"; mark: MarkKind; children: Drawn[] };
type Drawn = Node | Mark;

const MARKS = /([\uE020-\uE023])/;

/**
 * Turns the marks in the text into marked pieces. A mark may open in one
 * piece of text and close in another — around a bold word, a link — so the
 * tree is walked in reading order, with what is open carried along.
 */
function withMarks(nodes: Node[]): Drawn[] {
  if (!MARKS.test(JSON.stringify(nodes))) return nodes;
  const state: { open: MarkKind | null } = { open: null };
  const wrap = (node: Drawn): Drawn => (state.open ? { kind: "mark", mark: state.open, children: [node] } : node);
  const read = (text: string, each: (piece: string) => void) => {
    for (const piece of text.split(MARKS)) {
      if (piece === MARK.added[0]) state.open = "added";
      else if (piece === MARK.removed[0]) state.open = "removed";
      else if (piece === MARK.added[1] || piece === MARK.removed[1]) state.open = null;
      else if (piece) each(piece);
    }
  };
  const walk = (list: Node[]): Drawn[] => {
    const out: Drawn[] = [];
    for (const node of list) {
      switch (node.kind) {
        case "text":
          read(node.text, (text) => out.push(wrap({ kind: "text", text })));
          break;
        case "code":
        case "code-block": {
          let text = "";
          const at = state.open;
          read(node.text, (piece) => (text += piece));
          state.open = state.open ?? at;
          out.push(wrap({ ...node, text } as Node));
          break;
        }
        case "list":
          out.push({ ...node, items: node.items.map((item) => walk(item) as Node[]) });
          break;
        case "table":
          out.push({ ...node, head: node.head.map((cell) => walk(cell) as Node[]), rows: node.rows.map((row) => row.map((cell) => walk(cell) as Node[])) });
          break;
        default:
          if ("children" in node) out.push({ ...node, children: walk(node.children) as Node[] } as Node);
          else out.push(wrap(node));
      }
    }
    return out;
  };
  return walk(nodes);
}

/**
 * Several texts split at once — the parts of a block, whose headings fold
 * across them. Until all are split: none.
 */
export function useMarkdowns(texts: string[]): Node[][] | null {
  const key = JSON.stringify(texts);
  const [split, setSplit] = useState<{ key: string; trees: Node[][] } | null>(null);
  useEffect(() => {
    let cancelled = false;
    const all: string[] = JSON.parse(key);
    Promise.all(all.map((t) => api.markdownRead(t)))
      .then((trees) => !cancelled && setSplit({ key, trees }))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [key]);
  // While typing, the last split stays — as long as it has as many parts.
  return split && split.trees.length === texts.length ? split.trees : null;
}

/** A heading's words, as plain text — what names it in `shared/Sections`. */
export function headingOf(node: Node): { level: number; text: string } | undefined {
  return node.kind === "heading" ? { level: node.level, text: plain(node.children) } : undefined;
}

function draw(node: Drawn[], resolve: Resolve, onPlace: (o: Place) => void, folds?: TopFolds): ReactNode {
  const drawn = withMarks(node as Node[]);
  // Marks may split what stands at the top; then nothing folds.
  const fold = folds && folds.length === drawn.length ? folds : undefined;
  return drawn.map((k, i) =>
    fold?.[i].hidden ? null : <Element key={i} node={k} resolve={resolve} onPlace={onPlace} fold={fold?.[i].fold} />,
  );
}

function Element({
  node,
  resolve,
  onPlace,
  fold,
}: {
  node: Drawn;
  resolve: Resolve;
  onPlace: (o: Place) => void;
  /** A heading at the top of the text: what it folds. */
  fold?: HeadingFold;
}) {
  const deep = (children: Drawn[]) => draw(children, resolve, onPlace);
  const valueOf = useContext(Values);
  const world = useWorldData();
  const readable = useContext(ReadableParts);
  const current = useContext(CurrentDocument);

  switch (node.kind) {
    case "mark":
      return <span className={`mark-${node.mark}`}>{deep(node.children)}</span>;
    case "paragraph":
      // Each line names its own rolls: `**Hit Points** 7 (2d6)` under `**Armor Class** 15`.
      return (
        <p>
          {byLine(node.children).map((line, i) => (
            <Fragment key={i}>
              {i > 0 && <br />}
              <RollLabel.Provider value={leadingWords(line)}>{deep(line)}</RollLabel.Provider>
            </Fragment>
          ))}
        </p>
      );
    case "heading": {
      const Tag = `h${Math.min(6, Math.max(1, node.level + 1))}` as "h2";
      if (!fold) return <Tag className="markdown-head">{deep(node.children)}</Tag>;
      // The heading folds — a click on a link in it still follows the link.
      return (
        <Tag
          className="markdown-head foldable"
          aria-expanded={!fold.folded}
          onClick={(e) => !(e.target as HTMLElement).closest("a, button:not(.head-fold), input") && fold.toggle()}
        >
          <button className="head-fold" title={fold.folded ? "Open" : "Fold"} tabIndex={-1}>
            <FoldChevron open={!fold.folded} />
          </button>
          {deep(node.children)}
        </Tag>
      );
    }
    case "list":
      return node.ordered ? (
        <ol>
          {node.items.map((p, i) => (
            <li key={i}>{deep(p)}</li>
          ))}
        </ol>
      ) : (
        <ul>
          {node.items.map((p, i) => (
            <li key={i}>
              <RollLabel.Provider value={leadingWords(p)}>{deep(p)}</RollLabel.Provider>
            </li>
          ))}
        </ul>
      );
    case "quote":
      return <blockquote>{deep(node.children)}</blockquote>;
    case "code-block":
      return (
        <pre className="codeblock">
          <code>{node.text}</code>
        </pre>
      );
    case "rule":
      return <hr />;
    case "table": {
      const align = (i: number) => (node.align[i] ? { textAlign: node.align[i] as "left" } : undefined);
      return (
        <div className="markdown-table">
          <table>
            <thead>
              <tr>
                {node.head.map((cell, i) => (
                  <th key={i} style={align(i)}>
                    {deep(cell)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {node.rows.map((row, r) => (
                <tr key={r}>
                  {row.map((cell, i) => (
                    <td key={i} style={align(i)}>
                      {deep(cell)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    }
    case "text":
      return <>{node.text}</>;
    case "strong":
      return <strong>{deep(node.children)}</strong>;
    case "emphasis":
      return <em>{deep(node.children)}</em>;
    case "strikethrough":
      return <s>{deep(node.children)}</s>;
    case "code":
      return <code>{node.text}</code>;
    case "soft-break":
      // A line break stays one — as in the editor, where one sees it.
      return <br />;
    case "link":
      // No `href`: a click should open nothing we have not checked. The
      // target stands beside it as a hint.
      return (
        <span className="external-link" title={node.target}>
          {deep(node.children)}
        </span>
      );
    case "image": {
      // A stand-in steps back where the reader sees the real image.
      if (node.for && readable?.has(node.for)) return null;
      // The text runs past it left or right. Without a world (a template
      // preview) or an image id, a placeholder holds the place.
      const style: React.CSSProperties = node.width
        ? { width: node.width, maxWidth: "100%" }
        : {};
      return (
        <Figure className={`text-image ${node.align}${node.width ? " sized" : ""}${node.wrap || node.align === "center" ? "" : " nowrap"}`} style={style}>
          {world && node.src.startsWith("img:") ? (
            <ZoomImage world={world.world.id} id={node.src} alt={node.caption} />
          ) : node.for ? (
            // A stand-in without an image of its own: someone, not shown yet.
            <span className="image-placeholder unknown">?</span>
          ) : (
            <span className="image-placeholder">🖼</span>
          )}
          {node.caption && <figcaption>{node.caption}</figcaption>}
        </Figure>
      );
    }
    case "value":
      return valueOf ? <>{valueOf(node.name)}</> : <code className="value-name">%%{node.name}%%</code>;
    case "bound":
      return valueOf ? (
        <span className="bound-value">{valueOf(node.property)}</span>
      ) : (
        <code className="value-name">{node.label || node.property}</code>
      );
    case "roll":
      return <Roll formula={node.formula} text={node.text} />;
    case "abilities":
      return <Abilities show={node.show} with={node.with} short={node.short} also={node.also} heading={node.heading} alsoHeading={node.also_heading} />;
    case "view":
      return <GroupView of={node.of} resolve={resolve} onPlace={onPlace} />;
    case "map":
      return <MapBlock map={node} />;
    case "listing":
      return <Listing node={node} />;
    case "graph":
      return <RelationGraph query={node} />;
    case "timeline":
      return <Timeline node={node} />;
    case "calendar":
      return <MonthView node={node} />;
    case "create":
      return <CreateButton of={node.of} label={node.label} />;
    case "embed":
      return <Embed of={node.of} fold={node.fold} title={node.title} section={node.section} />;
    case "anchor":
      return <Anchor id={node.id} />;
    case "wikilink": {
      // Resolved only here: only at this spot is it known which articles
      // this reader could decrypt at all.
      const link = { of: node.of ?? "", name: node.target };
      const target = node.target ? resolve(link) : undefined;
      // Text of its own is shown as written; otherwise the current name.
      // An id this reader cannot open reveals nothing — not even a name.
      const own = node.text !== node.target;
      // `#^2f9e17`: a line of the target, or of this document without a name.
      if (node.property.startsWith("^")) {
        const doc = node.target ? target : world?.articles.find((a) => a.id === current);
        const shown = node.text || (doc?.name ?? "???");
        if (!doc) return <span className="dead-link">{shown}</span>;
        return (
          <DocLink
            id={doc.id}
            articleType={doc.articleType}
            templates={world?.templates ?? []}
            anchor={node.property.slice(1)}
            onOpen={() => leadTo(doc.id, node.property.slice(1), current, () => onPlace({ kind: "article", id: doc.id }))}
          >
            {shown}
          </DocLink>
        );
      }
      // `#Status`: the value of that property of the target — as it is now.
      if (node.property && !own) {
        const type = world && target ? typeOf(world.templates, target.articleType) : undefined;
        const field = propertyNamed(type, node.property);
        if (!world || !target || !field) return <span className="dead-link">???</span>;
        return (
          <PropertyValue
            field={field}
            value={summedValues(world.templates, world.articles, target)[field.key]}
            articles={world.articles}
            templates={world.templates}
            articleType={target.articleType}
            onPlace={onPlace}
          />
        );
      }
      const shown = own ? node.text : (target?.name ?? (idOf(link) ? "???" : node.text));
      if (!target) {
        return <span className="dead-link">{shown}</span>;
      }
      return (
        <DocLink id={target.id} articleType={target.articleType} templates={world?.templates ?? []} onOpen={() => onPlace({ kind: "article", id: target.id })}>
          {shown}
        </DocLink>
      );
    }
  }
}

/** Where a link leads to: nothing to see, but a place to scroll to. */
function Anchor({ id }: { id: string }) {
  const current = useContext(CurrentDocument);
  useEffect(() => anchorDrawn(current, id), [current, id]);
  return <span className="anchor" data-anchor={id} />;
}

/**
 * An image in the text. Floating, but leaving no room beside it, it stands
 * on its own — as wide as the text, the text below it, as in the editor.
 */
function Figure({ className, style, children }: { className: string; style: React.CSSProperties; children: React.ReactNode }) {
  const ref = useRef<HTMLElement>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    const box = el?.parentElement;
    if (!el || !box || el.classList.contains("nowrap") || !(el.classList.contains("left") || el.classList.contains("right"))) return;
    const fit = () => {
      el.classList.remove("full");
      const side = parseFloat(getComputedStyle(el).marginLeft) + parseFloat(getComputedStyle(el).marginRight);
      if (el.getBoundingClientRect().width + side > box.clientWidth - 120) el.classList.add("full");
    };
    fit();
    const watch = new ResizeObserver(fit);
    watch.observe(box);
    watch.observe(el);
    return () => watch.disconnect();
  }, [className]);
  return (
    <figure ref={ref} className={className} style={style}>
      {children}
    </figure>
  );
}

/** A paragraph's pieces, cut at its line breaks — the breaks themselves left out. */
function byLine(children: Drawn[]): Drawn[][] {
  const out: Drawn[][] = [[]];
  for (const c of children) {
    if (c.kind === "soft-break") out.push([]);
    else out[out.length - 1].push(c);
  }
  return out;
}

/** The text of a run of nodes, without marks. */
function plain(nodes: Drawn[]): string {
  return nodes
    .map((n) => (n.kind === "text" ? n.text : "children" in n ? plain(n.children as Drawn[]) : ""))
    .join("");
}

/**
 * What a paragraph's rolls are for: the bold words it starts with, as a
 * stat block writes an action — `***Scimitar.*** Melee Weapon Attack …`.
 */
function leadingWords(children: Drawn[]): string {
  const first = children.find((c) => !(c.kind === "text" && !c.text.trim()));
  if (!first || !(first.kind === "strong" || first.kind === "emphasis" || first.kind === "mark")) return "";
  return plain([first]).trim().replace(/[.:]+$/, "");
}

/**
 * `[[…]]` or dice in the text. With dice it is a button that rolls into the
 * tray; without, it is the number it reckons. A formula that does not read
 * stands as written, its problem on hover.
 */
export function Roll({ formula, text }: { formula: string; text: string }) {
  const scope = useContext(RollScopeContext);
  const dice = useDice();
  const label = useContext(RollLabel);
  const lookup: Lookup = scope?.lookup ?? (() => undefined);
  const asWritten = text || `[[${formula}]]`;
  const bad = (why: unknown) => (
    <span className="roll-bad" title={why instanceof Error ? why.message : String(why)}>
      {asWritten}
    </span>
  );
  const rules = lookup.rules ?? [];
  // `[[+4]]`: a check where the rules say what one rolls — otherwise “+4”.
  const check = isCheck(formula);
  let ast: Ast;
  try {
    ast = parseRoll(formula, rules);
  } catch (e) {
    return bad(e);
  }
  if (!hasDice(ast)) {
    // Nothing to roll: the number. Outside a document there is none yet.
    if (!scope) return <code className="value-name">{asWritten}</code>;
    try {
      const n = reckon(ast, lookup).total;
      return (
        <span className="reckoned" title={formula}>
          {check ? signed(n) : n}
        </span>
      );
    } catch (e) {
      return bad(e);
    }
  }
  const how = written(ast, lookup) ?? formula;
  let shown = text;
  if (!shown) {
    // A check shows its value — or what the rule `shown` makes of it.
    const judged = isJudged(formula, rules) ? checkShown(formula, rules, lookup) : undefined;
    try {
      shown = judged ?? (check ? signed(reckon(parse(formula, rules), lookup).total) : how);
    } catch {
      shown = "";
    }
    if (!shown) return scope ? bad(new Error("a name here has no number")) : <code className="value-name">{asWritten}</code>;
  }
  if (!dice || !scope) return <span className="roll">{shown}</span>;
  return (
    <button
      className="roll"
      title={how}
      onClick={() => dice.roll({ document: scope.document, label, formula, lookup, secret: scope.secret })}
    >
      {shown}
    </button>
  );
}

/** How a roll reads with the names filled in — `1d20 + 2` — or nothing. */
function rolls(formula: string, rules: Rule[], lookup: Lookup) {
  try {
    return written(parseRoll(formula, rules), lookup) ?? "";
  } catch {
    return "";
  }
}

/** How a tile is headed: as `short=` says, or by the first three letters. */
const headed = (name: string, i: number, short: string[] = []) => short[i]?.trim() || name.slice(0, 3).toUpperCase();

/** A property group by id or name. */
export const groupNamed = (templates: Template[], name: string) =>
  templates.find((t) => t.group && (t.id === name.trim() || t.name.toLowerCase() === name.trim().toLowerCase()));

/**
 * `<view of=Stats>` — the layout a property group defines, drawn here with
 * the values of the document it stands in. One layout, kept in one place,
 * shown wherever it is taken in.
 */
export function GroupView({ of, resolve, onPlace }: { of: string; resolve: Resolve; onPlace: (o: Place) => void }) {
  const world = useWorldData();
  const chain = useContext(ViewChain);
  const current = useContext(CurrentDocument);
  const group = world ? groupNamed(world.templates, of) : undefined;
  const nodes = useMarkdown(group?.blocks[0]?.parts[0]?.text ?? "");
  if (!group || chain.includes(group.id)) return <code className="value-name">&lt;view of={group?.name ?? of}&gt;</code>;
  // A document whose type does not have the group's values shows nothing of
  // it: one character sheet for all, the player's part only for players.
  const doc = world?.articles.find((a) => a.id === current);
  if (doc && world && !takesIn(world.templates, doc.articleType, group.id)) return null;
  return (
    <ViewChain.Provider value={[...chain, group.id]}>
      <div className="group-view">{draw(nodes, resolve, onPlace)}</div>
    </ViewChain.Provider>
  );
}

/**
 * `<abilities show=Abilities with=mod>` — the ability row of a stat block:
 * each score, below it what the rule `with` makes of it — the modifier, as
 * the world defines it. Where the rules say what a check rolls, a click
 * rolls one: on the rule's result, or on the score itself.
 */
export function Abilities({
  show,
  with: rule,
  short,
  also,
  heading,
  alsoHeading,
}: {
  show: string[];
  with?: string;
  short?: string[];
  also?: string;
  heading?: string;
  alsoHeading?: string;
}) {
  const scope = useContext(RollScopeContext);
  const dice = useDice();
  const world = useWorldData();
  const editing = useContext(SheetEdit);
  const lists = useContext(ListEditing);
  const current = useContext(CurrentDocument);
  // A group names its numbers: `show=Abilities` is the six of that group.
  const names = show.flatMap((n) => {
    const group = world && groupNamed(world.templates, n);
    return group ? effective(world.templates, group).properties.filter((p) => p.kind === "number").map((p) => p.label) : [n];
  });
  if (names.length === 0) return <code className="value-name">&lt;abilities&gt;</code>;
  const rules = scope?.lookup.rules ?? [];
  /** What the tile rolls and shows below the score: `mod(Strength)`. */
  const of = (name: string) => (rule ? `${rule}(${name})` : name);
  const below = (name: string) => {
    if (!rule || !scope) return undefined;
    try {
      return reckon(parse(of(name), rules), scope.lookup).total;
    } catch {
      return undefined;
    }
  };
  // `also="Saving throws.Bonus"`: the pick named like the score, its number.
  const dot = also?.indexOf(".") ?? -1;
  const ref = also && dot > 0 ? also.slice(0, dot).trim() : "";
  const prop = also && dot > 0 ? also.slice(dot + 1).trim() : "";
  const doc = world?.articles.find((a) => a.id === current);
  const refField = ref && world && doc ? propertyNamed(typeOf(world.templates, doc.articleType), ref) : undefined;
  /** The second line of a tile: how well, the number, a roll — or its number to change. */
  const second = (name: string) => {
    if (!ref || !scope) return null;
    const path = `${ref}[${name}]`;
    const pick = scope.lookup.scopes?.(path)?.[0];
    const value = scope.lookup(`${path}.${prop}`);
    if (!pick || value === undefined) return null;
    const n = Math.max(0, pick.look.count ?? 0);
    const marks = <TallyMarks sign="●" n={n} className="ability-marks" title={`${refField?.kind === "reference" ? refField.count : ""} ${n}`} />;
    if (lists && refField) {
      return (
        <span className="ability-also">
          <input
            className="pick-count"
            inputMode="decimal"
            value={lists.count(refField.key, pick.pick.id)}
            placeholder="0"
            title={`${ref} — ${name}`}
            onChange={(e) => lists.setCount(refField.key, pick.pick.id, e.target.value)}
          />
          {signed(value)}
        </span>
      );
    }
    const formula = `+ ${path}.${prop}`;
    return dice && checks(rules) ? (
      <button
        className="ability-also"
        title={`${name} — ${ref}\n${rolls(formula, rules, scope.lookup)}`}
        onClick={() => dice.roll({ document: scope.document, label: `${name} · ${ref}`, formula, lookup: scope.lookup, secret: scope.secret })}
      >
        {marks}
        {signed(value)}
      </button>
    ) : (
      <span className="ability-also">
        {marks}
        {signed(value)}
      </span>
    );
  };
  return (
    <>
    {heading && <div className="ability-caption">{heading}</div>}
    <div className="ability-row" style={{ ["--count" as string]: names.length }}>
      {names.map((name, i) => {
        const score = scope?.lookup(name);
        const small = below(name);
        const how = scope?.explain(name) ?? "";
        const head = <span className="ability-name">{headed(name, i, short)}</span>;
        if (score === undefined)
          return (
            <div key={name} className="ability empty" title={name}>
              {head}
              <span className="ability-score">—</span>
              {editing?.(name)}
            </div>
          );
        const inside = (
          <>
            {head}
            <span className="ability-score">{score}</span>
            {small !== undefined && <span className="ability-mod">{signed(small)}</span>}
          </>
        );
        const title = how ? `${name} — ${how}` : name;
        // While writing, the tile holds the score's own value to change.
        const own = editing?.(name);
        if (own)
          return (
            <div key={name} className="ability" title={title}>
              {inside}
              {own}
            </div>
          );
        // The whole tile rolls a check — where the rules say what one is.
        const formula = `+ ${of(name)}`;
        return dice && scope && checks(rules) ? (
          <button
            key={name}
            className="ability"
            title={`${title}\n${rolls(formula, rules, scope.lookup)}`}
            onClick={() => dice.roll({ document: scope.document, label: name, formula, lookup: scope.lookup, secret: scope.secret })}
          >
            {inside}
          </button>
        ) : (
          <div key={name} className="ability" title={title}>
            {inside}
          </div>
        );
      })}
      {/* The line of what `also` shows — the saving throws, under their scores. */}
      {ref && alsoHeading && <div className="ability-caption also">{alsoHeading}</div>}
      {ref &&
        names.map((name) => (
          <div key={`also-${name}`} className="ability-also-cell">
            {second(name)}
          </div>
        ))}
    </div>
    </>
  );
}
