// What crosses the bridge to Rust: the shapes of worlds, documents, types
// and the rest, as the interface knows them.

import { type WorldCalendar } from "../shared/Calendar";

export type IdentityInfo = {
  name: string;
  id: string;
  fingerprint: string;
  file: string;
  publicKey: string;
  /** data: URL, or null if no image is set. */
  avatar: string | null;
};

export type Role = "gm" | "supporter" | "player";

export type MemberInfo = {
  id: string;
  name: string;
  fingerprint: string;
  role: Role;
  attributes: string[];
  /** Their writing counts without waiting for the GM. */
  trusted: boolean;
  /** Their mailbox, in a world without a server. */
  slot: string;
};

export type WorldInfo = {
  id: string;
  name: string;
  icon: string;
  folder: string;
  role: Role;
  isGm: boolean;
  attributes: string[];
  members: MemberInfo[];
  articleCount: number;
  /** Id of the home page, empty if there is none yet. */
  home: string;
  /** Requests to join, waiting for the GM. */
  requests: number;
  /** Ids let in ahead of time, not seen on the network yet. */
  invited: string[];
  /** New ids that take a player's place once let in: new id → old id. */
  takeovers: Record<string, string>;
  /** Members' own colours by the terms that name them — `player:<name>`, `doc:…`. */
  colours: Record<string, string>;
  /** Whether players' writing waits for the GM before the others see it. */
  moderated: boolean;
  /** Versions waiting: for the GM, what players wrote; for a player, their own. */
  waiting: number;
  /** How many versions of each document stay. */
  versionLimit: number;
  /** Worked on online: the host holds it, this device asks it for everything. */
  online: boolean;
  /** The server it lives on, by code — empty: this device. */
  server: string;
  /** Hosting identity even when its address has been removed. */
  serverId?: string;
  /** The GM's: offered to players over the network. */
  offered: boolean;
  /** Named on its server to everyone who asks — else only to its GM and members. */
  listed: boolean;
  /** The dice the world rolls with, by sides — empty for the usual set. */
  dice: number[];
  /** How the world counts its days. */
  calendar: WorldCalendar;
  /** Who supports the world, by id: the players the GM chose and the admins of its server. */
  supporters: string[];
  /** This device's member supports it: lets players in, and may take the GM's place. */
  supporter: boolean;
  /** The roles there are to give — the GM's and a supporter's. */
  roles: string[];
  /** Without a server: the folder of its mailboxes — the GM's. */
  mailboxes: string;
};

/** What this device does on the local network. */
/** The door for browser extensions (`browser.rs`). */
export type BrowserView = {
  enabled: boolean;
  open: boolean;
  port: number;
  problem: string | null;
};

export type NetworkView = {
  enabled: boolean;
  /** Direct over the internet, met through the shared folder. */
  internet: boolean;
  /** Where others can reach this device from outside — empty: nowhere. */
  reachable: string[];
  /** How: `router`, `stun`, `ipv6`. */
  via: string[];
  /** Whom to ask how this device looks from outside. */
  stun: string[];
  /** Whether the network actually runs — it can fail to start. */
  running: boolean;
  /** The port asked for. */
  port: number;
  /** The port in use — another one while something else holds the one asked for. */
  active_port?: number | null;
  server_port?: boolean;
  /** What players enter to reach this device as host — empty while nothing is reachable. */
  code: string;
  /** Servers that keep worlds for the group, by their code. */
  servers: string[];
  /** Each of them as it answered last. */
  server_list: ServerEntry[];
  /** The worlds this device could offer: those it is GM of. */
  worlds: SharedWorld[];
};

/** A server as it answered last: its name (empty until it answered), whether this device is its admin. */
export type ServerEntry = {
  code: string;
  name: string;
  admin: boolean;
  /** This device may bring worlds there. */
  may_bring: boolean;
  /** It asked to. */
  asked: boolean;
  reached: boolean;
};

