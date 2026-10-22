/**
 * Several windows of the app — a document opened apart beside another —
 * share one process and one profile, but each holds its own copy of what
 * it shows. What one window changes, it tells the others; they load again.
 *
 * Every command that is not a mere reading and names a world tells of a
 * change in that world — after it went through. A window ignores what it
 * told itself. Loading again only reads, so nothing echoes back and forth.
 */
import { invoke as call } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";

/** Commands that only read — nothing to tell. */
const READS = new Set([
  "article_versions",
  "dev_tools",
  "document_open",
  "document_read",
  "gallery",
  "gallery_folders",
  "identity_status",
  "image_data",
  "join_requests",
  "legend_open",
  "markdown_read",
  "navigation_load",
  "package_export",
  "package_read",
  "network_settings",
  "network_sync",
  "network_worlds",
  "phone_view",
  "presence",
  "world_presence",
  "world_switch_progress",
  "storage_locations",
  "storage_peek",
  "storage_status",
  "template_read",
  "templates_load",
  "texts_written",
  "today_in_world",
  "version_compare",
  "version_document",
  "version_marks",
  "view",
  "waiting_versions",
  "window_open",
  "world_open",
  "world_token_check",
  "worlds_list",
]);

const EVENT = "world-changed";

/** This window's name among the app's windows. */
const me = (() => {
  try {
    return (window as unknown as { __TAURI_INTERNALS__?: { metadata?: { currentWindow?: { label?: string } } } }).__TAURI_INTERNALS__?.metadata?.currentWindow?.label ?? "main";
  } catch {
    return "main";
  }
})();

export const isMainWindow = me === "main";

const pending = new Map<string, ReturnType<typeof setTimeout>>();

/** Tells the other windows that a world changed — once for a burst, as one types. */
export function announce(world: string) {
  clearTimeout(pending.get(world));
  pending.set(
    world,
    setTimeout(() => {
      pending.delete(world);
      emit(EVENT, { world, from: me }).catch(() => {});
    }, 250),
  );
}

/**
 * Worlds worked on online: their host holds them, and does each command in
 * this member's name. Only what concerns this device stays here.
 */
const online = new Set<string>();
const HERE = new Set(["dice_log", "dice_log_add", "dice_log_clear", "dice_favorites_set", "window_open", "world_delete", "network_join"]);

export function setOnline(worlds: string[]) {
  online.clear();
  for (const w of worlds) online.add(w);
}

/** A command of the app; what changes a world is told to the other windows. */
export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const world = args?.world;
  const result =
    typeof world === "string" && online.has(world) && !HERE.has(cmd)
      ? await call<T>("remote_call", { world, command: cmd, args: args ?? {} })
      : await call<T>(cmd, args);
  if (!READS.has(cmd) && typeof args?.world === "string") announce(args.world);
  return result;
}

/** Calls `onChange` when another window changed `world`. Returns how to stop. */
export function onWorldChanged(world: string, onChange: () => void): () => void {
  let stop: (() => void) | null = null;
  let gone = false;
  listen<{ world: string; from: string }>(EVENT, (e) => {
    if (e.payload.from !== me && e.payload.world === world) onChange();
  })
    .then((un) => (gone ? un() : (stop = un)))
    .catch(() => {});
  return () => {
    gone = true;
    stop?.();
  };
}
