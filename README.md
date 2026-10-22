<p align="center"><img src="source/apps/desktop/src/assets/logo.png" width="128" alt=""></p>

<h1 align="center">AnotherWorldbuilder</h1>

<p align="center">The wiki for your worlds and campaigns in tabletop role-playing games –<br>on your computer, with your data, together with your group.</p>

<p align="center"><a href="../../releases/latest"><b>Download</b></a> · <a href="source/docs/README.md">Documentation</a> · <a href="source/README.md">Build it yourself</a></p>

> **In development.** Not meant for real use yet: formats may change,
> and older worlds are not carried over when they do.

## What it does

- **A wiki for the world:** articles with types and properties you define yourself –
  character, place, faction, monster. Templates give you a start.
- **Maps** to pan and zoom, with pins that lead to the article, and layers that
  switch to the next map.
- **Calendar and timeline** in your world's own calendar, plus family trees.
- **Stat blocks and dice:** click rolls in the text, a dice tray with log and
  favourites, 3D dice. Game rules are data of the world – no system is built in.
- **Images** in a gallery of the world; metadata such as location and camera is removed.
- **Who sees what:** sections for the GM only, for single players or for everyone,
  with read and write rights apart. Characters can be assigned to players.
- **Versions:** everything is saved automatically; earlier versions can be viewed.
- **Writing together**, with visible cursors:
  on the home network, through a shared cloud folder, directly over the internet, through
  the GM's device or through a server of your own on a NAS.

## Your data

- Everything lies in a folder on your computer; you choose which one on first start.
- No account and no server of the maker. The group connects directly or through
  what it runs itself.
- Encryption can be chosen per world and costs nothing extra.

## Download

The current version is under **[Releases](../../releases/latest)**:

| File | For |
| --- | --- |
| `AnotherWorldbuilder_<version>_amd64.AppImage` | Linux, no installation |
| `AnotherWorldbuilder_<version>_amd64.deb` | Linux (Debian, Ubuntu), installed |
| `AnotherWorldbuilder.exe` | Windows, no installation (needs WebView2) |
| `DnD-5e-template.zip` | Template for D&D 5e: types and outline only, no content |
| `aw-server`, `aw-server-<version>-docker.tar` | Server for the group, e.g. on a NAS |

The server is also available directly from Docker Hub:
[`dicefoxcreations/anotherworldbuilder-server`](https://hub.docker.com/r/dicefoxcreations/anotherworldbuilder-server)
– [setting it up on a Synology NAS](source/docs/testing/server.md).

To use a template: unzip it, then in the app *Options → Transfer → Import* and choose the
folder `D&D 5e.awpack`.

## What is in here

| Folder | Content |
| --- | --- |
| [`programs/`](programs/) | Checksums, license notices and server files of the current version; the programs themselves are under [Releases](../../releases/latest) |
| [`templates/`](templates/) | Templates to start a world with |
| [`source/`](source/) | Source code, [build guide](source/README.md) and [documentation](source/docs/README.md) |
