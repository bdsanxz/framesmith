# One clip, five costumes

A 22 s film that re-costumes one clip in real time. Every look is a live `data-color-grading` effect payload on the clip or a still of it (here on labelled stand-ins; see Media). The effects play the entrances and exits themselves, and four Registry overlay blocks dress the frame on top. `npx hyperframes@0.8.103 check` passes on `hyperframes@0.8.103` (0 errors, 0 warnings); a draft render was measured frame by frame.

**Media.** This inspiration ships without its media files. In the compositions each photo or video is a labelled stand-in (its `data-media` names the file it held; `data-media-kind="video"` marks a clip), each sound a `SOUND SLOT` comment, and each font a `local()` face; the guide comment at the top of each composition says how to plug a real file back in. The files named below are the ones the film was made with. `./make-footage.sh` makes `footage/clip.mp4` again, and the cut-out and the still come from it by the commands under "How the media was made"; put each back in its slot to see the film as it was.

```bash
./make-footage.sh          # footage/clip.mp4: the clean still-life clip (not committed)
npx hyperframes@0.8.103 check && npx hyperframes@0.8.103 render -q draft -o renders/draft.mp4
```

| Time | Costume | How |
|---|---|---|
| 0–4 | focus pull | `{"effects":{"blur":0.8}}`, with `--hf-color-grading-blur` 0.8 → 0 while the title lands |
| 4–8 | bloom under a camcorder HUD | `bloom 2.2` on the cut, settled to 0.55 in 0.8 s; `camcorder-hud` block on top (date edited, font swapped to JetBrains Mono) |
| 8 | flash cut | `editorial-flash-overlay` peaks at 58 % of its window, so a 1 s slot starts at 8 − 0.58 = 7.42 |
| 8–12 | pixelate as entrance and exit | `--hf-color-grading-pixelate` 1 → 0 (8.05–9.25), holds, 0 → 1 (11.15–12.0), so the cut lands on mosaic |
| 12–15.5 | paint the world, not the subject | Kuwahara (radius 1) ramps 0 → 1 on the full clip; `assets/layers/subject.webm` (the cutout) rides on top with no payload |
| 15.1–16.5 | warm handoff | `organic-light-leak-overlay` |
| 15.5–22 | the freeze frame becomes a print | the clip in a 4:5 card freezes at 17 s on `freeze-frame-dressing`; the real still (exposure flash 1.4 → 0) fades off a `monoScreen` twin printed in `#17130d` on `#f4f0e0` |

How the media was made:
- The cutout: `ffmpeg -ss 11.5 -t 4 -i footage/clip.mp4 seg.mp4`, then `npx hyperframes@0.8.103 remove-background seg.mp4 -o assets/layers/subject.webm` on the clip's 11.5–15.5 s (120 frames, ~340 ms each on CoreML). The model picked the plant and the peach. The output was 4.9 MB, and a VP9 re-encode (`-c:v libvpx-vp9 -pix_fmt yuva420p -crf 38 -b:v 0`, then a stream-copy remux so the WebM has a duration) kept the alpha at 210 KB. It is placed with `data-media-start="0.5"` at `data-start="12"`.
- The still: `ffmpeg -ss 17 -i footage/clip.mp4 -frames:v 1 -q:v 3 assets/stills/freeze-17s.jpg`. It is the same frame the card video shows at 17 s, in the same box with the same `object-fit` crop, so the swap is invisible.

Build notes:
- **Kuwahara 0.4 did nothing visible** on this smooth, wide source. Radius 1 paints it (grain and speculars smear while the cutout stays crisp). Tune art effects to the shot, at final resolution.
- **Mono Screen has no animatable strength**, and global intensity blends only the grading stage. So the print reveal is two stacked copies of the still: the printed one below, the real one above, with the real one's wrapper faded out.
- **Empty containers show.** The 4:5 cards have a shadow, so they read as a box over every beat until their media start. They are hidden in CSS and switched on at 15.5 s.
- **The dressing's paper covered the photo** at opacity 0.64, so the card lifts above the block at the freeze (`zIndex` set at 17 s) and takes its own flash through `--hf-color-grading-exposure`.
- Two `<img>` with the same `src` + `data-start` + `data-duration` raise `duplicate_media_discovery_risk`. The real still ends at 19.5 s (after its fade), and the print runs to 22 s.
- `check` reports `container_overflow` *info* from inside `organic-light-leak-overlay`: its light layers overflow on purpose.
- `remove-background -o <dir>/file.webm` **hangs silently when `<dir>` doesn't exist**. Create the folder first.
