// Signs a package for the updater with the publisher's key from ~/.tauri —
// for what was built elsewhere (the Windows installer of the workflow):
// the key never leaves this machine. Writes <file>.sig beside it.
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { desktop } from "./paths.mjs";

const file = process.argv[2];
if (!file || !fs.existsSync(file)) throw new Error("Pass the package to sign");
const key = path.join(os.homedir(), ".tauri", "anotherworldbuilder.key");
const password = key.replace(/key$/, "password");
const result = spawnSync(process.execPath, [path.join(desktop, "node_modules", "@tauri-apps", "cli", "tauri.js"), "signer", "sign", file], {
  stdio: ["ignore", "ignore", "inherit"],
  env: {
    ...process.env,
    TAURI_SIGNING_PRIVATE_KEY: fs.readFileSync(key, "utf8").trim(),
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD: fs.existsSync(password) ? fs.readFileSync(password, "utf8").replace(/\r?\n$/, "") : "",
  },
});
if (result.status !== 0 || !fs.existsSync(file + ".sig")) process.exit(result.status || 1);
console.log(file + ".sig");
