import { useContext } from "react";
import type { Node, Template, TemplateField } from "../../api/client";
import { attr, withAttr, writeListLine, writeTagLine, type ImageAttrs } from "../../shared/ImageTag";
import { findType, isA, isProperties, typeOf } from "../../shared/types";
import { SearchPick, type Pickable } from "../../shared/ui/SearchPick";
import { CurrentDocument, useWorldData } from "../worlds/WorldData";
import { Listing } from "./Listing";
import { AccessSection, InspectorSection, Surface, TagSection } from "./Inspector";
import { MonthView } from "./MonthView";
import { Timeline } from "./Timeline";
import { CreateButton } from "./CreateButton";
import { Embed } from "./Embed";
import { Abilities, GroupView, groupNamed } from "./MarkdownText";
import { linker } from "../../shared/Links";
import { RollScopeContext } from "../dice/DiceTray";

type ListingNode = Extract<Node, { kind: "listing" }>;

const STYLES = ["list", "cards", "table", "tree"] as const;

const listed = (value: string) =>
  value
    .split(",")
    .map((v) => v.trim())
    .filter(Boolean);

/** A `<list …>` line read as Rust reads it (`markdown.rs`, `read_list`) — forgiving. */
export function listNode(a: ImageAttrs): ListingNode {
  const style = attr(a, "style");
  return {
    kind: "listing",
    of: attr(a, "type"),
    style: (STYLES as readonly string[]).includes(style) ? (style as ListingNode["style"]) : "list",
    columns: Math.min(3, Math.max(1, Number(attr(a, "columns")) || 1)),
    show: listed(attr(a, "show")),
    filter: listed(attr(a, "where")).flatMap((c) => {
      const i = c.indexOf("=");
      return i < 0 ? [] : [[c.slice(0, i).trim(), c.slice(i + 1).trim()] as [string, string]];
    }),
    below: attr(a, "below"),
    from: attr(a, "from"),
    when: attr(a, "when"),
    choose: attr(a, "filter"),
    sort: listed(attr(a, "sort")),
    count: attr(a, "count"),
    by: attr(a, "by"),
  };
}

/**
 * A list in the visual editor — drawn as it is read. A click chooses it;
 * the side bar then shows who may see it, what it lists and how, and its tag.
 */
export function ListCard({ list, onChange }: { list: ImageAttrs; onChange: (next: string | null) => void }) {
  const node = listNode(list);
  return (
    <Surface
      label="List"
      inspector={
        <>
          <AccessSection attrs={list} onAttrs={(next) => onChange(writeListLine(next))} />
          <InspectorSection title="Shown as">
            <div className="segmented">
              {STYLES.map((s) => (
                <button key={s} className={node.style === s ? "on" : ""} onClick={() => onChange(writeListLine(withAttr(list, "style", s === "list" ? "" : s)))}>
                  {s}
                </button>
              ))}
            </div>
          </InspectorSection>
          <InspectorSection title="Content">
            <ListSettings attrs={list} onChange={(next) => onChange(next)} />
          </InspectorSection>
          <TagSection code={writeListLine(list)} onCode={onChange} />
          <button className="danger compact inspector-remove" onClick={() => onChange(null)}>
            Remove the list
          </button>
        </>
      }
    >
      <Listing node={node} />
    </Surface>
  );
}

/** Types to pick from, each with the type it belongs to — `NSC · in Charakter`. */
function typeItems(templates: Template[], only?: (t: Template) => boolean): Pickable[] {
  return templates
    .filter((t) => !t.group && (!only || only(t)))
    .map((t) => ({ id: t.id, name: t.name, icon: t.icon, hint: t.supertype ? `in ${findType(templates, t.supertype)?.name ?? "?"}` : undefined }))
    .sort((a, b) => a.name.localeCompare(b.name));
}

/** The type a tag names, by id or by name. */
const typeNamed = (templates: Template[], name: string) => templates.find((t) => t.id === name || t.name.toLowerCase() === name.toLowerCase());