/** A world on a server, as its admin sees it — nothing of its content. */
export type HeldWorld = { world: string; name: string; gm: string; gm_id: string; bytes: number; closed: boolean; requests: Person[]; members: Person[]; supporters: string[] };
/** Someone a server knows; `last` in seconds since 1970, 0: not yet. */
export type Person = { id: string; name: string; last: number; blocked: boolean };
/** What a server's admin sees. */
export type ServerState = { worlds: HeldWorld[]; admins: Person[]; worldbuilders: Person[]; asking: Person[]; users: Person[] };
/** What a server's admin orders — see `aw_sync::world::Order`. */
export type ServerOrder =
  | { kind: "allow" | "refuse" | "block" | "unblock" | "kick"; id: string }
  | { kind: "admin"; id: string; on: boolean }
  | { kind: "approve" | "remove"; world: string; id: string }
  | { kind: "close" | "open" | "delete"; world: string };

/** Where the data lies: the profile (identity, settings) and the worlds. */
export type StorageView = {
  /** `unset`: nothing chosen yet; `missing`: a folder is not there; `busy`: another copy of the app uses it. */
  state: "unset" | "missing" | "busy" | "ready";
  profile: string;
  worlds: string;
  locations: { path: string; available: boolean; primary: boolean }[];
  /** When both lie side by side below one folder: that folder. */
  together: string | null;
  /** Where to keep everything, if nothing is chosen yet. */
  suggested: string;
  problem: string | null;
};

/** One folder for both, or each on its own. */
export type StorageChoice = { base: string } | { profile: string; worlds: string; additional?: string[] };
/** What a server holds of a GM's world, measured against this device. */
/** How often a world backs itself up, how many backups stay, where (empty: beside the worlds). */
export type BackupPlan = { folder: string };
/** How the server a world lives on backs it up by itself. */
/** `at`: minutes after midnight, `day`: 0 Monday … 6 Sunday (weekly) — both UTC, as the server keeps time. */
export type ServerBackupPlan = { every: "off" | "daily" | "weekly"; keep: number; at: number; day: number };
/** `now`: the server's clock when it answered — 0 from a server that does not tell. */
export type ServerBackups = { plan: ServerBackupPlan; count: number; last: number; now: number };
export type BackupView = { plan: BackupPlan; folder: string; backups: { at: number; path: string }[] };
/** A backup where backups go by default, of any world; `path` is its `backup.awbackup`. */
export type BackupFound = { at: number; name: string; world: string; path: string };

export type WorldDestination = { kind: "local"; path: string } | { kind: "server"; server: string };

/** A world held here, and whether it is offered or kept somewhere. */
export type SharedWorld = { id: string; name: string; icon: string; shared: boolean; server: string };

/** A world found on the network. */
export type NetWorld = {
  id: string;
  name: string;
  icon: string;
  gm: string;
  /** `player` when this device already holds it. */
  local: "player" | null;
  /** The server holding it, by code — empty: found on the local network. */
  server: string;
};

/** What asking a host came to. */
export type JoinOutcome = {
  kind: "joined" | "updated" | "current" | "not-member" | "requested" | "not-offered";
  world: WorldInfo | null;
};

/** What one round of keeping up brought. */
export type SyncOutcome = {
  /** Worlds that arrived because the GM said yes. */
  joined: WorldInfo[];
  /** Worlds others changed. */
  updated: string[];
  /** Worlds that went elsewhere — opened anew. */
  moved: string[];
};

/** Someone who asked to join. */
export type JoinRequest = { id: string; name: string; fingerprint: string; at: number };

export type LayerView = {
  id: string;
  title: string;
  /** Key of the part in the template. */
  section: string;
  /** Whether this reader may change the part. */
  mayEdit: boolean;
  text: string;
  fields: Record<string, unknown>;
  policy: string;
  /** Who may change it — to know whom to ask. */
  edit: string;
  epoch: number;
};

export type ArticleView = {
  id: string;
  articleType: string;
  slug: string;
  name: string;
  /** Visibility of the article as a whole. */
  visibility: string;
  /** Who may change the document as a whole. */
  edit: string;
  layers: LayerView[];
  layerTotal: number;
  /** The order of its sections by key: its own — and its type's, where it keeps them in an order of its own. */
  order?: string[];
  /** How its type's sections look in it, where that differs from the type — by block key. */
  looks?: Record<string, BlockLook>;
  /** Known only by its cover: links lead to it, nothing lists it — see `Links.ts`. */
  cover?: boolean;
  /** The type's sections with nothing in them yet that this player may start — never told the GM. */
  startable?: string[];
  /** Out of the index: its texts not read yet — see `WorldReader`. */
  partial?: boolean;
};

