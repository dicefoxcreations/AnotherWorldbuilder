# DiceFox artwork, revision 12

Unchanged copies from the publisher's DiceFox asset library (kept outside this repository; animation `aw-world-loader`, revision 12). `provenance.json` records the source and the checksums of every layer.

The nine WebP files are source layers on a 1254 × 1254 canvas and stay registered together. `scene.svg` holds the composition: geometry, metallic gradients, layer order and the foreground finger mask.

`DiceFox.tsx` loads the template as text, puts in the bundled image URLs and gives every SVG id a prefix per instance. It drives the growth phases and the ambient movement; `FoxLoader` gives it the real progress, or `null` for a build that repeats.

A new revision is exported from the library and replaces these files as a whole. Visual and lifecycle check: `tests/browser/dicefox.html` in the desktop test bench.
