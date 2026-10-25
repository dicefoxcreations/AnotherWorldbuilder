# Server: aw-server on a NAS

As of October 2026. Code: `apps/desktop/src-tauri/src/server.rs` (start), `keeper.rs` (door keeper `Keeper`, management — the same one the app answers with, `keeper::here`), `src/bin/aw-server.rs`; protocol `crates/aw-sync/src/world.rs` (`Ask::Manage`, `Order`, `ServerState`); build `tools/build/server-image.py`; files for Docker in `apps/server/`.

`aw-server` is the door keeper of the worlds: always on, so that players can work even when the GM's device is off. It holds **no keys** of a world and is not the GM. Everyone identifies themselves when connecting (a signed challenge); the server lets them where the world itself says they belong:

- **The GM of a world** – whoever stands as GM in the world entry of the journal: everything, and the join requests.
- **Members** – whoever has an admission (grant) written by the GM. If someone is removed, `Revoked` replaces the admission, and the server no longer lets them in.
- **Everyone else** may ask to join; the request goes to the GM, and the decision is made in her app.

What it passes on depends on the world:

| World | What the server does |
| --- | --- |
| encrypted | the app decrypts with the member's own keys in a disposable session. No device keeps a copy of the world, the GM's neither: only identity, keys and membership metadata persist. The temporary data in `<profile>/online-sessions/` is removed when the session closes and at startup after a crash. Opening and editing require the server. |
| unencrypted | stays with the server. Players work online ([host mode](host.md)): the server reads it – like anyone who holds it – and shows and changes for each only what their view and rights say. It writes as itself, in the player's name; the GM vouches for it (`Item::Hosts`), which is why the version history names the player. |

So that the server can check views in an unencrypted world, the admissions stand open in the journal there: only the attributes, without keys (`regrant`). If the world is encrypted again, they are sealed again, with keys.

In 0.1.56, a server world lies on its server only: no device keeps a copy, and loss of connectivity blocks the open world. Copies made with *Move or copy … → Copy* are worlds of their own, with their own IDs. The server receives only encrypted journal entries and image parts for an encrypted world, never private identity keys, the world token or the GM's keyring.

Edits are staged privately (`Ask::Commit`) and become canonical only on the final commit; a usual edit is staged and committed in one ask. The app reports success after the server acknowledges it. A failed command is never queued for a later upload: the session goes on with only what the server acknowledged. Three missed polls in a row stop the open world; the next good one lets it go on. A lost final acknowledgement may mean that a save already reached the server; reconnecting reads the authoritative result. New client keys are retained before publishing ciphertext so this case cannot strand newly encrypted data. Signed structural changes check the starting revision; concurrent text changes retain the journal's merge behavior. Durable commit intents recover on server restart. Completed receipts are released by the client; abandoned uncommitted stages expire after a day on server restart.

The app downloads all journal batches and required image parts before opening a fresh session. An initial upload remains unavailable for editing until records and images are fully acknowledged. Removing a server address does not turn its worlds into local worlds. Move back to a local folder explicitly to change a world's location; the server must be reachable for that move.

Update the server and all participating apps to 0.1.56 together. Older apps keep their previous copy and fallback behavior; older servers do not support acknowledged online commands. Existing encrypted worlds and their keys remain compatible.

On upgrade, an old GM working copy is never silently uploaded. Existing server assignments read the authoritative server; only a new, explicit create/move request sets the persisted initial-upload marker. A missing server world therefore stays unavailable until deliberately restored, instead of being resurrected from a stale device copy.

Online operation cannot prevent someone deliberately copying information they can see. Explicit backups are independent of the automatic copy setting.

## Who sees a world

A world on a server is seen by its GM, its members, the ids its GM let in ahead of time (*Players → by id*) and the server's admin — nobody else. Asked about it, anyone else hears that it is not there (`Reply::NotOffered`): it is neither named nor can it be asked for. The GM's switch *Options → World → Where it lives → Seen by everyone on the server* (`Item::Listing`, signed by the GM, off by default) names it to everyone and lets anyone ask to join. The ids let in ahead of time stand in the same record — ids only — so the server can tell them apart; a request from one of them is taken in by the GM's device at once. At most 50 requests wait per world.

The network side: a connection is accepted only from an address that proved itself (QUIC retry), one address holds at most eight connections at once, and past 20 asks a second (bursts of 60) a connection is slowed down. A flood of traffic that fills the line itself is beyond any program: that is for the router or the provider.

## Roles

| Role | For | May |
| --- | --- | --- |
| Admin (one or more) | the server | manage it, name admins and worldbuilders, bring worlds, let players into any world on it; supporter of every world on it |
| Worldbuilder | the server | bring worlds |
| GM | one world | everything in it; let players in; name supporters; hand the world over |
| Supporter | one world | let players in; take the GM's place when needed |
| Player | one world | what their view and rights say |