/** A document in the trash: when, and who put it there — by name. */
export type TrashedView = { id: string; name: string; articleType: string; at: number; by: string };

/** A part of a world's index, begun at write `writes` of its opening `epoch`, for `reader`; `next`: the id the next part begins after. */
export type IndexPart = { epoch: number; writes: number; reader: string; next: string | null; articles: ArticleView[] };
export type TextsPart = { articles: ArticleView[]; gone: string[]; rest: string[] };
/** What changed: whole, gone — and, where too many for one answer, by id only (`changed`). */
export type SincePart = { epoch: number; writes: number; reader: string; whole: boolean; articles: ArticleView[]; gone: string[]; changed: string[] };

export type NewLayer = {
  title: string;
  text: string;
  policy: string;
  fields: Record<string, unknown>;
  /** Key of the part in the template. */
  section: string;
  /** Who may change this part. */
  edit: string;
  /** Index of the enclosing part in the same list. */
  parent: number | null;
};

// --- Templates ---------------------------------------------------------

export type FieldKind =
  | { kind: "text"; multiline: boolean }
  /** Several short texts, stored as a list — written `Xan, Old Fox`. */
  | { kind: "list" }
  | { kind: "rich-text" }
  | { kind: "heading"; level: number }
  | { kind: "number"; unit?: string }
  | { kind: "world-date" }
  /** A date and time of ours — when a session was played: `2026-09-12T19:00`. */
  | { kind: "date-time" }
  /**
   * Picks documents of a type — or of its subtypes. A data type makes it a
   * plain selection, an article type a link. `parent` marks the axis of the
   * tree; with an empty `articleType` it points at the root of the own type.
   * `adds`: what it picks adds its numbers to this document's — see `shared/Sheet`.
   */
  | { kind: "reference"; articleType: string; multiple: boolean; parent: boolean; adds?: boolean; count?: string; every?: boolean; from?: string }
  /**
   * Relations to documents of a type, each with a role on either side —
   * the roles are documents of `roleType`. See `shared/Relations`.
   */
  | { kind: "relation"; articleType: string; roleType: string }
  | { kind: "image"; width: number; height: number; align: string }
  /**
   * The properties of a property group, as if they were the type's own:
   * `Stats: group Abilities`. Unfolded by `effective` in `shared/types`.
   */
  | { kind: "group"; group: string };

/**
 * A property of a type — a variable every document of it has. `label` is its
 * name: what documents write before the colon, and what `bind=` and
 * `%%…%%` call it by.
 */
export type TemplateField = {
  key: string;
  label: string;
  /** Access of its own — then its value is a layer of its own. */
  access?: Access | null;
  /** What a new document starts with, as written. */
  default?: string;
  /** The groups it lies in, outermost first, by the names they are given — set where groups are unfolded. */
  inGroup?: string[];
} & FieldKind;

/** A property shown in the infobox: `<select bind=Species label=Kind>`. */
export type Shown = { element: string; property: string; label: string };

/** Where the infobox sits. */
export type Alignment = "right" | "left" | "top" | "none";

/**
 * The infobox — a box beside the text that shows properties. It holds no
 * data of its own. Four alignments cover every wiki page anyone actually
 * wants to see.
 */
export type Infobox = {
  align: Alignment;
  /** `auto`, a width in pixels, or `narrow`/`wide`. */
  width: string;
  show: Shown[];
  /** Who sees and changes its values — `<infobox view=… edit=…>`; none: as the article. */
  access?: Access;
};

/** The style of a block — steers its look, not its access. */
export type BlockStyle = "text" | "box" | "quote" | "statblock";

/**
 * A piece of a block — and at the same time a unit of access: every part is
 * stored with a rule of its own, so a region with access of its own can sit
 * in the middle of flowing text.
 */
