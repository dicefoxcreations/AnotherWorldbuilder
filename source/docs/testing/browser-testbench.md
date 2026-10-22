# The test bench

The Rust tests check **rules**. The test bench checks what comes out of
**using** the app — with real keystrokes, real clicks and the same Rust core
that does the work in the app.

The difference is not academic. The bugs of the last rounds were all on this
side: a half-typed `<` that swallows the line below it; a building block that
lands in the head instead of the body; two rights lines that say the same
thing. A screenshot does not show such things, and neither does a type checker.

## What it is

Three parts that together stand in for the app — without a world, without keys,
without a file:

| Part | Where | What |
| --- | --- | --- |
| Core | `tools/testbench/` | Offers `template_read`, `template_write` and `markdown_read` over HTTP. Calls the same functions as the Tauri command — `aw_model::editor`. |
| Page | `apps/desktop/tests/browser/` | Loads the **real** components. Only `invoke` comes from `core.ts` and talks to the core instead of Tauri. |
| Driver | `apps/desktop/tests/browser/driver.mjs` | Remote-controls Chrome: click, type, take pictures. No Playwright — Chrome brings the protocol, Node can do WebSocket. |

The core listens only on `localhost` and knows nothing but the text the
browser sends it. It cannot open a world, read a file or see a key.

## Starting

```sh
cargo run -p aw-testbench &                  # core on :5320
npm --prefix apps/desktop run testbench &   # page on :5321
google-chrome-stable --headless=new --remote-debugging-port=9222 \
  --user-data-dir=/tmp/aw-chrome about:blank &
```

The page also works without remote-controlling Chrome — then you do it by
hand: <http://localhost:5321/>. At the top sits a bar with the stages
(template editor, new article type, create article, options, legend, compare versions, relations, stat block).

## Runs

`apps/desktop/tests/browser/scenarios.mjs` holds the runs that can be repeated.
Each ends with a claim:

```sh
cd apps/desktop
node tests/browser/scenarios.mjs              # all
node tests/browser/scenarios.mjs typing       # one
```

```text
typing
  ✓ what was typed stands in the body
  ✓ the skeleton is whole
  ✓ the preview shows the block
  ✓ and there is no error
```

A new run is an entry in `SCENARIOS`. The driver offers, among others,
`click`, `type`, `key`, `caretAfter`, `text`, `js` and `shot`.

## A single picture

```sh
cd apps/desktop
node tests/browser/driver.mjs "http://localhost:5321/?stage=blank" /tmp/aw-picture.png
```

## What it cannot do

No Tauri, so no saving, no world, no encryption — the test bench stores
nothing. Everything to do with keys is checked by the Rust tests in
`aw-policy` and `aw-crypto`. And the window size at start you only see in the
real app.
