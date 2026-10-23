# aw-server

The door keeper of a group's worlds — on a NAS, in a container. It holds no world's keys: everyone proves who they are, and it lets them in where the world says they belong. An encrypted world it passes on as it is, for the members' own keys; an unencrypted one it hosts online, with each member's view and rights. Its admin — a GM named at the start — manages it in the app: which GMs may bring worlds, which worlds are closed, who is turned away. GMs and players enter its code under *Options → Network → Servers and hosts*.

- Build: `python3 tools/build/server-image.py` → `programs/server-linux-x86_64/` beside `source/` (static binary and a Docker image archive).
- Run: `docker load -i aw-server-<version>-docker.tar`, then `compose.yaml` with `AW_ADMIN` set.
- Setting it up, with Docker or on a Synology NAS: `docs/server-setup.md`; how it works: `docs/testing/server.md`.

The code is the desktop app's (`apps/desktop/src-tauri`, `src/server.rs`), built without its window: `cargo build -p aw-desktop --no-default-features --bin aw-server`.