export type BlockPart = {
  key: string;
  /** Markdown a new article starts with. */
  text: string;
  view: string;
  edit: string;
};

/** Who may see and who may change. */
export type Access = { view: string; edit: string };

/** A block a new article of this type is given. */
export type BlockPlace = "flow" | "top" | "bottom" | "side";
export type BlockFold = "open" | "folded" | "off";

/** How a type's section looks in one document, where it differs from the type — see `document::Look`. */
export type BlockLook = Partial<Pick<TemplateBlock, "title" | "style" | "place" | "fold" | "heading" | "width">>;

export type TemplateBlock = {
  key: string;
  title: string;
  style: BlockStyle;
  /** At least one. A new part begins at every inner block or element with access of its own. */
  parts: BlockPart[];
  /** In the text beside the infobox, above or below both, across the page — or in a column at the side. */
  place?: BlockPlace;
  /** At the side: how wide, in pixels — 0 or missing: as the app finds right. */
  width?: number;
  /** Whether its title folds it, and how it starts. */
  fold?: BlockFold;
  /** How large its title stands: 1 as a `#` heading, down to 4, the text's size. */
  heading?: number;
  /** The types a button beside its title makes documents of, by name. */
  new?: string[];
};

/** Where one currently stands in the world. */
/**
 * An entry of the side bar on the left — see `aw_model::navigation`. A type
 * unfolds to its documents, a group to its entries.
 */
export type NavEntry = {
  id: string;
  kind: "type" | "document" | "group";
  /** The type's or the document's id; empty for a group. */
  target?: string;
  /** Empty: the type's or the document's name. */
  label?: string;
  /** Only for the GM — players never receive it. */
  gm?: boolean;
  children?: NavEntry[];
};

/** What a package holds, measured against a world — see `transfer.rs`. */
export type PackageSummary = {
  name: string;
  typesNew: number;
  typesKnown: number;
  documentsNew: number;
  documentsKnown: number;
  imagesNew: number;
  imagesKnown: number;
  /** Pictures the app could not read — left out. */
  imagesSkipped: string[];
};

export type Place =
  | { kind: "types" }
  | { kind: "list"; articleType: string }
  | { kind: "article"; id: string };

/** Key of the layer that carries the properties. */
export const PROPERTIES = "properties";

/** `mod(x) = floor((x - 10) / 2)`, `crit = 20` — a rule of the game, as a world writes it. */
export type Rule = { name: string; params: string[]; formula: string };

export type Template = {
  /**
   * The id. Generated, unchanging, never shown.
   *
   * Articles and references hang on it. The text carries the **name** of the
   * type instead — only so does a reference survive a rename.
   */
  id: string;
  name: string;
  icon: string;
  /** Id of the supertype, empty for a root type. Fields and blocks are inherited. */
  supertype: string;
  /** A property group, not a type: properties only, no documents, no page. */
  group: boolean;
  /** Not in the outline the app sets out by itself — a type only other documents point to. */
  navigationHidden?: boolean;
  /** The rules of the game its documents reckon with — see `shared/Dice`. */
  rules: Rule[];
  description: string;
  /** The properties every document of this type has — the data. */
  properties: TemplateField[];
  /** Which of them the infobox shows. */
  infobox: Infobox;
  /** What `<article>` says — the default for anything without access of its own. */
  auth: Access | null;
  /**
   * How the name of a new article is built.
   *
   * `%%input%%` is the input, `%%count%%` counts up. See NamePattern.ts.
   */
  namePattern: string;
  /**
   * The type's own page — a document with text, images and `<list>`s.
   * Empty until someone writes one.
   */
  page: string;
  blocks: TemplateBlock[];
  /**
   * The order its documents show their blocks in, by key — inherited ones
   * among them. Empty: the supertypes' first, then its own.
   */
  order?: string[];
  /** How its documents line up: by these properties, by key — `name` for the name. Empty: by name. */
  sort?: string[];
  /**
   * Who besides the GM may make documents of it — a view like `players`.
   * Empty: the GM alone. A player's new document goes to the GM's device,
   * which makes it with this type's access.
   */
  create?: string;
};

