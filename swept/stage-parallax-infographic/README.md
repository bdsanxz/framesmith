# Parallax infographic

One 8 s infographic scene (1920×1080) filmed through a parallax camera: six depth planes under one camera that trucks right and pushes in gently, while the chart develops inside the shot. Built for `references/parallax.md`. `npx hyperframes@0.8.103 check` passes on `hyperframes@0.8.103` with 0 errors and 0 warnings (67/67 contrast checks).

```bash
npx hyperframes@0.8.103 check
npx hyperframes@0.8.103 render -q draft -o renders/draft.mp4                              # parallax (strength 1)
npx hyperframes@0.8.103 render -q draft --variables '{"parallax":0}' -o renders/flat.mp4   # the same scene as one flat plane
```

How it is built (`compositions/stage.html`):

- **Planes:** each is a full-frame layer with its own `--depth`.

  | Plane | Depth | Contents |
  |---|---|---|
  | backdrop | 0.15 | glow and dot grid |
  | ghost | 0.45 | the "2024" numerals |
  | title | 0.7 | the title, sitting behind the data |
  | chart | 1 | the focal plane |
  | labels | 1.2 | values floating just in front of their bars |
  | near | 2.2 | out-of-focus discs crossing the frame edges |

- **The rig is CSS.** The timeline tweens only `--cam-x` and `--cam-push` on `#pi-cam`. Each plane's transform is a `calc()` of those and its depth, so there is no per-frame script and nothing is measured.
- **The `parallax` variable is a strength dial.** `--k = 1 + (depth − 1) × parallax`, so 0 collapses every plane to depth 1: the flat "slideshow" move. Compare the two renders to see what the depth adds: the ghost numerals crawl (18 px of travel), the chart moves 120 px, and the near discs sweep 264 px and leave the frame.
- **Overscan only where needed.** Only the backdrop fills the frame, so only it carries overscan (`--base: 1.07`). Overscanning every plane enlarged the authored layout by 14% and pushed the chart off the right edge in the first pass.
- **Depth cues:** the shot opens with a focus pull (the chart plane goes from 8 px of blur to sharp in 0.9 s), the near plane holds 14 px of blur, and the backdrop keeps 2 px.
- **The data develops inside the moving shot:** bars rise in order, and values count up.
- **Reading the variable:** the host's variable reaches the sub-composition only as the inherited CSS property `--parallax`. Reading it in script failed both ways: at build time it was not set yet (the flat render came out identical to the parallax one), and inside a timeline callback the lint flags `gsap_callback_dom_measurement`. So CSS consumes it directly.
