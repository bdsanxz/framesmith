# Keyframes — arc-throw sting

A 6s 1:1 sting written the way Studio's keyframe tools write motion, so every move shows up as diamonds a person can edit in Studio and as plain GSAP an agent can edit in the file. `npx hyperframes@0.8.103 check` passes on `hyperframes@0.8.103`.

**Media.** This inspiration ships without its media files. In the compositions each photo or video is a labelled stand-in (its `data-media` names the file it held; `data-media-kind="video"` marks a clip), each sound a `SOUND SLOT` comment, and each font a `local()` face; the guide comment at the top of each composition says how to plug a real file back in. The files named below are the ones the film was made with.

| Time | Move | Keyframe form |
|---|---|---|
| 0–0.6s | headline rises in | `fromTo` (two keyframes) |
| 0.8–1.75s | punch in, overshoot back, settle | one tween with a `keyframes: [...]` pose ladder, parent `ease: "none"`, an ease per stop |
| 1.0–2.6s | the mark is thrown along a curve and turns to face its travel | `motionPath: { path: [3 points], curviness: 1.5, autoRotate: true }` (Studio's Arc Motion) |
| 2.6–2.9s | the mark squares up and shrinks into place | `to` (flat) |
| 2.6–6.0s | title and mark drift together | `fromTo` on a shared group, `sine.inOut` |

Prove the motion from the CLI, not only by eye:

```bash
npx hyperframes@0.8.103 keyframes .                         # every animated target and its stops
npx hyperframes@0.8.103 keyframes . --selector "#sting-mark" --shot arc.png --samples 12 --from 1 --to 2.9
npx hyperframes@0.8.103 snapshot --at 0.9,1.8,3,5.9
```

`keyframes .` reported the mark as `fromTo/keyframes @1s→2.6s: 0% {x:0 y:0} 50% {x:390 y:-560} 100% {x:790 y:-270}`, and the onion shot drew a smooth arc with the mark rotating along it, tight ghosts at both ends (ease in and out), and wide gaps mid-flight.

Two fixes the checks forced:
- `text_occluded`: the mark crosses the headline for a few frames mid-throw. That is intended, so `data-layout-allow-occlusion` goes on the occluded text (on the moving mark it has no effect).
- The first drift moved only the headline, which then slid into the landed mark. Drifting a group that holds both keeps their spacing.