/** All field kinds for the picker in the template editor. */
export const FIELD_KINDS: { value: FieldKind["kind"]; text: string }[] = [
  { value: "text", text: "Text" },
  { value: "rich-text", text: "Flowing text" },
  { value: "heading", text: "Heading" },
  { value: "number", text: "Number" },
  { value: "world-date", text: "World date" },
  { value: "date-time", text: "Date and time" },
  { value: "reference", text: "Reference to documents" },
  { value: "relation", text: "Relations" },
  { value: "image", text: "Image" },
  { value: "group", text: "Property group" },
];

/** A node of the Markdown tree Rust delivers. */
export type Node =
  | { kind: "paragraph"; children: Node[] }
  | { kind: "heading"; level: number; children: Node[] }
  | { kind: "list"; ordered: boolean; items: Node[][] }
  | { kind: "quote"; children: Node[] }
  | { kind: "code-block"; language: string; text: string }
  | { kind: "rule" }
  | { kind: "table"; align: string[]; head: Node[][]; rows: Node[][][] }
  | { kind: "text"; text: string }
  | { kind: "strong"; children: Node[] }
  | { kind: "emphasis"; children: Node[] }
  | { kind: "strikethrough"; children: Node[] }
  | { kind: "code"; text: string }
  | { kind: "soft-break" }
  | { kind: "link"; target: string; children: Node[] }
  /** `@@Type:Name@@` — `of` is the type as written, empty when it says none. */
  /** `#property` shows that property's value instead of the name; `#^anchor` leads to that line of it. */
  | { kind: "wikilink"; target: string; text: string; of: string; property: string }
  /** `^2f9e17` closing a line — where `@@#^2f9e17|…@@` leads. Not shown. */
  | { kind: "anchor"; id: string }
  /** `%%Name%%` — the value of a property of this document. */
  | { kind: "value"; name: string }
  /** `<select bind=Species>` — an element showing a property. */
  | { kind: "bound"; element: string; property: string; label: string }
  /** `[[1d20 + Strength mod]]`, or dice as they stand in text — `text` then holds them. See `shared/Dice`. */
  | { kind: "roll"; formula: string; text: string }
  /** `<abilities show="Strength, …">` — the ability row of a stat block. */
  | { kind: "abilities"; show: string[]; with?: string; short?: string[]; also?: string; heading?: string; also_heading?: string }
  /** `<view of=Stats>` — a property group's layout, drawn with this document's values. */
  | { kind: "view"; of: string }
  /** `<embed of=Sessions>` — another document's text under its name. */
  | { kind: "embed"; of: string; fold?: string; title?: string; section?: string }
  /** `for`: a stand-in for a part this reader may not open. */
  | { kind: "image"; src: string; align: string; wrap: boolean; width: number; caption: string; for: string; beside?: boolean }
  /** `<list type=Place …>` — which documents it shows, the display decides. */
  | {
      kind: "listing";
      of: string;
      style: "list" | "cards" | "tree" | "table" | "inline";
      columns: number;
      show: string[];
      filter: [string, string][];
      below: string;
      /** A property of this document whose documents are listed — empty for a type's. A path goes through picks. */
      from: string;
      /** A formula a listed document must not make 0 — reckoned inside the pick. */
      when?: string;
      /** A property the reader filters by, with a button for each value (`filter=Role`). */
      choose?: string;
      /** The order it starts in and goes back to: `Role desc`, `name` — empty: the type's. */
      sort?: string[];
      /** Counts instead of listing: a row for each value of this property (`count=Killer`). */
      count?: string;
      /** With `count`: a column for each value of this property too (`by=Kind`). */
      by?: string;
      /** On a list of what names this document: `new` — ＋ makes one, × deletes it; else ＋ picks, × lets go. */
      add?: string;
    }
  /** `<timeline type="Event, Session">` — what happened when. */
  | { kind: "timeline"; of: string[]; show: string[]; around: string; until: string; text: string[] }
  /** `<calendar type="Event, Session">` — a month, its moons, what happened on its days. */
  | { kind: "calendar"; of: string[] }
  | { kind: "create"; of: string[]; label: string }
  /** `<graph …> … </graph>` — people and their relations as a tree. */
  | ({ kind: "graph" } & GraphNode)
  /** `<map src=img:…> <pin …> </map>` — a picture to move and zoom, with pins. */
  | ({ kind: "map" } & MapNode);

