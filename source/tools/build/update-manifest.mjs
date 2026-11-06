// What the apps ask for (plugins.updater.endpoints): the newest version, and
// per platform where its package lies and its signature. Written beside the
// programs, published with the release as latest.json.
import { readdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { native, programs } from "./paths.mjs";

const RELEASES = "https://github.com/dicefoxcreations/AnotherWorldbuilder/releases/download";
const config = JSON.parse(await readFile(path.join(native, "tauri.conf.json"), "utf8"));
const version = config.version;
const notes = process.argv[2] ?? "";
const platforms = {};
for (const [platform, folder, ending] of [
  ["linux-x86_64", "linux-x86_64", ".AppImage"],
  ["windows-x86_64", "windows-x86_64", "-setup.exe"],
]) {
  const names = await readdir(path.join(programs, folder)).catch(() => []);
  const name = names.find((n) => n.includes(`_${version}_`) && n.endsWith(ending));
  if (!name || !names.includes(name + ".sig")) throw new Error(`No signed ${ending} of ${version} in programs/${folder}`);
  platforms[platform] = {
    signature: (await readFile(path.join(programs, folder, name + ".sig"), "utf8")).trim(),
    url: `${RELEASES}/v${version}/${encodeURIComponent(name)}`,
  };
}
const out = path.join(programs, "latest.json");
await writeFile(out, JSON.stringify({ version, notes, pub_date: new Date().toISOString(), platforms }, null, 2) + "\n");
console.log(out);
