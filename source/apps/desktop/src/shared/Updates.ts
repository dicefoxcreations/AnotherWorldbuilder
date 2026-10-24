import { useEffect, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { invoke } from "@tauri-apps/api/core";
import { isMainWindow } from "../api/changes";

/**
 * Updates from the releases: looked for once at the start where this device
 * says so (off until switched on), or when asked under Options → Updates.
 * Only what the publisher's key signed installs (`plugins.updater.pubkey`).
 */
export type UpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "current" }
  | { kind: "ready"; version: string }
  | { kind: "loading"; version: string; done: number; total: number }
  | { kind: "failed"; error: string };

const KEY = "aw:updates";
let state: UpdateState = { kind: "idle" };
let found: Update | null = null;
const listeners = new Set<(s: UpdateState) => void>();

function set(next: UpdateState) {
  state = next;
  for (const l of listeners) l(state);
}

/** Whether this device looks for updates at the start. */
export function lookingAtStart(): boolean {
  try {
    return localStorage.getItem(KEY) === "on";
  } catch {
    return false;
  }
}

export function setLookingAtStart(on: boolean) {
  try {
    localStorage.setItem(KEY, on ? "on" : "off");
  } catch {
    // Not kept: as before at the next start.
  }
}

export async function lookForUpdate() {
  if (state.kind === "checking" || state.kind === "loading") return;
  set({ kind: "checking" });
  try {
    found = await check();
    set(found ? { kind: "ready", version: found.version } : { kind: "current" });
  } catch (e) {
    set({ kind: "failed", error: String(e) });
  }
}

/** Downloads, installs and starts the new version. */
export async function installUpdate() {
  if (!found) return;
  const version = found.version;
  let done = 0;
  let total = 0;
  set({ kind: "loading", version, done, total });
  try {
    await found.downloadAndInstall((event) => {
      if (event.event === "Started") total = event.data.contentLength ?? 0;
      if (event.event === "Progress") done += event.data.chunkLength;
      set({ kind: "loading", version, done, total });
    });
    await invoke("app_restart");
  } catch (e) {
    set({ kind: "failed", error: String(e) });
  }
}

export function useUpdates(): UpdateState {
  const [now, setNow] = useState(state);
  useEffect(() => {
    listeners.add(setNow);
    return () => {
      listeners.delete(setNow);
    };
  }, []);
  return now;
}

let started = false;
/** Once per start, in the main window, in a build that was released. */
export function lookAtStart() {
  if (started || !isMainWindow || import.meta.env.DEV || !lookingAtStart()) return;
  started = true;
  void lookForUpdate();
}