/** A map — see `aw_model::markdown::Map`. */
export type MapNode = { src: string; height: number; pins: MapPin[]; areas: MapArea[] };
/**
 * Who sees a pin or region, where that is narrower than its map — set apart
 * in a part of its own when saved (`aw_model::document`, “Secret pins”).
 * Reading, only `view` comes, to mark it.
 */
export type MapAccess = { view?: string; edit?: string };
/** Where a pin's name stands: right of it (none), left, top, bottom. */
export type LabelAt = "left" | "top" | "bottom";
/**
 * Where on the picture, in tenths of a percent (0–1000); what it says; where
 * it leads; its marker; where its name stands, and whether only under the pointer.
 */
export type MapPin = { x: number; y: number; label: string; to: string; color: string; icon?: string; label_at?: LabelAt; label_hover?: boolean } & MapAccess;
/** A region: its corners in tenths of a percent, what it is called, where it leads. */
export type MapArea = { points: [number, number][]; label: string; to: string; color: string } & MapAccess;

/** What a graph shows, and how — see `aw_model::markdown::Graph`. */
export type GraphNode = {
  /** Only people of this type and its subtypes — empty for those the relations name. */
  of: string;
  /** The relation properties drawn — empty for all. */
  via: string[];
  /** Only who is related to this document — `self` for the one shown. */
  around: string;
  depth: number;
  /** Where people were set by hand — by id as stored, by name as written — and how their cards look. */
  people: GraphPerson[];
  /** Lines drawn other than by default. */
  lines: GraphLine[];
  /** What cards and lines show: `portrait`, `type`, `roles` — empty for all. */
  show: string[];
};

/** Someone set on the grid, and their card: frame colour, what stands under the name. */
export type GraphPerson = { who: string; x: number; y: number; color: string; sub: string; note: string };

/** How the line between two people is drawn — empty means as usual. */
export type GraphLine = {
  /** The two ends: someone — `Tim` — or the line between two — `Anna & Franklin`. */
  between: string[];
  color: string;
  /** Where it leaves each card, in the order of `between`: `bottom`, `bottom 30`. */
  sides: string[];
  shape: string;
  /** Arrowheads: `end` (at the second), `start`, `both` — empty for none. */
  arrow: string;
  /** For an end that is a line: how far along it, 0–100, in the order of `between` — empty for the middle. */
  dock: string[];
  /** For an end of several: `each` — each one's role — or empty: the roles once. */
  group: string[];
  /** For an end of several: a word of one's own in place of their roles — `Parents`. */
  say: string[];
  /** 0–100 along the line, or `none`. */
  label: string;
};

/** A roll of the dice as the session sees it — see `network::Roll` in Rust. */
export type SharedRoll = {
  id: string;
  /** Filled in by Rust: who rolled. */
  member?: string;
  name?: string;
  document: string;
  label: string;
  formula: string;
  detail: string;
  total: number;
  /** Sides and what each die showed. */
  dice: [number, number][];
  /** `crit` or `fumble`, as the rules of the document rolled from say; `clear` empties the log. */
  mark?: string;
  at: number;
  /** Rolled for oneself — kept only in one's own log. */
  secret?: boolean;
};

/** A roll kept to roll again with a click. */
export type Favorite = { name: string; formula: string };

/** What this device keeps of a world's dice. */
export type DiceLog = { rolls: SharedRoll[]; favorites: Favorite[] };

/** A property of a document: a name and its value as written. */
/** `group`: the groups it lies in, outermost first — the code writes it indented under their names. */
export type Property = { name: string; value: string; group?: string[] };

/**
 * A document in the shape its code has: the skeleton's name and bracket,
 * then properties and blocks.
 */
/** What those who may not open a document know of it: an alias (empty: its name), and who may follow links to it. */
export type Cover = { alias: string; access: Access };