Whoever brings or creates a world is its GM. Handing the world over (*Players → Make GM*), the GM becomes a player of it and sees no more than the others. The server holds no world's keys: letting someone in hands them keys, and only those who hold some can — the GM, and a supporter with the keys every player has (what only a character may see, the GM gives later). An admin who is not in the world lets in (*Manage → Let in*); the next device holding its keys, the GM's or a supporter's, does it without asking (`Reply::Digest.approved`).

**Supporters** (`Item::Supporters`, signed by the GM): the players the GM chose (*Players → Supporter*) and the server's admins, whose identities the server tells the GM's device (`Reply::Digest.admins`). For each, the GM's keys — world token, keyring, members — lie sealed in the world (`Item::Keys`), sealed anew when they change, taken back from whoever is no supporter any more. A supporter could read everything with them; the GM chooses them for that. *Players → Take the GM's place …* makes a supporter the GM, for everyone to see: the world names them; the old GM becomes a player, and every key they no longer need turns over. Handing the world over works the same way (`Item::Handover`): the member's device takes it over when it next reads the world.

## Admin

At start one admin is named (`AW_ADMIN`, her ID under *Profile* in the app); she can name more. Admins manage the server in the app under *Options → Network → Servers and hosts → Manage*:

- **Worlds:** every world with name, GM, size on disk and whether encrypted – no more than the server itself can read. **Close** closes a world for everyone, its GM too; **Open** opens it again; a closed world can be deleted (type its name) and stays closed, so that it is not brought straight back. Who asks to join a world stands below it, with **Let in**.
- **Admins:** the first, and those added by ID (**Remove** again; the first one stays).
- **Worldbuilders:** who may bring worlds besides the admins. If someone else brings a world, it waits ("would bring a world") until an admin says **Allow**; meanwhile their app shows "A world of yours waits for the server's admin." A worldbuilder can also be added directly by ID or removed again (their worlds stay).
- **Users:** everyone who was there, with "last seen". **Block** turns someone away, whatever they ask; not an admin.

A GM brings only her own worlds: the entries must name her as GM, otherwise the server does not take the world.

```text
<data>/profile/      the server's own identity, its endpoint key
<data>/worlds/       the worlds, as in the app
<data>/server.json   admins, worldbuilders, closed worlds, who was there, who is turned away
<data>/host-code.txt what GMs and players enter
```

## Building

```
python3 tools/build/server-image.py
```

Result beside `source/` in `programs/server-linux-x86_64/`:

- `aw-server` – a static program (musl), runs on any x86-64 Linux.
- `aw-server-<version>-docker.tar` – a Docker image (the format of `docker save`), built without Docker: nothing but the program, the license notices, `/data` and `/tmp` ("FROM scratch"). Checked with `crane validate --tarball`.
- `compose.yaml`, `Dockerfile`, `README.md`, `THIRD-PARTY-NOTICES.txt`.

