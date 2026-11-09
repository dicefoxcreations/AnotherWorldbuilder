// Between Rust's names and the interface's: snake_case ↔ camelCase, and
// what an older answer leaves out filled in.

import { DEFAULT_CALENDAR, type WorldCalendar } from "../shared/Calendar";
import { type Access, type Alignment, type ArticleView, type Bases, type BlockLook, type Cover, type DocumentCode, type DocumentText, type IdentityInfo, type Infobox, type LayerView, type MemberInfo, type PackageSummary, type Shown, type Template, type TemplateBlock, type TemplateField, type WorldInfo } from "./model";

export type Raw = Record<string, any>;

export const toMember = (r: Raw): MemberInfo => ({
  id: r.id,
  name: r.name,
  fingerprint: r.fingerprint,
  role: r.role,
  attributes: r.attributes ?? [],
  trusted: !!r.trusted,
  slot: (r.slot as string | undefined) ?? "",
});

export const toWorld = (r: Raw): WorldInfo => ({
  id: r.id,
  name: r.name,
  icon: r.icon,
  folder: r.folder,
  role: r.role,
  isGm: r.is_gm,
  attributes: r.attributes ?? [],
  members: ((r.members as Raw[]) ?? []).map(toMember),
  articleCount: r.article_count,
  home: r.home ?? "",
  requests: r.requests ?? 0,
  invited: r.invited ?? [],
  takeovers: (r.takeovers as Record<string, string>) ?? {},
  colours: (r.colours as Record<string, string>) ?? {},
  moderated: !!r.moderated,
  waiting: r.waiting ?? 0,
  versionLimit: r.version_limit ?? 50,
  server: (r.server as string | undefined) ?? "",
  serverId: (r.server_id as string | undefined) ?? "",
  offered: !!r.offered,
  listed: !!r.listed,
  online: !!r.online || !!r.server_id || !!r.server,
  dice: (r.dice as number[] | undefined) ?? [],
  calendar: (r.calendar as WorldCalendar | undefined) ?? DEFAULT_CALENDAR,
  supporters: (r.supporters as string[] | undefined) ?? [],
  supporter: !!r.supporter,
  roles: (r.roles as string[] | undefined) ?? [],
  mailboxes: (r.mailboxes as string | undefined) ?? "",
});

export const toIdentity = (r: Raw): IdentityInfo => ({
  name: r.name,
  id: r.id,
  fingerprint: r.fingerprint,
  file: r.file,
  publicKey: r.public_key,
  avatar: r.avatar ?? null,
});

export const toLayer = (l: Raw): LayerView => ({
  id: l.id,
  title: l.title,
  section: l.section,
  mayEdit: l.may_edit ?? false,
  text: l.text,
  fields: l.fields ?? {},
  policy: l.policy,
  edit: l.edit ?? "gm",
  epoch: l.epoch,
});

export const toArticle = (r: Raw): ArticleView => ({
  id: r.id,
  articleType: r.article_type,
  slug: r.slug,
  name: r.name,
  visibility: r.visibility,
  edit: r.edit ?? "gm",
  layers: ((r.layers as Raw[]) ?? []).map(toLayer),
  layerTotal: r.layer_total,
  order: r.order ?? [],
  cover: !!r.cover,
  startable: (r.startable as string[] | undefined) ?? [],
  partial: !!r.partial,
  looks: (r.looks as Record<string, BlockLook> | undefined) ?? {},
});

