# Directly over the internet

As of October 2026. Code: `crates/aw-sync/src/world.rs` (`Reach`, `reachable`, `add_remote`, `punch_to`, STUN), `crates/aw-sync/src/admission.rs` (`PeerNote`), `apps/desktop/src-tauri/src/network.rs` (`meet_through_folder`, `on_the_internet`, `folder_notes`).

Players and GM talk to each other directly, without a server of the author's and without a relay — with the same QUIC connection as on the [LAN](lan-join.md). The group's shared folder only serves for arranging: who can be reached where stands there. If the direct connection does not work, changes come with the folder's files, as fast as the cloud app brings them.

## Switching on

Since 0.1.45 *Options → Network* has two separate switches (`NetworkSettings.internet` and `.folders`):

- **From the internet** – this device can be reached from outside (router port, STUN, IPv6). With it a fixed port (7447 unless set otherwise, for a forwarding on the router; if another program holds it, any free one) and a fixed endpoint (`profile/endpoint.key`); from these comes the **host code** `<endpoint>@<address:port>,…`, which players enter under *Servers and hosts* like a server code. The device then answers `Ask::Worlds` with the worlds it offers and is their host ([host mode](host.md)).
- **Peer to peer through shared folders** – devices of a world in a shared folder meet there and connect directly (what follows). The addresses in the note are the outside ones if *From the internet* is on, and always the home network ones.

Peer to peer needs the second switch on both sides. The world must lie in a shared folder: an additional storage location kept in step by a cloud or sync app (see [folders](folder-sync.md)). The app writes and reads the notes in the world folder under `peers/` (`network.rs`, `folder_notes`) – in the background, so that a slow drive never holds up syncing. The status says:

- *on · reachable via router, STUN, IPv6*: how the device can be reached from outside — the router opened a port (UPnP, PCP, NAT-PMP), STUN named the outside address, or it has a global IPv6 address of its own.
- *on · not reachable from outside*: none of these (e.g. mobile networks, CGNAT). Connecting still works if the other side is reachable; otherwise the folder remains.

(The status belongs to *From the internet*.)

## Flow

1. **Becoming reachable:** at start the app asks the router to open its UDP port to the outside (UPnP, PCP, NAT-PMP; package `portmapper`). In addition it asks a STUN server how its port looks from outside — before the endpoint starts, on the same port, because most routers keep the same mapping afterwards. STUN servers can be set; the default is Cloudflare, which offers its STUN service for free use (Google's `stun.l.google.com` comes with no promise of use and is therefore not in the list). Empty means: the router only. A STUN server sees the IP address, nothing else.
   The app also listens on IPv6 and names its own global IPv6 address (`2000::/3`, found without sending a packet). There is no NAT there — that helps above all behind a provider's NAT (CGNAT, DS-Lite), where IPv6 is nearly always present.
   To check what the router can do: `cargo run -p aw-sync --example portmap_probe`. A firewall on the computer (e.g. ufw) drops the answer to the UPnP search (multicast); PCP and NAT-PMP ask the router directly and get through.
2. **Leaving a note:** each side leaves a note in the world folder under `peers/`: world, endpoint (QUIC key, new at every start), addresses (from outside, plus its own in the home network — so members in the same network find each other even when the router does not lead its public address back inside, which most home routers do not), time — signed with its own identity (`PeerNote`). It is written anew when the address changes, otherwise every 10 minutes; one's own older notes go to the bin. Notes older than 15 minutes count as gone. A note with a wrong signature or under someone else's name is not read.
3. **Meeting:** every device asks another one for the world — like one found on the LAN, every two seconds: first the GM's device, and where that does not answer, a reachable member's (the newest note first). Only the GM's device asks nobody as long as it is reachable itself: the others come to it. Every reachable device connects every three seconds to each member with a fresh note (like the answer to a knock on the LAN); so both firewalls let the other side's packets through as an answer.
4. **Answering:** over the internet the GM's device answers for the worlds it holds that lie in a shared folder — without announcing them on the LAN — under the same rules as a server (`server::here`): members by their grant, and only the GM's device takes join requests. A player's device answers for nobody: the GM's is the one place to ask. When it cannot be reached, the players work on, and their changes meet through the folder's files. As on the LAN, only those count who prove by signature who they are. If the GM removes someone, her device writes an entry `revoked`, signed by the world, under the same key as the admission; a new admission replaces it again.
5. **Falling back after a failure:** if a device does not answer, the app does not ask it again for 20 seconds and takes the next one meanwhile — a dead address does not hold up the rest.

## Behind a provider's NAT (CGNAT)

- The router forwarding does not help (the second NAT at the provider stays closed); STUN names the address of the provider's NAT.
- Hole punching often works anyway: CGNATs are supposed to use the same port for every destination. Where they do not, it fails.
- IPv6 bypasses the NAT entirely.
- Players call; a player behind CGNAT usually reaches a reachable device without more ado. If the GM sits behind it, a reachable member carries the world.
- If nobody is reachable, syncing through the folder remains.

## Privacy

- No relay: contents go only between the devices, end-to-end encrypted (QUIC/TLS, key = endpoint).
- The addresses in `peers/` are seen by everyone with access to the folder — the group and the provider of the cloud app.
- A STUN server sees the IP address of whoever asks. Can be switched off.
- The router port is open while the app runs; answers come only after proof of identity.

## Checking

Automatic: `cargo test -p aw-sync --test world met_through_the_folder` (two endpoints without mDNS, note address and counter-connection), `cargo test -p aw-sync peer_note`, `cargo test -p aw-sync --lib stun`, `cargo test -p aw-desktop folder_notes` (notes in the shared folder, forged ones stay out, old own ones go), `cargo test -p aw-desktop carries_the_others` (a player's device answers another member, no stranger, no join request, no removed member — a re-admitted one it does).

**Measured on 2026-10-20** on one computer, GM and player each with their own profile and their own rclone mount of the same Drive folder (so the files really travel through the provider), LAN off:

- World created → visible at the player's: 2:49 min (upload, `--poll-interval=30s`, directory cache).
- Notes in `peers/`: both sides read the other's; the player asks the GM's device directly.
- A text saved by the GM arrives at the player's directly in **2.2 s**; through the files alone it took 52–127 s.
- The router (UPnP + STUN fine) does not lead its public address back inside (UDP hairpin test negative): without the home address in the note the two did not find each other.
- A player now always counts the GM as a member (before, only those with a grant).

**Still open:** a connection between two lines in different networks has not been signed off. Symmetric NATs (some mobile and company networks) allow no hole punching; there the folder remains.
