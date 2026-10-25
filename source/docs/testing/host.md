# Host mode: players work online

As of October 2026. Code: `apps/desktop/src-tauri/src/host.rs` (`Acting`, `acting_for`, `call`, `online_only`, `may_write`), `network.rs` (`OnlineWorld`, `remote_call`, `refresh_online`, `network_copies_set`), `crates/aw-sync/src/world.rs` (`Ask::Call`, `Reply::Called`, `Reply::OnlineOnly`), frontend `api/changes.ts` (`setOnline`, the detour in `invoke`).

A world stays with the host – the GM's device or a [server](server.md) – and the players get no copy. Their app asks the host for everything it does with the world, and the host does it in their name: with exactly what they may see and change.

## When

- **Unencrypted and offered on the network:** always. Without encryption a copy would give everyone everything; this way the host enforces views and rights.
- **Encrypted, hosted by a GM device:** by choice. *Allow offline editing* chooses whether players keep a copy (and work offline) or use host RPC. The GM device can decrypt for the player.
- **Encrypted, on a server (0.1.56):** GM and players always work through an authenticated online session. No device keeps a copy of it. Decryption remains on each member's device; the server holds no world keys (see [server](server.md)).

Explicit local worlds and copies remain separate from server worlds.

## Settings

- *Options → World → Where it lives*: **Offered to players** (from this device, over the network) and **Allow offline editing** (switchable only for encrypted worlds; off and greyed out when unencrypted); further down **Encrypted** – what applies to the world.
- *Options → Network*: how this device is reached (**Local network**, **From the internet** with a host code, **Peer to peer through shared folders**; with one of the first two on, the **Port** can be chosen – when chosen fixed, the device tells its code there on TCP like a server, and players then enter only `IP:port`), and which servers or hosts it works with (**Servers and hosts**: codes of `aw-server` and host codes of other devices).

## What the host does

- **Seeing:** it takes from its keyring exactly the keys that the player's admission (grant) names – the same decryption attempt as on the player's device, older generations included. In an unencrypted world, where nothing is decrypted, it checks the view of each section against the player's attributes (`all`, `players` and their own).
- **Changing:** when it writes in a player's name, it first checks, for each section that really changes, whether the player may: encrypted, whether their keys open the write key; unencrypted, whether the edit rule names them.
- **Signing:** the host signs what it writes; the session carries the player's id (`<id>:<session>`). The version history shows the player as the author – only if the GM signed, or a server she vouches for (`Item::Hosts`, `Journal::written_for`); for any other signature it does not believe the name in the session.
- **Presence and rolls** meet at the host as on the LAN.
- **Allowed online** (`host::call`): world info, view, types, outline, opening/reading/saving a document, ending a session, images and gallery, image fields, references, viewing versions, presence, rolls. Everything else – above all everything of the GM's – answers "Not available online."

## On the player's side

- Joining as before (LAN, internet or server). After admission the host answers the sync with `OnlineOnly`; the app remembers the world in `profile/online.json` (as the host shows it) and lists it with the others.
- Every command for this world goes to the host as `Ask::Call` (`remote_call`), except what concerns only this device (dice log, favourites, windows, taking the world off the list).
- At each sync tick the app asks whether the world has moved on at the host (`changes`) and then reloads.
- Without a network, or without a reachable host, the world is not open.

## Checking

Automatic: `cargo test -p aw-desktop the_host_shows` – encrypted and unencrypted: the player sees their part and that of the players, not the GM's; they change what they may and not what they may only see; the host itself stays GM.

**Through the host code** (2026-10-21): Marla turns on *From the internet* (port 7448), code `…@203.0.113.20:7448,192.168.0.50:7448` (STUN, home network). Tim, local network off, enters only the code → Marla's worlds in the list → joins "Plain Test" (unencrypted) → admitted → Tim reads it online.

**Measured on 2026-10-21** with two fresh profiles on one computer (GM "Marla", player "Tim", LAN on): world "Host Test" (encrypted, *Allow offline editing* off) with "Rumours" (players see and write) and "Truth" (GM only). Tim finds the world and asks, Marla admits him → Tim is an online member, his world folder stays empty. Tim's interface shows "Rumours", "Truth" is missing ("1 of 2 closed"). Tim types a sentence in the editor → it appears in Marla's document.
