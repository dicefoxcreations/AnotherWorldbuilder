# Local desktop build pipeline

The pipeline needs **neither Git nor GitHub**. Configuration, lockfiles and build scripts are in the project and can run on a local build machine. They produce the regular application without identities, campaigns or prepared test profiles. On first start the existing setup flow applies.

## Files there now

- Linux: `programs/linux-x86_64/AnotherWorldbuilder_<version>_amd64.AppImage` (about 129 MiB), beside it a DEB package.
- Windows: `programs/windows-x86_64/AnotherWorldbuilder.exe` (about 34 MiB). This is the portable application, not an installer. WebView2 must already be present.
- In both folders: `SHA256SUMS.txt` and `build-info.json`.

The Linux files were built locally on CachyOS. The main program alone already needs glibc 2.39; the bundled libraries can have newer requirements. They are therefore **not yet the broadly compatible release built on Ubuntu 22.04**. On the build machine itself, a short start test ran without an error message. The Windows file compiles successfully for x86-64/MSVC and, according to its import table, needs no separate Visual C++ runtime.

The package check found only the application and its runtime and desktop resources, no profiles, campaigns or prototype helpers.

To start, make the AppImage executable in the file properties if needed and double-click it. On Windows copy the `.exe` to the computer and start it. No Windows installer is produced yet; its configuration is there for the regular native build.

## Package formats

| Target | Regular build environment | Output |
| --- | --- | --- |
| Linux x86-64 | For broad distribution: Ubuntu 22.04 as an older build base | `.AppImage`, `.deb` |
| Windows x86-64 | Windows 10/11 with the MSVC Build Tools | NSIS installer `.exe` |

Intel/AMD computers with a 64-bit system are the first target. ARM64 is another architecture and later a build of its own. The age or graphics power of a computer does not decide binary compatibility.

