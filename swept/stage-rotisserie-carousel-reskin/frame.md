---
version: alpha
name: Aurora Labs — Frame
description: >
  Video-first companion to Aurora Labs' DESIGN.md. Atoms are identical and sacred: night ground,
  a single aurora accent, frost headlines, Outfit display + JetBrains Mono chrome, no shadows.
  Adds the motion language: how things enter, exit, move, and how long they dwell.
unit: the frame — 1920×1080 primary; 1080×1920 documented
principle: atoms are sacred · composition is free · motion is precise, never bouncy

colors:
  night: "#06141F"
  ink: "#0D2233"
  aurora: "#00E3FF"
  frost: "#E8F6FA"
  mist: "#8AA4B2"
  signal: "#FFB547"

typography:
  h1:    { fontFamily: "Outfit", px: 168, weight: 700, tracking: "-0.04em", color: "frost" }
  h2:    { fontFamily: "Outfit", px: 96,  weight: 700, tracking: "-0.03em", color: "frost" }
  body:  { fontFamily: "Outfit", px: 40,  weight: 400, lineHeight: 1.45, color: "mist" }
  label: { fontFamily: "JetBrains Mono", px: 24, weight: 400, tracking: "0.12em", upper: true, color: "aurora" }

spacing: { safe-x: "120px", safe-y: "96px", gap: "32px" }
radii: { card: "16px", chip: "999px" }

motion:
  character: "precise, engineered, calm — a well-tuned machine, not a toy"
  ease:
    enter: "expo.out"        # fast arrival, long settle
    exit: "power3.in"        # leaves decisively
    move: "power2.inOut"     # camera and layout moves
    never: ["back.*", "elastic.*", "bounce"]   # no overshoot anywhere
  duration:
    micro: 0.25              # chips, dots, cursor
    enter: 0.6
    exit: 0.4
    move: 1.2
  stagger: 0.06              # letters 0.03, list items 0.06, cards 0.12
  distance: "24-40px"        # entrances travel short; nothing flies across the frame
  entrance: "rise 32px + fade, or clip-path wipe left→right for type"
  exit: "fade + 16px drift up; type wipes out right"
  transition: "aurora wipe — a full-height accent panel sweeps in from the left (0.4s), then out to the right (0.4s), and the next scene is revealed behind it"
  pacing: "one idea per scene; a scene is 4-6s"
  dwell: "readable text holds ≥ 1.5s after it settles; a number holds ≥ 1s after counting"
  ambient: "nothing freezes: holds carry a 2-3% slow push or drift"
rules:       []   # the user stated no rules for this inspiration (SKILL.md step 1)
---

# Aurora Labs — Frame (video / frame layer)

## Overview

At frame scale Aurora Labs is a night-sky control room: a dark field, one cyan signal, crisp white type. The motion matches the voice — quick to arrive, unhurried to settle, never playful.

## Composition Rules

### Do

- Use the aurora accent once per frame, at full saturation, on the thing that matters.
- Wipe type in with `clip-path`; rise cards and chips 32px.
- Keep a slow push on every hold.

### Don't

- No overshoot easing, no bounces, no spins.
- No full-frame linear gradients (they band under H.264); use a localized radial glow instead.
- No shadows. Depth is the ink panel plus a 1px aurora hairline at 30% alpha.

## Pre-Render Self-Audit

- Every entrance uses `motion.ease.enter`; every exit uses `motion.ease.exit`.
- No scene shorter than 4s; every text block dwells ≥ 1.5s.
- One aurora accent per frame.
