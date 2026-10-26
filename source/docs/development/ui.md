# Interface: building blocks and rules

All looks live in `apps/desktop/src/styles/global.css`. The goal is that
the same things look and behave the same everywhere. A new element takes an
existing building block; a variant of its own needs a reason, written in the
CSS as a comment.

## Colours

Only through the tokens in `:root`, no hex values in rules:

| Token | What for |
| --- | --- |
| `--base`, `--sunken`, `--card`, `--popover`, `--field`, `--raised`, `--hover` | surfaces, from deep to high |
| `--border`, `--text`, `--muted` | lines and type |
| `--heading` | titles and headings, like `--text` unless chosen |
| `--track` | switches in the off state |
| `--accent`, `--accent-soft`, `--accent-line`, `--on-accent` | teal: focus, hover, selection, main action |
| `--warn`, `--danger-*` | red: deleting, errors, GM only |
| `--ok-*`, `--info-*` | green: open to all; blue: players |

The only exceptions are the comparison marks (red/green), the author colours
when writing together and the line colours in the relation graph
(`--relation-0` … `--relation-7`). The stat block has no colours of its own
either: it is a card like `box`.

### Own colours

Under *Options → Appearance* every user picks four colours, for this device
and all worlds: **background**, **text**, **accent** (links, selection, main
button, focus) and **headings**. There are four presets as well: *Night* (the
shipped one), *Ember*, *Parchment* and *Paper*.

Everything else follows from these four in `shared/Theme.ts`:

- The surfaces lie between background and text, from `--sunken` (2.5 %
  towards the text) to `--border` (20 %). `--muted` lies at two thirds.
- The accent steps lie between accent and background. `--on-accent` is nearly
  black or nearly white depending on the accent's brightness.
- Red, green and blue keep their meaning and only move towards the background
  (lines, surfaces) or towards the text (type). So they read on a light page
  as on a dark one.
- A light background switches `color-scheme` to `light` (scroll bars, the
  system's input fields).

The shipped colours stay those of `global.css` with their hint of teal. The
four are only stored when they differ from it. Every window of the app
follows at once.

Why four and no more: background and text give the whole surface, the accent
everything one can click, and the headings give a world its tone. A fifth
colour would replace red, green or blue, and those carry meaning (GM only,
everyone, players).

## Width

Nothing is wider than its content needs.

- A page of the options (`.type-page`) is a **column of at most 760 px**, aligned left. Rows, lists and switches do not grow with the window.
- Only what shows many things side by side – the gallery, an editor – takes the room (`.type-page.wide`, classes of their own).
- Input fields in a bar have a **fixed, fitting width**: a search about 220 px, not "the rest of the row".
- The areas of the options stand in three groups: **This world** (World with name, symbol, where it lives, backups and versions; Players, Types, Images, Calendar, Dice, Transfer), **This device** (Appearance, Shortcuts, Network, Storage) and **App** (About). Dice are in one place: at the top those of the world (GM), below them their look on this device. Without an open world only the device and About.
- The **options window** is the same size for all areas: as wide as a page of the world (`--page-width` plus navigation) and centred. On a wide screen it does not grow; the gallery fills it, forms stay in their 760 px column.
- The desktop window is at least 1080 px wide (`tauri.conf.json`), because that is what a document with its version bar needs. Windows showing something apart, and the phone, are narrower.

## Narrow: phone and windows beside

Below 900 px nothing stands beside the text any more. The versions then open over the document. Below 820 px the options fill the screen: their areas are a row of tabs that scrolls sideways, with × at the end. In the gallery the folders then stand above the images.

Whether infobox and side column stand beside the text is decided by the room of the page, not of the screen (container query on `.article-page` and `.visual-editor`, below 860 px stacked): beside the open outline on a tablet there is less room than the screen says. Stacked, every piece is as wide as the page – a wide table scrolls within itself, a map gets its width.

On the phone (below 600 px) the header is one row: ☰, back, the page's name, dice and ⋮. The path before it is dropped – back and the outline lead there. ⋮ opens a sheet with all other tools one below the other, each with its name (the button's `title`); a click beside it closes it, profile and notices open above it at screen width. "In a new window" and the developer tools are missing there.

Checking is done with every new screen through the command `phone_view` (no button in the app): `invoke("phone_view", {on: true})` for a phone; other sizes: `invoke("phone_view", {on: true, size: [768, 1024]})` (tablet portrait), `[1024, 768]` (landscape).

## The outline on the left and the path at the top

**On the left**, every world has the outline (`features/navigation/NavBar.tsx`), as the counterpart to the side bar on the right. The GM sets its entries for everyone, in their order:

