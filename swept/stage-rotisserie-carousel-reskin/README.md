# Rotisserie carousel, re-skinned

One scene pulled from a shipped launch film and rebuilt on a different brand. The source is the "rotisserie" scene of the HeyGen × Stripe launch film (`heygen-com/hyperframes-launches`, `HF-heygen-stripe/compositions/rotary.html`, Apache-2.0), kept as it was in `source/rotary.original.html`, with only a guide comment added on top. The rebuild in `compositions/rotary.html` uses this inspiration's fictional Aurora Labs `frame.md`. `npx hyperframes@0.8.103 check` passes on `hyperframes@0.8.103`.

**Media.** This inspiration ships without its media files. In the compositions each photo or video is a labelled stand-in (its `data-media` names the file it held; `data-media-kind="video"` marks a clip), each sound a `SOUND SLOT` comment, and each font a `local()` face; the guide comment at the top of each composition says how to plug a real file back in. The files named below are the ones the film was made with. `CREDITS.md` names the source of each.

```bash
gh api repos/heygen-com/hyperframes-launches/contents/HF-heygen-stripe/compositions --jq '.[].name'
gh api repos/heygen-com/hyperframes-launches/contents/HF-heygen-stripe/compositions/rotary.html --jq .content | base64 -d
```

What carried over (the mechanic):
- one line, "fixed · drum · fixed", with only the middle word changing
- words placed on a cylinder: slot n at `rotateX(−n·30°) translateZ(R)`, and the axis pushed back by R so the front word sits at the same scale as the fixed words
- a vertical mask that fades the rows above and below behind the "wall"
- each advance is a 30° `rotationX` step with `power3.inOut` over 0.35s, landing on its cue
- motion blur on the fixed words only (a CSS filter on the drum would flatten the 3D)

What changed (the skin and the contract):
- brand: Outfit, night/ink grounds, aurora accent, all from `frame.md`; copy "Aurora [deploys · scales · heals · ships] itself."
- the background changes on every flip, as the brief asked: four grounds crossfade on the same beats as the drum
- contract: `<template>` transport, `#root` styling, px instead of `vw`, no empty padding tween (render length comes from `data-duration`), no `tl.play()` or URL-driven seeking, an ambient push through the hold, and the drum's back-facing words marked `data-layout-allow-overlap`