/** The types a create button makes, by name — to add to and take from. */
export function MakesPick({ of, onChange }: { of: string[]; onChange: (names: string[]) => void }) {
  const templates = useWorldData()?.templates ?? [];
  const idOf = (t: string) => typeNamed(templates, t)?.id ?? t;
  const nameOf = (id: string) => findType(templates, id)?.name ?? id;
  return (
    <SearchPick
      items={typeItems(templates)}
      chosen={of.map(idOf)}
      placeholder="Add a type"
      onAdd={(id) => onChange([...of, nameOf(id)])}
      onRemove={(id) => onChange(of.filter((t) => idOf(t) !== id))}
    />
  );
}

/** The properties a list can show or filter by — all but headings. */
const listable = (t: Template | undefined): TemplateField[] => (t?.properties ?? []).filter((f) => f.kind !== "heading");

/**
 * A list's settings, by click: what it lists — documents of a type, or what
 * a property of this document picks — which properties it shows, in how
 * many columns, under which conditions, below which document. Each click
 * writes the tag.
 */
function ListSettings({ attrs, onChange }: { attrs: ImageAttrs; onChange: (next: string) => void }) {
  const data = useWorldData();
  const current = useContext(CurrentDocument);
  if (!data) return null;
  const { templates, articles } = data;
  const set = (name: string, value: string) => onChange(writeListLine(withAttr(attrs, name, value)));
  const node = listNode(attrs);

  // What this document can list from itself: its properties that pick several.
  const own = typeOf(templates, articles.find((a) => a.id === current)?.articleType ?? "");
  const picking = (own?.properties ?? []).filter((f): f is TemplateField & { kind: "reference" } => f.kind === "reference" && f.multiple);
  const from = node.from ? picking.find((f) => f.label.toLowerCase() === node.from.toLowerCase() || f.key === node.from) : undefined;
  const listed = from ? typeOf(templates, from.articleType) : node.of.replace(/[«»]/g, "") ? typeOf(templates, typeNamed(templates, node.of)?.id ?? "") : undefined;
  const fields = listable(listed);
  const tree = fields.some((f) => f.kind === "reference" && f.parent);
  const show = node.show;
  const on = (label: string) => show.some((x) => x.toLowerCase() === label.toLowerCase());
  const filters = node.filter;
  const writeFilters = (next: [string, string][]) => set("where", next.map(([f, v]) => `${f}=${v}`).join(", "));

  return (
    <div className="time-settings">
      {picking.length > 0 && (
        <div className="time-setting">
          <span className="small muted">Lists</span>
          <div className="segmented">
            <button className={!node.from ? "on" : ""} onClick={() => onChange(writeListLine(withAttr(withAttr(attrs, "from", ""), "type", listed?.name ?? "")))}>
              of a type
            </button>
            <button className={node.from ? "on" : ""} onClick={() => onChange(writeListLine(withAttr(withAttr(attrs, "type", ""), "from", picking[0].label)))}>
              this document's
            </button>
          </div>
        </div>
      )}
      {node.from ? (
        <div className="time-setting">
          <span className="small muted">From</span>
          <select value={from?.label ?? ""} onChange={(e) => set("from", e.target.value)}>
            {!from && <option value="">—</option>}
            {picking.map((f) => (
              <option key={f.key} value={f.label}>
                {f.label}
              </option>
            ))}
          </select>
        </div>
      ) : (
        <div className="time-setting">
          <span className="small muted">Type</span>
          <SearchPick
            single
            items={typeItems(templates)}
            chosen={listed ? [listed.id] : []}
            placeholder="Find a type"
            none="all documents"
            onAdd={(id) => set("type", findType(templates, id)?.name ?? id)}
            onRemove={() => set("type", "")}
          />
        </div>
      )}
      {fields.length > 0 && (
        <div className="time-setting">
          <span className="small muted">Shows</span>
          <div className="time-chips">
            {fields.map((f) => (
              <button
                key={f.key}
                className={`chip${on(f.label) ? " on" : ""}`}
                onClick={() => set("show", (on(f.label) ? show.filter((x) => x.toLowerCase() !== f.label.toLowerCase()) : [...show, f.label]).join(", "))}
              >
                {f.label}
              </button>
            ))}
            {show.length === 0 && <span className="small muted">names only</span>}
          </div>
        </div>
      )}
      {node.style === "list" && (
        <div className="time-setting">
          <span className="small muted">Columns</span>
          <div className="segmented">
            {[1, 2, 3].map((n) => (
              <button key={n} className={node.columns === n ? "on" : ""} onClick={() => set("columns", n === 1 ? "" : String(n))}>
                {n}
              </button>
            ))}
          </div>
        </div>
      )}
      {!node.from && fields.length > 0 && (
        <div className="time-setting">
          <span className="small muted">Only</span>
          <div className="list-filters">
            {filters.map(([field, value], i) => {
              const f = fields.find((x) => x.label.toLowerCase() === field.toLowerCase() || x.key === field);
              const choices = f?.kind === "reference" ? articles.filter((a) => isA(templates, a.articleType, f.articleType)).map((a) => a.name).sort() : null;
              const change = (next: [string, string]) => writeFilters(filters.map((x, j) => (j === i ? next : x)));
              return (
                <div key={i} className="list-filter">
                  <select value={f?.label ?? field} onChange={(e) => change([e.target.value, ""])}>
                    {!f && <option value={field}>{field}</option>}
                    {fields.map((x) => (
                      <option key={x.key} value={x.label}>
                        {x.label}
                      </option>
                    ))}
                  </select>
                  <span className="muted">is</span>
                  {choices ? (
                    <select value={value} onChange={(e) => change([field, e.target.value])}>
                      <option value="">— nothing</option>
                      {!choices.includes(value) && value && <option value={value}>{value}</option>}
                      {choices.map((c) => (
                        <option key={c} value={c}>
                          {c}
                        </option>
                      ))}
                    </select>
                  ) : (
                    <input value={value} placeholder="empty" onChange={(e) => change([field, e.target.value])} />
                  )}
                  <button className="pick-x" title="Remove the condition" onClick={() => writeFilters(filters.filter((_, j) => j !== i))}>
                    ×
                  </button>
                </div>
              );
            })}
            <button className="compact ghost" onClick={() => writeFilters([...filters, [fields[0].label, ""]])}>
              ＋ Condition
            </button>
          </div>
        </div>
      )}
      {!node.from && tree && listed && (
        <div className="time-setting">
          <span className="small muted">Below</span>
          <SearchPick
            single
            items={articles.filter((a) => isA(templates, a.articleType, listed.id)).map((a) => ({ id: a.id, name: a.name }))}
            chosen={articles.filter((a) => a.id === node.below || a.name === node.below).map((a) => a.id)}
            placeholder="Find a document"
            none="the whole tree"
            onAdd={(id) => set("below", articles.find((a) => a.id === id)?.name ?? "")}
            onRemove={() => set("below", "")}
          />
        </div>
      )}
    </div>
  );
}

