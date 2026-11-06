# Testing the network by hand

How to try worlds between devices with two copies of the app on one machine
and a local `aw-server`. How it works is in [the network](../architecture/network.md).

## Two copies of the app

Each copy needs folders of its own, so nothing reaches one's own profile:

```sh
B=build/cargo/x86_64-unknown-linux-gnu/release/aw-desktop
for n in gm player; do
  d=/tmp/aw-$n; mkdir -p $d
  AW_PROFILE_ROOT=$d XDG_CONFIG_HOME=$d/xdg-config XDG_DATA_HOME=$d/xdg-data \
    $B > $d/app.log 2>&1 &
done
```

Choose a folder at the first start of each, and an identity.

## A world on the GM's app

1. Both: *Options → Network → Local network* on.
2. GM: make a world, *Options → World → Where it lives → Offered to players*
   and *Seen by everyone on the network*.
3. Player: the world appears on the start page under *Other devices* →
   *Ask to join*.
4. GM: *Options → Players → Asking to join → Accept*. A few seconds later the
   world is in the player's list; open it.

Check: a part `view=gm` never reaches the player; what the GM writes appears
on the player's page within seconds and the other way round; the player
cannot change the world's icon or a GM-only part.

## A server

```sh
cargo build --release -p aw-desktop --no-default-features --features server --bin aw-server
build/cargo/release/aw-server --admin id:<the GM's id> --data /tmp/aw-server \
  --port 7611 --address <this machine's LAN address>:7611 --name Test
```

The address must be one of the machine's own; `127.0.0.1` is passed over.
Then in the GM's app *Add server* with that address. The GM is its admin:
*Manage* lists worlds, users, requests. *Move or copy … → Server* brings a
world there — afterwards it is gone from the GM's device. A player who knew
it on the GM's app follows by themselves (`Moved`). *Options → World → Backups*
shows the server's own.

Moving it back (*Move or copy … → This device*) takes it off the server.

## Over the internet

With *Internet* on, the app asks the router to forward its port and STUN
servers how it looks from outside; *Network* then shows its code. A player
enters the code, or the address, under *Add server*.

## Automatic tests

`cargo test -p aw-sync` (connections, limits, mDNS), `cargo test -p
aw-desktop --lib keeper` (joining, rights, bringing and fetching, admin
orders), `mailbox` ([mailboxes](mailboxes.md)).
