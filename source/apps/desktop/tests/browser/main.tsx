import { useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { TemplateEditor } from "../../src/features/templates/TemplateEditor";
import { ArticleArea } from "../../src/features/wiki/Article";
import { Legend } from "../../src/features/templates/TemplateLegend";
import { Options } from "../../src/features/settings/Options";
import { StorageSetup } from "../../src/features/settings/Storage";
import type { ArticleView, IdentityInfo, Template, WorldInfo } from "../../src/api/client";
import { setDirectory, setFixture } from "./core";
import { VersionsPanel } from "../../src/features/wiki/Versions";
import { GalleryArea } from "../../src/features/gallery/Gallery";
import { ImageCard, imageSurface } from "../../src/features/wiki/ImageCard";
import { writeImageLine, type ImageAttrs } from "../../src/shared/ImageTag";
import { FlowField, FlowHandle } from "../../src/shared/editor/FlowField";
import { pieces as cut } from "../../src/shared/GraphText";
import { ImagePicker } from "../../src/shared/ui/ImagePicker";
import { MarkdownText } from "../../src/features/wiki/MarkdownText";
import { MarkedField } from "../../src/shared/editor/MarkedField";
import { changes, marked } from "../../src/shared/Marks";
import { RelationsStage } from "./relations-stage";
import { StatsStage } from "./stats-stage";
import { MapsStage } from "./maps-stage";
import { PackageStage } from "./package-stage";
import { WorldDataContext } from "../../src/features/worlds/WorldData";
import { linker } from "../../src/shared/Links";
import { followKeptColors } from "../../src/shared/Theme";
import "../../src/styles/global.css";
import "./testbench.css";

// The reader's colours, as the app draws with them.
followKeptColors();

/**
 * The test bench.
 *
 * A page that connects the real components with the real Rust core —
 * without a world, without keys, without a file. With it the interface can
 * really be used: typing, clicking, hovering with the mouse. Exactly the bugs
 * one only finds that way used to end up with the user.
 *
 * It is not part of the app and is not shipped. Start it:
 *
 * ```text
 * cargo run -p aw-testbench      # the core
 * npm run testbench              # the page
 * ```
 */

const WORLD: WorldInfo = {
  id: "test-world",
  name: "Test world",
  icon: "🌍",
  publicKey: "",
  tokenFile: "",
  folder: "",
  role: "gm",
  isGm: true,
  attributes: ["class:rogue", "house:raven"],
  members: [
    { id: "me", name: "Tester", fingerprint: "ab cd ef", role: "gm", attributes: ["gm", "players", "all"], trusted: false },
    { id: "id:0000000000000002", name: "Bob", fingerprint: "12 34 56", role: "player", attributes: ["players", "all", "class:rogue"], trusted: false },
  ],
  requests: 0,
  invited: ["id:00000000000000c3"],
  handover: null,
  moderated: true,
  waiting: 0,
  versionLimit: 50,
  articleCount: 0,
  home: "",
};

// The players page asks for these; the core does not have them.
setFixture("join_requests", []);
setFixture("waiting_versions", []);

const ME: IdentityInfo = {
  name: "Tester",
  id: "me",
  fingerprint: "ab cd ef",
  file: "",
  publicKey: "",
  avatar: null,
};

/** An empty template — the same one “New” creates in the app. */
const emptyTemplate = (): Template => ({
  id: "",
  name: "",
  icon: "📄",
  supertype: "",
  group: false,
  rules: [],
  description: "",
  properties: [],
  infobox: { align: "none", width: "auto", show: [] },
  auth: null,
  namePattern: "%%input%%",
  page: "",
  blocks: [],
});

const CHARACTER: Template = {
  ...emptyTemplate(),
  id: "character",
  name: "Character",
  icon: "🧑",
  properties: [
    { key: "portrait", label: "Portrait", kind: "image", width: 200, height: 260, align: "center" },
    { key: "title", label: "Title", kind: "text", multiline: false },
    { key: "gender", label: "Gender", kind: "reference", articleType: "gender", multiple: false, parent: false },
  ],
  infobox: {
    align: "right",
    width: "auto",
    show: [
      { element: "image", property: "Portrait", label: "" },
      { element: "text", property: "Title", label: "" },
      { element: "select", property: "Gender", label: "" },
    ],
  },
  blocks: [
    {
      key: "story",
      title: "What they say",
      style: "text",
      parts: [
        {
          key: "story",
          text: "She came in the autumn of 412 to @@Morningstone@@ and stayed.",
          view: "all",
          edit: "all",
        },
        {
          key: "story-2",
          text: "In truth she was looking for the second passage.",
          view: "gm",
          edit: "gm",
        },
      ],
    },
  ],
};

const GENDER: Template = {
  ...emptyTemplate(),
  id: "gender",
  name: "Gender",
  icon: "🏷",
  description: "Values for reference fields",
};

/** A kind of character — it sits inside Character in the types overview. */
const NPC: Template = {
  ...emptyTemplate(),
  id: "npc",
  name: "NPC",
  icon: "🎭",
  supertype: "character",
};

const ARTICLES: ArticleView[] = ["female", "male", "nonbinary"].map(
  (n, i) =>
    ({
      id: `g${i}`,
      articleType: "gender",
      slug: n,
      name: n,
      layers: [],
    }) as unknown as ArticleView,
);

type Stage = "template" | "blank" | "create" | "options" | "legend" | "compare" | "versions" | "images" | "flow" | "storage" | "relations" | "stats" | "maps" | "package";

const STAGES: { id: Stage; name: string }[] = [
  { id: "template", name: "Template editor" },
  { id: "blank", name: "New type" },
  { id: "create", name: "New article" },
  { id: "options", name: "Options" },
  { id: "legend", name: "Legend" },
  { id: "compare", name: "Compare versions" },
  { id: "versions", name: "Versions" },
  { id: "images", name: "Images" },
  { id: "flow", name: "Flow" },
  { id: "storage", name: "First start" },
  { id: "relations", name: "Relations" },
  { id: "stats", name: "Stat block" },
  { id: "maps", name: "Maps" },
  { id: "package", name: "Package" },
];

/** The versions of a document as the GM sees them: the newest stays, older ones may go. */
const VERSIONS = [
  { session: "s-3", author: "Tester", author_id: "me", from: 1791300000000, to: 1791300600000, arrived: 1791300600000, parts: ["Text"], structure: false, stale: false, overlaps: [], review: "counted", removable: false },
  { session: "s-2", author: "Bob", author_id: "id:0000000000000002", from: 1791200000000, to: 1791200300000, arrived: 1791200300000, parts: ["Text"], structure: false, stale: false, overlaps: [], review: "counted", removable: true },
  { session: "s-1", author: "Tester", author_id: "me", from: 1791100000000, to: 1791100200000, arrived: 1791100200000, parts: ["Text", "Secret"], structure: false, stale: false, overlaps: [], review: "counted", removable: true },
];

function VersionsStage({ onError }: { onError: (e: unknown) => void }) {
  setFixture("article_versions", VERSIONS);
  const [chosen, setChosen] = useState<string | null>("s-2");
  const [said, setSaid] = useState("");
  return (
    <div className="body-wrap">
      <p className="small">{said}</p>
      <VersionsPanel
        world="test-world"
        article="art"
        revision={0}
        chosen={chosen}
        fresh={[]}
        mayRevert={true}
        gm={true}
        onChosen={setChosen}
        onRevert={(s) => setSaid(`Would take back ${s}.`)}
        onCarry={() => {}}
        onVerdict={() => {}}
        onDelete={(s) => setSaid(`Would delete ${s}.`)}
        onError={onError}
      />
    </div>
  );
}

/** A picture for every image of the bench — a coast, drawn. */
const PICTURE =
  "data:image/svg+xml;utf8," +
  encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="1200" viewBox="0 0 240 180"><rect width="240" height="180" fill="#2b5d73"/><path d="M0 120 C60 90 110 140 170 100 S240 110 240 110 V180 H0Z" fill="#c9b27c"/><circle cx="190" cy="45" r="18" fill="#f2d16b"/></svg>',
  );

