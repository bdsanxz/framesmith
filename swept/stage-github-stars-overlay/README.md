# GitHub stars — stat overlay

A 6s stat badge built the motion-graphics way: one sentence of intent, a real number fetched at build time, and a canvas with no background so the same composition renders as an MP4 or as a transparent overlay for any editor. `npx hyperframes@0.8.103 check` passes on `hyperframes@0.8.103`.

**Media.** This inspiration ships without its media files. In the compositions each photo or video is a labelled stand-in (its `data-media` names the file it held; `data-media-kind="video"` marks a clip), each sound a `SOUND SLOT` comment, and each font a `local()` face; the guide comment at the top of each composition says how to plug a real file back in. The files named below are the ones the film was made with.

```text
Make a 6-second update card: HyperFrames on GitHub, the star count counts up
to today's real number, dated. It should sit on top of footage.
```

- The number is live data: `gh api repos/heygen-com/hyperframes --jq .stargazers_count` returned 54,957 on 2026-10-01, and the card says "AS OF 2026-10-01 · GITHUB API" so it stays honest after the count moves.
- Value → meaning: the star slams in, the count runs up (proxy tween, tabular figures), then the label and date arrive after the number lands.
- Overlay rules: no background on `html`, `body`, or any root; the card fades out by the end so the clip can butt against anything; the card sits in the lower-left inside title-safe.

```bash
npx hyperframes@0.8.103 render -q delivery -o renders/stat.mp4           # opaque (black behind the card)
npx hyperframes@0.8.103 render -q delivery --format webm -o renders/stat.webm   # VP9 with alpha (alpha_mode=1)
ffmpeg -c:v libvpx-vp9 -i renders/stat.webm -frames:v 1 renders/f.png     # decode WITH alpha: force libvpx-vp9
```

The contrast audit reports 0/0 checks on a transparent canvas; check legibility on the snapshot or over real footage instead.
