# Licenses

As of 14 October 2026. Not legal advice but a technical inventory; before selling, a lawyer specialised in IT law should check it.

## The project's own license

The project has no open-source license: `LICENSE` at the top of the repository reserves all rights (as of 23 October 2026). The notices in the app say the same. The third-party libraries below allow selling the app as proprietary software (no GPL in the program itself).

## What is inside the app

**JavaScript (in the frontend bundle):** react, react-dom, scheduler (MIT), three (MIT), @tauri-apps/api and @tauri-apps/plugin-dialog (MIT or Apache-2.0).

**Rust (linked into the program):** about 520 packages for Linux and Windows, among them Tauri, automerge, iroh, tokio, reqwest/rustls, ring, image, pulldown-cmark, the dalek cryptography and blake3. Nearly all are under MIT, Apache-2.0, BSD, ISC, Zlib or Unlicense. In particular:

- **The server's log (`AW_LOG`):** `tracing-subscriber` (with `matchers`, `sharded-slab`, `nu-ansi-term`, `tracing-log`), MIT; `regex-automata`, `thread_local`, MIT or Apache-2.0.
- **Passphrase for key backups:** `argon2` (with `blake2`, `password-hash`, `base64ct`), MIT or Apache-2.0.
- **MPL-2.0:** `cssparser`, `cssparser-macros`, `selectors`, `dtoa-short`, `option-ext` (through Tauri). Weak copyleft at file level: used unchanged, a note on where their source code can be found is enough (crates.io, stated in the notices).
- **Unicode-3.0:** the ICU packages (Unicode data), permissive with a notice requirement.
- **CDLA-Permissive-2.0:** `webpki-roots` (root certificates), permissive with a notice requirement.
- **r-efi** is MIT, Apache-2.0 or LGPL at choice; MIT/Apache is chosen.

**Sound:** the dice sound (`features/dice/dice-roll.mp3`) is a throw from “2 dice roll (29 throws)” by RPG on OpenGameArt (https://opengameart.org/content/2-dice-roll-29-throws), CC0 — no notice required; filtered, compressed and cut to 1.5 s.

## Notice requirement

MIT, Apache, BSD and the others require copyright notices and license texts to come with every copy. For that, `tools/licenses/notices.mjs` generates `licenses.txt` on every build (through `cargo about` for Rust, from `node_modules` for JavaScript). The file is inside the app itself and can be read under *Options → About → Third-party licenses*; so it also travels with an EXE passed on by itself.

`tools/licenses/about.toml` lists the allowed licenses for Rust, `ACCEPTED` in `notices.mjs` the same for JavaScript. If a library with another license comes in, the build stops until someone has looked at it. Only permissive licenses and MPL-2.0 are allowed, no GPL or LGPL in the program. Needed: `cargo install cargo-about --features cli`. The project's own packages are `publish = false` and do not count as third parties.

**Rule for new libraries:** check the license before adding one; rather build something small yourself than pull in a large dependency. The check in the build catches mistakes but does not replace looking beforehand.

**Where the notices are:**

| Package | Notices |
| --- | --- |
| All | in the app under *Options → About → Third-party licenses* |
| All | `THIRD-PARTY-NOTICES.txt` beside the package in the programs folder (`collect-release.mjs`) |
| DEB | also `/usr/share/doc/anotherworldbuilder/THIRD-PARTY-NOTICES.txt` |
| AppImage | GStreamer for sound with a chosen few plugins only, all LGPL or MIT (`SOUND` in `tools/build/desktop.mjs`) — not every plugin of the build machine, some are GPL. Also `APPIMAGE-LIBRARIES.txt`: every bundled system library with package, version, license and license text (`tools/licenses/appimage.mjs`, through pacman). A library that cannot be assigned to a package stops the collecting. |
| Windows | The Rust notices cover the Windows target too (`targets` in `about.toml`). Tauri links Microsoft's WebView2 loader statically; its SDK license allows passing it on as part of an app. |
| aw-server | The same Rust packages as the app without Tauri, GTK and WebKit, linked statically against musl (musl: MIT). `THIRD-PARTY-NOTICES.txt` lies in the programs folder beside the program and the image, and in the image under `/usr/share/doc/aw-server/`. |

## Open points for a release

- **AppImage:** it bundles about 140 system libraries of the build machine (98 packages): WebKitGTK, GTK, GLib, GStreamer, GnuTLS, libsoup, librsvg and more, mostly **LGPL**, some packages contain GPL parts too. Licenses and texts are now in `APPIMAGE-LIBRARIES.txt`. What remains open is the duty to offer the source code of exactly these versions (enclose it, provide it for download yourself, or a written offer). The libraries can be swapped, as they are linked dynamically. Simpler: publish as a **Flatpak** with the GNOME runtime (the runtime provides the libraries, not us) or as a **DEB**, which only requires the system libraries.
- **Windows:** the WebView2 runtime belongs to Windows and is not shipped. If the offline installer is enclosed later, Microsoft's redistribution terms apply.
- **Own license:** settle it (above), add `LICENSE`.
- **Content:** the app contains no third-party pictures, fonts or icons (system font, the system's emoji, own map symbols and dice). Pictures of a world are brought by the user.
- **Name and trademark:** before selling, check whether "AnotherWorldbuilder" is free.
