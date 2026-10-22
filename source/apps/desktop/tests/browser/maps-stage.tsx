import { useState } from "react";
import type { ArticleView, DocumentText, Template, WorldInfo } from "../../src/api/client";
import { ArticlePage } from "../../src/features/wiki/ArticlePage";
import { VisualEditor } from "../../src/features/wiki/Document";
import { WorldDataContext } from "../../src/features/worlds/WorldData";
import { DiceTray } from "../../src/features/dice/DiceTray";
import { LinkPreview } from "../../src/features/wiki/LinkPreview";
import { setFixture } from "./core";

/**
 * Maps: a harbour to drag and zoom, its pins — and a stair down to the
 * cellar, a map of its own, gone to in place.
 */

const WORLD = {
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
  waiting: 0,
  versionLimit: 50,
} as unknown as WorldInfo;

/** A picture 1600 by 1000: sea, a shore, a tower. */
const PICTURE =
  "data:image/svg+xml;utf8," +
  encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="1000" viewBox="0 0 160 100"><rect width="160" height="100" fill="#2b5d73"/><path d="M0 70 C40 55 80 80 120 60 S160 65 160 65 V100 H0Z" fill="#c9b27c"/><rect x="110" y="30" width="8" height="30" fill="#ddd"/></svg>',
  );

const PLACE: Template = {
  id: "place",
  name: "Place",
  icon: "📍",
  supertype: "",
  group: false,
  rules: [],
  description: "",
  properties: [],
  infobox: { align: "none", width: "auto", show: [] },
  auth: null,
  namePattern: "%%input%%",
  page: "",
  blocks: [{ key: "text", title: "", style: "text", parts: [{ key: "text", text: "", view: "all", edit: "all", level: "open" }] }],
};

const HARBOUR = `The harbour, from above.

<map src=img:harbour height=400>
<pin at=30,40 label="Stair down" to=cellar color=amber icon=stairs>
<pin at=72.5,45 label=Lighthouse label-at=bottom>
<area at="40,55 60,52 66,80 38,85" label="Old town" to=cellar color=green>
</map>

<create type=Place label="New place">
`;
const CELLAR = `<map src=img:cellar height=400>
<pin at=50,50 label="Stair up" to=harbour>
</map>
`;

const article = (id: string, name: string, text: string): ArticleView =>
  ({
    id,
    articleType: "place",
    slug: id,
    name,
    visibility: "all",
    layerTotal: 1,
    layers: [{ id: `${id}-text`, title: "", section: "text", mayEdit: true, text, fields: {}, policy: "all", level: "open", epoch: 1, parent: null }],
  }) as unknown as ArticleView;

// A pin set apart for the GM — a part of its own, as saving leaves it.
const SECRET = `<pins for=img:harbour>
<pin at=15,20 label="Smugglers' cave">
</pins>`;

const ARTICLES = [article("harbour", "Harbour", HARBOUR), article("cellar", "Cellar", CELLAR)].map((a) =>
  a.id === "harbour"
    ? ({ ...a, layers: [...a.layers, { ...a.layers[0], id: "harbour-secret", section: "text-2", text: SECRET, policy: "gm" }] } as ArticleView)
    : a,
);

const HARBOUR_DOC: DocumentText = {
  name: "Harbour",
  access: { view: "all", edit: "all", level: "open" },
  properties: [],
  blocks: [{ key: "text", title: "", style: "text", parts: [{ key: "text", text: HARBOUR, view: "all", edit: "all", level: "open" }] }],
};

export function MapsStage() {
  setFixture("image_data", PICTURE);
  const [doc, setDoc] = useState(HARBOUR_DOC);
  const [placed, setPlaced] = useState("");
  const data = { world: WORLD, templates: [PLACE], articles: ARTICLES, onPlace: (o: { id?: string }) => setPlaced(o.id ?? ""), onChanged: () => {}, onApart: () => {}, onNew: (t: string) => setPlaced(`new ${t}`) };
  return (
    <WorldDataContext.Provider value={data as never}>
      <DiceTray world={WORLD}>
        <LinkPreview />
        <div className="body-wrap">
          <div id="read">
            <ArticlePage world={WORLD} article={ARTICLES[0]} templates={[PLACE]} all={ARTICLES} onPlace={data.onPlace as never} onChanged={() => {}} onError={() => {}} />
          </div>
          {/* Without a type, as the world's page: its sections are loose, the pins' part with them. */}
          <div id="loose">
            <ArticlePage world={WORLD} article={ARTICLES[0]} templates={[]} all={ARTICLES} onPlace={data.onPlace as never} onChanged={() => {}} onError={() => {}} glance />
          </div>
          {/* A document the player asked for, waiting on the GM's app. */}
          <div id="pending">
            <ArticlePage world={WORLD} article={{ ...ARTICLES[1], id: "den", name: "Smugglers' den", pending: true }} templates={[PLACE]} all={ARTICLES} onPlace={data.onPlace as never} onChanged={() => {}} onError={() => {}} />
          </div>
          <p id="placed">{placed}</p>
          <button id="placed-reset" onClick={() => setPlaced("")}>reset</button>
          <h4>The harbour, edited visually</h4>
          <div id="visual" className="document">
            <VisualEditor
              world={WORLD}
              article={ARTICLES[0]}
              templates={[PLACE]}
              all={ARTICLES}
              doc={doc}
              propsOpen={false}
              onPropsOpen={() => {}}
              links={{ all: ARTICLES, templates: [PLACE], selfId: "harbour" }}
              carets={() => []}
              marks={{ kind: "added", of: () => [] }}
              onDoc={setDoc}
              onCaret={() => {}}
            />
          </div>
          <pre id="map-text">{doc.blocks[0].parts[0].text}</pre>
        </div>
      </DiceTray>
    </WorldDataContext.Provider>
  );
}
