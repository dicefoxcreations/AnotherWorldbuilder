// The bridge to Rust.
//
// There is deliberately no visibility logic here. The frontend asks for
// articles and gets back what its reader may see — a second implementation
// of the rule in TypeScript would be exactly the gap through which players
// would otherwise see what they should not.

import { invoke, setOnline, setWorldOnline } from "./changes";
import { type WorldCalendar } from "../shared/Calendar";
import { type BackupFound, type BackupPlan, type BackupView, type Bases, type BlockLook, type BrowserView, type Caret, type DiceLog, type DocumentText, type Favorite, type FolderNode, type GalleryImage, type Here, type ImageDefaults, type IndexPart, type JoinOutcome, type JoinRequest, type LineError, type NavEntry, type NetWorld, type NetworkView, type NewLayer, type PartChange, type Place, type ReadResult, type Seen, type ServerBackupPlan, type ServerBackups, type ServerOrder, type ServerState, type SharedRoll, type SincePart, type StorageChoice, type StorageView, type SyncOutcome, type Template, type TextsPart, type TrashedView, type Version, type VersionDocument, type VersionMarks, type WaitingVersion, type WorldDestination, type Node } from "./model";
import { type Raw, fromDocument, fromTemplate, toArticle, toCode, toDocument, toIdentity, toSummary, toTemplate, toWorld } from "./convert";

export * from "./model";

/** Documents being made or made just now, by world, type and slug — see `once`. */
const creating = new Map<string, Promise<string>>();
/** How long one made stays the answer to making it again. */
const MADE_MS = 15_000;
/** Ids made here just now — on their way into the world as it is read. */
const madeIds = new Set<string>();

/** Whether a document was made here just now — not there yet, only not read yet. */
export const justMade = (id: string) => madeIds.has(id);

/**
 * Makes a document once: a second click while the first is on its way — or
 * soon after, before the new one shows — gets the same document, not
 * another one of the same name. A failure may be tried again at once.
 */
