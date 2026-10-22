import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdir, readdir, readFile, copyFile, writeFile, access, mkdtemp, rename, rm } from "node:fs/promises";
import path from "node:path";
import { root, native, cargoOutput, programs, templates } from "./paths.mjs";

const target = process.argv[2];
const portable = process.argv.includes("--portable");
const hostOutput = process.argv.includes("--host-output");
const expected = {
  "x86_64-unknown-linux-gnu": [".deb", ".AppImage"],
  "x86_64-pc-windows-msvc": [".exe"],
}[target];
if (!expected) throw new Error("Pass the release target triple");
if (portable && target !== "x86_64-pc-windows-msvc") throw new Error("Portable collection currently supports the Windows executable");
const config = JSON.parse(await readFile(path.join(native, "tauri.conf.json"), "utf8"));
const version = config.version;
if (typeof version !== "string" || !/^\d+\.\d+\.\d+(?:-[\w.-]+)?(?:\+[\w.-]+)?$/.test(version)) throw new Error("Expected a semantic application version in tauri.conf.json");
const platform = target === "x86_64-unknown-linux-gnu" ? "linux-x86_64" : "windows-x86_64";
const release = path.join(cargoOutput, ...(hostOutput ? [] : [target]), "release");
const input = path.join(release, "bundle");
// Only the newest: older versions live in the history of the repository.
const output = path.join(programs, platform);
// Only this version's packages — older builds stay behind in the bundle folder.
const isPackage = (name) => name.includes(`_${version}_`) && expected.some(ext => name.endsWith(ext)) && (target !== "x86_64-pc-windows-msvc" || name.endsWith("-setup.exe"));
async function files(folder, wanted = isPackage) {
  const found = [];
  for (const entry of await readdir(folder, { withFileTypes: true })) {
    const full = path.join(folder, entry.name);
    if (entry.isDirectory()) found.push(...await files(full, wanted));
    else if (wanted(entry.name)) found.push(full);
  }
  return found;
}
const bundles = portable ? [path.join(release, "aw-desktop.exe")] : await files(input);
for (const extension of expected) {
  if (!bundles.some(file => file.endsWith(extension))) throw new Error(`Missing package: ${extension}`);
}
const manifest = [];
// Probe tools before creating output; staging prevents incomplete final folders.
const rust = execFileSync("rustc", ["--version"], { encoding: "utf8" }).trim();
await mkdir(path.dirname(output), { recursive: true });
const stagingRoot = path.join(root, "build", "packaging");
await mkdir(stagingRoot, { recursive: true });
const staging = await mkdtemp(path.join(stagingRoot, platform + ".building-"));
for (const file of bundles.sort()) {
  const bytes = await readFile(file);
  const name = portable ? "AnotherWorldbuilder.exe" : path.basename(file);
  await copyFile(file, path.join(staging, name));
  manifest.push({ name, bytes: bytes.length, sha256: createHash("sha256").update(bytes).digest("hex") });
}
// What the app is built from, with its licenses — beside every package
// too, not only inside the app (tools/licenses/notices.mjs).
const notices = path.join(root, "build", "public", "licenses.txt");
try { await access(notices); } catch { throw new Error("Missing build/public/licenses.txt — build with tools/build/desktop.mjs"); }
await copyFile(notices, path.join(staging, "THIRD-PARTY-NOTICES.txt"));
// The templates are published beside the programs (../templates), never
// with content: what a library holds stays in its owner's worlds.
for (const file of await files(templates, (name) => name === "package.json")) {
  const pack = JSON.parse(await readFile(file, "utf8"));
  if (pack.images.length > 0 || pack.documents.some((d) => d.parts.length > 0)) throw new Error(`A template with content: ${file}`);
}
// An AppImage carries libraries of the system it was built on: those too.
const appImage = bundles.find((f) => f.endsWith(".AppImage"));
if (appImage) execFileSync(process.execPath, [path.join(root, "tools", "licenses", "appimage.mjs"), appImage, path.join(staging, "APPIMAGE-LIBRARIES.txt")], { stdio: "inherit" });
let commit = "uncommitted";
try { commit = execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim(); } catch {}
await writeFile(path.join(staging, "SHA256SUMS.txt"), manifest.map(f => `${f.sha256}  ${f.name}\n`).join(""));
await writeFile(path.join(staging, "build-info.json"), JSON.stringify({ version, target, portable, buildHost: process.platform, commit, rust, node: process.version, packages: manifest }, null, 2) + "\n");
await rm(output, { recursive: true, force: true });
await rename(staging, output);
console.log(output);
