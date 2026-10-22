// Lists the system libraries an AppImage carries, with their licenses —
// `node tools/licenses/appimage.mjs <AppImage> <out.txt>`.
//
// An AppImage brings the libraries of the machine it was built on (WebKitGTK,
// GTK, GStreamer, …), most of them under the LGPL. Whoever passes them on
// must say so, include their license texts and offer their source. This
// writes what the first two need, from the package manager of the build
// machine (pacman); the source stays to be offered — see
// docs/development/licenses.md.
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const [appImage, out] = process.argv.slice(2);
if (!appImage || !out) throw new Error("usage: appimage.mjs <AppImage> <out.txt>");
const work = fs.mkdtempSync(path.join(os.tmpdir(), "aw-appimage-"));
execFileSync(path.resolve(appImage), ["--appimage-extract"], { cwd: work, stdio: "ignore" });

const libs = [];
const walk = (dir) => {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, e.name);
    if (e.isDirectory()) walk(full);
    else if (e.isFile() && /\.so(\.|$)/.test(e.name)) libs.push(e.name);
  }
};
walk(path.join(work, "squashfs-root"));
fs.rmSync(work, { recursive: true, force: true });

const run = (cmd, args) => {
  try {
    return execFileSync(cmd, args, { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"], env: { ...process.env, LANG: "C", LC_ALL: "C" } }).trim();
  } catch {
    return "";
  }
};
const havePacman = !!run("pacman", ["--version"]);
const packages = new Map();
const unknown = [];
for (const lib of [...new Set(libs)].sort()) {
  // Where the system keeps it — its own folder first, not a copy some program carries.
  const direct = path.join("/usr/lib", lib);
  const where = !havePacman ? "" : fs.existsSync(direct) ? direct : run("find", ["/usr/lib", "-name", lib, "-print", "-quit"]);
  const pkg = where ? run("pacman", ["-Qoq", where]) : "";
  if (!pkg) {
    unknown.push(lib);
    continue;
  }
  if (!packages.has(pkg)) packages.set(pkg, []);
  packages.get(pkg).push(lib);
}

let text = `System libraries in the AnotherWorldbuilder AppImage
=====================================================

The AppImage carries these libraries of the system it was built on, each
under its own license, linked dynamically: they may be replaced. Their source
code, in exactly these versions, is that of the packages named below
(https://archlinux.org/packages/ and the upstream projects).
`;
for (const [pkg, files] of [...packages].sort(([a], [b]) => a.localeCompare(b))) {
  const info = run("pacman", ["-Qi", pkg]);
  const field = (name) => new RegExp(`^${name}\\s*:\\s*(.*)$`, "m").exec(info)?.[1]?.trim() ?? "";
  text += `\n------------------------------------------------------------------------\n${pkg} ${field("Version")} — ${field("Licenses")}\n${field("URL")}\nFiles: ${files.join(", ")}\n`;
  const dir = path.join("/usr/share/licenses", pkg);
  if (fs.existsSync(dir)) {
    for (const f of fs.readdirSync(dir)) {
      const full = path.join(dir, f);
      if (fs.statSync(full).isFile()) text += `\n${fs.readFileSync(full, "utf8").trim()}\n`;
    }
  }
  // Common licenses (LGPL, GPL, …) by name, from the system's SPDX texts.
  for (const id of new Set(field("Licenses").split(/[\s()]+/).filter((w) => /^[A-Za-z0-9.+-]+$/.test(w) && !/^(AND|OR|WITH)$/.test(w)))) {
    const spdx = path.join("/usr/share/licenses/spdx", `${id}.txt`);
    if (fs.existsSync(spdx) && !text.includes(`\n[${id}]\n`)) text += `\n[${id}]\n${fs.readFileSync(spdx, "utf8").trim()}\n`;
  }
}
// What the package manager does not own, but is known by its name.
const KNOWN = { "libintl.so.8": ["GNU gettext runtime", "LGPL-2.1-or-later", "https://www.gnu.org/software/gettext/"] };
for (const lib of unknown.filter((l) => KNOWN[l])) {
  const [name, id, url] = KNOWN[lib];
  text += `\n------------------------------------------------------------------------\n${name} — ${id}\n${url}\nFiles: ${lib}\n`;
  const spdx = path.join("/usr/share/licenses/spdx", `${id}.txt`);
  if (fs.existsSync(spdx) && !text.includes(`\n[${id}]\n`)) text += `\n[${id}]\n${fs.readFileSync(spdx, "utf8").trim()}\n`;
}
const left = unknown.filter((l) => !KNOWN[l]);
if (left.length) text += `\n------------------------------------------------------------------------\nNot found in the package manager: ${left.join(", ")}\n`;
fs.writeFileSync(out, text);
console.log(`AppImage libraries: ${packages.size} packages${left.length ? `, ${left.length} files unknown` : ""} → ${out}`);
if (left.length) process.exit(1);