export const toField = (r: Raw): TemplateField => {
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

export const fromField = (f: TemplateField): Raw => {
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

export const toInfobox = (r: Raw | undefined): Infobox => ({
  align: ((r?.align as Alignment) ?? "none") as Alignment,
  width: (r?.width as string) ?? "auto",
  show: ((r?.show as Shown[]) ?? []).map((s) => ({ ...s, label: s.label ?? "" })),
  ...(r?.access ? { access: r.access as Access } : {}),
});

export const fromInfobox = (s: Infobox): Raw => ({
  align: s.align,
  width: s.width,
  show: s.show,
  access: s.access ?? null,
});

export const toBlock = (r: Raw): TemplateBlock => ({
  key: r.key,
  title: r.title ?? "",
  style: r.style === "box" || r.style === "quote" || r.style === "statblock" ? r.style : "text",
  parts: ((r.parts as Raw[]) ?? []).map((t: Raw) => ({
    key: t.key,
    text: t.text ?? "",
    view: t.view ?? "players",
    edit: t.edit ?? "gm",
  })),
  place: r.place === "top" || r.place === "bottom" || r.place === "side" ? r.place : "flow",
  width: typeof r.width === "number" ? r.width : 0,
  fold: r.fold === "folded" || r.fold === "off" ? r.fold : "open",
  heading: typeof r.heading === "number" ? r.heading : 1,
  new: (r.new as string[] | undefined) ?? [],
});

export const fromBlock = (b: TemplateBlock): Raw => ({
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

export const toTemplate = (r: Raw): Template => ({
  id: r.id,
  name: r.name,
  icon: r.icon ?? "",
  supertype: r.supertype ?? "",
  group: !!r.group,
  navigationHidden: !!r.navigation_hidden,
  rules: ((r.rules as Raw[]) ?? []).map((x) => ({ name: x.name, params: (x.params as string[]) ?? [], formula: x.formula ?? "" })),
  description: r.description ?? "",
  properties: ((r.properties as Raw[]) ?? []).map(toField),
  infobox: toInfobox(r.infobox as Raw | undefined),
  auth: (r.auth as Access | undefined) ?? null,
  namePattern: (r.name_pattern as string) || "%%input%%",
  page: r.page ?? "",
  blocks: ((r.blocks as Raw[]) ?? []).map(toBlock),
  order: (r.order as string[]) ?? [],
  sort: (r.sort as string[]) ?? [],
  create: (r.create as string) ?? "",
});

export const fromTemplate = (t: Template): Raw => ({
  id: t.id,
  name: t.name,
  icon: t.icon,
  supertype: t.supertype,
  group: t.group,
  navigation_hidden: !!t.navigationHidden,
  rules: t.rules,
  description: t.description,
  properties: t.properties.map(fromField),
  infobox: fromInfobox(t.infobox),
  auth: t.auth,
  name_pattern: t.namePattern,
  page: t.page,
  blocks: t.blocks.map(fromBlock),
  order: t.order ?? [],
  sort: t.sort ?? [],
  create: t.create ?? "",
});

export const toDocument = (r: Raw): DocumentText => ({
  name: (r.name as string) ?? "",
  access: r.access as Access,
  ...(r.infobox ? { infobox: r.infobox as Access } : {}),
  ...(r.cover ? { cover: r.cover as Cover } : {}),
  properties: ((r.properties as Raw[]) ?? []).map((p) => ({ name: p.name, value: p.value ?? "", ...(p.group?.length ? { group: p.group as string[] } : {}) })),
  blocks: ((r.blocks as Raw[]) ?? []).map(toBlock),
});

export const fromDocument = (d: DocumentText): Raw => ({
  name: d.name,
  access: d.access,
  infobox: d.infobox ?? null,
  cover: d.cover ?? null,
  properties: d.properties,
  blocks: d.blocks.map(fromBlock),
});

export const toSummary = (r: Raw): PackageSummary => ({
  name: r.name ?? "",
  typesNew: r.types_new ?? 0,
  typesKnown: r.types_known ?? 0,
  documentsNew: r.documents_new ?? 0,
  documentsKnown: r.documents_known ?? 0,
  imagesNew: r.images_new ?? 0,
  imagesKnown: r.images_known ?? 0,
  imagesSkipped: r.images_skipped ?? [],
});

export const toCode = (r: Raw): DocumentCode => ({
  code: r.code as string,
  document: toDocument(r.document as Raw),
  base: r.base as Bases,
});
