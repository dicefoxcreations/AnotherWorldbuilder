import { useState } from "react";
import type { ArticleView, DocumentText, GraphNode, Template, WorldInfo } from "../../src/api/client";
import { ArticlePage } from "../../src/features/wiki/ArticlePage";
import { VisualEditor } from "../../src/features/wiki/Document";
import { CurrentDocument, WorldDataContext } from "../../src/features/worlds/WorldData";
import { RelationGraph, type GraphEdit } from "../../src/features/wiki/RelationGraph";

/**
 * Relations in a small family: the web, the family tree, and the graph as
 * it is edited — on documents held in memory. Saving both sides is Rust's
 * job; here `onSet` does the same by hand.
 */

const CHARACTER: Template = {
  id: "character",
  name: "Character",
  icon: "🧑",
  supertype: "",
  group: false,
  rules: [],
  description: "",
  properties: [{ key: "rel", label: "Relations", kind: "relation", articleType: "character", roleType: "relation" }],
  infobox: { align: "none", width: "auto", show: [] },
  auth: null,
  namePattern: "%%input%%",
  page: "",
  blocks: [],
};
const RELATION: Template = { ...CHARACTER, id: "relation", name: "Relation", icon: "🔗", properties: [] };

const ROLES = ["Mother", "Father", "Daughter", "Son", "Wife", "Husband", "Friend", "Enemy", "Nemesis", "Sister", "Brother"];
const PEOPLE = ["Greta", "Hartmut", "Olga", "Marie", "Olaf", "Ada", "Kurt", "Uwe", "Tim", "Simon", "Franklin", "Anna"];

const id = (name: string) => name.toLowerCase();
type Raw = { with: string; role?: string; own_role?: string };

function article(name: string, articleType: string, rel: Raw[] = []): ArticleView {
  return {
    id: id(name),
    articleType,
    slug: id(name),
    name,
    visibility: "all",
    layerTotal: 1,
    layers: [
      {
        id: `${id(name)}-p`,
        title: "",
        section: "properties",
        mayEdit: true,
        text: "",
        fields: { rel },
        policy: "all",
        epoch: 1,
        parent: null,
      } as unknown as ArticleView["layers"][number],
    ],
  };
}

/** Both sides of every relation, as Rust stores them. */
function family(): Map<string, Raw[]> {
  const out = new Map<string, Raw[]>(PEOPLE.map((p) => [id(p), []]));
  const relate = (a: string, b: string, roleOfB: string, roleOfA: string) => {
    out.get(id(a))!.push({ with: id(b), role: id(roleOfB), own_role: id(roleOfA) });
    out.get(id(b))!.push({ with: id(a), role: id(roleOfA), own_role: id(roleOfB) });
  };
  relate("Hartmut", "Greta", "Mother", "Son");
  relate("Hartmut", "Olga", "Wife", "Husband");
  relate("Marie", "Olga", "Mother", "Daughter");
  relate("Marie", "Hartmut", "Father", "Daughter");
  relate("Olaf", "Olga", "Mother", "Son");
  relate("Olaf", "Hartmut", "Father", "Son");
  relate("Marie", "Olaf", "Brother", "Sister");
  relate("Marie", "Ada", "Friend", "Friend");
  relate("Olaf", "Kurt", "Nemesis", "Nemesis");
  relate("Uwe", "Olga", "Mother", "Son");
  // The user's test world, exactly.
  relate("Tim", "Franklin", "Father", "Son");
  relate("Tim", "Anna", "Mother", "Son");
  relate("Tim", "Simon", "Brother", "Brother");
  relate("Simon", "Franklin", "Father", "Son");
  relate("Simon", "Anna", "Mother", "Son");
  // Someone this reader cannot open: Ada's mother.
  out.get("ada")!.push({ with: "art:secret", role: "mother", own_role: "daughter" });
  return out;
}

const WORLD: WorldInfo = {
  id: "w",
  name: "Test",
  icon: "🌍",
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
  waiting: 0, versionLimit: 50,
};

/** Tim's page as the test world has it: one block with the tree. */
const TIM_TREE = '<graph around=self depth=2>\n</graph>\n';
const TIM_DOC: DocumentText = {
  name: "Tim",
  access: { view: "all", edit: "gm" },
  properties: [
    { name: "Relations", value: "@@Character:Franklin@@ Father/Son, @@Character:Anna@@ Mother/Son, @@Character:Simon@@ Brother/Brother" },
  ],
  blocks: [
    { key: "tim-rel", title: "Relations", style: "text", parts: [{ key: "tim-rel", text: TIM_TREE, view: "all", edit: "gm" }] },
  ],
};

const TREE = {
  of: "",
  via: [],
  around: "self",
  depth: 2,
  people: [] as GraphNode["people"],
  lines: [] as GraphNode["lines"],
  show: [] as string[],
};