const IMAGES = [
  { id: "img:a-1", name: "Map of the raven coast", width: 8000, height: 6000, size: 48_200_000, uploaded: 1791300000000, own: true, used_in: [{ id: "a1", name: "The raven coast" }, { id: "a2", name: "Harbour" }], folder: "f-coast", in_versions: false, complete: true, defaults: {} },
  { id: "img:a-2", name: "Aldric unmasked", width: 1200, height: 1600, size: 830_000, uploaded: 1791200000000, own: true, used_in: [{ id: "a3", name: "Aldric" }], folder: "f-chars", in_versions: false, complete: true, defaults: { view: "gm", edit: "gm", hidden: "img:a-4", hidden_caption: "A stranger in the shadows" } },
  { id: "img:b-3", name: "Bob's sketch of the inn", width: 900, height: 700, size: 310_000, uploaded: 1791100000000, own: false, folder: "f-places", used_in: [], in_versions: true, complete: true, defaults: {} },
  { id: "img:a-4", name: "Unused sketch", width: 640, height: 480, size: 120_000, uploaded: 1791000000000, own: true, used_in: [], in_versions: false, complete: false, defaults: {} },
];

const FLOW =
  "<image src=img:a-1 align=right width=260 caption=\"The raven coast\">\n" +
  "The coast runs north for three days' ride. Fishers live in the coves, and nobody goes out when the fog comes in from the sea. " +
  "The old lighthouse stands empty since the storm; some say a light still burns there on moonless nights, others that smugglers keep it. " +
  "Further inland the heath begins, wide and brown, with standing stones nobody has counted twice the same. " +
  "Past the stones the road forks: west to the salt marsh, where the ferryman asks no names, east to the hills and the monastery that sold its bells. " +
  "Travellers say the monks still ring at noon, though there is nothing left to ring.\n" +
  "\n" +
  "## The harbour\n" +
  "Second paragraph, after a blank line, with **bold**, *italic* and `code`.\n" +
  "A line of its own, after a single line break.\n" +
  "<image src=img:a-2 align=center width=300 caption=\"Aldric\">\n" +
  "After the second image the text goes on.";

