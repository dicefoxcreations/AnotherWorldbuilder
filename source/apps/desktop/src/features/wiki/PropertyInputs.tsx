import { useRef } from "react";
import { Icon } from "../../shared/ui/Icons";
import { type ArticleView, type Property, type Template, type TemplateField } from "../../api/client";
import { linker, linkText, linkTo, scanLinks, typeNames, type Link } from "../../shared/Links";
import { useMentions } from "../../shared/editor/Mentions";
import { documentsOf, targetOf, articleById } from "../../shared/types";
import { DEFAULT_CALENDAR } from "../../shared/Calendar";
import { findRole, holdsRelations, isRelation, readRelations, relationFields, relationsOf, roleChoices, writeRelations, type RelationField, type WrittenRelation } from "../../shared/Relations";
import { useWorldData } from "../worlds/WorldData";
import { formulaOf, sumOf, summedValues } from "../../shared/Sheet";
import { EveryPick, MultiPick, TextPicks } from "../../shared/ui/MultiPick";
import { OurMomentInput, WrittenMomentPicker } from "../../shared/ui/DateInputs";
import { type Links } from "./EditorTools";

/** The properties the elements in a piece of text bind: `<select bind=Species>`. */
export function boundIn(text: string): { property: string; label: string }[] {
  return [...text.matchAll(/<\w+\b[^>]*?\bbind=(?:"([^"]*)"|([^\s>]+))[^>]*>/g)].map((m) => {
    const label = /\blabel=(?:"([^"]*)"|([^\s>]+))/.exec(m[0]);
    return { property: (m[1] ?? m[2]).trim(), label: label ? (label[1] ?? label[2]) : "" };
  });
}

/** The ids a reference's value holds — one or several. */
const idsOfValue = (v: unknown): string[] => (Array.isArray(v) ? v.map(String) : v ? [String(v)] : []);

export const uniqueName = (props: Property[]) => {
  let n = 1;
  const taken = new Set(props.map((p) => p.name.toLowerCase()));
  while (taken.has(n === 1 ? "property" : `property ${n}`)) n++;
  return n === 1 ? "Property" : `Property ${n}`;
};

/**
 * The input for one property — shaped by the field it stands for.
 *
 * A reference offers the documents of its type; picking one writes
 * `@@Type:Name@@` into the property, and so into the code.
 */
