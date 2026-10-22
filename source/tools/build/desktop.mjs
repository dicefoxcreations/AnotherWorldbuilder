// Same entry point on developer machines and CI; no campaign data is bundled.
import { spawnSync } from "node:child_process";
import path from "node:path";
import { root, cargoOutput, rustflags } from "./paths.mjs";

function run(script, args, cwd) {
  const result = spawnSync(process.execPath, [path.join(root, script), ...args], {
    cwd: path.join(root, cwd), stdio: "inherit",
    // linuxdeploy's bundled binutils may predate modern RELR relocations.
    // Do not rewrite already built system libraries while packaging them.
    env: { ...process.env, RUSTFLAGS: rustflags, CARGO_TARGET_DIR: cargoOutput, APPIMAGE_EXTRACT_AND_RUN: process.env.APPIMAGE_EXTRACT_AND_RUN ?? "1", NO_STRIP: process.env.NO_STRIP ?? "1" },
  });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
run("apps/desktop/node_modules/typescript/bin/tsc", ["-p", "tsconfig.json"], "apps/desktop");
// The third-party notices go into the app — see tools/licenses/notices.mjs.
run("tools/licenses/notices.mjs", [], ".");
run("apps/desktop/node_modules/vite/bin/vite.js", ["build"], "apps/desktop");
run("apps/desktop/node_modules/@tauri-apps/cli/tauri.js", ["build", "--ci", ...process.argv.slice(2)], "apps/desktop");