function once(key: string, make: () => Promise<string>): Promise<string> {
  const known = creating.get(key);
  if (known) return known;
  const made = make();
  creating.set(key, made);
  made.then(
    (id) => {
      madeIds.add(id);
      setTimeout(() => {
        madeIds.delete(id);
        if (creating.get(key) === made) creating.delete(key);
      }, MADE_MS);
    },
    () => creating.get(key) === made && creating.delete(key),
  );
  return made;
}

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
  /** Writes one's identity, locked with a passphrase. */
  keysBackup: (path: string, passphrase: string) => invoke<string>("keys_backup", { path, passphrase }),
  /** Brings keys back from a backup: the identity where there is none, what is missing of each world. */
  keysBackupPhone: (passphrase: string) => invoke<string>("keys_backup_phone", { passphrase }),
  keysRestoreBytes: async (bytes: Uint8Array, passphrase: string) => toIdentity(await invoke<Raw>("keys_restore_bytes", { bytes: Array.from(bytes), passphrase })),
  keysRestore: async (path: string, passphrase: string) => toIdentity(await invoke<Raw>("keys_restore", { path, passphrase })),
  avatarSet: (path: string) => invoke<string>("avatar_set", { path }),
  avatarRemove: () => invoke<void>("avatar_remove"),

  worldsList: async () => {
    const worlds = (await invoke<Raw[]>("worlds_list")).map(toWorld);
    setOnline(worlds.filter((w) => w.online).map((w) => w.id));
    return worlds;
  },
  /** Confirms server access before exposing any world's content. */
  worldConnect: async (world: string) => {
    const connected = toWorld(await invoke<Raw>("world_connect", { world }));
    setWorldOnline(world, connected.online);
    return connected;
  },
  worldDisconnect: (world: string) => invoke<void>("world_disconnect", { world }),
  worldCreate: async (name: string, destination: WorldDestination) => toWorld(await invoke<Raw>("world_create", { name, destination })),
  worldCopy: async (world: string, destination: WorldDestination) => toWorld(await invoke<Raw>("world_copy", { world, destination })),
  /** A world's backups: how often, how many stay, where — and those there are. */
  backupView: (world: string) => invoke<BackupView>("backup_view", { world }),
  backupPlanSet: (world: string, plan: BackupPlan) => invoke<BackupView>("backup_plan_set", { world, plan }),
  backupNow: (world: string) => invoke<BackupView>("backup_now", { world }),
  /** The server's backups of a world it holds — with a plan, set first. */
  serverBackups: (world: string, plan: ServerBackupPlan | null = null) => invoke<ServerBackups>("server_backups", { world, plan }),
  backupOpen: (world: string) => invoke<void>("backup_open", { world }),
  /** Brings a world back from a backup — `path` is its `backup.awbackup`. */
  backupRestore: async (path: string) => toWorld(await invoke<Raw>("backup_restore", { path })),
  backupFound: () => invoke<BackupFound[]>("backup_found"),
  /** A packed backup handed over piece by piece, in Base64; answers where it lies, for `backupRestore`. */
  backupReceive: (piece: string, first: boolean) => invoke<string>("backup_receive", { piece, first }),
  /** Moves a GM's world into a folder here (empty: where its copy lies) or onto a server. */
  worldMove: async (world: string, destination: WorldDestination) => toWorld(await invoke<Raw>("world_move", { world, destination })),
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
  /** One's own colour in a world: on one's name and the documents assigned to one. */
  memberColourSet: async (world: string, colour: string) => toWorld(await invoke<Raw>("member_colour_set", { world, colour })),
  /** A player's new id takes the old one's place — at once, or once it is let in. */
  memberReplace: async (world: string, old: string, id: string) =>
    toWorld(await invoke<Raw>("member_replace", { world, old, new: id })),
  /** Hands the world over once the member shows up; `null` takes it back. */
  gmHandover: async (world: string, member: string | null) =>
    toWorld(await invoke<Raw>("gm_handover", { world, member })),

  /** Makes a world one without a server: its players' mailboxes in `folder`. */
  mailboxSetup: async (world: string, folder: string) => toWorld(await invoke<Raw>("mailbox_setup", { world, folder })),
  /** Places for players, by name — a mailbox each, to share with them. */
  mailboxAdd: async (world: string, names: string[]) => toWorld(await invoke<Raw>("mailbox_add", { world, names })),
  /** Frees a player's place: whoever writes into the mailbox next takes it. */
  mailboxReset: async (world: string, member: string) => toWorld(await invoke<Raw>("mailbox_reset", { world, member })),
  /** Takes part in a world through one's mailbox folder. */
  mailboxOpen: async (folder: string) => toWorld(await invoke<Raw>("mailbox_open", { folder })),
  networkSettings: () => invoke<NetworkView>("network_settings"),
  browserSettings: () => invoke<BrowserView>("browser_settings"),
  browserEnable: (enabled: boolean) => invoke<BrowserView>("browser_enable", { enabled }),
  /** AW Server, the dedicated server on this computer, in a process of its own. */
  serverLaunch: () => invoke<void>("server_launch"),
  networkEnable: (enabled: boolean) => invoke<NetworkView>("network_enable", { enabled }),
  networkInternet: (enabled: boolean, stun: string[]) => invoke<NetworkView>("network_internet", { enabled, stun }),
  networkPort: (port: number | null) => invoke<NetworkView>("network_port", { port }),
  networkServers: (servers: string[]) => invoke<NetworkView>("network_servers", { servers }),
  /** What a server holds and who uses it — for its admin; with an order, carried out first. */
  serverManage: (server: string, order: ServerOrder | null = null) => invoke<ServerState>("server_manage", { server, order }),
  networkShare: (world: string, shared: boolean) => invoke<NetworkView>("network_share", { world, shared }),
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
    return { joined: r.joined.map(toWorld), updated: r.updated, moved: r.moved ?? [] };
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
  ) => {
    return once(`${world}\u0000${articleType}\u0000${slug}`, () =>
      invoke<string>("article_create", { world, articleType, slug, name, visibility, layers }),
    );
  },
  /**
   * A new document, by a player: made with the type's access, where the
   * type lets this player make its documents. Once, as `articleCreate`.
   * Resolves with its id.
   */
  articlePropose: (world: string, articleType: string, slug: string, name: string, layers: NewLayer[]) =>
    once(`${world}\u0000${articleType}\u0000${slug}`, () => invoke<string>("article_propose", { world, articleType, slug, name, layers })),
  /** Texts as they are written: the ids stored in them turned back into names. */
  textsWritten: (world: string, articleType: string, texts: string[]) =>
    invoke<string[]>("texts_written", { world, articleType, texts }),
  /**
   * Changes who may see a part. Leaving `layer` out means: the whole
   * article.
   *
   * Rust decides who may. There is deliberately no check here — one in
   * the frontend could be bypassed.
   */
  /** A type's access for documents of it already there — and their sections that had theirs. How many changed. */
  typeAccessApply: (world: string, articleType: string, articles: string[], view: string | null, edit: string | null) =>
    invoke<number>("type_access_apply", { world, articleType, articles, view, edit }),
  visibilitySet: (world: string, article: string, layer: string | null, policy: string) =>
    invoke<void>("visibility_set", { world, article, layer, policy }),

  /**
   * Deletes an article for good.
   *
   * `confirmation` has to be the article's name — Rust checks that.
   */
  /** What lies in the trash, newest first — the GM's. */
  trashList: async (world: string): Promise<TrashedView[]> =>
    (await invoke<Raw[]>("trash_list", { world })).map((r) => ({ id: r.id, name: r.name, articleType: r.article_type, at: r.at, by: r.by })),
  trashRestore: (world: string, article: string) => invoke<void>("trash_restore", { world, article }),
  /** Deletes for good what lies in the trash. */
  trashPurge: (world: string, article: string) => invoke<void>("trash_purge", { world, article }),
  articleDelete: (world: string, article: string, confirmation: string) =>
    invoke<void>("article_delete", { world, article, confirmation }),
  /** How a type's section looks in this one document — what differs from the type is kept. */
  blockLookSet: (world: string, article: string, block: string, look: BlockLook) =>
    invoke<void>("block_look_set", { world, article, block, look }),
  /** The order of a document's sections, as far as one sees them — the others keep their place. */
  blockOrderSet: (world: string, article: string, keys: string[]) => invoke<void>("block_order_set", { world, article, keys }),
  /** Makes a document one of another type — `parents`: what it then belongs to, where that type has a parent. */
  articleTypeSet: (world: string, article: string, articleType: string, parents: string[]) =>
    invoke<void>("article_type_set", { world, article, articleType, parents }),

  /** A part of the world's index: each document without its text — see `WorldReader`. */
  viewIndex: async (world: string, previewAs: string[] | null, after: string): Promise<IndexPart> => {
    const r = await invoke<Raw>("view_index", { world, previewAs, after });
    return { epoch: r.epoch, writes: r.writes, reader: r.reader, next: r.next ?? null, articles: (r.articles as Raw[]).map(toArticle) };
  },
  /** Whole documents by id, as many as fit; what did not fit is `rest`. */
  viewTexts: async (world: string, previewAs: string[] | null, ids: string[]): Promise<TextsPart> => {
    const r = await invoke<Raw>("view_texts", { world, previewAs, ids });
    return { articles: (r.articles as Raw[]).map(toArticle), gone: r.gone ?? [], rest: r.rest ?? [] };
  },
  /** What changed after a write — or `whole`: the index anew. */
  viewSince: async (world: string, previewAs: string[] | null, epoch: number, since: number): Promise<SincePart> => {
    const r = await invoke<Raw>("view_since", { world, previewAs, epoch, since });
    return { epoch: r.epoch, writes: r.writes, reader: r.reader, whole: !!r.whole, articles: ((r.articles as Raw[]) ?? []).map(toArticle), gone: r.gone ?? [], changed: r.changed ?? [] };
  },
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
  /** Shows the window as a phone, or as it was — for layout checks in tests. */
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
  /**
   * Sets what `article` says about `with` in a relation property — and the
   * other side too, as far as this reader may. `null` removes it.
   */
  /** Sets relations — several as one change. `roles: null` removes one. */
  relationsSet: (
    world: string,
    changes: { article: string; property: string; with: string; roles: [string, string] | null; was?: [string, string] }[],
  ) =>
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
  /** Rolls for everyone in the session. */
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
  worldListedSet: async (world: string, listed: boolean) => toWorld(await invoke<Raw>("world_listed_set", { world, listed })),
  /** The GM's: which players support the world. */
  worldSupportersSet: async (world: string, ids: string[]) => toWorld(await invoke<Raw>("world_supporters_set", { world, ids })),
  /** A supporter takes the GM's place — everyone sees it. */
  worldTakeOver: async (world: string) => toWorld(await invoke<Raw>("world_take_over", { world })),
  /** The dice the world rolls with — the GM's. Empty: the usual set. */
  worldDiceSet: async (world: string, sides: number[]) => toWorld(await invoke<Raw>("world_dice_set", { world, sides })),
  /** Writes a package — types, or documents with their types and images — into a folder, as plain text and files. */
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
  /** Takes a version back, as part of the editing session `into`. */
  versionRevert: (world: string, article: string, session: string, into: string) =>
    invoke<number>("version_revert", { world, article, session, into }),
  /** Splits Markdown into the tree the display builds elements from. */
  markdownRead: (text: string) => invoke<Node[]>("markdown_read", { text }),
};
