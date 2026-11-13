# Flat footage, five grades

A 24 s film that grades itself. One flat, cast-tinted clip is mounted once per stage, and every look on screen is a real `data-color-grading` payload on the clip (here on labelled stand-ins; see Media). The film walks the pipeline in order: source → correct → grade → look → finish, then ends on a hue-curve end card. `npx hyperframes@0.8.103 check` passes on `hyperframes@0.8.103` with 0 warnings; a draft render was measured frame by frame.

**Media.** This inspiration ships without its media files. In the compositions each photo or video is a labelled stand-in (its `data-media` names the file it held; `data-media-kind="video"` marks a clip), each sound a `SOUND SLOT` comment, and each font a `local()` face; the guide comment at the top of each composition says how to plug a real file back in. The files named below are the ones the film was made with. `./make-footage.sh` fetches or makes them again; put each back in its slot to see the film as it was.

```bash
./make-footage.sh                                                               # the flat source (not committed)
python3 ../../scripts/make-cube-lut.py assets/luts/teal-amber.json              # the LUT (teal-amber.cube, made again from its spec)
npx hyperframes@0.8.103 check && npx hyperframes@0.8.103 render -q draft -o renders/draft.mp4
```

(`scripts/render-inspirations.sh flat-footage-grade` regenerates both and renders a draft.)

| Time | Stage | Payload |
|---|---|---|
| 0–3 | sting | the mono instance (`{"preset":"mono-fade"}`) under the graded instance, whose wrapper wipes on with `clip-path` |
| 3–6 | source | none. `--analyze` measured `Y 82–214, satAvg 8`, which became the on-screen numbers |
| 6–10 | correct | `adjust` only (contrast +0.3, blacks +0.6, shadows −0.1, highlights −0.15, whites +0.1, temperature −0.12, saturation +0.5, vibrance +0.2), swept across the source as a before/after |
| 10–13.5 | grade | correction + split-tone `wheels` + one `secondaries` selection (greens, saturation −0.4) |
| 13.5–18 | look | four presets alone at intensity 1 in a 2×2 grid; the mono tile measures `SAT 0.0` |
| 18–21 | finish | grade + `lut` (`assets/luts/teal-amber.cube`) faded 0 → 0.6 through `--hf-color-grading-lut-intensity`, + vignette 0.3, grain 0.08 |
| 21–24 | end | finish + `hueCurves.hueVsHue [[150,0],[190,80],[230,0]]`: the cyan mug turns violet, revealed by a circle from the mug |

Build notes:
- **The correction came from numbers.** `--analyze` reported no imbalance on this flat clip (it checks clipping and casts, not flatness), so the black and white points and the saturation were set by hand and checked with `scripts/frame-stats.sh`.
- **Signs were measured, not assumed.** `blacks` **+** deepens the black point. The first try (`blacks −0.3`) lifted it instead.
- **Presets stay alone.** Adding `adjust.saturation` on top of `mono-clean` measured SAT 12.8 instead of 0, because the patch overrides the preset's own value.
- **Wrappers move; videos don't.** Every wipe, reveal, and tile pop animates the untimed wrapper, because the grade is drawn on a sibling canvas.
- **Instances that overlap start at different times** (the sting's color instance starts at 0.5 s, and the four tiles are staggered by 0.15 s). Same `src` + `data-start` + `data-duration` in one file raises `duplicate_media_discovery_risk`. Each instance uses `data-media-start = data-start`, so they stay frame-locked.
- All words live in one graphics sub-composition (`compositions/labels.html`) whose groups switch on at stage boundaries. Timed groups with nested children in the root raised `nested_structure_needs_subcomposition` and `timed_element_missing_clip_class`.
