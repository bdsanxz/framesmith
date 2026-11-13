---
version: alpha
name: Week Sizzle — Frame
description: Lime on sage, heavy 900-weight display. Hype register, hard edges, the beat is the motion.
colors:
  sage: "#C9D6C2"      # canvas
  moss: "#1E2A1C"      # ink: all type on sage
  lime: "#B6F23A"      # the one accent: pills, banner, flip cards
  bone: "#F4F7EF"      # type on moss
typography:
  display: { fontFamily: "Inter", weight: 900, tracking: "-0.04em", upper: true }
  kicker:  { fontFamily: "JetBrains Mono", weight: 700, tracking: "0.14em", upper: true }
motion:
  character: "percussive — every change happens on a hit"
  cut: "hard 0ms cuts on downbeats; no crossfades in beat_cut frames"
  hit: "slam in from scale 1.25 with expo.out in 0.18s"
  accent: "the lime pill punches on the snare after each downbeat"
  seams: "phrase seams get the biggest hits: a palette flip (moss/lime ground)"
  ambient: "1.5% push per card so holds never freeze"
rules:       []   # the user stated no rules for this inspiration (SKILL.md step 1)
---

# Week Sizzle

Sage canvas and moss type by default; a palette flip to moss ground with lime type marks a phrase seam. One lime element per card. Copy is all caps, three to five words.

## Pre-Render Self-Audit

- **Beat** — every cut and hit sits on an `audiomap.json` anchor.
- **Accent** — one lime element per card.
- **Copy** — all caps, three to five words per card.
- **Holds** — every card keeps its slow push; nothing freezes.