/** Names in a tag's list attribute, by the name each stands for — or as written. */
const has = (list: string[], name: string, id: string) => list.some((x) => x === id || x.toLowerCase() === name.toLowerCase());

/** A row of chips to switch on and off — what a list attribute holds. */
function Chips({ label, all, on, onToggle, hint }: { label: string; all: { id: string; name: string }[]; on: string[]; onToggle: (name: string) => void; hint: string }) {
  return (
    <div className="time-setting">
      <span className="small muted">{label}</span>
      <div className="time-chips">
        {all.map((x) => (
          <button key={x.id} className={`chip${has(on, x.name, x.id) ? " on" : ""}`} onClick={() => onToggle(x.name)}>
            {x.name}
          </button>
        ))}
        {on.length === 0 && <span className="small muted">{hint}</span>}
      </div>
    </div>
  );
}

/**
 * A timeline's or calendar's settings, by click: which types, which dates,
 * what ends a span, whose life, which part's text. Each click writes the tag.
 */
function TimeSettings({ tag, attrs, onChange }: { tag: "timeline" | "calendar"; attrs: ImageAttrs; onChange: (next: string) => void }) {
  const data = useWorldData();
  if (!data) return null;
  const { templates, articles } = data;
  const set = (name: string, value: string) => onChange(writeTagLine(tag, withAttr(attrs, name, value)));
  const toggle = (name: string, list: string[], item: string) =>
    set(name, (has(list, item, item) ? list.filter((x) => x.toLowerCase() !== item.toLowerCase()) : [...list, item]).join(", "));

  const of = listed(attr(attrs, "type"));
  // Only types with dates make a timeline.
  const dated = templates.filter((t) => typeOf(templates, t.id)?.properties.some((f) => f.kind === "world-date"));
  const chosen = dated.filter((t) => of.length === 0 || of.some((o) => o === t.id || o.toLowerCase() === t.name.toLowerCase()));
  const dateNames = [...new Set(chosen.flatMap((t) => typeOf(templates, t.id)!.properties.filter((f) => f.kind === "world-date").map((f) => f.label)))];
  const typesRow = (
    <div className="time-setting">
      <span className="small muted">Types</span>
      <SearchPick
        items={typeItems(templates, (t) => dated.includes(t))}
        chosen={chosen.length === dated.length && of.length === 0 ? [] : chosen.map((t) => t.id)}
        placeholder="Find a type"
        none="all with dates"
        onAdd={(id) => set("type", [...of, findType(templates, id)?.name ?? id].join(", "))}
        onRemove={(id) => set("type", of.filter((o) => o !== id && typeNamed(templates, o)?.id !== id).join(", "))}
      />
    </div>
  );
  if (tag === "calendar") return <div className="time-settings">{typesRow}</div>;

  const until = attr(attrs, "until");
  const show = listed(attr(attrs, "show"));
  const text = listed(attr(attrs, "text"));
  const around = attr(attrs, "around");
  // The parts the documents shown have, by title — where an entry's text comes from.
  const parts = [
    ...new Set(
      articles
        .filter((a) => chosen.some((t) => isA(templates, a.articleType, t.id)))
        .flatMap((a) => a.layers.filter((l) => !isProperties(l.section) && l.title.trim()).map((l) => l.title)),
    ),
  ];
  const named = around && around !== "self" ? articles.find((a) => a.id === around || a.name === around) : undefined;
  return (
    <div className="time-settings">
      {typesRow}
      <Chips
        label="Dates"
        all={dateNames.filter((n) => n.toLowerCase() !== until.toLowerCase()).map((n) => ({ id: n, name: n }))}
        on={show}
        onToggle={(n) => toggle("show", show, n)}
        hint="all"
      />
      <div className="time-setting">
        <span className="small muted">Ends</span>
        <select value={dateNames.find((n) => n.toLowerCase() === until.toLowerCase()) ?? ""} onChange={(e) => set("until", e.target.value)}>
          <option value="">—</option>
          {dateNames.map((n) => (
            <option key={n} value={n}>
              {n}
            </option>
          ))}
        </select>
      </div>
      <div className="time-setting">
        <span className="small muted">Of</span>
        <select value={around === "self" ? "self" : named ? named.id : ""} onChange={(e) => set("around", e.target.value === "" ? "" : e.target.value === "self" ? "self" : named?.name ?? "")}>
          <option value="">everyone</option>
          <option value="self">this document</option>
          {named && <option value={named.id}>{named.name}</option>}
        </select>
      </div>
      <div className="time-setting">
        <span className="small muted">Text</span>
        <div className="time-chips">
          {parts.map((p) => (
            <button key={p} className={`chip${has(text, p, p) ? " on" : ""}`} onClick={() => set("text", (has(text, p, p) ? text.filter((x) => x.toLowerCase() !== p.toLowerCase() && x !== "none") : [...text.filter((x) => x !== "none"), p]).join(", "))}>
              {p}
            </button>
          ))}
          <button className={`chip${text.includes("none") ? " on" : ""}`} onClick={() => set("text", text.includes("none") ? "" : "none")}>
            none
          </button>
          {text.length === 0 && <span className="small muted">first text</span>}
        </div>
      </div>
    </div>
  );
}

