---
version: alpha
name: Recut Noir — Frame
description: True-black canvas, a three-step gray ladder, one blue accent, pill radii. Cards float over the speaker; the clip itself is never altered.
colors:
  black: "#000000"
  panel: "#111113"
  gray-1: "#c7c7cc"
  gray-2: "#8e8e93"
  white: "#ffffff"
  blue: "#0a84ff"
typography:
  display: { fontFamily: "Inter", weight: 900, tracking: "-0.04em" }
  body:    { fontFamily: "Inter", weight: 400 }
  mono:    { fontFamily: "JetBrains Mono", weight: 400, tracking: "0.06em" }
radii: { pill: "999px", card: "28px" }
motion:
  enter: "power3.out, 0.5s, rise 24px"
  layout: "video wrapper moves with power2.inOut over 0.7s, starting just before the cue word"
  numbers: "count-ups land exactly on the spoken number"
  safe: "every graphic lives above the caption band (bottom 240px) and inside title-safe"
rules:       []   # the user stated no rules for this inspiration (SKILL.md step 1)
---

## Pre-Render Self-Audit

- **Footage** — the clip itself is never altered; only its wrapper moves.
- **Safe zones** — every card sits inside title-safe and above the caption band.
- **Sync** — every card and count-up lands on its spoken word.
- **Accent** — one blue; grays carry everything else.