export function PropertyInput({
  field,
  value,
  templates,
  articleType,
  stored,
  selfId,
  all,
  links,
  onValue,
}: {
  field: TemplateField | undefined;
  value: string;
  templates: Template[];
  articleType: string;
  /** The document as saved — what its text leaves out, it still holds. */
  stored?: ArticleView;
  selfId: string;
  all: ArticleView[];
  links: Links;
  onValue: (v: string) => void;
}) {
  const calendar = useWorldData()?.world.calendar ?? DEFAULT_CALENDAR;
  if (isRelation(field)) {
    return <RelationInput field={field} value={value} templates={templates} stored={stored} selfId={selfId} all={all} onValue={onValue} />;
  }
  if (field?.kind === "reference" && field.from) {
    // What points here: shown, never set — it changes where it points from.
    const self = articleById(all, selfId);
    const ids = self ? idsOfValue(summedValues(templates, all, self)[field.key]) : [];
    const names = ids.map((id) => articleById(all, id)?.name).filter(Boolean);
    return (
      <span className="small muted" title={`Set by their “${field.from}”`}>
        {names.length ? names.join(", ") : "—"} · from their “{field.from}”
      </span>
    );
  }
  if (field?.kind === "reference") {
    const target = targetOf(templates, articleType, field);
    const choices = target ? documentsOf(all, templates, target).filter((a) => a.id !== selfId) : [];
    const resolve = linker(all, templates);
    // Each link with what stands after it — a pick's own number: `@@Skill:Sailing@@ 3`.
    const chosen: { link: Link; article: ArticleView | undefined; count: string }[] = [];
    for (const piece of scanLinks(value)) {
      if (typeof piece !== "string") chosen.push({ link: piece, article: resolve(piece), count: "" });
      else if (chosen.length) chosen[chosen.length - 1].count = piece.replace(/,/g, " ").trim();
    }
    // A link nobody here can resolve — a closed document, a typo in the
    // code — is written back as it stood instead of vanishing.
    const write = (items: typeof chosen) =>
      items
        .map((c) => (c.article ? linkTo(templates, c.article) : linkText(c.link.of, c.link.name)) + (field.count && c.count ? ` ${c.count}` : ""))
        .join(", ");
    // Every document of the type, always: only their numbers change, and only
    // a number other than 0 is written.
    if (field.every) {
      const countOf = (c: ArticleView) => chosen.find((x) => x.article?.id === c.id)?.count ?? "";
      return (
        <EveryPick
          items={choices.map((c) => ({ key: c.id, name: c.name, count: countOf(c) }))}
          count={field.count || "Number"}
          onCount={(i, n) => {
            const next = choices.map((c, j) => ({ link: { of: "", name: c.name, property: "", text: c.name }, article: c, count: j === i ? n.trim() : countOf(c) }));
            onValue(write(next.filter((c) => c.count !== "" && Number(c.count) !== 0)));
          }}
        />
      );
    }
    if (field.multiple) {
      const open = choices.filter((c) => !chosen.some((x) => x.article?.id === c.id));
      return (
        <MultiPick
          chosen={chosen.map((c, i) => ({ key: c.article?.id ?? `?${i}`, name: c.article?.name ?? `${c.link.name} (?)`, count: c.count }))}
          open={open}
          onRemove={(i) => onValue(write(chosen.filter((_, j) => j !== i)))}
          onAdd={(c) => onValue(write([...chosen, { link: { of: "", name: c.name, property: "", text: c.name }, article: c, count: field.count ? "1" : "" }]))}
          count={field.count || undefined}
          onCount={(i, n) => onValue(write(chosen.map((c, j) => (j === i ? { ...c, count: n.trim() } : c))))}
        />
      );
    }
    const current = chosen[0];
    const known = current?.article && choices.some((c) => c.id === current.article?.id);
    return (
      <select
        value={current ? (known ? current.article!.id : "?") : ""}
        onChange={(e) => {
          const picked = choices.find((c) => c.id === e.target.value);
          if (picked) onValue(linkTo(templates, picked));
          else if (e.target.value === "") onValue("");
        }}
      >
        <option value="">— none —</option>
        {current && !known && <option value="?">{current.link.name} (?)</option>}
        {choices.map((c) => (
          <option key={c.id} value={c.id}>
            {c.name}
          </option>
        ))}
      </select>
    );
  }
  if (field?.kind === "list") {
    const items = value.split(",").map((t) => t.trim()).filter(Boolean);
    return <TextPicks items={items} onItems={(next) => onValue(next.join(", "))} />;
  }
  if (field?.kind === "world-date") return <WrittenMomentPicker calendar={calendar} value={value} onValue={onValue} />;
  if (field?.kind === "date-time") return <OurMomentInput value={value} onValue={onValue} />;
  if (field?.kind === "number") {
    // Empty, a reckoned number shows what its formula comes to — to be
    // overwritten by a value of its own.
    const formula = formulaOf(field);
    const reckons = formula !== undefined && !value.trim() && stored ? sumOf(templates, all, stored, field.label).own : undefined;
    const hint = formula === undefined ? undefined : reckons === undefined ? "formula" : String(reckons);
    // Text, not a number field: that one shows 24 as “24,0” and swallows what it cannot read.
    return (
      <span className="number-input">
        <input
          inputMode="decimal"
          size={Math.max(2, (value || hint || "").length + 1)}
          value={value}
          placeholder={hint}
          title={formula === undefined ? undefined : `[[${formula}]]`}
          onChange={(e) => onValue(e.target.value)}
        />
        {field.unit && <span className="unit">{field.unit}</span>}
      </span>
    );
  }
  return (
    <TextProperty
      multiline={field?.kind === "text" && field.multiline}
      value={value}
      links={links}
      onValue={onValue}
    />
  );
}

/**
 * Relations, one line each: who, what they are to this document, and what
 * it is to them. Picking writes the property's text — and so the code.
 *
 * An entry whose person this reader cannot open is not in the text at all;
 * it stands here greyed out and cannot be changed. Saving keeps it.
 */