/** A part of a document as one flow field — the text runs past the image and on below it. */
function FlowStage() {
  setFixture("image_data", PICTURE);
  const [text, setText] = useState(FLOW);
  const handle = useRef(new FlowHandle());
  return (
    <div className="body-wrap" style={{ maxWidth: 900 }}>
      <div className="block-part">
        <FlowField
          handle={handle}
          text={text}
          className="block-text"
          placeholder="Write here."
          surfaceClass={(p) => (p.kind === "image" ? imageSurface(p.image) : p.kind)}
          surface={(piece, i) =>
            piece.kind === "image" ? (
              <ImageCard
                world={WORLD}
                image={piece.image}
                ownPart={false}
                onChange={(next) =>
                  setText(cut(text).map((p, j) => (j !== i ? p.text : next ? writeImageLine(next) + "\n" : "")).join(""))
                }
              />
            ) : null
          }
          onText={setText}
        />
      </div>
      <h4 className="small">As read</h4>
      <WorldDataContext.Provider value={{ world: WORLD, templates: [], articles: [], onPlace: () => {}, onChanged: () => {}, onApart: () => {} }}>
        <div className="block-part" id="flow-read">
          <MarkdownText text={text} resolve={linker([], [])} onPlace={() => {}} />
        </div>
      </WorldDataContext.Provider>
      <pre id="flow-text" className="small">{text}</pre>
    </div>
  );
}

