import { type DocumentText } from "../../api/client";

/** A trace of the editor's saves and merges, for tests: on with `localStorage.awTrace = "1"`. */
export function trace(what: string, data: unknown) {
  try {
    if (localStorage.getItem("awTrace") !== "1") return;
    const all = ((window as unknown as { __awTrace?: unknown[] }).__awTrace ??= []);
    all.push([Date.now() % 100000, what, data]);
    if (all.length > 400) all.shift();
  } catch {
    // No trace — nothing else changes.
  }
}

/** The parts' texts, one after the other — for the trace. */
export const flat = (d: DocumentText) => d.blocks.flatMap((b) => b.parts.map((p) => p.text)).join("|");
