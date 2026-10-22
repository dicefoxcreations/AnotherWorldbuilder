// The bridge to Rust.
//
// There is deliberately no visibility logic here. The frontend asks for
// articles and gets back what could be decrypted — a second implementation
// of the rule in TypeScript would be exactly the gap through which players
// would otherwise see what they should not.

import { invoke, setOnline } from "./changes";
import { DEFAULT_CALENDAR, type WorldCalendar } from "../shared/Calendar";

export type IdentityInfo = {
  name: string;
  id: string;
  fingerprint: string;
  file: string;
  publicKey: string;
  /** data: URL, or null if no image is set. */
  avatar: string | null;
};

export type Role = "gm" | "player";

export type MemberInfo = {
  id: string;
  name: string;
  fingerprint: string;
  role: Role;
  attributes: string[];
  /** Their writing counts without waiting for the GM. */
  trusted: boolean;
};

export type WorldInfo = {
  id: string;
  name: string;
  icon: string;
  publicKey: string;
  tokenFile: string;
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
  /** The member the GM hands the world over to, once they show up. */
  handover: string | null;
  /** Whether players' writing waits for the GM before the others see it. */
  moderated: boolean;
  /** Versions waiting: for the GM, what players wrote; for a player, their own. */
  waiting: number;
  /** How many versions of each document stay. */
  versionLimit: number;
  /** Encrypted: views and rights count. Without, whoever has the world reads and edits all of it. */
  encrypted: boolean;
  /** Where this device keeps the readable copy — empty: nowhere. */
  readable: string;
  /** The copy lies in the world's folder (GM, unencrypted): always on. */
  readableInWorld: boolean;
  /** Worked on online: the host holds it, this device asks it for everything. */
  online: boolean;
  /** The GM's: players of this encrypted world get a copy — else they work online. */
  copies: boolean;
  /** Held here only while online — it goes when the app starts again. */
  fleeting: boolean;
  /** The server it lives on, by code — empty: this device. */
  server: string;
  /** The dice the world rolls with, by sides — empty for the usual set. */
  dice: number[];
  /** How the world counts its days. */
  calendar: WorldCalendar;
};

/** What this device does on the local network. */
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
  /** Peer to peer through shared folders. */
  folders: boolean;
  /** The port asked for. */
  port: number;
  /** What players enter to reach this device as host — empty while nothing is reachable. */
  code: string;
  /** Servers that keep worlds for the group, by their code. */
  servers: string[];
  /** Each of them as it answered last. */
  server_list: ServerEntry[];
  /** The worlds this device could offer: those it is GM of. */
  worlds: SharedWorld[];
  /** Worlds on their way to their server, by id. */
  uploads: Record<string, Upload>;
};

/** How far a world has come over to its server: records and image parts, [there, all]. */
export type Upload = { records: [number, number]; images: [number, number] };

/** A server as it answered last: its name (empty until it answered), whether this device is its admin, whether a world waits for the admin. */
export type ServerEntry = {
  code: string;
  name: string;
  admin: boolean;
  waiting: boolean;
  /** This GM's worlds waiting for the admin, by id. */
  waiting_worlds: string[];
  /** This device may bring worlds there. */
  may_bring: boolean;
  /** It asked to. */
  asked: boolean;
  reached: boolean;
};

/** A world on a server, as its admin sees it — nothing of its content. */
export type HeldWorld = { world: string; name: string; gm: string; gm_id: string; bytes: number; encrypted: boolean; closed: boolean };
/** Someone a server knows; `last` in seconds since 1970, 0: not yet. */
export type Person = { id: string; name: string; last: number; blocked: boolean };
/** What a server's admin sees. */
export type ServerState = { worlds: HeldWorld[]; gms: Person[]; asking: Person[]; users: Person[] };
/** What a server's admin orders — see `aw_sync::world::Order`. */
export type ServerOrder =
  | { kind: "allow" | "refuse" | "block" | "unblock"; id: string }
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
export type WorldDestination = { kind: "local"; path: string } | { kind: "server"; server: string };

/** A world held here, and whether it is offered or kept somewhere. */
/** How far switching a world's encryption is. */
export type SwitchProgress = { step: number; steps: number; what: string; done: number; total: number };
export type SharedWorld = { id: string; name: string; icon: string; shared: boolean; encrypted: boolean; copies: boolean; server: string };

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
  /** Worlds held here that changed — by a swap, or because someone swapped with us. */
  updated: string[];
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
  level: string;
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
  /** The order of its own sections — those its type does not set out — by key. */
  order?: string[];
  /** Asked of the GM's device and not made yet — only on the device of the player who asked. */
  pending?: boolean;
};