export type DocumentText = {
  name: string;
  access: Access;
  /** The infobox's own access — none: as the document's. */
  infobox?: Access;
  cover?: Cover;
  properties: Property[];
  blocks: TemplateBlock[];
};

/** An error message from the text form, with its line. */
export type LineError = { line: number; text: string };

export type ReadResult = {
  template: Template | null;
  errors: LineError[];
  /**
   * The text as it would have to stand — set when the skeleton was missing.
   *
   * `<article>`, `<name>` and `<body>` can be changed but not
   * deleted. If something comes back here, it replaces the text.
   */
  repair: string | null;
};

/** The state of each layer an editor started from — saving writes against it. */
export type Bases = Record<string, { generation: number; heads: string[] }>;

/** A document opened for editing. */
export type DocumentCode = { code: string; document: DocumentText; base: Bases };

/** One version of a document: an editing session. */
export type Version = {
  session: string;
  author: string;
  authorId: string;
  /** Milliseconds since 1970. */
  from: number;
  to: number;
  /** When it came to this device — long after `to`: written offline, merged only now. */
  arrived: number;
  /** The parts it changed. */
  parts: string[];
  /** Changed who sees or edits a part, added or removed one. */
  structure: boolean;
  /** Versions by others, written at the same time on the same lines. */
  overlaps: string[];
  /** In the text, waiting for the GM, or turned down. */
  review: Review;
  /** Whether the GM may delete it — not the newest, not waiting. */
  removable: boolean;
};

export type Review = "counted" | "waiting" | "rejected";

/** A folder of the gallery. `parent` empty: at the top. */
export type FolderNode = { id: string; name: string; parent: string };

/** How an image is put into a text unless the text says otherwise. Empty: nothing of its own. */
export type ImageDefaults = { view: string; edit: string; hidden: string; hiddenCaption: string };

/** An image as the gallery shows it. */
export type GalleryImage = {
  id: string;
  /** Empty where this reader may not learn it — someone else's upload. */
  name: string;
  width: number;
  height: number;
  size: number;
  /** Milliseconds since 1970. */
  uploaded: number;
  own: boolean;
  /** The documents whose parts use it now. */
  usedIn: { id: string; name: string }[];
  /** The folder it lies in, by id. Empty: in none. */
  folder: string;
  /** Named only in older versions — for the GM. */
  inVersions: boolean;
  /** Whether all of it is here yet. */
  complete: boolean;
  defaults: ImageDefaults;
};

/** A version waiting for review, and where. */
export type WaitingVersion = { article: string; name: string; session: string; author: string; to: number };

/** Stretches of text, in UTF-16 units — as the browser counts. */
export type TextRange = { start: number; end: number };

/** What is marked on one side of a version and now. */
export type SideMarks = {
  /** In the document's code. */
  code: TextRange[];
  /** In each part's text as written, by part key. */
  parts: Record<string, TextRange[]>;
  /** In each layer's stored text, by layer id — as the page reads it. */
  layers: Record<string, TextRange[]>;
};

/** Where a version and now differ, by the characters themselves. */
export type VersionMarks = {
  /** The version: what now no longer has. */
  then: SideMarks;
  /** Now: what the version did not have. */
  now: SideMarks;
  /** The code `now` was marked in — once the editor's text differs, the marks no longer fit. */
  nowCode: string;
  /** Not counted: the version stands as the text would be with it taken in. */
  review: Review;
};

/** Where one's caret stands: in the code, or in a part's text on the visual side. */
export type Caret = { code?: string; part?: string; start: number; end: number };

/** Someone else in the same document — and where their caret stands, for this reader. */
/** Someone else in the world: on which page, and whether writing there. */
export type Here = { id: string; name: string; article: string; editing: boolean };

export type Seen = {
  id: string;
  name: string;
  caret: { part: string; partStart: number; partEnd: number; codeStart: number; codeEnd: number } | null;
};

/** A document as it stood with a version — as the page shows it, and as the editor does. */
export type VersionDocument = { view: ArticleView; code: string; document: DocumentText };

/** A part before and after a version. */
export type PartChange = { layer: string; part: string; before: string; after: string; mayRevert: boolean };
