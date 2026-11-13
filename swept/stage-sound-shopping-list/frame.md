---
version: alpha
name: Sound shopping list — Frame
description: Black stage, white heavy type, one green that marks the hit.
colors: { black: "#000000", white: "#ffffff", gray: "#a1a1aa", green: "#3CE6AC" }
typography:
  display: { fontFamily: "Inter", weight: 900, tracking: "-0.04em", upper: true }
  mono: { fontFamily: "JetBrains Mono", weight: 400 }
motion:
  cut: "hard cuts on audiomap downbeats"
  sound: "every SFX is placed by its measured peak or onset (scripts/audio-peak.py), never by file start"
  hit: "the loudest hit flips the frame to green"
rules:       []   # the user stated no rules for this inspiration (SKILL.md step 1)
---

## Pre-Render Self-Audit

- **Sound** — every SFX starts at its measured peak or onset, never at file start.
- **Cuts** — hard cuts on audiomap downbeats only.
- **Accent** — green appears only on the loudest hit.
