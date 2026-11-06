# Worlds without a server: mailboxes

As of October 2026. Code: `apps/desktop/src-tauri/src/mailbox.rs`,
`WorldDb::view_copy` in `crates/aw-store/src/db.rs`.

For a group that plays apart and at different times, without anything
running all day. The world lies with its GM; beside it, in a folder a cloud
app keeps in step, each player has a **mailbox** the GM shares with them
alone:

```text
<folder>/alle/media/          pictures every player sees — shared with all
<folder>/<name>/ein/          the world as <name> sees it
  world.sqlite                a copy of the world with only their parts
  media/                      pictures only they see
  done.json                   which of their changes the GM's app took or refused
<folder>/<name>/aus/          <name>'s changes, one signed file each
```

The cloud app's sharing is who may read and write a mailbox; there is no key
of its own, and the provider sees what lies in it.

## The GM

- *Options → World → Where it lives → Move or copy … → Mailboxes, no server*
  and a folder. Every player the world has gets a mailbox, named after them;
  `alle/` is made too. A world on a server is taken here first.
- *Options → Players → Add a player* by name: a place `slot:<name>` and its
  mailbox. Share `<name>/` with that player alone, and `alle/` with everyone.
- The first identity whose change arrives in a mailbox takes its place —
  from then on only their signed changes count. *Unlink* frees the place.

## A round of the GM's app

Every few seconds while the app runs:

1. **Take in:** each new file in each `aus/`, checked: the player's signature,
   the world, the place theirs.
2. **Do it:** a change is a command of the app (`remote::call`), done in the
   player's name — the same rules as anywhere. What they may not do is
   refused and named in `done.json`. A save carries the states it was written
   against, so text written offline merges with what the GM wrote meanwhile.
3. **Deliver:** where the world changed, each player gets their copy anew
   (`view_copy`: their documents and parts, the members GM and themselves, no
   versions, requests or gallery folders). Pictures every player sees go to
   `alle/media/` once, the others into their own mailbox. The database comes
   last, so its pictures are there before it.

## A player

- *Open world → Open a mailbox …* and their folder. The app finds `alle/`
  beside it.
- They work on a copy of their own, offline too. Every change stands at once
  and goes into `aus/` as a signed file.
- A new delivery replaces the copy; what they sent that the GM's app has not
  taken yet is put onto it again, as written. What was taken or refused they
  let go of; a refusal is written into the app's log.

## Switching

- **Server → mailboxes:** the GM fetches the world, then sets up the
  mailboxes; known players keep their places, linked. They open their mailbox
  once.
- **Mailboxes → server:** a last round takes in what players sent, the world
  goes onto the server, and each mailbox gets `ein/moved.json` naming it.
  Players' apps follow by themselves — they are members there already.

## Limits

- Changes reach the others only when the GM's app has run and the cloud app
  has carried the files.
- Every mailbox holds one player's view, and a whole copy of its database is
  written anew when the world changed.
- Only the GM's app delivers.
