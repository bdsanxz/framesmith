---
format: 1920x1080
duration: 16s
message: "Four days of HyperFrames examples, one per beat"
arc: cold open → four day cards on the downbeats → roll into the drop → banner on the final hit
audience: people following the HyperFrames course
mode: companion
music: assets/bgm.wav (synthesized by scripts/synth-drum-track.py)
version: v1
compositionId: hyperframes-week-sizzle
read: "117.5 BPM per the analyzer; sparse kick-only intro, 4-bar groove from 2.02s, accelerating roll 10.0–14.0s leading to a DROP, final hit at 14.0s, silence after 15.0s"
style: "Inter 900 + JetBrains Mono; sage #C9D6C2 · moss #1E2A1C · lime #B6F23A · bone #F4F7EF"
---

Sizzle reel, 16s, cut to the beat. Every card keeps a 1.5% push, so no hold freezes. No narration, no footage.

## Frame 1 — 01-cold-open

- span_sec: [0, 2.02]
- pacing: phrase_flow
- mood: hype
- feel: "two lone kicks under ticking hats, no groove yet"
- src: compositions/frames/01-cold-open.html

### Groups

- words on the grid: THIS @0 · WEEK @0.534 · IN @1.022 · HYPERFRAMES @1.533 (beats_sec)

## Frame 2 — 02-cards

- span_sec: [2.02, 10.031]
- pacing: beat_cut
- mood: hype
- feel: "steady four-bar groove, snare on 2 and 4"
- src: compositions/frames/02-cards.html

### Groups

- card 1 @2.02 (downbeat): DAY 1 · ONE COMMAND, EVERY AGENT. — pill on "EVERY AGENT" punches @2.531
- card 2 @4.017 (downbeat, SURGE — biggest hit): palette flip. DAY 2 · YOUR BRAND, MOVING. — pill @4.528
- card 3 @6.037 (downbeat): DAY 3 · 60 SECONDS, NOT 400 LINES. — pill @6.525
- card 4 @8.034 (downbeat): DAY 4 · THE SONG IS THE EDITOR. — pill @8.522

## Frame 3 — 03-roll-counter

- span_sec: [10.031, 14.0]
- pacing: beat_cut
- mood: hype, tense
- feel: "accelerating 25-hit roll from the phrase seam into a drop"
- src: compositions/frames/03-roll-counter.html

### Groups

- moss ground (phrase-seam flip @10.031). DAY counter steps 05 → 30 on each roll event in audiomap.events between 10.0 and 14.0; "DAYS. ONE A DAY." under it

## Frame 4 — 04-banner

- span_sec: [14.0, 16.0]
- pacing: phrase_flow
- mood: hype
- feel: "one final hit and crash at 14.0, decays to a hard stop at 15.0, then silence"
- src: compositions/frames/04-banner.html

### Groups

- lime banner sweeps on the hit @14.0: THAT'S THIS WEEK IN HYPERFRAMES. Holds through the silence to the end