function RelationInput({
  field,
  value,
  templates,
  stored,
  selfId,
  all,
  onValue,
}: {
  field: RelationField;
  value: string;
  templates: Template[];
  stored?: ArticleView;
  selfId: string;
  all: ArticleView[];
  onValue: (v: string) => void;
}) {
  const resolve = linker(all, templates);
  const rows = readRelations(value);
  const people = documentsOf(all, templates, field.articleType).filter((a) => a.id !== selfId);
  const roles = roleChoices(templates, all, field);
  const name = (id: string) => articleById(all, id)?.name ?? "???";
  const saved = stored ? relationsOf(stored, field.key) : [];
  const locked = saved.filter((e) => !articleById(all, e.with));
  const write = (next: WrittenRelation[]) => onValue(writeRelations(next));
  const set = (i: number, part: Partial<WrittenRelation>) => write(rows.map((r, j) => (j === i ? { ...r, ...part } : r)));
  const linkFor = (a: ArticleView): WrittenRelation["link"] => ({
    of: typeNames(templates, a.articleType)[0] ?? "",
    name: a.name,
    property: "",
    text: a.name,
  });
  // Written, saved — and the other side says nothing back.
  const oneSided = (target: ArticleView | undefined) => {
    if (!target || !saved.some((e) => e.with === target.id)) return false;
    const theirs = relationFields(templates, target.articleType).find(
      (f) => f.key === field.key || f.label.toLowerCase() === field.label.toLowerCase(),
    );
    return !!theirs && holdsRelations(target, theirs) && !relationsOf(target, theirs.key).some((e) => e.with === selfId);
  };
  const rolePicker = (text: string, onPick: (name: string) => void) => {
    const known = findRole(roles, text);
    return (
      <select value={known ? known.id : text ? "?" : ""} onChange={(e) => onPick(roles.find((r) => r.id === e.target.value)?.name ?? "")}>
        <option value="">—</option>
        {text && !known && <option value="?">{text} (?)</option>}
        {roles.map((r) => (
          <option key={r.id} value={r.id}>
            {r.name}
          </option>
        ))}
      </select>
    );
  };
  const present = new Set(rows.map((r) => resolve(r.link)?.id));
  // Whom one can still add: everyone of the property's type not listed yet.
  const addable = people.filter((p) => !present.has(p.id));
  return (
    <div className="relation-rows">
      {rows.map((row, i) => {
        const target = resolve(row.link);
        return (
          <div key={i} className="relation-row">
            <select
              value={target ? target.id : "?"}
              onChange={(e) => {
                const picked = people.find((p) => p.id === e.target.value);
                if (picked) set(i, { link: linkFor(picked) });
              }}
            >
              {!target && <option value="?">{row.link.name} (?)</option>}
              {people.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
            {rolePicker(row.role, (role) => set(i, { role }))}
            <span className="muted">/</span>
            {rolePicker(row.ownRole, (ownRole) => set(i, { ownRole }))}
            {oneSided(target) && (
              <span className="small muted" title="The other document does not say it back">
                one-sided
              </span>
            )}
            <button className="bubble-x" title="Remove" onClick={() => write(rows.filter((_, j) => j !== i))}>
              <Icon name="close" />
            </button>
          </div>
        );
      })}
      {locked.map((e, i) => (
        <div key={`locked-${i}`} className="relation-row locked" title="Not yours to change">
          <span>???</span>
          <span>{e.role ? name(e.role) : "—"}</span>
          <span className="muted">/</span>
          <span>{e.ownRole ? name(e.ownRole) : "—"}</span>
        </div>
      ))}
      {addable.length > 0 ? (
        <select
          className="compact"
          value=""
          onChange={(e) => {
            const picked = people.find((p) => p.id === e.target.value);
            if (picked) write([...rows, { link: linkFor(picked), role: "", ownRole: "" }]);
          }}
        >
          <option value="">＋ Relation</option>
          {addable.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      ) : (
        <span className="small muted">Related to everyone already.</span>
      )}
    </div>
  );
}

/** A property written as text — where `@@` links a document, as in the body. */
function TextProperty({
  multiline,
  value,
  links,
  onValue,
}: {
  multiline: boolean;
  value: string;
  links: Links;
  onValue: (v: string) => void;
}) {
  const area = useRef<HTMLTextAreaElement>(null);
  const line = useRef<HTMLInputElement>(null);
  const mentions = useMentions({ field: multiline ? area : line, text: value, onText: onValue, ...links });
  return (
    <>
      {multiline ? (
        <textarea
          {...mentions.handlers}
          ref={area}
          className="grows"
          rows={3}
          value={value}
          onChange={(e) => onValue(e.target.value)}
        />
      ) : (
        <input {...mentions.handlers} ref={line} size={Math.max(4, value.length + 1)} value={value} onChange={(e) => onValue(e.target.value)} />
      )}
      {mentions.element}
    </>
  );
}
