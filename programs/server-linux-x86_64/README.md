# aw-server

The keeper of a group's worlds — on a NAS, in a container. Everyone proves who they are; it lets them in where the world says they belong and does each member's commands in their name, with their view and rights. Its admin — a GM named at the start — manages it in the app: which GMs may bring worlds, which worlds are closed, who is turned away. GMs and players add it on the start page with *Add server*.

- Build: `python3 tools/build/server-image.py` → `programs/server-linux-x86_64/` beside `source/` (static binary and a Docker image archive).
- Run: `docker load -i aw-server-<version>-docker.tar`, then `compose.yaml` with `AW_ADMIN` set.
- Setting it up, with Docker or on a Synology NAS: `docs/server-setup.md`; how it works: `docs/architecture/network.md`.

The code is the desktop app's (`apps/desktop/src-tauri`, `src/server.rs`, `src/keeper.rs`), built without its window: `cargo build -p aw-desktop --no-default-features --features server --bin aw-server`.