Without a window: `cargo build -p aw-desktop --no-default-features --features server --bin aw-server` (feature `gui` off: no Tauri, no GTK, no WebKit; `server` on: the program is built only when asked for, so the app's packages do not carry it).

## Synology (Container Manager)

Requirement: a NAS with an x86-64 processor (the "+" models; Container Manager only runs there).

1. **Look up the ID:** in the app under *Profile*, your own ID (`id:` and 16 characters).
2. **Create a folder:** in File Station, `docker/aw-server`.
3. **Load the image:** *Container Manager → Registry* → search `anotherworldbuilder` → `dicefoxcreations/anotherworldbuilder-server`, tag `latest`. Without internet: *Image → Add → Add from file* → `aw-server-<version>-docker.tar` (it is then called `aw-server:latest`).
4. **Create the container** – as a *Project* with `compose.yaml` (adjust ID and address) or by hand:
   - Image `dicefoxcreations/anotherworldbuilder-server:latest`, *Enable auto-restart*.
   - Network: **host** (recommended; then the address in the code is right by itself, and `AW_LAN=1` makes it findable in the home network). With "bridge": map the ports `7447/udp` and `7447/tcp` (for the address) to the same numbers and set `AW_ADDRESS`.
   - Volume: `docker/aw-server` → `/data`.
   - Environment: `AW_ADMIN` and `AW_NAME` stand there empty and are to be filled in – `AW_ADMIN=id:…` (the 16 characters alone work too); optionally `AW_PORT` (default 7447), `AW_ADDRESS=<NAS IP>:7447,<DynDNS name>:7447`, `AW_LAN=1`, `AW_NAME` (what the server is called), `AW_LOG=1` (writes to the log how connections go – only for finding faults).
   - **One address:** the server answers only at one address of the NAS and names no other – the first address of the NAS itself in `AW_ADDRESS`, otherwise the one of its home network. A NAS has several (a second network port, VPN, containers), and an app that tries one it cannot reach gets no answer.
5. **Start.** The log shows `aw-server on UDP port 7447 — admin id:…` and the **code**; it is also in `docker/aw-server/host-code.txt`.
6. **Firewall:** if the NAS firewall is on, allow port 7447 for **UDP and TCP**. For players from outside: forward both to the NAS on the router (outside may use another port, e.g. 4001 → 7447) and set `AW_ADDRESS` with the public name.

**Address instead of code:** on TCP at the same port the server tells everyone its code (`tell_code`, a small text page – `http://<address>:<port>` also shows it in the browser). In the app the address is therefore enough: `worlds.example.org:4001`, `192.168.0.20:7447` or just the name (then port 7447). The app fetches the code (`Server::look_up`), puts the entered address first and remembers the result. Someone sitting between app and server could slip in another code on the first fetch; whoever wants to be sure compares the code with the one in the server's log or enters the full code right away.

## In the app

- **Header of the start page:** ＋ (New world) and 📂 (Open world).
- **Start page:** *On this device* (worlds in the device's own folder and in further folders – a drive, a cloud folder), *Other devices* (worlds held with another device's GM – "Hosted by …" – and worlds found on the local network, only if there are any) and *Servers* with **＋ Add server** on the right: per server its name (the server's identity, `AW_NAME` at the first start), "admin" for the admin and **Edit** (change the address, remove the server); below it one's own worlds there and those one can ask to join (**Ask to join** → "Waiting for the GM").
- **Adding/changing a server:** address (`host:port`, only `host` = port 7447) or the full code. Also under *Options → Network → Servers and hosts* (**Edit**, **Remove**, for the admin **Manage**); there, behind every server, stands which of one's own worlds live on it.
- **Who may bring worlds:** the server tells everyone (`Reply::Worlds.may_bring`). Only such servers are listed under *Create in*. Whoever may not yet sees "Only GMs the admin lets in host worlds here." at the server and **Ask to host worlds** (`Ask::HostWorlds`) – the request appears for the admin under *GMs*. A world waiting for the admin shows "waits for its admin".
- **New world → Create in:** besides the folders, every server reached that one may bring worlds to. The world then lives there (`NetworkSettings.homes`, by the server's endpoint ID – it stays if the server's address changes) and goes only to this server, not to the local network; after the acknowledged initial upload, the GM uses a disposable online session; keys stay separately in the profile; the world's folder leaves the device.
- **Where a world lives:** where it was made, and nowhere else. *Options → World → Where it lives* shows it to everyone: *This device*, a further folder by its name, a server, or – for a player – "Hosted by …" (with "a copy lies on this device" or "worked on online"). A copy on a player's device does not count as the world's place. A world made on this device stays here: *Offered to players* offers it from this device, on the local network or the internet, and no server gets it – however many are listed.
- **Move or copy …** (the GM only): one dialog, *Move* or *Copy*, *To* any folder of *Storage*, **Another folder …** (picked, and from then on one of Storage's locations) or any server the GM may bring worlds to; the place it lies now is marked "now", a folder's path stands below. Moved into another folder, the world's folder goes there with everything – on the same drive by renaming, else copied beside it first, file by file with "120 of 4679", and only then swapped in; meanwhile the network answers "busy" for it. Moved onto a server, it lives there and leaves this device. Moved off a server – to another one or back here – the app first takes what players wrote there (not reached: it stays), then the server deletes it (`Order::Delete`, the admin's own world too, without closing it first). *Copy* makes a world of its own – here or on a server – with the types, documents and images (through a package, kept among the copy's files meanwhile), without players and versions: "1 of 3 · reading documents", "2 of 3 · packing images 219 of 4,893", "3 of 3 · writing the copy". A copy that breaks off is deleted at once; one left by an app closed meanwhile goes at the next start.
- **Bringing a world over:** the app names everything it has once (`Ask::Swap`), then gives what the server lacks in pieces of at most 4 MB (`Ask::Give`), for 20 seconds per sync round; the next round goes on where the last one stopped – after a restart of the app or the server too, as the server keeps every piece it took. Then the images follow, 24 MB per exchange. "uploading 34 %", then "uploading images 120 of 4679", stands at the world on the start page and under *Where it lives*.
- **Players:** a world that came through a server is listed under that server (remembered at the first sync).

## Connections, reading and writing

The app keeps one connection to each server and asks on it again and again, each ask a stream of its own with its own proof; the server answers up to eight asks of a connection at once and closes it after five quiet minutes.

A session reads the server every two seconds (faster while writing together): one digest when nothing changed, carrying presence and dice. When something did, it asks only for what came after its mark in the server's order of arrivals (`Ask::Since`); only a mark of another order — the server restarted — names everything it has (`Ask::Swap`). The server keeps the digest until its world changes. Pictures come a few at a time while polling: the world opens before all its images are there. A command reads the server itself only when the last read is older than 1.5 s; reading commands then run on the session alone.

## When it is busy

A server writes one piece of a world at a time. Reading never waits for a write. A write waits up to two seconds for the one before it, then answers `Reply::Busy`, and the app asks again a moment later. It lists the world meanwhile as it last knew it, so *Worlds* and *Manage* answer as usual. The pieces are written together: all files of a piece first, then one wait on the disk (`durable::write_all`), not two waits for every record – on a NAS with hard disks that is the difference between minutes and hours.

Found on the NAS, 2026-10-22: the app brought every offered world to every listed server, in pieces of 32 MB; the NAS took longer than the app's two minutes per piece, the app sent it again, and the server answered nobody any more.

## Limits

- Encrypted server sessions use the app's normal GM/player commands, including document creation, image upload and browser import. The older host RPC mode for unencrypted player access still has its narrower command set.
- The server reads an unencrypted world, so whoever runs it does too. On one's own NAS that does not matter; on someone else's server, better encrypt the world.
- x86-64 only; ARM NAS devices would need a build of their own.

## Checking

Automatic (`cargo test -p aw-desktop server::`):

- `the_admin_lets_gms_in_and_closes_and_nobody_brings_what_is_not_theirs` – the admin brings her world, a second GM waits until *Allow*; a foreign world is not accepted; only the admin manages and sees name, GM, form and size; when closed nobody gets the world, when deleted it does not come back; blocked users get nothing, the admin cannot be blocked; after a restart everything is still there.
- `an_encrypted_world_goes_to_its_members_and_without_copies_only_while_online` – joining through the server, decided by the GM; Bob opens only his part ("Rumours", not "Truth"); the legacy transport no-copy hint is honored. The new online-session regressions additionally cover both roles and never permit offline fallback.
- `a_world_comes_a_little_at_a_time_and_while_it_is_written_nobody_waits` – named once, given in pieces of two records, the world is whole on the server; while it is being written, an ask about it gets `Busy` at once, and *Worlds* and *Manage* still list it with its GM.
- `an_unencrypted_world_is_read_and_written_by_the_server_in_the_members_name` – the admission stands open and without keys; Bob sees his part through the server, writes in "Rumours", not in the name (GM only); the GM gets the sentence, the version history names Bob.

**Is a server answering?** `cargo run -p aw-sync --example server_probe -- <address or code> [times]` looks the server up as the app does and asks it for its worlds; `AW_LOG=1` shows how the connection goes. A connection that stands but gets no answer means the server itself is stuck, not the network.

**Is all of a world there?** *Options → World → Where it lives → Check* (GM, world on a server, `network::world_check`): the server names which entries it lacks (those follow with the next sync), and every image part comes back and is measured against its name; parts missing or no longer whole are given again and asked for once more. While the server writes the world (the usual sync), it answers `Busy` — the check waits and asks again instead of stopping.

**By hand, 2026-10-21**, locally with the static program (port 7450, `AW_ADMIN` = Marla) and three fresh app profiles (Marla, Oscar, Eve):

1. The server starts without keys, only with Marla's ID.
2. Marla creates "Host Test" (encrypted, without copies) and "Plain Test" (unencrypted) and enters the code → both lie on the server; her line has **Manage**.
3. Oscar brings "Thule" → it waits ("A world of yours waits for the server's admin."), the server accepts nothing. Marla sees him under *GMs* with "would bring a world", clicks **Allow** → "Thule" arrives. While "Thule" waits, Eve sees only Marla's worlds in the list.
4. Eve joins both; the requests reach Marla through the server, Marla accepts.
5. **Marla's app closed.** "Host Test" lies at Eve's in `profile/online/`, "Plain Test" not at all (online). In both she sees "Rumours", not "Truth", and writes a sentence; she may not change the document's name (GM only) ("You may not rename this document.").
6. Marla opens again → both sentences are there, the version history names "Eve" in both worlds.
7. Marla closes "Host Test" in *Manage* → at Eve's next sync the world is gone. That Eve's app fetches it again by itself after reopening only came with `fleeting.json`; this is checked through the restart (8) and the automatic test, not once more through closing and opening.
8. Eve killed hard: the copy is still there, at the next start it is deleted and fetched again online. Eve closed normally: `profile/online/` is gone.