- a **type** – unfolded, its documents including subtypes, by name; if the type has a tree (`parent`), as a tree;
- a single **document**;
- a **group** with a name of its own, holding entries again (up to four levels).

As long as the GM has set nothing, the start page and all top types stand there. ✎ at the top of the bar switches to arranging: entries can be dragged (before, after, into a group), ＋ creates new ones, a click opens the settings on the right – *Access* (all or GM only), what it shows, *Label*, *Remove*. An entry "GM only" is marked red.

The outline is stored in the world (`navigation`). Players never get the GM entries: where the world is held, they are left out of what a player receives (`navigation::for_players`). Which documents stand under a type is decided by what the player may see; entries whose type or document they cannot open are missing. In the preview as a player the GM entries are missing too.

The device remembers what is folded open or shut. Wherever one goes, the path there folds open. "Find" filters by name. A type, unfolded, shows the first 60 documents and the one open right now; "… N more" below leads to the type's list. With a whole library (11,000 documents) the bar would otherwise stay at thousands of rows, and every page change would redraw it. Folded entries do not even work out their documents.

From 1000 px window width on – so in every desktop window – the bar stands beside the page, the page moves to the right, and a click inside leaves it open. ☰ at the far left of the header hides it; the device remembers. Narrower (phone), it lies over the page and closes as soon as one goes anywhere or clicks beside it.

**At the top** the header shows the **path one has gone** – up to ten places. Coming back to a place already in it drops everything after it; so the path never runs in circles. The back button (right of ☰) goes back along this path, at its start up the outline as before. On the world's start page, where back leads out, it is a door: *Leave world*. The same stands in the options as *Switch world*. The app starts in the world that was open last; whoever leaves it starts in the main menu next time.

## The inspector: settings in the side bar

In the visual editor nothing lies over the text. What one sets stands in the side bar at the right edge of the window (`features/wiki/Inspector.tsx`), always in this order:

1. **Access** – who sees it, who changes it, how far it travels. Only those who may hand out rights can do this.
2. What it shows: alignment and width of an image, content and columns of a list, types and text of a timeline, the settings of a graph.
3. **Tag**: the written tag, to change by hand; Enter applies.
4. **Remove**.

How it opens:
- An **element** (image, list, timeline, calendar, graph) is a `Surface` (`.surface-card`) and looks the same everywhere. When pointed at, it gets a soft ring in the accent colour, and its name stands at the top right as a pill. When selected, the ring is strong and the pill filled. Ring and glow are `outline` and `box-shadow`, so they take no room; nothing shifts. A click selects the element; links and buttons in it work from the second click on.
- For the **graph** the surface is the element: a click on it opens "Graph" with Access at the top. Cards and lines open their own settings.
- A click into the text leaves the page where it is: `scrollOf` in `FlowField.tsx` undoes what WebKit shifts on focusing.
- If the cursor is in a **part of the text**, the bar shows its access, for the GM. This does not happen while the versions are open. Closed with ×, it stays shut until the cursor moves into another part.

Rights always belong to a part. An element alone in its part has that part's rights. One between text shares them with everything in it; "Own access" writes `view=`/`edit=` on its tag, and on saving it becomes a part of its own (`element_with_access` in Rust). "Same as its part" undoes that. A graph spans several lines; whoever wants to protect it separately puts it in a part of its own.

## Headings

One scale for text headings and block titles: level 1 is `#` in the text
(22 px), 2 is `##` (19 px), 3 is `###` (17 px), 4 has the size of the text
(15 px). All are bold and in `--heading`. A block title stands at level 1,
`heading=2` to `4` makes it smaller (side bar: *Heading* H1 to H4).

What folds has its arrow on the left in the margin (`.head-chevron`), so the
title is flush with the text. In a box, a stat block, a quote and in the link
preview there is no room there; then the arrow stands before the title.

Every heading in the read text folds what follows it, up to the next one of
the same or a higher level — text, lists, timeline, map, also across the parts
of a block (`shared/Sections.ts`). A part whose content is folded away entirely
disappears with it. The device remembers what a reader folded. In the editor
the text stays open, because one cannot write in folded lines.

## Sections that fold

A long settings page is divided into **`Fold`** (`shared/ui/Fold.tsx`). The head always shows the title and, in one line, what is inside ("12 months · 362 days"). The content only appears when the section is open. Actions such as × stand on the right of the head and work when folded too. Level 1 is a card, level 2 a part within it with a dividing line. An example is the calendar: eras, with months, weekdays and leap day inside; beside them moons and preview.

## Buttons

One button, a few variants — nothing else:

| Class | When |
| --- | --- |
| *(none)* | every action that is not the main action |
| `primary` | the main action of a place, at most one per view |
| `danger` | deletes or takes away |
| `ghost` | without a frame until hover: menus, navigation, header |
| `compact` | small size for actions in lists and in the content |
| `icon` (+ `round`) | only a symbol; round in the header |
| `chip` | a pill that names something (type, document) and opens it |
| `on` | chosen, switched on, open — the same look everywhere |
| `.segmented` (group) | one choice among few, e.g. Code/Both/Visual |

Plus, for text that opens something: `link-button` (a name in a list) and
`wikilink` (a reference in running text).

Rules:

- **Hover is the teal frame everywhere.**
- **Labels without "…".** The button says what happens ("Make GM",
  "From file"), even if a dialog follows.
- **Deleting** is `DeleteButton` everywhere (a red bin) followed by
  `DeleteDialog` with the name typed.
- **Actions in a list row** are `compact` and stand on the right of the row,
  the bin last.
- A component sets only **layout** for its buttons (width, alignment,
  spacing), never colours or frames — those come from the variant.

## Right click

The browser's context menu is switched off in the whole app (`main.tsx`).
Where a right click means something, the app shows a menu of its own.

With text selected, the right click offers only what goes with it: *Link "…"*,
the formats and, for the GM, *Hide from players* (the words go into an area for
the GM only). Without a selection it offers every element **once**:
*List*, *Timeline*, *Calendar*, *Relation graph*, *Map*, plus heading, link,
image, dividing line, rolls and values. Which kind of list (cards, table,
tree, filtered, from a property) and which timeline (around this document)
one picks afterwards in the element's side bar. Variants the side bar can set
get no menu item of their own.

For the GM it also offers *Create button* (see below).

Under *Document* the GM finds **New section below**: a new section of the
type, directly below the one being written in (see
[stat blocks](../architecture/stat-blocks.md#order)).

If the cursor is on a heading in a section of the document itself,
**## headings to sections** is offered too: every heading of that level
becomes a section of its own with it as the title, as large as the heading was
(`##` becomes *H2*). What stands before the first one stays. So a page of `##`
headings becomes one of sections, with side bar, order and create button as
for typed documents.

If text is selected, **Link "…"** stands at the very top: the documents whose
name matches the words (equal, starts with them, contains them); a click links.
If there is none, *＋ New …* creates one: pick a type (typing filters), the
document is created under this name (with its type's name pattern), and the
words become the link. If they differ from the name, they stay as the link's
text. Only the GM creates.

Below that stands **Selected**: bold, italic, code, link (`@@`),
heading `#`/`##`, quote and list. Chosen again, it takes the marks away.

## Keyboard shortcuts

Fixed in every world: **Ctrl+P** opens a document by name, **Ctrl+Shift+F**
searches the text of all documents (the chosen one opens with the words
marked), **Alt+← / Alt+→** and the mouse's side buttons go back and forward,
**?** outside a text field shows all keys. **Ctrl+F** searches the page (all
hits light, the current one strong, framed in a link; Enter/Shift+Enter jumps,
Escape closes), **Ctrl+B** bold, **Ctrl+I** italic, **Ctrl+K** opens the menu
with *Link "…"* at the cursor, **Ctrl+E** switches between reading and editing,
Ctrl+Z/Y undo and redo.

Under Options → *Shortcuts* one sets keys for types per world
(`shared/Shortcuts.ts`, stored on the device): press keys, pick a type, *Add*.
Selected words – or without a selection the word at the cursor – become, with
the key, a link to the document of this type with that name; if there is none,
the GM creates it. Keys already taken (the fixed ones, copy, paste …) cannot be
assigned.

## Format bar

Only over **selected** text (code as well as visual) stands a small bar:
bold, italic, heading, list, quote, link. Whoever only writes or reads does not
see it. Inserting (image, list, map …) goes through the right click, `@@` and
the keys. A click does not remove the selection.

## Create buttons

Where a create button stands is decided by the text, not the app:

```text
<create type="Event, Session log" label="New event">
```

- One type: a click creates a new document of this type (the form for a new document opens). Several types: a click asks which.
- Without `label` it says "＋ New" and the type.
- It shows only the types one may create: the GM all, players those whose `<article>` allows it with `create=` (`<article view=players edit=players create=players>`). Without such a type it is missing, in the preview as a player too. With `view=` it can be restricted further.
- The same applies to "＋ New" on a type's page.
- In the editor it is a surface like list and timeline. The side bar picks access, types and label. Right click → *Create button* inserts it.
- The type names become IDs on saving; renaming breaks nothing.
- The type pages keep their own "＋ New" button.
- **Beside a section title:** `<block title=Places new="Place, Region">` puts a small ＋ beside the title, with the same rules. In the section's side bar it is called *New*. For a section of the type it applies to all documents of the type, for an own section only there.
