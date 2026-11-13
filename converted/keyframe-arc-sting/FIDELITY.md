# Fidelity report: keyframe-arc-sting → main

- translated: 5
- approximated (with deltas): 6
- refused: 2

## Refused (cause named; conversion refused)

- `compositions/sting.html`:70 [tl.registerPlugin] timeline call "registerPlugin" is outside the translatable grammar (to/from/fromTo/set); refusing the call site
- `compositions/sting.html`:89 [tl.fromTo] property "motionPath" (value "{ path: [{ x: 0, y: 0 }, { x: 390, y: -560 }, { x: 790, y: -270 }], curviness: 1.5, autoRotate: true }") has no KDL motion property; refusing the call site

## Approximated (measured deltas)

- `compositions/sting.html`:61 [placement] element #sting-group has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/sting.html`:62 [placement] element #sting-title has no own left/top; placed by flow approximation at (0, 470) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/sting.html`:61 [container element] element #sting-group is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/sting.html`:62 [container element] element #sting-title-wrap is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/sting.html`:78 [tl.to] tl.to on "#sting-title-wrap" at start=0.8000 duration=0.9500 delta: keyframes scale chain start value 1 resolves from the clip's static scale value; GSAP captures the live value at the tween start, so early eval may differ
- `index.html`:41 [frame rate] fps=30 emitted on the composition root delta: frame rate undeclared in source; HyperFrames default 30fps assumed

## Translated

- `compositions/sting.html`:71 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/sting.html`:74 [tl.fromTo] tl.fromTo on "#sting-title" at start=0.0000 duration=0.6000
- `compositions/sting.html`:95 [tl.to] tl.to on "#sting-mark" at start=2.6000 duration=0.3000
- `compositions/sting.html`:99 [tl.fromTo] tl.fromTo on "#sting-group" at start=2.6000 duration=3.3000
- `index.html`:48 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
