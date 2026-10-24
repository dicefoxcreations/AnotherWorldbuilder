// Same entry point on developer machines and CI; no campaign data is bundled.
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { root, cargoOutput, rustflags } from "./paths.mjs";

// The AppImage carries GStreamer for sound — only these plugins, all LGPL:
// a source, a decoder for the dice and an output. Not every plugin the build
// machine has (some are GPL).
const SOUND = ["coreelements", "app", "gio", "playback", "typefindfunctions", "audioconvert", "audioresample", "volume", "autodetect", "pulseaudio", "pipewire", "audioparsers", "id3demux", "mpg123", "wavparse", "interleave", "audiomixer"];
const systemPlugins = "/usr/lib/gstreamer-1.0";
const plugins = path.join(cargoOutput, "..", "gstreamer-plugins");
let gstreamer = {};
if (process.platform === "linux" && fs.existsSync(systemPlugins)) {
  fs.rmSync(plugins, { recursive: true, force: true });
  fs.mkdirSync(plugins, { recursive: true });
  for (const name of SOUND) fs.copyFileSync(path.join(systemPlugins, `libgst${name}.so`), path.join(plugins, `libgst${name}.so`));
  gstreamer = { GSTREAMER_PLUGINS_DIR: plugins, GSTREAMER_HELPERS_DIR: systemPlugins };
}

// Updates are signed with the publisher's key, kept outside the project
// (~/.tauri). Without it the build makes no update files.
const keyFile = path.join(os.homedir(), ".tauri", "anotherworldbuilder.key");
const signing = fs.existsSync(keyFile)
  ? {
      TAURI_SIGNING_PRIVATE_KEY: keyFile,
      TAURI_SIGNING_PRIVATE_KEY_PASSWORD: fs.existsSync(keyFile.replace(/key$/, "password")) ? fs.readFileSync(keyFile.replace(/key$/, "password"), "utf8").replace(/\r?\n$/, "") : "",
    }
  : {};
const unsigned = fs.existsSync(keyFile) ? [] : ["--config", JSON.stringify({ bundle: { createUpdaterArtifacts: false } })];

function run(script, args, cwd) {
  const result = spawnSync(process.execPath, [path.join(root, script), ...args], {
    cwd: path.join(root, cwd), stdio: "inherit",
    // linuxdeploy's bundled binutils may predate modern RELR relocations.
    // Do not rewrite already built system libraries while packaging them.
    env: { ...process.env, ...gstreamer, ...signing, RUSTFLAGS: rustflags, CARGO_TARGET_DIR: cargoOutput, APPIMAGE_EXTRACT_AND_RUN: process.env.APPIMAGE_EXTRACT_AND_RUN ?? "1", NO_STRIP: process.env.NO_STRIP ?? "1" },
  });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
run("apps/desktop/node_modules/typescript/bin/tsc", ["-p", "tsconfig.json"], "apps/desktop");
// The third-party notices go into the app — see tools/licenses/notices.mjs.
run("tools/licenses/notices.mjs", [], ".");
run("apps/desktop/node_modules/vite/bin/vite.js", ["build"], "apps/desktop");
run("apps/desktop/node_modules/@tauri-apps/cli/tauri.js", ["build", "--ci", ...unsigned, ...process.argv.slice(2)], "apps/desktop");
