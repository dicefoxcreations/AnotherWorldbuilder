# Testing mailboxes by hand

A world without a server, with two copies of the app as in
[testing the network](network.md) and one folder standing in for the cloud.
How it works is in [mailboxes](../architecture/mailboxes.md).

1. GM: a world, *Options → World → Where it lives → Move or copy … →
   Mailboxes, no server*, and an empty folder, say `/tmp/aw-cloud`.
2. GM: *Options → Players → Add a player* — `Leo`. `/tmp/aw-cloud/leo/` and
   `/tmp/aw-cloud/alle/` appear; Leo stands there as *Not linked*.
3. Player: *Open world → Open a mailbox …* → `/tmp/aw-cloud/leo`. The world
   opens at once, offline.
4. A few seconds later the GM's app has taken Leo's first file: the place is
   linked, the fingerprint beside it.

Check:

- `sqlite3 /tmp/aw-cloud/leo/ein/world.sqlite "select text from layers"`
  holds no GM-only text.
- What Leo writes stands for him at once; after the GM's round it is in the
  world, and what the GM wrote meanwhile in the same paragraph stays.
- What Leo may not do (the world's icon, a GM-only part) the GM's app
  refuses; `done.json` names it.
- *Move or copy … → Server*: each mailbox gets `ein/moved.json`; Leo's app
  goes on working with the world on the server.
