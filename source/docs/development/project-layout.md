# Project layout

The folders separate product code, development tools, generated intermediate files
and finished packages for distribution. A change of folders changes neither the
world format nor identities or the user's storage locations.

The repository is the release repository: three folders at the top.

```text
AnotherWorldbuilder/
├── source/                          The source code (everything below lies in here)
├── programs/                        Finished programs, only the current version
│   ├── linux-x86_64/               AppImage, DEB, checksums, license notices
│   ├── windows-x86_64/             Portable EXE, checksums
│   └── server-linux-x86_64/        aw-server and its Docker image
├── templates/                       Templates beside the programs (types only, no content)
└── testdata/                        Test profiles and test worlds (not in Git)
```

In Git, `programs/` keeps only checksums, notices and the server's compose file; the
program files themselves are attached to the GitHub releases.

Inside `source/`:

```text
source/
├── Cargo.toml, Cargo.lock           Rust workspace and locked dependencies
├── .cargo/config.toml              Compiler output: build/cargo
├── apps/
│   ├── desktop/                    One coherent desktop project
│   │   ├── package.json, package-lock.json
│   │   ├── vite.config.ts, tsconfig.json, index.html
│   │   ├── src/
│   │   │   ├── main.tsx            Entry of the interface
│   │   │   ├── app/                Main view, menu and header
│   │   │   ├── api/                Typed Tauri call contract
│   │   │   ├── features/
│   │   │   │   ├── wiki/           Articles, editor, display and properties
│   │   │   │   ├── worlds/         Campaign context and navigation
│   │   │   │   ├── templates/      Templates, data types and name patterns
│   │   │   │   ├── settings/       Settings
│   │   │   │   └── collaboration/  Editor queue and React binding
│   │   │   ├── shared/             Shared type and reference logic
│   │   │   │   ├── editor/         Reusable text input helpers
│   │   │   │   └── ui/             Reusable controls
│   │   │   └── styles/             Global styles
│   │   ├── src-tauri/
│   │   │   ├── Cargo.toml, build.rs, tauri*.conf.json
│   │   │   ├── src/                Rust backend (also aw-server)
│   │   │   ├── capabilities/       Tauri permissions
│   │   │   └── icons/              Application icons
│   │   └── tests/
│   │       ├── unit/               Tests without a browser
│   │       └── browser/            Test bench, scenarios and browser driver
│   └── server/                     The server's README, Dockerfile and compose file
├── crates/
│   ├── aw-crypto/                  Identities, ids, the passphrase lock
│   ├── aw-model/                   Types, documents as text, rules, calendar, parser
│   ├── aw-store/                   The world's database, merging, images, durable files
│   └── aw-sync/                    Transport: QUIC, mDNS, router ports, STUN, messages
├── tools/
│   ├── build/                     Shared build call and release collection
│   ├── import/                    Package writer and the D&D 5e template
│   ├── licenses/                  Third-party notices
│   └── testbench/                 Rust HTTP server for the UI test bench
├── docs/
│   ├── architecture/              Designs of the parts
│   ├── development/               Building and project organisation
│   └── testing/                   Test runs and limits
├── build/                         Generated, not for distribution
│   ├── cargo/                     Rust builds, dependencies and bundle working data
│   ├── frontend/                  Embedded Vite interface
│   ├── testbench/                 Built browser test bench
│   ├── packaging/                 Release intermediates and incomplete collections
│   └── cache/                     Further local tool caches
```

## Rules for where things go

- A product feature has its UI code under `src/features/`. Shared controls and
  helpers belong under `src/shared/`, the IPC boundary under `src/api/`.
- Platform-independent Rust logic stays in `crates/`. Rust tests stay with their
  library, so that `cargo test -p …` keeps working.
- `src-tauri` is the normal Tauri project convention. Frontend and backend thereby
  share one application folder but stay clearly apart.
- `tools/` holds programs for developers. The browser test bench uses the same
  React components; its Rust server is not a campaign server to ship.
- `build/` holds generated intermediate files. Tauri caches hold absolute paths;
  after moving the project they are generated anew.
- `programs/` (beside `source/`) holds nothing but finished output, per platform
  only the current version. `collect-release.mjs` replaces a platform folder only
  after a successful collection.
- `templates/` (beside `source/`) is published beside the programs, not packed into
  them. `collect-release.mjs` stops as soon as a template contains content.
- `node_modules/` stays where npm expects it, under `apps/desktop/`.
  Tauri still generates its schemas itself in `src-tauri/gen/`. These two
  tool-specific folders are generated and excluded.
- Private campaigns do not belong in the repository. Test profiles (`AW_PROFILE_ROOT`)
  lie only in the explicitly chosen place outside of it.
- Built programs name no folder of the build machine: the build scripts remap the
  source to `/aw` and the home folder to `/build` (`rustflags` in `tools/build/paths.mjs`).