/** A timeline's or calendar's tag, read as Rust reads it. */
const timelineNode = (a: ImageAttrs) => ({
  kind: "timeline" as const,
  of: listed(attr(a, "type")),
  show: listed(attr(a, "show")),
  around: attr(a, "around"),
  until: attr(a, "until"),
  text: listed(attr(a, "text")),
});

/**
 * A timeline or a calendar in the visual editor — drawn as it is read. A
 * click chooses it; the side bar then shows who may see it, what it shows,
 * and its tag.
 */
export function TimeCard({ tag, attrs, onChange }: { tag: "timeline" | "calendar"; attrs: ImageAttrs; onChange: (next: string | null) => void }) {
  const name = tag === "timeline" ? "Timeline" : "Calendar";
  return (
    <Surface
      label={name}
      inspector={
        <>
          <AccessSection attrs={attrs} onAttrs={(next) => onChange(writeTagLine(tag, next))} />
          <InspectorSection title="Shows">
            <TimeSettings tag={tag} attrs={attrs} onChange={onChange} />
          </InspectorSection>
          <TagSection code={writeTagLine(tag, attrs)} onCode={onChange} />
          <button className="danger compact inspector-remove" onClick={() => onChange(null)}>
            Remove the {tag}
          </button>
        </>
      }
    >
      {tag === "timeline" ? <Timeline node={timelineNode(attrs)} /> : <MonthView node={{ kind: "calendar", of: listed(attr(attrs, "type")) }} />}
    </Surface>
  );
}

