import { fileURLToPath } from "node:url";
import os from "node:os";
import path from "node:path";

export const root = fileURLToPath(new URL("../../", import.meta.url));
export const desktop = path.join(root, "apps", "desktop");
export const native = path.join(desktop, "src-tauri");
export const cargoOutput = path.resolve(root, process.env.CARGO_TARGET_DIR ?? "build/cargo");
// Beside the source: the finished programs, and the templates shipped beside them.
export const programs = path.join(root, "..", "programs");
export const templates = path.join(root, "..", "templates");
// Programs name no folder of the machine they were built on: the source and
// everything under home (cargo's registry, rustup) appear as /aw and /build.
export const rustflags = [process.env.RUSTFLAGS, `--remap-path-prefix=${os.homedir()}=/build`, `--remap-path-prefix=${path.resolve(root)}=/aw`]
  .filter(Boolean)
  .join(" ");
