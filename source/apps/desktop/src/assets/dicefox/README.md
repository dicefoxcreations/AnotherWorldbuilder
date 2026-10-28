# Approved DiceFox artwork, v12

Approved by the user on 2026-10-27. Source: `artifacts/branding/dicefox-layered-v12/` at the repository root.

The nine WebP files are unchanged source layers on a 1254 × 1254 canvas. `scene.svg` preserves the approved geometry, metallic gradients, layer order, placement and corrected foreground finger mask. The large left finger stays fixed; two rear fingers remain behind the world. Keep these layers registered together.

`DiceFox.tsx` loads this repository-owned template as text, replaces the image tokens with bundled Vite URLs, and prefixes every SVG ID per instance. It controls the approved growth phases and ambient movement. `FoxLoader` supplies the real progress value or `null` for a repeating build.

To export again, run from the repository root:

```sh
python3 artifacts/branding/dicefox-layered-v12/export-app.py
```

This requires only Python's standard library and does not regenerate artwork. `provenance.json` records hashes of the approved SVG and all source images. Substituting the original IDs and embedded images into the exported template reproduces the approved `full.svg` exactly.

Visual and lifecycle checks: `tests/browser/dicefox.html` in the desktop test bench. Results and comparison images: `artifacts/branding/dicefox-layered-v12/qa/app-*`.
