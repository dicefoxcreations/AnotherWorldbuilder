// Writes build/public/licenses.txt — every third-party library the app is
// built from, with its license text — so the notices travel inside the app,
// with every copy of it. Run before the frontend is built
// (`tools/build/desktop.mjs` does).
//
// The Rust libraries come from `cargo about` (install it with
// `cargo install cargo-about --features cli`). It refuses any license not
// listed in `about.toml`: a library under another license stops the build
// until someone has looked at it.
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const desktop = path.join(root, "apps/desktop");
const out = path.join(root, "build/public/licenses.txt");

let rust;
try {
  // Into a file, not through a pipe: under PowerShell cargo-about refuses that.
  const rustFile = path.join(root, "build", "rust-notices.txt");
  fs.mkdirSync(path.dirname(rustFile), { recursive: true });
  execFileSync(
    "cargo",
    ["about", "generate", "-c", "tools/licenses/about.toml", "-m", "apps/desktop/src-tauri/Cargo.toml", "-o", rustFile, "tools/licenses/notices.hbs"],
    { cwd: root, stdio: ["ignore", "inherit", "inherit"] },
  );
  rust = fs.readFileSync(rustFile, "utf8");
} catch (e) {
  console.error("Third-party notices: `cargo about` failed — is it installed (`cargo install cargo-about --features cli`), and are all licenses accepted in tools/licenses/about.toml?");
  process.exit(1);
}

// The licenses JavaScript in the app may come under — the same as for Rust
// (tools/licenses/about.toml). Anything else stops the build.
const ACCEPTED = new Set(["MIT", "MIT-0", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "0BSD", "ISC", "Zlib", "Unlicense", "CC0-1.0", "BSL-1.0", "MPL-2.0"]);

/** Whether an SPDX expression lets us ship it: one of its `OR` choices made of accepted licenses only. */
export function acceptable(expression) {
  const plain = String(expression ?? "").replace(/[()]/g, " ");
  return plain.split(/\s+OR\s+/i).some((choice) => choice.split(/\s+AND\s+/i).every((id) => ACCEPTED.has(id.trim())));
}

// The JavaScript in the bundle: the app's dependencies and theirs.
const read = (dir) => JSON.parse(fs.readFileSync(path.join(dir, "package.json"), "utf8"));
const own = read(desktop);
const seen = new Map();
const visit = (name) => {
  if (seen.has(name)) return;
  const dir = path.join(desktop, "node_modules", name);
  if (!fs.existsSync(dir)) return;
  const pkg = read(dir);
  const file = fs.readdirSync(dir).find((f) => /^(licen[sc]e|copying|notice)/i.test(f));
  seen.set(name, { pkg, text: file ? fs.readFileSync(path.join(dir, file), "utf8").trim() : `${pkg.license} — see https://spdx.org/licenses/${pkg.license}.html` });
  for (const dep of Object.keys(pkg.dependencies ?? {})) visit(dep);
};
for (const dep of Object.keys(own.dependencies ?? {})) visit(dep);
const refused = [...seen.values()].filter(({ pkg }) => !acceptable(pkg.license));
if (refused.length) {
  for (const { pkg } of refused) console.error(`Third-party notices: ${pkg.name} ${pkg.version} is under “${pkg.license}”, not an accepted license.`);
  process.exit(1);
}

const js = [...seen.values()]
  .sort((a, b) => a.pkg.name.localeCompare(b.pkg.name))
  .map(({ pkg, text }) => `------------------------------------------------------------------------\n${pkg.name} ${pkg.version} (${pkg.license})\n\n${text}\n`)
  .join("\n");

const text = `AnotherWorldbuilder © 2026 DiceFox Creations. All rights reserved.
It is built with the third-party software listed below, each under its own
license. Their copyright notices and license texts follow.

Third-party software in AnotherWorldbuilder — JavaScript libraries
===================================================================

${js}
${rust}`;

fs.mkdirSync(path.dirname(out), { recursive: true });
fs.writeFileSync(out, text);
console.log(`Third-party notices: ${seen.size} JavaScript packages, ${(rust.match(/^ {2}- /gm) ?? []).length} Rust crates → ${path.relative(root, out)}`);