A Linux AppImage does not lower the glibc version used for compiling afterwards. For broad releases, build on the oldest supported base, at first Ubuntu 22.04 with glibc 2.35. A local CachyOS build works on that system but is no proof of compatibility with older distributions. See [Tauri Linux packaging](https://v2.tauri.app/distribute/appimage/).

The Windows installer installs for the current user and contains the WebView2 offline installer if needed. The portable Windows program needs an already installed WebView2 runtime. See [Tauri Windows packaging](https://v2.tauri.app/distribute/windows-installer/).

## One-time setup of the build machine

Only the build machine needs Rust (at least 1.91; checked here with 1.98.1), Node 24 or later and the Tauri system dependencies. On Windows the MSVC Build Tools with the C++ and Windows SDK components are required. Later users do not need to install these tools.

For the older Linux build base Ubuntu 22.04:

```sh
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential libssl-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf libfuse2 file
```

For the license notices in the app, the build needs `cargo-about` (see [licenses](licenses.md)):

```sh
cargo install cargo-about --features cli
```

In the project folder, install the web dependencies from the lockfile:

```sh
npm ci --prefix apps/desktop
```

## Building

All commands run in `source/`. The same call on Linux and Windows:

```sh
node tools/build/desktop.mjs -- --locked
```

The script checks TypeScript, builds the interface and calls the local Tauri CLI pinned in the npm lockfile. The platform files `tauri.linux.conf.json` and `tauri.windows.conf.json` choose the package formats. `custom-protocol` makes the app start offline with the embedded interface. The output lies under `build/cargo/release/bundle/`.

The script also remaps paths for the compiler (`--remap-path-prefix`): the built programs name `/aw` for the source and `/build` for the home folder, not the folders of the build machine. `tools/build/server-image.py` does the same for `aw-server`.

For separate target folders:

```sh
# On the Linux build machine:
node tools/build/desktop.mjs --target x86_64-unknown-linux-gnu -- --locked
node tools/build/collect-release.mjs x86_64-unknown-linux-gnu
```

```powershell
# On the Windows build machine:
$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = '-C target-feature=+crt-static'
node tools/build/desktop.mjs --target x86_64-pc-windows-msvc -- --locked
node tools/build/collect-release.mjs x86_64-pc-windows-msvc
```

The collector takes nothing but the finished package files and adds SHA-256 checksums and toolchain information. If a Git commit exists, it is noted only as extra information; Git is optional. The platform folder is only replaced once everything is collected; old and new packages never mix.

The finished output lies beside `source/` under `programs/<platform>/`, that is
`programs/linux-x86_64/` or `programs/windows-x86_64/`, always only the current version.
The version comes from `apps/desktop/src-tauri/tauri.conf.json`. Incomplete collections
lie under `build/packaging/`.

Without an explicit target folder, add `--host-output` to the collector. For an already compiled portable Windows EXE the call is `node tools/build/collect-release.mjs x86_64-pc-windows-msvc --portable`.

## Updates

The app looks for a newer version once at the start (main window, released builds only; *Options → About → Look for updates at the start* switches it off) and on *Look now*. It asks `latest.json` of the newest GitHub release (`plugins.updater.endpoints`). A ⬆ in the header bar installs it and restarts. Only packages signed with the publisher's key install; its public half is `plugins.updater.pubkey` in `tauri.conf.json`.

What updates itself: the AppImage on Linux and the installed Windows app (NSIS installer, `-setup.exe`, for the current user). The DEB and a loose EXE do not; their users download the new version.

The private key lies outside the project, in `~/.tauri/anotherworldbuilder.key`, its password in `~/.tauri/anotherworldbuilder.password`; both belong in the publisher's password manager too — without the key no update can be published any more. `tools/build/desktop.mjs` signs with it when it is there (`TAURI_SIGNING_PRIVATE_KEY`), otherwise it builds without update files. The collector takes the `.sig` files along, and

```sh
node tools/build/update-manifest.mjs "What is new"
```

writes `programs/latest.json` from both platforms' signed packages. It goes into the release with the packages, which must keep their names.

The Windows installer is built on Linux too, with NSIS (`makensis`, from the AUR on Arch):

```sh
node tools/build/desktop.mjs --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis -- --locked
node tools/build/collect-release.mjs x86_64-pc-windows-msvc
```

AppImage packaging extracts its build tools without a FUSE mount. Stripping the system libraries afterwards is switched off, because the bundled older binutils do not understand current RELR relocations. These settings are part of the shared build script; they change no campaign data.

The C runtime of the portable Windows build made here is linked statically. For private builds that are not signed yet, Windows may show a publisher/SmartScreen prompt. Code signing and public release are separate release steps; this pipeline uploads nothing.

## Checking

```sh
cargo test --locked --workspace
npm --prefix apps/desktop test
```

The two real LAN tests need UDP/multicast sockets and a suitable firewall profile. For a build machine without network test permission:

```sh
cargo test --locked --workspace --exclude aw-sync
cargo test --locked -p aw-sync --lib
```

The online server regression also has a real encrypted loopback transport test (requires
UDP socket permission, no real profiles or server):

```sh
cargo test --locked -p aw-desktop --lib encrypted_online_commands_use_a_real_server_and_fail_closed_after_disconnect -- --ignored
```

The ordinary server tests cover no-copy GM/player sessions, forbidden local fallbacks,
chunked admission, acknowledged saves, lost connections and acknowledgements, signed
transaction validation, conflict handling and recovery after a server restart. Frontend
command routing is covered by `apps/desktop/tests/unit/online.test.mjs`.

This does not replace the real two-computer acceptance. Getting through the firewall is checked by `tools/testbench/firewall-lan.sh` in two network namespaces (see [LAN test](../testing/lan-join.md)).

## Building Windows from Linux

For the local Windows build, `cargo-xwin` with the Rust target `x86_64-pc-windows-msvc` and LLVM was used in addition. This is the cross-compile route documented by Tauri:

```sh
node tools/build/desktop.mjs --runner cargo-xwin --target x86_64-pc-windows-msvc --no-bundle -- --locked
node tools/build/collect-release.mjs x86_64-pc-windows-msvc --portable
```

For the cross build, **without** `+crt-static`: Tauri already links the VC runtime statically and the UCRT dynamically (part of Windows 10/11). Together with `+crt-static`, the maths functions (`round`, `floor` …) are missing at link time. The regular build scripts can still run natively on Windows; they do not depend on this local helper installation.

A dependency conflict found in the Windows build was resolved in the lockfile: `wmi` must use the same 0.62 version of `windows` and `windows-core`. Independent 0.61 dependencies of Tauri/Wry remain. Future dependency updates must therefore be compiled for Windows again too.

## What the packages contain

Normal packages contain the application without test data and without profiles. Network and syncing are built in but off until switched on in the settings.

Beside `programs/` lies `templates/`: templates to start a world with (Options → Transfer → Import). A template contains only types, short value lists (sizes, spell schools …) and the outline, no content. `templates/D&D 5e.awpack` is made with `python3 tools/import/dnd_template.py` (writes `../templates/D&D 5e.awpack`; the test bench must be running). Templates are not packed into the programs; `collect-release.mjs` stops as soon as a template contains images or sections with text.

`Example-world.zip` (a release file) is *The Gullwind Coast*, a small made-up world to show what the app does: `python3 tools/import/example_world.py` writes `build/packaging/Example world.awpack` with its map, drawn from noise (needs Pillow and NumPy, and the test bench running). The screenshots in `docs/screenshots/` are of it.

Content from books or online libraries (one's own import scripts, not part of the project) is meant only for the own worlds of the person who owns the library. It is never shipped with the app and never published.

## App icon

`apps/desktop/src-tauri/icons/icon.png` (512 px) and `icon.ico` (16–256 px) show the DiceFox "dice world", version 5: the fox from the own DiceFox stock holds a d20 without numbers with a landscape inside it; paw and landscape made with image generation, put together with `build_icon.py`. `apps/desktop/src/assets/logo.png` (256 px) is the same picture for the *About* page. Own artwork, no third-party license.