/** Images as the visual editor draws them: plain, hidden with a stand-in, a stand-in a player sees. */
function Cards() {
  const [cards, setCards] = useState<ImageAttrs[]>([
    [["src", "img:a-1"], ["align", "right"], ["width", "300"], ["caption", "The raven coast"]],
    [["src", "img:a-2"], ["caption", "Aldric, the traitor"], ["hidden", "img:a-4"], ["hidden-caption", "A stranger"]],
    [["src", "img:a-4"], ["caption", "A stranger"], ["for", "notes-2"]],
  ]);
  const text = "The coast runs north for three days' ride. Fishers live in the coves, and nobody goes out when the fog comes in from the sea.";
  // As the visual editor lays a part out: a card, then the text field after it.
  return (
    <div className="block-part">
      {cards.map((c, i) => (
        <div key={i}>
          <ImageCard
            world={WORLD}
            image={c}
            ownPart={i === 1}
            onChange={(next) => setCards(cards.map((x, j) => (j === i ? (next ?? x) : x)))}
          />
          <div className="caret-box">
            <textarea className="block-text grows" rows={4} defaultValue={text} />
          </div>
        </div>
      ))}
    </div>
  );
}

function ImagesStage({ onError }: { onError: (e: unknown) => void }) {
  setFixture("gallery", IMAGES);
  setFixture("gallery_folders", [
    { id: "f-places", name: "Places", parent: "" },
    { id: "f-coast", name: "Coast", parent: "f-places" },
    { id: "f-towns", name: "Towns", parent: "f-places" },
    { id: "f-chars", name: "Characters", parent: "" },
    { id: "f-npc", name: "NPCs", parent: "f-chars" },
  ]);
  setFixture("gallery_folders_set", null);
  setFixture("image_move", null);
  setFixture("image_data", PICTURE);
  const [picker, setPicker] = useState(false);
  return (
    <div className="body-wrap">
      <button onClick={() => setPicker(true)}>Open the picker</button>
      <div className="options-body" style={{ overflow: "visible" }}>
        <GalleryArea world={WORLD} onOpen={() => {}} onError={onError} />
      </div>
      <h3>In the visual editor</h3>
      <Cards />
      {picker && <ImagePicker world={WORLD.id} onChosen={() => setPicker(false)} onRemove={() => setPicker(false)} onCancel={() => setPicker(false)} />}
    </div>
  );
}

const THEN = "## The old inn\n\nThe inn is **old** and dark.\nGuests: few.\n\n- beer\n- bread";
const NOW = "## The new inn\n\nThe inn is **new** and dark. It rains.\n\n- beer\n- bread\n- wine";

/**
 * A version beside now — the marks as the page draws them, and behind text
 * fields as the editor does. Both texts can be changed here.
 */
function CompareStage() {
  const [then, setThen] = useState(THEN);
  const [now, setNow] = useState(NOW);
  const code = useRef<HTMLTextAreaElement>(null);
  const part = useRef<HTMLTextAreaElement>(null);
  const { removed, added } = changes(then, now);
  const none = () => undefined;
  return (
    <div className="body-wrap compare-stage">
      <div className="two-columns compare-pages">
        <div>
          <h4 className="compare-label">This version</h4>
          <MarkdownText text={marked(then, removed, "removed")} resolve={none} onPlace={() => {}} />
        </div>
        <div>
          <h4 className="compare-label">Now</h4>
          <MarkdownText text={marked(now, added, "added")} resolve={none} onPlace={() => {}} />
        </div>
      </div>
      <div className="two-columns">
        <div>
          <h4 className="compare-label">Code, then</h4>
          <div className="text-field">
            <div className="caret-box">
              <textarea className="template-text" value={then} onChange={(e) => setThen(e.target.value)} />
            </div>
          </div>
        </div>
        <div>
          <h4 className="compare-label">Code, now — marked behind the field</h4>
          <div className="text-field">
            <div className="caret-box">
              <textarea ref={code} className="template-text" value={now} onChange={(e) => setNow(e.target.value)} />
              <MarkedField field={code} text={now} ranges={added} kind="added" />
            </div>
          </div>
          <h4 className="compare-label">Visual, now</h4>
          <div className="caret-box">
            <textarea ref={part} className="block-text" rows={9} value={now} onChange={(e) => setNow(e.target.value)} />
            <MarkedField field={part} text={now} ranges={added} kind="added" />
          </div>
        </div>
      </div>
    </div>
  );
}

setDirectory([
  { id: "character", name: "Character" },
  { id: "gender", name: "Gender" },
]);

