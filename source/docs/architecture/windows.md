# Windows beside

As of October 2026. Code: `lib.rs` (`window_open`), `capabilities/default.json`, `api/changes.ts`, `shared/apart.ts`, `app/App.tsx`.

A document can open in its own window, to lay it beside another: read in one, write in the other.

- **Opening:** Ctrl+click (⌘ on a Mac) or the middle mouse button on any link, card or entry that leads elsewhere. All of these go through the app's one navigation (`go` in `App.tsx`), and that asks `wantsApart()`. Added to that are ⧉ in a document's header bar and ⧉ at a section's title. The latter opens the document at that section and briefly highlights it.
- **A window is the whole app**, started with `index.html?world=…&place=…&part=…`. It opens the world at that place and can go anywhere from there. All windows run in the same process and share profile and keys. The profile lock is per process, so it does not get in the way.
- **Keeping in step:** every command that does more than read and names a world tells all windows `world-changed` once it succeeded (bundled, 250 ms). The others reload types and documents. Reloading only reads, so nothing echoes back and forth. Which commands only read is listed in `READS` in `api/changes.ts`. A forgotten read command costs no more than one needless reload.
- **Network:** only the main window syncs. What arrives that way it tells the others just the same.
- **Size and position** the app remembers only for the main window. Windows beside are for the moment, 860 × 820, narrow down to 380 px.
