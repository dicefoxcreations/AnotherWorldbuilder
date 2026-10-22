# Maps

As of October 2026. Code: `crates/aw-model/src/markdown.rs` (`Node::Map`, `read_map`), `names.rs` (`<pin to=…>`); interface: `shared/GraphText.ts` (`readMap`, `mapText`, `firstMap`), `features/wiki/MapView.tsx` (`MapView`, `MapBlock`, `MapCard`), `tests/browser/maps-stage.tsx`.

A map is a picture of the world that you pan and zoom like a map on the web. Markers and regions lie on it.

```text
<map src=img:… height=480>
<pin at=30,40 label="Stairs down" to=@@Place:Cellar@@ color=amber icon=stairs>
<pin at=72.5,45 label=Lighthouse>
<area at="40,55 60,52 66,80 38,85" label="Old town" to=@@Place:Old town@@ color=green>
</map>
```

- `src` is a picture of the world (as with `<image>`), `height` the height in the page (default 480 px).
- `at` is the spot on the picture in percent of width and height, to a tenth. A pin stays at its spot when zooming; only its marker keeps its size.
- `label` is the name, `color` one of the colours of the lines in the family tree.
- `label-at` says where a marker's name stands: `left`, `top`, `bottom`, right when not given. With `hover` it only appears under the pointer, on the same side: `label-at="top hover"`. In the side bar: *Always* / *On hover* and beside it ← ↑ → ↓.
- `icon` is the marker's symbol: `door`, `ladder`, `stairs`, `chest`, `skull`, `castle`, `tower`, `house`, `camp`, `cave`, `ship`, `anchor`, `tree`, `mountain`, `flag`, `star`, `sword`, `eye`, `question`. Without `icon` it is the pin. The symbols are the app's own line drawings (`shared/ui/MapIcons.tsx`), no pictures from outside.
- `<area at="x,y x,y x,y …">` is a **region**: a surface around part of the picture, its corners in percent, at least three. When reading, only its edge shows faintly. Under the pointer, surface and border light up, and its name appears at the pointer, above everything else, so no marker hides it. This way countries on a continent map, quarters of a city or rooms of a floor plan become clickable.
- `to` leads to a document, stored with its ID. Renaming breaks nothing.

## Secret markers and regions

```text
<pin at=30,40 label="Smugglers' cave" view=gm edit=gm>
<pin at=50,50 label=Wreck view="doc:ilva OR gm" edit=gm>
```

A marker or a region can say who sees it. So the GM prepares a whole map and uncovers bit by bit what the players discover: first for one character only, then for all players, finally without a setting of its own, like the map itself.

This is not hiding in the interface. On saving, such markers leave the map: all with the same access go together into a part of their own at the end of the block, as `<pins for=img:…>` with the map's picture, by which the read view finds them again. This part is an encrypted layer of its own. Whoever cannot open it does not have the markers, not even encrypted in the map's part. The GM's code and editor show the map as one again, each marker with its access on the tag (`document::joined`).

- The GM sees such markers paler, with a dashed ring, regions with a dashed edge. The list in the side bar marks them with ◌.
- At the top of the side bar stands **Access**: *GM only* makes a marker secret. After that, *Sees*, *Changes* and *Travels* set who has it. *As the map* uncovers it fully.
- The map itself takes the access of its block. `view=` on `<map>` is an error. A secret map stands in `<block view=…>`.
- Only in documents: in a type, a secret marker would be text that every document inherits openly. There it is an error.
- Parts that exist only in the document (an element with its own access, secret markers) belong to their block through their key (`story-2`, `story-3`) and appear there in the read view, not at the end of the page.

## Reading

- Dragging pans, the mouse wheel and a double click zoom around the pointer, `+` `−` zoom, `⤢` shows the whole picture.
- Hovering shows only the name; the document preview does not appear on maps, it would lie over the map. A click opens the target.
- **Layers:** if the document a marker or region leads to has a map itself, the map switches there in place: a staircase into the cellar or a door into the tower. `‹` goes back, the name beside it opens the document. Without a map, a click opens the document.

## Writing

- In the editor the map is one surface. When selected, the side bar shows the selection at the top, below it the **list** of all markers and regions (a click selects), then picture and height.
- `＋ Marker`, then a click on the picture sets a marker. A click selects it, dragging moves it. The side bar sets name, target, symbol and colour, or removes it.
- `＋ Region`, then a click at each corner along the border. A click on the first corner (or `✓ Region`) closes the surface, Escape cancels. When selected, a region shows its corners, and dragging moves it. Name, target and colour are in the side bar. *Colour each region* (section *Map*) gives all regions different colours in turn, for countries for example.
- **Finer and coarser:** between two corners of a selected region sits a small dot. Dragging it sets a corner there. A right click on the region inserts a corner at that spot, a right click on a corner removes it (at least three remain).
- **Right click** on a marker, a region or an entry of the list: remove.
- Building block *Map* in the right-click menu.

## Open

- Several layers on top of each other on one map (switching floors without a document per floor).
- Own pictures as markers.
