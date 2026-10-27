# The network: worlds worked in where they lie

As of October 2026. Code: `crates/aw-sync/src/world.rs` (transport and
messages), `apps/desktop/src-tauri/src/keeper.rs` (the side that holds
worlds), `remote.rs` (the commands done for members), `network.rs` (the
app's side: settings, finding, joining, forwarding, moving), `server.rs`
(`aw-server`).

## One place per world

A world lies in exactly one place, and whoever holds it decides who sees
what:

| Where | Who holds it | Reached |
| --- | --- | --- |
| This device | the app itself | by this device; by the members when the GM offers it on the network |
| A server | `aw-server` on a NAS or any machine | always, by its code or address |
| Without a server | the GM's app; players get [mailboxes](mailboxes.md) | through a folder a cloud app keeps in step |

No device keeps a copy of a world it does not hold — not even the GM's, of a
world on a server. A server world that cannot be reached is closed until it
can.

## The keeper

Whatever holds worlds for others answers through the same keeper: `aw-server`
for every world it holds, the app (`keeper::here`) for the GM's own worlds
offered on the network. A member asks for **commands of the app** — the same
ones the interface calls on this device — and the keeper does each in their
name (`store::acting_for` → `remote::call`). Every command has a `*_in(store,
…)` form for that. So the same rules decide what a member sees and may
change, wherever the world lies; nothing is checked in the interface alone.

What names a file on the member's device travels as its content: a picture
to upload (`image_put`), a player card (`member_admit`), a package from the
browser (`package_take`).

## Messages

`aw-sync` speaks QUIC (iroh) between two endpoints, without relays or
address lookups. The asking side proves who it is by signing a fresh
challenge with its identity; the holder checks the id belongs to those keys.

| Ask | Answer |
| --- | --- |
| `Worlds` | the worlds the asking side may know of; who the holder is; whether they are its admin, may bring worlds, asked to |
| `Join` | `Joined` — a member, or let in ahead of time — or `Requested` |
| `Call { command, args }` | `Called { value }` or `Called { error }`; `NotMember`; `NotOffered` |
| `Manage { order }` | the server as its admin sees it; a GM may order their own world `Delete`d |
| `Backups { plan }` | how a server backs a world up |
| `MayIBring` | asks a server's admin to let one bring worlds |
| `Put` / `Bring` | a world brought to a server, 4 MiB at a time, then taken in |
| `Files { taking }` / `Get` | a world fetched by its GM, 4 MiB at a time — taken away, or backed up |

A world moved from a GM's app onto a server leaves a note there: whoever asks
for it gets `Moved { server }`, and their app follows.

## Keeping up

The main window asks every few seconds (`network_sync`):

- **Open server worlds:** `world_sync` with how far the world's writes were
  last time. The holder notes who wrote what (`keeper::others_wrote`) and
  says whether *someone else* wrote since — then the page loads again. One's
  own typing does not echo back.
- **This device's offered worlds:** the same, for what members changed from
  afar.
- **Asked to join:** asked again until let in.
- **Servers:** what each holds for this device, every half minute.
- **Mailboxes:** taken in and delivered ([mailboxes](mailboxes.md)).

Presence (who is where, carets) and dice rolls live in memory where the
world is held; members ask for them with the same calls.

## Reaching each other

- **Local network:** with *Local network* on, the app announces over mDNS the
  worlds it offers — those on the list by name, the others without one, so
  their members still find the device. A device found there is asked at the
  address it has now: an app takes any free port when 7447 is taken.
- **Internet:** the app asks its router to forward its port (UPnP, PCP,
  NAT-PMP) and STUN servers how it looks from outside; its **code**
  (`<endpoint>@<address>:<port>,…`) is shown under *Network* and told on TCP
  at the same port, so its address alone is enough.
- **A server** is always reached at its code or address.

## Moving

*Move or copy* (the GM's): from this device onto a server, back, from one
server to another, or into [mailboxes](mailboxes.md). Onto a server the
database (as it is at that moment) and the pictures go in pieces; the server
takes the world in only if the one bringing it is its GM and the admin lets
them bring worlds. Afterwards the world lies there alone: the GM's copy is
gone, the server it left deletes its own.

While it moves the world is **closed**: on the GM's device (`store::Away`)
and, taken from a server (`Files { taking: true }`), there too — every other
ask gets `Busy` until it is gone, or ten minutes after the last piece. The
members see that it is moving; the GM sees how far. Four files travel at once.

## A server's admin

Named when the server starts (`--admin`). Sees every world (name, GM, size,
members, requests), lets GMs bring worlds, approves requests and takes
players out, closes, opens and deletes worlds, blocks and kicks. More admins
may be added. A server backs up the worlds it holds by itself
([backups](backups.md)).
