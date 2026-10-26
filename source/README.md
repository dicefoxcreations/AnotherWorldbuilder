# AnotherWorldbuilder

A worldbuilding and campaign wiki for tabletop role-playing games that works locally.
The application is a Tauri desktop app with a React interface and shared Rust
libraries for the data model, storage and the network.

## Running the application

Finished programs lie beside this folder in **`../programs/<platform>/`**, always the
current version: [Linux](../programs/linux-x86_64/) (AppImage and DEB),
[Windows](../programs/windows-x86_64/) (portable EXE), [Server](../programs/server-linux-x86_64/).
On GitHub the program files themselves are attached to the
[releases](../../../releases/latest). Templates lie beside them in [`../templates/`](../templates/).

The Linux build was made on CachyOS; older distributions are not covered yet.
The Windows EXE needs WebView2. Details are in the [build guide](docs/development/builds.md).

Where the data lives, the app asks on first start ([storage locations](docs/architecture/storage.md)):
simply one folder with `profile/` and `worlds/` in it, or both apart.

## Project layout

```text
apps/desktop/             The desktop application
  src/                    React/TypeScript, organised by feature
  src-tauri/              Rust backend, Tauri configuration and icons
  tests/unit/             Frontend tests without a browser
  tests/browser/          Browser test bench with the real UI components
apps/server/              The server's README, Dockerfile and compose file
crates/                   Shared Rust libraries
tools/build/              Local build and release scripts
tools/testbench/          Rust server of the browser test bench
docs/                     Architecture, development and testing
build/                    Generated files and compiler caches
```

The detailed [folder overview](docs/development/project-layout.md) explains
the boundaries.

## Developing and building

Needed are Rust 1.91 or later, Node 24 or later and the
[Tauri system dependencies](docs/development/builds.md).
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

## Checking

```sh
cargo test --locked --workspace
npm --prefix apps/desktop test
apps/desktop/node_modules/.bin/tsc -p apps/desktop/tsconfig.json
```

The two LAN integration tests need local UDP and multicast sockets.
Interactive UI flows are described in the [browser test bench](docs/testing/browser-testbench.md).

## Current scope

Local identities, worlds, documents and types, a [world calendar with a timeline](docs/architecture/calendar-timeline.md),
sections with separate read and write rights, automatic saving with versions
and review ([world format 5](docs/architecture/world-format.md)), writing
together with visible cursors, and [several storage locations](docs/architecture/storage.md).
A world lies in one place — this device, a [server](docs/server-setup.md) or the GM's
app offering it on the network ([the network](docs/architecture/network.md)) —
or, without any server, with the GM and [mailboxes](docs/architecture/mailboxes.md)
in a cloud folder. Mobile is not built yet.

[Documentation overview](docs/README.md)