// For the scenarios: a type can be renamed without reloading the page. In
// the app Rust does that on save; there is no world here. It would not work
// via `import()` — Vite loads the same module under two addresses, and one
// would change a copy.
(window as unknown as { setDirectory: typeof setDirectory }).setDirectory =
  setDirectory;

function TestBench() {
  const initial = (new URLSearchParams(location.search).get("stage") ?? "template") as Stage;
  const [stage, setStage] = useState<Stage>(initial);
  const [draft, setDraft] = useState<Template>(CHARACTER);
  const [blank, setBlank] = useState<Template>(emptyTemplate());
  const [templates, setTemplates] = useState<Template[]>([CHARACTER, GENDER, NPC]);
  const [error, setError] = useState<string | null>(null);

  const report = (e: unknown) => setError(String(e));

  return (
    <div className="testbench">
      <nav className="bench-bar">
        <strong>Test bench</strong>
        {STAGES.map((b) => (
          <button
            key={b.id}
            className={b.id === stage ? "on" : ""}
            onClick={() => {
              setError(null);
              setStage(b.id);
            }}
          >
            {b.name}
          </button>
        ))}
        <span className="filler" />
        {error && <span className="bench-error">{error}</span>}
      </nav>

      <div className="bench-stage">
        {stage === "template" && (
          <TemplateEditor
            world={WORLD}
            draft={draft}
            templates={templates}
            articles={ARTICLES}
            onChange={setDraft}
            onSave={() => setError("Saved (the bench stores nothing).")}
            onBack={() => setStage("options")}
            onArticlesChanged={() => {}}
            onOpen={(id) => setError(`Would open ${id} (the bench has no documents).`)}
            onError={report}
            saved={false}
          />
        )}
        {stage === "blank" && (
          <TemplateEditor
            world={WORLD}
            draft={blank}
            templates={templates}
            articles={ARTICLES}
            onChange={setBlank}
            onSave={() => setError("Saved (the bench stores nothing).")}
            onBack={() => setStage("options")}
            onArticlesChanged={() => {}}
            onOpen={(id) => setError(`Would open ${id} (the bench has no documents).`)}
            onError={report}
            saved={false}
          />
        )}
        {stage === "create" && (
          // As in the app: the world's data around the form.
          <WorldDataContext.Provider value={{ world: WORLD, templates, articles: ARTICLES, onPlace: () => {}, onChanged: () => {}, onApart: () => {} }}>
          <div className="body-wrap">
            <ArticleArea
              world={WORLD}
              templates={templates}
              articles={ARTICLES}
              articleType="character"
              onDone={() => setError("Created (the bench stores nothing).")}
              onCancel={() => setStage("template")}
              onError={report}
            />
          </div>
          </WorldDataContext.Provider>
        )}
        {stage === "options" && (
          <Options
            world={WORLD}
            me={ME}
            templates={templates}
            articles={ARTICLES}
            onChanged={setTemplates}
            onArticlesChanged={() => {}}
            onWorldChanged={() => {}}
            onOpen={(id) => setError(`Would open ${id} (the bench has no documents).`)}
            onNetworkChanged={() => {}}
            onStorageChanged={() => {}}
            onClose={() => setStage("template")}
            onError={report}
          />
        )}
        {stage === "legend" && <Legend />}
        {stage === "compare" && <CompareStage />}
        {stage === "versions" && <VersionsStage onError={report} />}
        {stage === "images" && <ImagesStage onError={report} />}
        {stage === "flow" && <FlowStage />}
        {stage === "relations" && <RelationsStage />}
        {stage === "stats" && <StatsStage />}
        {stage === "maps" && <MapsStage />}
        {stage === "package" && <PackageStage />}
        {stage === "storage" && (
          // Shown only: the bench has no folders to keep anything in.
          <StorageSetup
            status={{
              state: "unset",
              profile: "",
              worlds: "",
              together: null,
              suggested: "/home/marla/Documents/AnotherWorldbuilder",
              problem: null,
            }}
            onDone={() => setError("The bench keeps nothing.")}
          />
        )}
      </div>
    </div>
  );
}

createRoot(document.getElementById("root")!).render(<TestBench />);
