---
version: alpha
name: Stat overlay — Frame
description: A badge that sits on any footage. Transparent canvas, one dark glass card, one gold accent for the star.
colors:
  glass: "rgba(12, 12, 14, 0.82)"
  text: "#ffffff"
  muted: "#b4b4bb"
  gold: "#f5c542"
typography:
  number: { fontFamily: "Inter", weight: 900, tracking: "-0.03em", numeric: "tabular-nums" }
  label:  { fontFamily: "JetBrains Mono", weight: 400, tracking: "0.08em", upper: true }
motion:
  enter: "card rises 30px + fades, power3.out 0.5s"
  hit: "star slams from scale 1.6, back to 1 with expo.out 0.35s"
  count: "proxy count-up 1.4s, expo.out, then hold"
  label: "after the number lands (value → meaning)"
  exit: "card fades down 20px, power2.in 0.4s, to fully clear (overlay must end empty)"
rules:       []   # the user stated no rules for this inspiration (SKILL.md step 1)
---

## Pre-Render Self-Audit

- **Transparency** — nothing but the card is painted; the first and last frames are empty.
- **Accent** — gold only on the star.
- **Order** — the number lands before its label (value → meaning).
- **Numbers** — the count matches its source and holds still on tabular digits.
