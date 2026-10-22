# Finding and joining worlds on the LAN

As of September 2026. In the normal case this replaces exchanging ID and world files. The files remain as the way without a network (Options → Players → "Without network", or "Open a world file" in the main menu). What is exchanged is described in [The journal of a world](../architecture/journal.md).

## Flow

1. **Switch on the network** (both sides): main menu → gear → *Network* → main switch *Local network*. The default is off; off means: nothing is announced, nothing answered, nothing searched.
2. **Offer a world** (GM): below the main switch a switch appears for each of one's own GM worlds. Only worlds switched on explicitly are announced – with name, icon and GM name, nothing else.
3. **Find** (player): below one's own worlds the main menu shows the section *On the network* with all offered worlds.
4. **Join**: a click on the world asks the GM.
   - If the ID is already a member or was entered in advance by the GM, the world comes at once and opens.
   - Otherwise: *Ask to join* → "Asked — waiting for the GM". The app then asks again every few seconds; as soon as the GM accepts, the world comes by itself.
5. **Accept** (GM): the gear carries a red number with the open requests, in the main menu across all worlds, in a world for that world. Options → *Players* (also with a number) → *Asking to join* → *Accept* / *Decline*.
6. **In advance by ID** (alternative): the player copies the ID (Profile → *Copy id*) and sends it to the GM, who enters it under *Add by id*. Whoever comes with this ID is admitted without a request. What counts is the full ID (`id:` + 16 hex characters); the short fingerprint is only for reading aloud and comparing.

While the network is on, the player's app syncs with the GM every two seconds – in **both directions**. First only a digest (a hash over the names of all entries the player may have); only if it or the own journal changed do both exchange the missing entries. What the GM writes arrives at the player, what the player writes at the GM – also in the world open right now. Nothing is overwritten: what was written at the same time merges (see [journal](../architecture/journal.md)).

**Who is there:** at the top of the bar stand all others who are in the world right now – on the LAN as well as [directly over the internet](internet-direct.md) – each as a circle with initials in their colour (the same on every device, from a palette of colours far apart). A ring means: writing right now. The tooltip names the page, a click goes there (`world_presence`, every two seconds). **Writing together:** if two are in the same document in edit mode, each sees the other's cursor in their colour, with initials. Saving (0.15 s) and syncing (0.4 s) then happen faster. 🕘 *Versions* opens the document's versions: a list of sessions; a click places that version in the document beside the current state – in read mode as a page, while editing as code or visually (removed red, added green); undo; ⚠ on overlaps. What was written offline and only arrives at the next meeting is reported in the document as a note.

## Testing with two profiles on one computer

```sh
npm --prefix apps/desktop run build
cargo build -p aw-desktop --bin aw-desktop
mkdir -p /tmp/aw-join/gm /tmp/aw-join/player
```

```sh
AW_PROFILE_ROOT=/tmp/aw-join/gm ./build/cargo/debug/aw-desktop
```

```sh
AW_PROFILE_ROOT=/tmp/aw-join/player ./build/cargo/debug/aw-desktop
```

In both windows take the suggested storage location (with `AW_PROFILE_ROOT` that is this folder), create an identity, create a world in the GM window, then go through the flow above. For two computers the normal application without `AW_PROFILE_ROOT` is enough. mDNS needs multicast on the LAN; guest networks, client isolation and firewalls can prevent finding.

## Firewall

No port needs to be opened. Firewalls such as ufw or the Windows firewall do not let connections from outside in, but they do let mDNS in, and the answers to what the computer itself sent. That is why the player **knocks** before connecting: it announces via mDNS where it is and which host it wants. The host then connects in the player's direction itself. So the player's packets count as answers for the host's firewall, and the join gets through (hole punching as in online games, only with mDNS instead of a broker server). The host reacts only if it offers a world at all, and to at most four knockers at once. The knock contains only an endpoint id, address and port, no identity.

Proof without a second computer, in two network namespaces with a ufw-like firewall on both sides (without root):

```sh
cargo build -p aw-sync --example world_probe
tools/testbench/firewall-lan.sh host     # the request arrives: "player: Requested"
tools/testbench/firewall-lan.sh closed   # the host ignores the knock: "does not answer"
```

`closed` proves that the firewall really blocks; without a firewall the same host answers with `NotOffered`.

## Security

- Off is the default, and each world has to be switched on by itself.
- The announcement is only a hint. Whoever asks proves their identity through a challenge: the signature is bound to the ALPN, the world, both QUIC endpoints and a nonce. The ID must match the keys sent along.
- Exchange happens only with members, and only what they may have: GM-only sections never leave the GM's devices, a grant goes only to its member.
- Every entry is checked on every device: structure against the world token, contents against author and write key. A new world arises only from entries signed by the key of the world asked for.
- The host accepts large requests (entries) only after the identity proof; before it at most 16 KB are allowed.

## Limits

- Only within the same LAN, no internet, no relay. Players sync only with the GM's computer, not with each other; if the GM is not there, everyone stays at their state and keeps writing – at the next meeting it merges. Across distance and without the GM, a [shared folder](folder-sync.md) with the [direct connection](internet-direct.md) helps.
- Live means here: about half a second of delay, not letter by letter in real time.
- A declined join request is not reported to the player.
- Operating it in two real desktop windows has not been signed off yet. Checked automatically are the protocol and getting through the firewall over real QUIC/mDNS (`crates/aw-sync`), the journal (`crates/aw-store/src/journal`) and the flow request → acceptance → world arrives → player writes → GM sees it → simultaneous writing merges → cursor stays at the word (`apps/desktop/src-tauri/src/network.rs`).
