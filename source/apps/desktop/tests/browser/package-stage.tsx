import { useEffect, useState } from "react";
import type { ArticleView, Template, WorldInfo } from "../../src/api/client";
import type { WorldCalendar } from "../../src/shared/Calendar";
import { DEFAULT_CALENDAR } from "../../src/shared/Calendar";
import { ArticlePage } from "../../src/features/wiki/ArticlePage";
import { WorldDataContext } from "../../src/features/worlds/WorldData";
import { VisualEditor } from "../../src/features/wiki/Document";
import type { DocumentText, TemplateBlock } from "../../src/api/client";

/**
 * A package's documents as the GM reads them — to look at real content
 * without a world. The package lies in `private/package.json` (never
 * committed); `?doc=Name` picks the page, `&edit` shows it in the visual
 * editor — its sections built as the core builds them, own shape and all.
 */

type Packed = {
  types: Template[];
  calendar?: WorldCalendar;
  documents: {
    id: string;
    type: string;
    slug: string;
    name: string;
    view: string;
    edit: string;
    parts: { section: string; title: string; text: string; fields: Record<string, unknown>; view: string; edit: string }[];
  }[];
};

const WORLD: WorldInfo = {
  id: "w",
  name: "Package",
  icon: "📦",
  publicKey: "",
  tokenFile: "",
  folder: "",
  storage: "local",
  cloudFolders: [],
  role: "gm",
  isGm: true,
  attributes: [],
  members: [],
  articleCount: 0,
  home: "",
  requests: 0,
  invited: [],
  handover: null,
  moderated: false,
  waiting: 0,
  versionLimit: 50,
  dice: [],
  calendar: DEFAULT_CALENDAR,
};

const asView = (d: Packed["documents"][number]): ArticleView => ({
  id: d.id,
  articleType: d.type,
  slug: d.slug,
  name: d.name,
  visibility: d.view,
  edit: d.edit,
  layerTotal: d.parts.length,
  layers: d.parts.map((p, i) => ({ id: `${d.id}-${i}`, title: p.title, section: p.section, mayEdit: true, text: p.text, fields: p.fields ?? {}, policy: p.view, edit: p.edit, epoch: 0 })),
});

/** The document as the editor gets it: a section per part key, the first part's shape its look. */
const asText = (d: Packed["documents"][number]): DocumentText => {
  const blocks: TemplateBlock[] = [];
  for (const p of d.parts) {
    if (p.section === "properties" || p.section === "infobox") continue;
    const key = p.section.replace(/-\d+$/, "");
    const at = blocks.find((b) => b.key === key);
    const part = { key: p.section, text: p.text, view: p.view, edit: p.edit };
    if (at) at.parts.push(part);
    else blocks.push({ key, title: p.title, style: "text", heading: 1, ...((p.fields?.shape as object) ?? {}), parts: [part] } as TemplateBlock);
  }
  return { name: d.name, access: { view: d.view, edit: d.edit }, properties: [], blocks };
};

export function PackageStage() {
  const [packed, setPacked] = useState<Packed | null>(null);
  const [name, setName] = useState(new URLSearchParams(location.search).get("doc") ?? "");
  const editing = new URLSearchParams(location.search).has("edit");
  const [doc, setDoc] = useState<DocumentText | null>(null);
  useEffect(() => {
    fetch("/private/package.json")
      .then((r) => r.json())
      .then(setPacked)
      .catch(() => {});
  }, []);
  if (!packed) return <p>No package in private/package.json.</p>;
  const world = { ...WORLD, calendar: packed.calendar ?? DEFAULT_CALENDAR };
  const articles = packed.documents.map(asView);
  const article = articles.find((a) => a.name === name) ?? articles[0];
  const place = (o: { kind: string; id?: string }) => {
    const to = articles.find((a) => a.id === o.id);
    if (to) setName(to.name);
  };
  const data = { world, templates: packed.types, articles, onPlace: place, onChanged: () => {}, onApart: () => {} };
  return (
    <WorldDataContext.Provider value={data}>
      <div className="body-wrap package-stage">
        {editing ? (
          <div className="document">
            <VisualEditor
              world={world}
              article={article}
              templates={packed.types}
              all={articles}
              doc={doc ?? asText(packed.documents.find((d) => d.id === article.id)!)}
              propsOpen={false}
              onPropsOpen={() => {}}
              links={{ all: articles, templates: packed.types, selfId: article.id }}
              carets={() => []}
              marks={{ kind: "added", of: () => [] }}
              onDoc={setDoc}
              onCaret={() => {}}
            />
          </div>
        ) : (
          <ArticlePage world={world} article={article} templates={packed.types} all={articles} onPlace={place} onChanged={() => {}} onError={() => {}} />
        )}
      </div>
    </WorldDataContext.Provider>
  );
}