export function RelationsStage() {
  const [rel, setRel] = useState(family);
  const [timDoc, setTimDoc] = useState(TIM_DOC);
  const [olafGraph, setOlafGraph] = useState<GraphNode>({ ...TREE, depth: 1 });
  const [propsOpen, setPropsOpen] = useState(true);
  const articles = [
    ...PEOPLE.map((p) => article(p, "character", rel.get(id(p)))),
    ...ROLES.map((r) => article(r, "relation")),
  ];
  const edit: GraphEdit = {
    overrides: new Map(),
    onSet: async (changes) => {
      setRel((old) => {
        const next = new Map(old);
        for (const { from, to, roles } of changes) {
          if (!next.has(from) || !next.has(to)) continue;
          next.set(from, [...next.get(from)!.filter((e) => e.with !== to), ...(roles ? [{ with: to, role: roles[0], own_role: roles[1] }] : [])]);
          next.set(to, [...next.get(to)!.filter((e) => e.with !== from), ...(roles ? [{ with: from, role: roles[1], own_role: roles[0] }] : [])]);
        }
        return next;
      });
    },
  };
  const data = { world: WORLD, templates: [CHARACTER, RELATION], articles, onPlace: () => {}, onChanged: () => {}, onApart: () => {} };
  return (
    <WorldDataContext.Provider value={data}>
      <div className="body-wrap relations-stage">
        <h4>Tim's page, as the app shows it</h4>
        <div id="page">
          <ArticlePage
            world={WORLD}
            article={timPage(articles)}
            templates={[CHARACTER, RELATION]}
            all={articles}
            onPlace={() => {}}
            onChanged={() => {}}
            onError={() => {}}
          />
        </div>
        <h4>Tim, edited visually</h4>
        <div id="visual" className="document">
          <VisualEditor
            world={WORLD}
            article={timPage(articles)}
            templates={[CHARACTER, RELATION]}
            all={articles}
            doc={timDoc}
            propsOpen={propsOpen}
            onPropsOpen={setPropsOpen}
            links={{ all: articles, templates: [CHARACTER, RELATION], selfId: "tim" }}
            carets={() => []}
            marks={{ kind: "added", of: () => [] }}
            onDoc={setTimDoc}
            onCaret={() => {}}
          />
          {/* What the visual side wrote — for the scenarios to read. */}
          <pre id="tim-code" hidden>
            {timDoc.blocks[0].parts[0].text}
          </pre>
        </div>
        <h4>Parents' line joined to their sons' line</h4>
        <div id="bridge" style={{ maxWidth: 760 }}>
          <RelationGraph
            query={{
              ...TREE,
              around: "",
              // As in the test world: neither pair stands quite above the other.
              people: [
                { who: "Simon", x: 1, y: -2, color: "", sub: "", note: "" },
                { who: "Anna", x: 23, y: -2, color: "", sub: "", note: "" },
                { who: "Franklin", x: 3, y: 9, color: "", sub: "", note: "" },
                { who: "Tim", x: 25, y: 9, color: "", sub: "", note: "" },
              ],
              lines: [
                { between: ["Tim", "Franklin"], color: "", sides: [], shape: "straight", arrow: "", label: "", dock: [], group: [], say: [] },
                { between: ["Simon", "Anna"], color: "", sides: ["right", "left"], shape: "straight", arrow: "", label: "", dock: [], group: [], say: [] },
                { between: ["Anna & Simon", "Tim & Franklin"], color: "", sides: [], shape: "straight", arrow: "", label: "", dock: ["", "64"], group: ["", "each"], say: ["Parents", ""] },
              ],
            }}
          />
        </div>
        <h4>A line docked where its line's label stands</h4>
        <div id="crossed" style={{ maxWidth: 760 }}>
          <RelationGraph
            query={{
              ...TREE,
              around: "",
              people: [
                { who: "Franklin", x: -1, y: -3, color: "", sub: "", note: "" },
                { who: "Anna", x: 19, y: -3, color: "", sub: "", note: "" },
                { who: "Tim", x: 3, y: 9, color: "", sub: "", note: "" },
                { who: "Simon", x: 21, y: 9, color: "", sub: "", note: "" },
              ],
              lines: [
                { between: ["Simon", "Tim"], color: "", sides: [], shape: "straight", arrow: "", label: "", dock: [], group: [], say: [] },
                { between: ["Anna & Franklin", "Simon & Tim"], color: "", sides: [], shape: "straight", arrow: "", label: "", dock: ["", "55"], group: [], say: [] },
              ],
            }}
          />
        </div>
        <h4>Tim, as in the test world</h4>
        <div id="tim" className="flowing-text" style={{ maxWidth: 760 }}>
          <CurrentDocument.Provider value="tim">
            <RelationGraph query={{ ...TREE, depth: 2 }} />
          </CurrentDocument.Provider>
        </div>
        <h4>Around Marie, three steps</h4>
        <div id="family">
          <CurrentDocument.Provider value="marie">
            <RelationGraph query={{ ...TREE, depth: 3 }} />
          </CurrentDocument.Provider>
        </div>
        <h4>Every character</h4>
        <div id="web">
          <RelationGraph query={{ ...TREE, of: "character", around: "" }} />
        </div>
        <h4>Without parents: friends and enemies, in steps from Ada</h4>
        <div id="steps">
          <CurrentDocument.Provider value="ada">
            <RelationGraph query={{ ...TREE, depth: 3 }} />
          </CurrentDocument.Provider>
        </div>
        <h4>Editing around Olaf</h4>
        <div id="edit">
          <CurrentDocument.Provider value="olaf">
            <RelationGraph
              query={olafGraph}
              edit={{ ...edit, onGraph: (g) => g && setOlafGraph(g) }}
            />
          </CurrentDocument.Provider>
        </div>
      </div>
    </WorldDataContext.Provider>
  );
}

/** Tim with the block that holds his tree. */
function timPage(articles: ArticleView[]): ArticleView {
  const tim = articles.find((a) => a.id === "tim")!;
  return {
    ...tim,
    layers: [
      ...tim.layers,
      { ...tim.layers[0], id: "tim-rel", section: "tim-rel", title: "Relations", text: TIM_TREE, fields: {} },
    ],
  };
}
