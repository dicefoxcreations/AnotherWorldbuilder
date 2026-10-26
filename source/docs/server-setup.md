# Running a server

`aw-server` keeps a group's worlds reachable while the GM's computer is off. Everyone
proves who they are; the server lets them in where the world says they belong and gives
each member what their view and rights allow. One GM is its **admin** and manages it
from the app.

It is a single static program for x86-64 Linux, also as a Docker image. Any machine
that runs Docker will do – a NAS, a home server, a small rented server.

## 1. Your ID

In the app, open your profile (the round button top right) and copy your ID –
`id:` and 16 characters. That GM becomes the admin.

## 2. Start it

With Docker Compose (`compose.yaml` is also in [`programs/server-linux-x86_64/`](../../programs/server-linux-x86_64/)):

```yaml
services:
  aw-server:
    image: dicefoxcreations/anotherworldbuilder-server:latest
    restart: unless-stopped
    network_mode: host
    volumes:
      - ./aw-server:/data
    environment:
      AW_ADMIN: "id:0000000000000000"   # your ID
      AW_NAME: "Our table"              # what the app shows
      AW_LAN: "1"                       # found in the home network
```

Or with one command:

```sh
docker run -d --name aw-server --restart unless-stopped --network host \
  -v "$PWD/aw-server:/data" -e AW_ADMIN=id:0000000000000000 -e AW_NAME="Our table" \
  dicefoxcreations/anotherworldbuilder-server:latest
```

The log then shows `aw-server on UDP port 7447` and the server's **code**; the code
also lies in `aw-server/host-code.txt`.

| Setting | Meaning |
| --- | --- |
| `AW_ADMIN` | The admin's ID. Required. |
| `AW_NAME` | The server's name in the app. |
| `AW_PORT` | The port, UDP and TCP. Default 7447. |
| `AW_ADDRESS` | How it is reached, comma-separated – `192.168.0.20:7447,worlds.example.org:7447`. Empty: what the server finds itself. It answers at the first own address it names, so put the machine's address in the home network first. |
| `AW_LAN` | `1`: findable in the home network. |
| `AW_LOG` | `1`: writes how connections go – only for finding faults. |

**Host network** is the simple way: the server sees the machine's own address. With a
bridge network instead, map `7447/udp` and `7447/tcp` to the same numbers and set
`AW_ADDRESS`.

## 3. Open the way

- **Firewall** of the machine: allow port 7447 for **UDP and TCP**.
- **From outside**, for players not at home: forward 7447 UDP and TCP on the router to
  the machine, and add the public name to `AW_ADDRESS`. A web reverse proxy does not
  help – the app talks UDP.

## 4. In the app

*Add server* on the start page, then its address – `192.168.0.20`, `worlds.example.org`
or with a port, `worlds.example.org:4001` – or the full code. The admin then sees
**Manage** at the server: which GMs may bring worlds, which worlds are closed, who is
turned away. A GM brings a world with *Options → World → Where it lives → Move or copy …*.

A world on a server lies there and nowhere else: GM and players open and edit it
through the server, and no device keeps a copy of it — moving it there takes it off
the GM's device. Without the server it stays closed. The server holds the worlds as
plain files; protect its disk as you would any machine holding your campaign (disk
encryption on the NAS volume). Update all apps and the server together: 0.2 speaks
with 0.2 only.

**Backups.** The server backs up each world it holds by itself — daily, the last
seven kept, until the world's GM changes it under *Options → World → Backups*. They lie
in `aw-server/backups/` beside the worlds, so the volume holds them too; copy that
folder elsewhere now and then.

## On a Synology NAS

Container Manager runs on the x86-64 models (the "+" series).

1. In File Station, create the folder `docker/aw-server`.
2. *Container Manager → Registry*: search `anotherworldbuilder`, download
   `dicefoxcreations/anotherworldbuilder-server`, tag `latest`.
3. *Project → Create*, path `docker/aw-server`, and paste the `compose.yaml` above with
   the volume `/volume1/docker/aw-server:/data`.
4. If the NAS firewall is on: allow 7447 UDP and TCP.

**Updating:** pull `latest` again under *Image* and recreate the container – the worlds
stay in the folder.

How it works – the admin's view, moving worlds, limits – is in
[the network](architecture/network.md).