/**
 * A create button in the visual editor — drawn as it is read, not pressed.
 * The side bar: who sees it, which types it makes (one: a click makes it;
 * several: a click asks which), what it says, its tag.
 */
export function CreateCard({ attrs, onChange }: { attrs: ImageAttrs; onChange: (next: string | null) => void }) {
  const of = listed(attr(attrs, "type"));
  const label = attr(attrs, "label");
  const set = (name: string, value: string) => onChange(writeTagLine("create", withAttr(attrs, name, value)));
  return (
    <Surface
      label="Create button"
      inspector={
        <>
          <AccessSection attrs={attrs} onAttrs={(next) => onChange(writeTagLine("create", next))} />
          <InspectorSection title="Makes">
            <MakesPick of={of} onChange={(names) => set("type", names.join(", "))} />
            <input value={label} placeholder="＋ New …" onChange={(e) => set("label", e.target.value)} />
          </InspectorSection>
          <TagSection code={writeTagLine("create", attrs)} onCode={onChange} />
          <button className="danger compact inspector-remove" onClick={() => onChange(null)}>
            Remove the button
          </button>
        </>
      }
    >
      <CreateButton of={of} label={label} still />
    </Surface>
  );
}

/**
 * An ability row in the visual editor — drawn as it is read. The side bar
 * picks which numbers it shows, in the order picked, and which rule of the
 * world says what stands below each.
 */
export function AbilitiesCard({ attrs, onChange }: { attrs: ImageAttrs; onChange: (next: string | null) => void }) {
  const scope = useContext(RollScopeContext);
  const show = listed(attr(attrs, "show"));
  const rule = attr(attrs, "with");
  const numbers = scope?.numbers ?? [];
  const oneValue = (scope?.lookup.rules ?? []).filter((r) => r.params.length === 1);
  const toggle = (name: string) =>
    onChange(
      writeTagLine(
        "abilities",
        withAttr(attrs, "show", (has(show, name, name) ? show.filter((x) => x.toLowerCase() !== name.toLowerCase()) : [...show, name]).join(", ")),
      ),
    );
  return (
    <Surface
      label="Abilities"
      inspector={
        <>
          <AccessSection attrs={attrs} onAttrs={(next) => onChange(writeTagLine("abilities", next))} />
          <InspectorSection title="Shows">
            <Chips label="Numbers" all={numbers.map((n) => ({ id: n, name: n }))} on={show} onToggle={toggle} hint="none yet" />
            <div className="time-setting">
              <span className="small muted">Below</span>
              <select value={rule} onChange={(e) => onChange(writeTagLine("abilities", withAttr(attrs, "with", e.target.value)))}>
                <option value="">—</option>
                {rule && !oneValue.some((r) => r.name === rule) && <option value={rule}>{rule}</option>}
                {oneValue.map((r) => (
                  <option key={r.name} value={r.name}>
                    {r.name}
                  </option>
                ))}
              </select>
            </div>
          </InspectorSection>
          <TagSection code={writeTagLine("abilities", attrs)} onCode={onChange} />
          <button className="danger compact inspector-remove" onClick={() => onChange(null)}>
            Remove the row
          </button>
        </>
      }
    >
      <Abilities show={show} with={rule} short={listed(attr(attrs, "short"))} />
    </Surface>
  );
}

/**
 * A group's layout in the visual editor — drawn as it is read, with the
 * document's values. The side bar picks the group; the layout itself is
 * changed where it is kept, in the group.
 */
