# Building and releasing

Every version is built for **Linux, Windows and Android**; the server's Docker image too when the server changed. Each package carries **AW Server** — the dedicated server, the host of `aw-server` — beside the app: `--server` on Linux and Windows (tray, menu entry, *Options → Network → Open AW Server*), a second icon on Android. The pipeline needs neither Git nor GitHub to build; releasing needs both.

## Every release

From `source/`, with the changes committed:

```sh
python3 tools/build/release.py build [--server]   # tests, Linux, Android (+ Docker image)
git add ../programs && git commit && git push
python3 tools/build/release.py windows             # installer of HEAD via the workflow, signed here
git add ../programs && git commit && git push
python3 tools/build/release.py publish "What is new" [--server]
```

`publish` writes `latest.json`, creates the GitHub release `v<version>` with every package (the template and example world come over from the release before; without `--server` the server files too) and pushes the image as `<version>` and `latest`. Keys stay outside the project: `~/.tauri/anotherworldbuilder.key` (updates), `~/.tauri/android/anotherworldbuilder.jks` (Android) — both belong in the publisher's password manager; the Docker Hub login is the publisher's own (`crane auth login`).

## Files there now

- Linux: `programs/linux-x86_64/AnotherWorldbuilder_<version>_amd64.AppImage`, beside it a DEB package.
- Windows: `programs/windows-x86_64/AnotherWorldbuilder_<version>_x64-setup.exe`, the installer (for the current user; it updates itself, see *Updates*).
- Android: `programs/android/AnotherWorldbuilder_<version>_android.apk` — see [`apps/desktop/ANDROID.md`](../../apps/desktop/ANDROID.md).
- Server: `programs/server-linux-x86_64/aw-server` and its Docker image.
- In each folder: `SHA256SUMS.txt`, `build-info.json`, `THIRD-PARTY-NOTICES.txt`.

The Linux files are built on CachyOS: the program needs glibc 2.39 — **not yet the broadly compatible build on Ubuntu 22.04**. The Windows installer is built by the workflow on Windows and needs no separate Visual C++ runtime.

## Package formats

| Target | Regular build environment | Output |
| --- | --- | --- |
| Linux x86-64 | For broad distribution: Ubuntu 22.04 as an older build base | `.AppImage`, `.deb` |
| Windows x86-64 | Windows 10/11 with the MSVC Build Tools | NSIS installer `.exe` |

Intel/AMD computers with a 64-bit system are the first target. ARM64 is another architecture and later a build of its own. The age or graphics power of a computer does not decide binary compatibility.

A Linux AppImage does not lower the glibc version used for compiling afterwards. For broad releases, build on the oldest supported base, at first Ubuntu 22.04 with glibc 2.35. A local CachyOS build works on that system but is no proof of compatibility with older distributions. See [Tauri Linux packaging](https://v2.tauri.app/distribute/appimage/).

The Windows installer installs for the current user. Where WebView2 is missing (rare: Windows 11 has it, Windows 10 through Edge) it downloads Microsoft's small bootstrapper — not the 215 MB offline installer, which every update would carry too. See [Tauri Windows packaging](https://v2.tauri.app/distribute/windows-installer/).

## One-time setup of the build machine

Only the build machine needs Rust (at least 1.91; checked here with 1.98.1), Node 24 or later and the Tauri system dependencies. On Windows the MSVC Build Tools with the C++ and Windows SDK components are required. Later users do not need to install these tools.

For the older Linux build base Ubuntu 22.04:

```sh
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential libssl-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf libfuse2 file
```

On Arch/CachyOS, AW Server's tray symbol needs `libayatana-appindicator` (`sudo pacman -S libayatana-appindicator`); the AppImage takes it along.

For Android: Java 17, the Android SDK 36 with NDK 28c (by default in `build/android-sdk`) and `rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android`.

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

*Options → Updates*: *Look now* asks `latest.json` of the newest GitHub release (`plugins.updater.endpoints`); *Look for updates at the start* (off until switched on, per device) asks once at each start, in released builds. A version found marks the gear and *Updates* with ↑; *Install and restart* loads it, checks its signature, replaces the program and starts it anew (`app_restart`: an AppImage is started without the old one's open files, or its old mount would stay). Only packages signed with the publisher's key install; its public half is `plugins.updater.pubkey` in `tauri.conf.json`.

What updates itself: the AppImage on Linux and the installed Windows app (NSIS installer, `-setup.exe`, for the current user). The DEB and a loose EXE do not; their users download the new version.

The private key lies outside the project, in `~/.tauri/anotherworldbuilder.key`, its password in `~/.tauri/anotherworldbuilder.password`; both belong in the publisher's password manager too — without the key no update can be published any more. `tools/build/desktop.mjs` signs with it when it is there (`TAURI_SIGNING_PRIVATE_KEY`), otherwise it builds without update files. The collector takes the `.sig` files along, and

```sh
node tools/build/update-manifest.mjs "What is new"
```

writes `programs/latest.json` from both platforms' signed packages. It goes into the release with the packages, which must keep their names.

The Windows installer is built on Windows, by the workflow `.github/workflows/windows-installer.yml` (started by hand with a tag or commit) — unsigned. It is signed here, where the key is:

```sh
gh workflow run windows-installer.yml -f ref=v0.1.57
gh run download <run> -n windows-installer -D build/cargo/x86_64-pc-windows-msvc/release/bundle/nsis
node tools/build/sign-update.mjs build/cargo/x86_64-pc-windows-msvc/release/bundle/nsis/AnotherWorldbuilder_0.1.57_x64-setup.exe
node tools/build/collect-release.mjs x86_64-pc-windows-msvc
```

(The NSIS of Arch's AUR makes 64-bit installers only, and Tauri's installer needs its 32-bit plugin.)

AppImage packaging extracts its build tools without a FUSE mount. Stripping the system libraries afterwards is switched off, because the bundled older binutils do not understand current RELR relocations. These settings are part of the shared build script; they change no campaign data.

The C runtime of the portable Windows build made here is linked statically. For private builds that are not signed yet, Windows may show a publisher/SmartScreen prompt. Uploading is `release.py publish` alone.

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

`apps/desktop/src-tauri/icons/icon.png` (512 px), `icon.ico` (16–256 px) and `apps/desktop/src/assets/logo.png` (256 px, *About*) are the finished end frame of the DiceFox artwork, revision 12: the fox holding a D20 with a world inside. The loader (`shared/ui/DiceFox.tsx`, `FoxLoader.tsx`) is the same artwork in layers (`apps/desktop/src/assets/dicefox/`): its world builds in phases — earth, grass, mountains, waterfall, trees, clouds, castle — with the real progress from 0 to 1, or over and over while that is not known (`null`); reduced motion shows the finished world. Visual check: `tests/browser/dicefox.html` in the test bench.

Their source is the publisher's DiceFox asset library, kept outside this repository. The app holds unchanged copies; `assets/dicefox/provenance.json` records the source revision and checksums. A new revision is exported from the library and copied in — nothing here links to the library, so the app builds without it. Own artwork, no third-party license.
