---
format: 1920x1080
duration: 60s
message: "Above ~900 frames, one verified drawElement worker beats screenshot-parallel, so auto renders now take that path"
arc: reviewer-brief (feature-reveal with fix) — ask → why it matters → mechanism → receipts → code → what to check → handoff
audience: HyperFrames maintainers reviewing the PR
mode: companion
music: calm low-pulse bed, under narration
pr: heygen-com/hyperframes#2026
version: v1
---

PR video (a reviewer briefing), 60s, 16:9, narrated. No footage; every code frame shows real diff lines.

## Frame 1 — The ask

- type: hook
- duration: 4s
- transition_in: cut
- status: animated
- src: compositions/frames/01-hook.html
- scene: Cream cover, EB Garamond line, +407/−40 · 6 files under it
- voiceover: "Before you scroll four hundred lines, give me sixty seconds."
- persuasion: direct address

Talk to the reviewer, not about the code. The diff stats are context, not the hook.

## Frame 2 — Why it matters

- type: problem
- duration: 8s
- transition_in: push-slide
- status: outline
- scene: A gauge stuck at 3.8% drawElement engagement; "50% of local renders" lands beside it
- voiceover: "Half of all local renders get routed to unverified screenshot capture. That's where our drawElement engagement stalls."
- persuasion: stakes

## Frame 3 — The idea

- type: mechanism
- duration: 10s
- transition_in: crossfade
- status: outline
- scene: Two lanes race a 2,380-frame render; five screenshot workers vs one verified drawElement stream; the single lane finishes first
- voiceover: "Past about nine hundred frames, one verified drawElement worker beats screenshot-parallel at every worker count. So when the worker count is auto, we invert it to one."
- persuasion: show the behavior

Animate the behavior, don't show code yet. One coral marker on the winning lane.

## Frame 4 — The receipts

- type: evidence
- duration: 8s
- transition_in: cut
- status: outline
- scene: number-lockup — 66.3s vs 109–127s on the graphics comp; a 900-frame threshold line on a frames axis
- voiceover: "On the long graphics comp, sixty-six seconds instead of one-ten to one-twenty-seven. Below nine hundred frames, nothing changes."
- persuasion: evidence before code

## Frame 5 — The code at its core

- type: diff
- duration: 6s
- transition_in: cut
- status: animated
- src: compositions/code-diff.html
- scene: code-diff block on renderOrchestrator.ts — the auto-calibration guard gains `!deInversionEligible`
- voiceover: "The whole switch is one guard: if the inversion is eligible, skip parallel calibration."
- persuasion: the turn

### Source excerpt

```diff
     if (
       job.config.workers === undefined &&
       totalFrames >= 60 &&
       !htmlInCanvasDetected &&
-      !cfg.lowMemoryMode
+      !cfg.lowMemoryMode &&
+      !deInversionEligible
     ) {
```

## Frame 6 — The predicate

- type: diff
- duration: 8s
- transition_in: push-slide
- status: outline
- scene: code-highlight on `shouldPreferSingleWorkerDrawElement` — the `requestedWorkers` check lit
- voiceover: "Eligibility lives in one exported predicate with seven unit tests. An explicit workers flag is always honored."
- persuasion: reassurance

## Frame 7 — What to check closely

- type: problem
- duration: 8s
- transition_in: crossfade
- status: outline
- scene: Three review cards rise: the HF_DE_SINGLE_MIN_FRAMES override, the disk-fallback clamp, the de_worker_inversion telemetry
- voiceover: "Three places deserve your eyes: the threshold override, the disk fallback clamp, and the new inversion telemetry."
- persuasion: rule of three

## Frame 8 — Handoff

- type: credits
- duration: 4s
- transition_in: crossfade
- status: animated
- src: compositions/frames/08-handoff.html
- scene: Author avatar, name, honest status pill
- voiceover: "Vance wrote it. It's your call now."
- persuasion: handoff

The pill shows the PR's real state. When the plan was written it was AWAITING REVIEW; it is MERGED now, so the pill says MERGED.
