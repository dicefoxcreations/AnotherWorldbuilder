import { useState } from "react";
import type { ArticleView, Template, WorldInfo } from "../../api/client";
import { familyId, findType, isA, mayCreate, parentField, realTypes, targetOf, typeOf, articleById } from "../../shared/types";
import { MultiPick } from "../../shared/ui/MultiPick";
import { InspectorSection } from "./Inspector";
import { byText } from "../../shared/Collate";

/**
 * Which type a document is — and another one for it, with what it then
 * belongs to where that type has a parent: an NPC that turns out to be
 * part of a character's past. Its text and the values both types have stay.
 * Only types of its family are offered.
 */
export function TypeRows({
  world,
  article,
  templates,
  all,
  onRetype,
}: {
  world: WorldInfo;
  article: ArticleView;
  templates: Template[];
  all: ArticleView[];
  onRetype: (articleType: string, parents: string[]) => Promise<void>;
}) {
  const [to, setTo] = useState(article.articleType);
  const [parents, setParents] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  // Within its family only: an item may become an armor, never an NPC.
  const now = findType(templates, article.articleType);
  const family = now ? familyId(templates, now) : null;
  const choices = realTypes(templates)
    .filter((t) => t.id === article.articleType || (mayCreate(world, t) && (family === null || familyId(templates, t) === family)))
    .sort((a, b) => byText(a.name, b.name));
  if (choices.length < 2) return null;
  const changed = to !== article.articleType;
  const field = changed ? parentField(typeOf(templates, to)) : undefined;
  const several = field?.kind === "reference" && field.multiple;
  const of = field ? targetOf(templates, to, field) : "";
  const open = field ? all.filter((a) => a.id !== article.id && !parents.includes(a.id) && isA(templates, a.articleType, of)) : [];
  const name = (id: string) => articleById(all, id)?.name ?? "???";
  const pick = (id: string) => setParents(several ? [...parents, id] : [id]);
  return (
    <InspectorSection title="Type">
      <div className="inspector-access">
        <span className="small muted">Is a</span>
        <select
          value={to}
          onChange={(e) => {
            setTo(e.target.value);
            setParents([]);
          }}
        >
          {choices.map((t) => (
            <option key={t.id} value={t.id}>
              {t.icon ? `${t.icon} ` : ""}
              {t.name}
            </option>
          ))}
        </select>
        {field && (
          <>
            <span className="small muted">{field.label}</span>
            <MultiPick
              chosen={parents.map((id) => ({ key: id, name: name(id) }))}
              open={several || parents.length === 0 ? open : []}
              onRemove={(i) => setParents(parents.filter((_, j) => j !== i))}
              onAdd={(a) => pick(a.id)}
            />
          </>
        )}
        {changed && (
          <>
            <span />
            <button
              className="compact"
              disabled={busy}
              onClick={() => {
                setBusy(true);
                onRetype(to, parents).finally(() => setBusy(false));
              }}
            >
              Change type
            </button>
          </>
        )}
      </div>
    </InspectorSection>
  );
}