export type NewLayer = {
  title: string;
  text: string;
  policy: string;
  level: string;
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
 * A piece of a block — and at the same time a unit of encryption.
 *
 * When an article is created, every part becomes a layer of its own with its
 * own data key. That is the only reason a region with access of its own can
 * sit in the middle of flowing text without hiding becoming a matter of the
 * interface.
 */
export type BlockPart = {
  key: string;
  /** Markdown a new article starts with. */
  text: string;
  view: string;
  edit: string;
  level: string;
};

/** Who may see and who may change, plus how far it travels. */
export type Access = { view: string; edit: string; level: string };

/** A block a new article of this type is given. */
export type BlockPlace = "flow" | "top" | "bottom" | "side";
export type BlockFold = "open" | "folded" | "off";

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

const toSummary = (r: Raw): PackageSummary => ({
  name: r.name ?? "",
  typesNew: r.types_new ?? 0,
  typesKnown: r.types_known ?? 0,
  documentsNew: r.documents_new ?? 0,
  documentsKnown: r.documents_known ?? 0,
  imagesNew: r.images_new ?? 0,
  imagesKnown: r.images_known ?? 0,
  imagesSkipped: r.images_skipped ?? [],
});

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

// --- Conversion snake_case ↔ camelCase -----------------------------------

type Raw = Record<string, any>;

const toMember = (r: Raw): MemberInfo => ({
  id: r.id,
  name: r.name,
  fingerprint: r.fingerprint,
  role: r.role,
  attributes: r.attributes ?? [],
  trusted: !!r.trusted,
});

const toWorld = (r: Raw): WorldInfo => ({
  id: r.id,
  name: r.name,
  icon: r.icon,
  publicKey: r.public_key,
  tokenFile: r.token_file,
  folder: r.folder,
  role: r.role,
  isGm: r.is_gm,
  attributes: r.attributes ?? [],
  members: ((r.members as Raw[]) ?? []).map(toMember),
  articleCount: r.article_count,
  home: r.home ?? "",
  requests: r.requests ?? 0,
  invited: r.invited ?? [],
  handover: r.handover ?? null,
  moderated: !!r.moderated,
  waiting: r.waiting ?? 0,
  versionLimit: r.version_limit ?? 50,
  encrypted: r.encrypted !== false,
  readable: (r.readable as string | undefined) ?? "",
  readableInWorld: !!r.readable_in_world,
  copies: !!r.copies,
  fleeting: !!r.fleeting,
  server: (r.server as string | undefined) ?? "",
  online: !!r.online,
  dice: (r.dice as number[] | undefined) ?? [],
  calendar: (r.calendar as WorldCalendar | undefined) ?? DEFAULT_CALENDAR,
});

const toIdentity = (r: Raw): IdentityInfo => ({
  name: r.name,
  id: r.id,
  fingerprint: r.fingerprint,
  file: r.file,
  publicKey: r.public_key,
  avatar: r.avatar ?? null,
});

const toLayer = (l: Raw): LayerView => ({
  id: l.id,
  title: l.title,
  section: l.section,
  mayEdit: l.may_edit ?? false,
  text: l.text,
  fields: l.fields ?? {},
  policy: l.policy,
  edit: l.edit ?? "gm",
  level: l.level,
  epoch: l.epoch,
});

const toArticle = (r: Raw): ArticleView => ({
  id: r.id,
  articleType: r.article_type,
  slug: r.slug,
  name: r.name,
  visibility: r.visibility,
  edit: r.edit ?? "gm",
  layers: ((r.layers as Raw[]) ?? []).map(toLayer),
  layerTotal: r.layer_total,
  order: r.order ?? [],
  pending: !!r.pending,
});

const toField = (r: Raw): TemplateField => {
  const base = {
    key: r.key,
    label: r.label,
    access: (r.access as Access | undefined) ?? null,
    default: (r.default as string | undefined) ?? "",
  };
  switch (r.kind) {
    case "reference":
      return {
        ...base,
        kind: "reference",
        articleType: r.article_type ?? "",
        multiple: r.multiple ?? false,
        parent: r.parent ?? false,
        adds: r.adds ?? false,
        count: r.count ?? "",
        every: r.every ?? false,
        from: r.from ?? "",
      };
    case "relation":
      return { ...base, kind: "relation", articleType: r.article_type ?? "", roleType: r.role_type ?? "" };
    case "group":
      return { ...base, kind: "group", group: r.group ?? "" };
    case "text":
      return { ...base, kind: "text", multiline: r.multiline ?? false };
    case "number":
      return { ...base, kind: "number", unit: r.unit ?? undefined };
    case "heading":
      return { ...base, kind: "heading", level: r.level ?? 2 };
    case "image":
      return {
        ...base,
        kind: "image",
        width: r.width ?? 0,
        height: r.height ?? 0,
        align: r.align ?? "center",
      };
    default:
      return { ...base, kind: r.kind };
  }
};

const fromField = (f: TemplateField): Raw => {
  const base: Raw = { key: f.key, label: f.label, kind: f.kind };
  if (f.access) base.access = f.access;
  if (f.default) base.default = f.default;
  if (f.kind === "reference") {
    base.article_type = f.articleType;
    base.multiple = f.multiple;
    base.parent = f.parent;
    if (f.adds) base.adds = true;
    if (f.count) base.count = f.count;
    if (f.every) base.every = true;
    if (f.from) base.from = f.from;
  } else if (f.kind === "relation") {
    base.article_type = f.articleType;
    base.role_type = f.roleType;
  } else if (f.kind === "group") {
    base.group = f.group;
  } else if (f.kind === "text") {
    base.multiline = f.multiline;
  } else if (f.kind === "heading") {
    base.level = f.level;
  } else if (f.kind === "number") {
    if (f.unit) base.unit = f.unit;
  } else if (f.kind === "image") {
    base.width = f.width;
    base.height = f.height;
    base.align = f.align;
  }
  return base;
};

const toInfobox = (r: Raw | undefined): Infobox => ({
  align: ((r?.align as Alignment) ?? "none") as Alignment,
  width: (r?.width as string) ?? "auto",
  show: ((r?.show as Shown[]) ?? []).map((s) => ({ ...s, label: s.label ?? "" })),
  ...(r?.access ? { access: r.access as Access } : {}),
});

const fromInfobox = (s: Infobox): Raw => ({
  align: s.align,
  width: s.width,
  show: s.show,
  access: s.access ?? null,
});

const toBlock = (r: Raw): TemplateBlock => ({
  key: r.key,
  title: r.title ?? "",
  style: r.style === "box" || r.style === "quote" || r.style === "statblock" ? r.style : "text",
  parts: ((r.parts as Raw[]) ?? []).map((t: Raw) => ({
    key: t.key,
    text: t.text ?? "",
    view: t.view ?? "players",
    edit: t.edit ?? "gm",
    level: t.level ?? "open",
  })),
  place: r.place === "top" || r.place === "bottom" || r.place === "side" ? r.place : "flow",
  width: typeof r.width === "number" ? r.width : 0,
  fold: r.fold === "folded" || r.fold === "off" ? r.fold : "open",
  heading: typeof r.heading === "number" ? r.heading : 1,
  new: (r.new as string[] | undefined) ?? [],
});

const fromBlock = (b: TemplateBlock): Raw => ({
  key: b.key,
  title: b.title,
  style: b.style,
  parts: b.parts,
  place: b.place ?? "flow",
  width: b.width ?? 0,
  fold: b.fold ?? "open",
  heading: b.heading ?? 1,
  new: b.new ?? [],
});

const toTemplate = (r: Raw): Template => ({
  id: r.id,
  name: r.name,
  icon: r.icon ?? "",
  supertype: r.supertype ?? "",
  group: !!r.group,
  rules: ((r.rules as Raw[]) ?? []).map((x) => ({ name: x.name, params: (x.params as string[]) ?? [], formula: x.formula ?? "" })),
  description: r.description ?? "",
  properties: ((r.properties as Raw[]) ?? []).map(toField),
  infobox: toInfobox(r.infobox as Raw | undefined),
  auth: (r.auth as Access | undefined) ?? null,
  namePattern: (r.name_pattern as string) || "%%input%%",
  page: r.page ?? "",
  blocks: ((r.blocks as Raw[]) ?? []).map(toBlock),
  order: (r.order as string[]) ?? [],
  create: (r.create as string) ?? "",
});

const fromTemplate = (t: Template): Raw => ({
  id: t.id,
  name: t.name,
  icon: t.icon,
  supertype: t.supertype,
  group: t.group,
  rules: t.rules,
  description: t.description,
  properties: t.properties.map(fromField),
  infobox: fromInfobox(t.infobox),
  auth: t.auth,
  name_pattern: t.namePattern,
  page: t.page,
  blocks: t.blocks.map(fromBlock),
  order: t.order ?? [],
  create: t.create ?? "",
});

const toDocument = (r: Raw): DocumentText => ({
  name: (r.name as string) ?? "",
  access: r.access as Access,
  ...(r.infobox ? { infobox: r.infobox as Access } : {}),
  properties: ((r.properties as Raw[]) ?? []).map((p) => ({ name: p.name, value: p.value ?? "", ...(p.group?.length ? { group: p.group as string[] } : {}) })),
  blocks: ((r.blocks as Raw[]) ?? []).map(toBlock),
});

const fromDocument = (d: DocumentText): Raw => ({
  name: d.name,
  access: d.access,
  infobox: d.infobox ?? null,
  properties: d.properties,
  blocks: d.blocks.map(fromBlock),
});

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
  /** `#property` shows that property's value instead of the name. */
  | { kind: "wikilink"; target: string; text: string; of: string; property: string }
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
  | { kind: "embed"; of: string; fold?: string; title?: string }
  /** `for`: a stand-in for a part this reader may not open. */
  | { kind: "image"; src: string; align: string; wrap: boolean; width: number; caption: string; for: string }
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
export type MapAccess = { view?: string; edit?: string; level?: string };
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
export type DocumentText = {
  name: string;
  access: Access;
  /** The infobox's own access — none: as the document's. */
  infobox?: Access;
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
  /** Written while the part's access changed and not carried over — in the history, not in the text. */
  stale: boolean;
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

const toCode = (r: Raw): DocumentCode => ({
  code: r.code as string,
  document: toDocument(r.document as Raw),
  base: r.base as Bases,
});

export const api = {
  storageLocations: (paths: string[]) => invoke<StorageView>("storage_locations", { paths }),
  storageStatus: () => invoke<StorageView>("storage_status"),
  /** Chooses where the data lies; takes what already lies there, copies nothing. */
  storageChoose: (choice: StorageChoice) => invoke<StorageView>("storage_choose", { choice }),
  /** Whether profile and worlds would be taken from there rather than brought along. */
  storagePeek: (choice: StorageChoice) =>
    invoke<{ profile: boolean; worlds: boolean }>("storage_peek", { choice }),
  /** Shows the profile, the worlds or a shared folder in the file manager. */
  storageOpen: (path: string) => invoke<void>("storage_open", { path }),
  /** Changes where the data lies — with `carry`, copying it there first. The old folder stays. */
  storageMove: (choice: StorageChoice, carry: boolean) =>
    invoke<StorageView>("storage_move", { choice, carry }),
  identityStatus: async () => {
    const r = await invoke<Raw | null>("identity_status");
    return r ? toIdentity(r) : null;
  },
  identityCreate: async (name: string) =>
    toIdentity(await invoke<Raw>("identity_create", { name })),
  identityRename: async (name: string) =>
    toIdentity(await invoke<Raw>("identity_rename", { name })),
  identityExport: (path: string) => invoke<string>("identity_export", { path }),
  /** Writes one's keys — identity, and the world keys this profile holds — locked with a passphrase. */
  keysBackup: (path: string, passphrase: string) => invoke<string>("keys_backup", { path, passphrase }),
  /** Brings keys back from a backup: the identity where there is none, what is missing of each world. */
  keysRestore: async (path: string, passphrase: string) => toIdentity(await invoke<Raw>("keys_restore", { path, passphrase })),
  avatarSet: (path: string) => invoke<string>("avatar_set", { path }),
  avatarRemove: () => invoke<void>("avatar_remove"),

  worldsList: async () => {
    const worlds = (await invoke<Raw[]>("worlds_list")).map(toWorld);
    setOnline(worlds.filter((w) => w.online).map((w) => w.id));
    return worlds;
  },
  worldCreate: async (name: string, destination: WorldDestination) => toWorld(await invoke<Raw>("world_create", { name, destination })),
  worldCopy: async (world: string, destination: WorldDestination) => toWorld(await invoke<Raw>("world_copy", { world, destination })),
  worldOpen: async (path: string, location: string) => toWorld(await invoke<Raw>("world_open", { path, location })),
  worldShare: (world: string, member: string, path: string) =>
    invoke<string>("world_share", { world, member, path }),
  worldTokenCheck: (world: string) => invoke<boolean>("world_token_check", { world }),
  worldIconSet: async (world: string, icon: string) =>
    toWorld(await invoke<Raw>("world_icon_set", { world, icon })),
  /** Deletes a world for good. `confirmation` has to be the world's name. */
  worldDelete: (world: string, confirmation: string) =>
    invoke<void>("world_delete", { world, confirmation }),
  /** What a player is assigned to: `doc:…`, `type:…` and attributes. The GM's. */
  memberAttributesSet: async (world: string, member: string, attributes: string[]) =>
    toWorld(await invoke<Raw>("member_attributes_set", { world, member, attributes })),
  memberAdd: async (world: string, path: string, attributes: string[]) =>
    toWorld(await invoke<Raw>("member_add", { world, path, attributes })),
  /** Lets a player in ahead of time, by their whole id. */
  memberInvite: async (world: string, id: string) =>
    toWorld(await invoke<Raw>("member_invite", { world, id })),
  /** Takes a member out — or an id let in ahead of time. */
  memberRemove: async (world: string, id: string) =>
    toWorld(await invoke<Raw>("member_remove", { world, id })),
  /** Hands the world over once the member shows up; `null` takes it back. */
  gmHandover: async (world: string, member: string | null) =>
    toWorld(await invoke<Raw>("gm_handover", { world, member })),
  /** Hands the world over as a file — this device steps down at once. */
  gmHandoverFile: (world: string, member: string, path: string) =>
    invoke<string>("gm_handover_file", { world, member, path }),

  networkSettings: () => invoke<NetworkView>("network_settings"),
  networkEnable: (enabled: boolean) => invoke<NetworkView>("network_enable", { enabled }),
  networkInternet: (enabled: boolean, stun: string[]) => invoke<NetworkView>("network_internet", { enabled, stun }),
  networkFolders: (enabled: boolean) => invoke<NetworkView>("network_folders", { enabled }),
  networkPort: (port: number | null) => invoke<NetworkView>("network_port", { port }),
  worldSwitchProgress: (world: string) => invoke<SwitchProgress | null>("world_switch_progress", { world }),
  networkCopiesSet: (world: string, copies: boolean) => invoke<NetworkView>("network_copies_set", { world, copies }),
  networkServers: (servers: string[]) => invoke<NetworkView>("network_servers", { servers }),
  /** What a server holds and who uses it — for its admin; with an order, carried out first. */
  serverManage: (server: string, order: ServerOrder | null = null) => invoke<ServerState>("server_manage", { server, order }),
  networkShare: (world: string, shared: boolean) => invoke<NetworkView>("network_share", { world, shared }),
  /** Where a GM's world lives: on a server, by its code, or on this device (`null`). */
  networkHome: (world: string, server: string | null) => invoke<NetworkView>("network_home", { world, server }),
  /** Asks a server's admin to let this device bring worlds there. */
  serverHostWorlds: (server: string) => invoke<NetworkView>("server_host_worlds", { server }),
  networkWorlds: () => invoke<NetWorld[]>("network_worlds"),
  /** Asks a found world's GM for it — or, with `join`, to become a member. */
  networkJoin: async (world: string, join: boolean): Promise<JoinOutcome> => {
    const r = await invoke<Raw>("network_join", { world, join });
    return { kind: r.kind, world: r.world ? toWorld(r.world) : null };
  },
  /** Keeps held and asked-for worlds current — over the network and through shared folders. */
  networkSync: async (): Promise<SyncOutcome> => {
    const r = await invoke<Raw>("network_sync");
    return { joined: r.joined.map(toWorld), updated: r.updated };
  },
  joinRequests: (world: string) => invoke<JoinRequest[]>("join_requests", { world }),
  joinAnswer: async (world: string, id: string, accept: boolean) =>
    toWorld(await invoke<Raw>("join_answer", { world, id, accept })),

  attributeCreate: (world: string, attribute: string) =>
    invoke<string[]>("attribute_create", { world, attribute }),
  attributeRevoke: (world: string, attribute: string) =>
    invoke<number>("attribute_revoke", { world, attribute }),

  templatesLoad: async (world: string) =>
    (await invoke<Raw[]>("templates_load", { world })).map(toTemplate),
  templateSave: async (world: string, template: Template) =>
    (await invoke<Raw[]>("template_save", { world, template: fromTemplate(template) })).map(
      toTemplate,
    ),
  templateDelete: async (world: string, slug: string) =>
    (await invoke<Raw[]>("template_delete", { world, slug })).map(toTemplate),

  articleCreate: (
    world: string,
    articleType: string,
    slug: string,
    name: string,
    visibility: string,
    layers: NewLayer[],
  ) =>
    invoke<string>("article_create", { world, articleType, slug, name, visibility, layers }),
  /**
   * A new document, by a player — or one asked for again, changed (`article`).
   * The GM's device makes it with the type's access; here it is the player's
   * at once (`pending`). Resolves with its id.
   */
  articlePropose: (world: string, article: string | null, articleType: string, slug: string, name: string, layers: NewLayer[]) =>
    invoke<string>("article_propose", { world, article, articleType, slug, name, layers }),
  /** Texts as they are written: the ids stored in them turned back into names. */
  textsWritten: (world: string, articleType: string, texts: string[]) =>
    invoke<string[]>("texts_written", { world, articleType, texts }),
  /**
   * Changes who may see a part. Leaving `layer` out means: the whole
   * article.
   *
   * Rust rejects the change when the full keyring is missing. There is
   * deliberately no check here — one in the frontend could be bypassed.
   */
  visibilitySet: (world: string, article: string, layer: string | null, policy: string) =>
    invoke<void>("visibility_set", { world, article, layer, policy }),

  /**
   * Deletes an article for good.
   *
   * `confirmation` has to be the article's name — Rust checks that and
   * refuses when the full keyring is missing.
   */
  articleDelete: (world: string, article: string, confirmation: string) =>
    invoke<void>("article_delete", { world, article, confirmation }),

  view: async (world: string, previewAs?: string[]) =>
    (await invoke<Raw[]>("view", { world, previewAs: previewAs ?? null })).map(toArticle),
  todayInWorld: (world: string, day: number) =>
    invoke<string>("today_in_world", { world, day }),

  /**
   * Reads a template from its text form.
   *
   * Does not abort on errors but returns them with a line number — while
   * typing, a half-finished line is the normal case.
   */
  templateRead: async (world: string, text: string, base: Template): Promise<ReadResult> => {
    const r = await invoke<Raw>("template_read", { world, text, base: fromTemplate(base) });
    return {
      template: r.template ? toTemplate(r.template as Raw) : null,
      errors: (r.errors as LineError[]) ?? [],
      repair: (r.repair as string | null) ?? null,
    };
  },
  templateWrite: (world: string, template: Template) =>
    invoke<string>("template_write", { world, template: fromTemplate(template) }),
  /** Opens the legend in a window of its own. */
  legendOpen: () => invoke<void>("legend_open"),
  /** A place of a world in a window of its own; `part` scrolls to a part by its title. */
  windowOpen: (world: string, place: Place, title: string, part?: string) =>
    invoke<void>("window_open", { world, place: JSON.stringify(place), title, part: part ?? null }),
  /** Whether the tools for developing show — see `DEV_TOOLS` in Rust. */
  devTools: () => invoke<boolean>("dev_tools"),
  /** Shows the window as a phone, or as it was. A development tool. */
  phoneView: (on: boolean) => invoke<void>("phone_view", { on }),
  /** Opens a document as code — only with what this reader can decrypt. */
  documentOpen: async (world: string, article: string) =>
    toCode(await invoke<Raw>("document_open", { world, article })),
  /** Reads code while it is typed. `base` is the state before the keystroke. */
  documentRead: async (world: string, article: string, text: string, base: DocumentText) => {
    const r = await invoke<Raw>("document_read", {
      world,
      article,
      text,
      base: fromDocument(base),
    });
    return {
      document: toDocument(r.document as Raw),
      errors: (r.errors as LineError[]) ?? [],
    };
  },
  /** Writes a document as code — after a change on the visual side. */
  documentWrite: (document: DocumentText) =>
    invoke<string>("document_write", { document: fromDocument(document) }),
  /**
   * Saves a document from its code — what changed against `base`, the state
   * the editor started from — and returns it as it stands now, with what
   * others wrote meanwhile.
   *
   * Rust decides what each change may do: text needs the right to edit,
   * access and new or removed parts need the game master's keys.
   */
  documentSave: async (world: string, article: string, text: string, base: Bases, session: string) =>
    toCode(await invoke<Raw>("document_save", { world, article, text, base, session })),
  /** Ends an editing session: what it wrote becomes a version of each document it wrote in. */
  sessionEnd: (world: string, session: string) => invoke<void>("session_end", { world, session }),
  /**
   * Sets what `article` says about `with` in a relation property — and the
   * other side too, as far as this reader may. `null` removes it.
   */
  /** Sets relations — several as one change. `roles: null` removes one. */
  relationsSet: (world: string, changes: { article: string; property: string; with: string; roles: [string, string] | null }[]) =>
    invoke<void>("relations_set", { world, changes }),
  /** The versions of a document, newest first. */
  articleVersions: async (world: string, article: string): Promise<Version[]> =>
    (await invoke<Raw[]>("article_versions", { world, article })).map((v) => ({
      session: v.session,
      author: v.author,
      authorId: v.author_id,
      from: v.from,
      to: v.to,
      arrived: v.arrived,
      parts: v.parts,
      structure: v.structure,
      stale: v.stale,
      overlaps: v.overlaps,
      review: v.review,
      removable: !!v.removable,
    })),
  /** What a version changed, part by part. */
  versionCompare: async (world: string, article: string, session: string): Promise<PartChange[]> =>
    (await invoke<Raw[]>("version_compare", { world, article, session })).map((c) => ({
      layer: c.layer,
      part: c.part,
      before: c.before,
      after: c.after,
      mayRevert: c.may_revert,
    })),
  /** Rolls for everyone in the session — passed on with the next digest. */
  diceRoll: (world: string, roll: SharedRoll) => invoke<void>("dice_roll", { world, roll }),
  /** The dice rolled lately in a world, by anyone — oldest first. */
  diceRolls: (world: string) => invoke<SharedRoll[]>("dice_rolls", { world }),
  /** This device's log and favourites of a world. */
  diceLog: (world: string) => invoke<DiceLog>("dice_log", { world }),
  /** Keeps a roll in the log — a clearing one from the GM empties it. */
  diceLogAdd: (world: string, roll: SharedRoll) => invoke<DiceLog>("dice_log_add", { world, roll }),
  /** Clears the log, here and for everyone in the session — the GM's. */
  diceLogClear: (world: string) => invoke<DiceLog>("dice_log_clear", { world }),
  diceFavoritesSet: (world: string, favorites: Favorite[]) => invoke<DiceLog>("dice_favorites_set", { world, favorites }),
  /** Tells where one writes — and learns where the others do. */
  presence: async (world: string, article: string, caret: Caret | null): Promise<Seen[]> =>
    (await invoke<Raw[]>("presence", { world, article, caret })).map((s) => ({
      id: s.id,
      name: s.name,
      caret: s.caret
        ? {
            part: s.caret.part,
            partStart: s.caret.part_start,
            partEnd: s.caret.part_end,
            codeStart: s.caret.code_start,
            codeEnd: s.caret.code_end,
          }
        : null,
    })),
  /** Tells on which page one is — and learns who else is in the world, and where. */
  worldPresence: (world: string, article: string) => invoke<Here[]>("world_presence", { world, article }),
  /** A document as it stood with a version. */
  versionDocument: async (world: string, article: string, session: string): Promise<VersionDocument> => {
    const r = await invoke<Raw>("version_document", { world, article, session });
    return { view: toArticle(r.view as Raw), code: r.code as string, document: toDocument(r.document as Raw) };
  },
  /** Where a version and now differ, by the characters themselves. */
  versionMarks: async (world: string, article: string, session: string): Promise<VersionMarks> => {
    const r = await invoke<Raw>("version_marks", { world, article, session });
    return { then: r.then, now: r.now, nowCode: r.now_code, review: r.review };
  },
  /** The GM takes a waiting version into the text — or turns it down. */
  versionVerdict: (world: string, article: string, session: string, accepted: boolean) =>
    invoke<void>("version_verdict", { world, article, session, accepted }),
  /** The versions waiting: for the GM, what players wrote; for a player, their own. */
  waitingVersions: (world: string) => invoke<WaitingVersion[]>("waiting_versions", { world }),
  /** Uploads an image under a name — its metadata is taken out first. Returns its id. */
  imageUpload: (world: string, path: string, name: string, folder = "") =>
    invoke<string>("image_upload", { world, path, name, folder }),
  /** The folders of the gallery — the GM's. */
  galleryFolders: (world: string) => invoke<FolderNode[]>("gallery_folders", { world }),
  /** Sets the folders of the gallery, as a whole tree — the GM's. */
  galleryFoldersSet: (world: string, folders: FolderNode[]) => invoke<void>("gallery_folders_set", { world, folders }),
  /** Puts an image into a folder of the gallery — empty: into none. */
  imageMove: (world: string, image: string, folder: string) => invoke<void>("image_move", { world, image, folder }),
  /** Gives an image a new name — its uploader or the GM. */
  imageRename: (world: string, image: string, name: string) => invoke<void>("image_rename", { world, image, name }),
  /** Deletes images for good, on every device — the GM's. */
  imageDelete: (world: string, images: string[]) => invoke<void>("image_delete", { world, images }),
  /** The gallery, as this reader may see it. */
  gallery: async (world: string): Promise<GalleryImage[]> =>
    (await invoke<Raw[]>("gallery", { world })).map((i) => ({
      id: i.id,
      name: i.name,
      width: i.width,
      height: i.height,
      size: i.size,
      uploaded: i.uploaded,
      own: !!i.own,
      usedIn: i.used_in ?? [],
      folder: i.folder ?? "",
      inVersions: !!i.in_versions,
      complete: !!i.complete,
      defaults: {
        view: i.defaults?.view ?? "",
        edit: i.defaults?.edit ?? "",
        hidden: i.defaults?.hidden ?? "",
        hiddenCaption: i.defaults?.hidden_caption ?? "",
      },
    })),
  /** Sets how an image is put into a text — its uploader or the GM. */
  imageDefaultsSet: (world: string, image: string, d: ImageDefaults) =>
    invoke<void>("image_defaults_set", {
      world,
      image,
      defaults: { view: d.view, edit: d.edit, hidden: d.hidden, hidden_caption: d.hiddenCaption },
    }),
  /** An image as a `data:` URL — the original or its preview. */
  referencesSet: (world: string, article: string, field: string, ids: string[]) =>
    invoke<void>("references_set", { world, article, field, ids }),
  imageData: (world: string, image: string, preview: boolean) => invoke<string>("image_data", { world, image, preview }),
  /** Puts an image into an image field of a document, or takes it out. */
  imageFieldSet: (world: string, article: string, field: string, image: string | null) =>
    invoke<void>("image_field_set", { world, article, field, image }),
  /** Deletes a version for good, on every device — the GM's. */
  versionDelete: (world: string, article: string, session: string) =>
    invoke<void>("version_delete", { world, article, session }),
  /** Sets how many versions of each document stay — the GM's. */
  calendarSet: async (world: string, calendar: WorldCalendar) => toWorld(await invoke<Raw>("calendar_set", { world, calendar })),
  worldVersionsSet: async (world: string, max: number) => toWorld(await invoke<Raw>("world_versions_set", { world, max })),
  /** Encrypts the world or opens it — the GM's. */
  worldReadableSet: async (world: string, on: boolean) => toWorld(await invoke<Raw>("world_readable_set", { world, on })),
  worldEncryptionSet: async (world: string, on: boolean) => toWorld(await invoke<Raw>("world_encryption_set", { world, on })),
  /** The dice the world rolls with — the GM's. Empty: the usual set. */
  worldDiceSet: async (world: string, sides: number[]) => toWorld(await invoke<Raw>("world_dice_set", { world, sides })),
  /** Writes a package — types, or documents with their types and images — into a folder, unencrypted. */
  packageExport: async (world: string, path: string, choice: { name: string; types: string[]; documentsOf: string[] }) =>
    toSummary(await invoke<Raw>("package_export", { world, path, choice: { name: choice.name, types: choice.types, documents_of: choice.documentsOf } })),
  /** What a package would bring into this world. */
  packageRead: async (world: string, path: string) => toSummary(await invoke<Raw>("package_read", { world, path })),
  /** Takes a package in; `package-progress` tells how far. */
  packageImport: async (world: string, path: string) => toSummary(await invoke<Raw>("package_import", { world, path })),
  /** The side bar's entries as far as this device may see them — `null` while the GM has set none. */
  navigationLoad: (world: string) => invoke<NavEntry[] | null>("navigation_load", { world }),
  /** Sets the side bar's entries — the GM's. */
  navigationSet: (world: string, entries: NavEntry[]) => invoke<void>("navigation_set", { world, entries }),
  /** Switches review on or off: whether players' writing waits for the GM. */
  worldModerationSet: async (world: string, on: boolean) =>
    toWorld(await invoke<Raw>("world_moderation_set", { world, on })),
  /** Trusts a member — their writing counts at once — or no longer. */
  memberTrustSet: async (world: string, member: string, trusted: boolean) =>
    toWorld(await invoke<Raw>("member_trust_set", { world, member, trusted })),
  /** Takes a version's lines into the text that a change of access left behind. */
  versionCarry: (world: string, article: string, session: string) =>
    invoke<number>("version_carry", { world, article, session }),
  /** Takes a version back, as part of the editing session `into`. */
  versionRevert: (world: string, article: string, session: string, into: string) =>
    invoke<number>("version_revert", { world, article, session, into }),
  /** Splits Markdown into the tree the display builds elements from. */
  markdownRead: (text: string) => invoke<Node[]>("markdown_read", { text }),
};
