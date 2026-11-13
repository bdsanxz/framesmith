# Lottie JSON — shown, then played

Found JSON is an animation, data, or design tokens. This inspiration takes the animation case: `assets/tile-pulse.json` is a small Lottie file written for this inspiration (one rounded tile that scales 30% → 100% → 60% and turns 180° over 2s at 30 fps). The video shows the JSON, then plays it untouched. `npx hyperframes@0.8.103 check` passes on `hyperframes@0.8.103`.

| Time | Scene | How |
|---|---|---|
| 0–2.5s | `compositions/json-view.html` | the file's own text in a code panel |
| 2.5–5s | `compositions/lottie-play.html` | `lottie.loadAnimation({ renderer: "svg", loop: false, autoplay: false, path: "assets/tile-pulse.json" })`, pushed onto `window.__hfLottie` |

Verified timing: snapshots at 2.55s, 3.0s, 3.5s, and 4.5s show the tile at the start, mid-growth, full size, and settled, so **a Lottie inside a sub-composition plays from its host's `data-start`**, not from global time. Wrapping a Lottie in a scene is how you give it a start offset (on its own, a registered Lottie plays against composition time from 0).

Rules: load from a local file (never a remote URL at render time), always set `loop` (lottie-web treats a missing `loop` as true), never `autoplay` or `play()`, keep the container's size fixed in CSS, register every player. The adapter seeks lottie-web with `goToAndStop(ms)`; dotLottie players (`@lottiefiles/dotlottie-web`, `<canvas>`) register the same way.
