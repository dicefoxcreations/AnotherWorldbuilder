# AnotherWorldbuilder

A worldbuilding and campaign wiki for tabletop role-playing games that works locally.
The application is a Tauri app — Linux, Windows, Android — with a React interface and
shared Rust libraries for the data model, storage and the network. Each package carries
AW Server, the dedicated server, beside the app.

## Running the application

Finished programs lie beside this folder in **`../programs/<platform>/`**, always the
current version: [Linux](../programs/linux-x86_64/) (AppImage and DEB),
[Windows](../programs/windows-x86_64/) (installer), [Android](../programs/android/) (APK),
[Server](../programs/server-linux-x86_64/) (without a window, and its Docker image).
On GitHub the program files themselves are attached to the
[releases](../../../releases/latest). Templates lie beside them in [`../templates/`](../templates/).

The Linux build was made on CachyOS; older distributions are not covered yet.
Windows needs WebView2 (the installer fetches it where missing).

Where the data lives, the app asks on first start: simply one folder with
`profile/` and `worlds/` in it, or both apart.

## Project layout

```text
apps/desktop/             The application (desktop and Android)
  src/                    React/TypeScript, organised by feature
  src-tauri/              Rust backend, Tauri configuration and icons
    gen/android/          The Android project, with AW Server's own screen
apps/server/              The server's README, Dockerfile and compose file
crates/                   Shared Rust libraries
tools/build/              Local build and release scripts
tools/import/             The template's types, written as a package
tools/licenses/           The third-party notices that ship with the app
build/                    Generated files and compiler caches
```

## Developing and building

Needed are Rust 1.91 or later, Node 24 or later and the
[Tauri system dependencies](https://v2.tauri.app/start/prerequisites/).
All of the following commands run in `source/`:

```sh
npm ci --prefix apps/desktop
npm --prefix apps/desktop run app         # build and start the local interface
npm --prefix apps/desktop run app:dev     # development with hot reload
```

Building the regular release packages:

```sh
node tools/build/desktop.mjs -- --locked
```

The build lands in `build/cargo/release/bundle/`. A separate step copies finished
packages with checksums to `../programs/`, for example on Linux:

```sh
node tools/build/collect-release.mjs x86_64-unknown-linux-gnu --host-output
```

The platform folder is only replaced once everything is collected. The version number
comes from `apps/desktop/src-tauri/tauri.conf.json`. Neither a Git repository nor a
cloud service is needed for these steps.

## Current scope

Local identities, worlds, documents and types, a world calendar with a timeline,
sections with separate read and write rights, automatic saving with versions
and review, writing together with visible cursors, and several storage locations.
A world lies in one place — this device, a [server](docs/server-setup.md) or the GM's
app offering it on the network — or, without any server, with the GM and mailboxes
in a cloud folder.