export function ViewCard({ attrs, onChange }: { attrs: ImageAttrs; onChange: (next: string | null) => void }) {
  const data = useWorldData();
  const of = attr(attrs, "of");
  const templates = data?.templates ?? [];
  const group = groupNamed(templates, of);
  const groups = templates
    .filter((t) => t.group && t.blocks[0]?.parts[0]?.text.trim())
    .map((t) => ({ id: t.id, name: t.name, icon: t.icon }))
    .sort((a, b) => a.name.localeCompare(b.name));
  const resolve = linker(data?.articles ?? [], templates);
  return (
    <Surface
      label="Group layout"
      inspector={
        <>
          <AccessSection attrs={attrs} onAttrs={(next) => onChange(writeTagLine("view", next))} />
          <InspectorSection title="Group">
            <SearchPick
              items={groups}
              chosen={group ? [group.id] : []}
              single
              placeholder="Group …"
              none="No group with a layout."
              onAdd={(id) => onChange(writeTagLine("view", withAttr(attrs, "of", findType(templates, id)?.name ?? id)))}
              onRemove={() => onChange(writeTagLine("view", withAttr(attrs, "of", "")))}
            />
          </InspectorSection>
          <TagSection code={writeTagLine("view", attrs)} onCode={onChange} />
          <button className="danger compact inspector-remove" onClick={() => onChange(null)}>
            Remove the layout
          </button>
        </>
      }
    >
      <GroupView of={of} resolve={resolve} onPlace={data?.onPlace ?? (() => {})} />
    </Surface>
  );
}

/**
 * Another page in the visual editor — drawn as it is read. The side bar
 * picks which document it shows.
 */
export function EmbedCard({ attrs, onChange }: { attrs: ImageAttrs; onChange: (next: string | null) => void }) {
  const data = useWorldData();
  const of = attr(attrs, "of");
  const articles = data?.articles ?? [];
  const chosen = articles.find((a) => a.id === of || a.name.toLowerCase() === of.toLowerCase());
  const fold = attr(attrs, "fold");
  const title = attr(attrs, "title");
  const set = (name: string, value: string) => onChange(writeTagLine("embed", withAttr(attrs, name, value)));
  const items = articles
    .filter((a) => !a.pending)
    .map((a) => ({ id: a.id, name: a.name, icon: findType(data?.templates ?? [], a.articleType)?.icon }))
    .sort((a, b) => a.name.localeCompare(b.name));
  return (
    <Surface
      label="Another page"
      inspector={
        <>
          <AccessSection attrs={attrs} onAttrs={(next) => onChange(writeTagLine("embed", next))} />
          <InspectorSection title="Page">
            <SearchPick
              items={items}
              chosen={chosen ? [chosen.id] : []}
              single
              placeholder="Page …"
              none="No such page."
              onAdd={(id) => onChange(writeTagLine("embed", withAttr(attrs, "of", articles.find((a) => a.id === id)?.name ?? id)))}
              onRemove={() => onChange(writeTagLine("embed", withAttr(attrs, "of", "")))}
            />
          </InspectorSection>
          <InspectorSection title="Look">
            <div className="inspector-access block-settings">
              <span className="small muted">Title</span>
              <div className="segmented">
                {(
                  [
                    ["", "Shown"],
                    ["off", "None"],
                  ] as const
                ).map(([v, t]) => (
                  <button key={v} className={title === v ? "on" : ""} onClick={() => set("title", v)}>
                    {t}
                  </button>
                ))}
              </div>
              <span className="small muted">Folds</span>
              <div className="segmented">
                {(
                  [
                    ["", "Open"],
                    ["folded", "Folded"],
                    ["off", "Never"],
                  ] as const
                ).map(([v, t]) => (
                  <button key={v} disabled={title === "off"} className={fold === v ? "on" : ""} onClick={() => set("fold", v)}>
                    {t}
                  </button>
                ))}
              </div>
            </div>
          </InspectorSection>
          <TagSection code={writeTagLine("embed", attrs)} onCode={onChange} />
          <button className="danger compact inspector-remove" onClick={() => onChange(null)}>
            Remove the page
          </button>
        </>
      }
    >
      <Embed of={of} fold={fold} title={title} />
    </Surface>
  );
}
