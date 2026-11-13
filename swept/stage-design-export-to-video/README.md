# Design export → video

A design tool that outputs real HTML/CSS (Claude Design, or any HTML/CSS mockup) gives you source material, not a picture to redraw. `design-export/index.html` stands in for such an export: a fictional "Lumen" design system's tokens and four static 540px frames. The composition rebuilds those frames as four 3s scenes on a 1080×1080 canvas. `npx hyperframes@0.8.103 check` passes on `hyperframes@0.8.103`.

**Media.** This inspiration ships without its media files. In the compositions each photo or video is a labelled stand-in (its `data-media` names the file it held; `data-media-kind="video"` marks a clip), each sound a `SOUND SLOT` comment, and each font a `local()` face; the guide comment at the top of each composition says how to plug a real file back in. The files named below are the ones the film was made with. `./make-footage.sh` fetches or makes them again; put each back in its slot to see the film as it was.

| Export frame | Scene | Kept from the export | Added for video |
|---|---|---|---|
| `#f1` grid | `compositions/grid.html` | 5-column grid, mint outline cells, mono chrome | 20 cells light one by one (0.1s apart) |
| `#f2` title | `compositions/title.html` | the copy, Archivo Black | entrance pop, slow widen (scale, not letter-spacing) |
| `#f3` count | `compositions/counter.html` | DAY label + big numeral | count-up to 20 |
| `#f4` end | `compositions/endcard.html` | ring mark + wordmark | ring and word in, ambient push |

The structural fix: the design tool's own export restarted its pattern video in every scene, so the background jumped at each cut. Here the pattern (`assets/bed.mp4`, a 12 s generated gradient loop in the Lumen palette, not kept in this folder; `./make-footage.sh` regenerates it, and any slow loop of that length can stand in) is **one** video under the whole cut, in an untimed full-bleed wrapper at z 0, with every scene's ground left transparent. The framework owns its playback, so the bed is frame-exact across all four scenes.

A shader transition (`cinematic-zoom` via `@hyperframes/shader-transitions`) was tried at the 3 → 4 seam. It rendered, but only in a real render (snapshots don't run shaders), and during its 0.5s it composited both scenes over a flat `bgColor`, hiding the bed. The seam is a hard cut instead, as most professional seams are. The working shader wiring is documented in `references/design-tools.md`.
