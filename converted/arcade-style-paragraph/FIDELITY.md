# Fidelity report: arcade-style-paragraph → main

- translated: 7
- approximated (with deltas): 23
- refused: 13

## Refused (cause named; conversion refused)

- `compositions/boot.html`:55 [tl.fromTo] ease "steps(4)" has no Framesmith curve; refusing the call site
- `compositions/boot.html`:56 [tl.to] ease "steps(4)" has no Framesmith curve; refusing the call site
- `compositions/boot.html`:58 [tl.fromTo] property "keyframes" (value "[{ scale: 1.25, duration: 0.12 }, { scale: 0.9, duration: 0.08 }, { scale: 1, duration: 0.08 }]") has no KDL motion property; refusing the call site
- `compositions/boot.html`:60 [tl.fromTo] ease "steps(1)" has no Framesmith curve; refusing the call site
- `compositions/boot.html`:61 [tl.to] ease "steps(1)" has no Framesmith curve; refusing the call site
- `compositions/score.html`:65 [tl.fromTo] property "v" (value "12450") has no KDL motion property; refusing the call site
- `compositions/score.html`:67 [tl.fromTo] property "keyframes" (value "[{ scale: 1.25, duration: 0.12 }, { scale: 0.9, duration: 0.08 }, { scale: 1, duration: 0.08 }]") has no KDL motion property; refusing the call site
- `compositions/score.html`:68 [tl.to] ease "steps(2)" has no Framesmith curve; refusing the call site
- `compositions/score.html`:78 [tl.fromTo] property "immediateRender" (value "false") has no KDL motion property; refusing the call site
- `compositions/score.html`:81 [tl.fromTo] property "keyframes" (value "[{ scale: 1.25, duration: 0.1 }, { scale: 1, duration: 0.1 }]") has no KDL motion property; refusing the call site
- `compositions/start.html`:51 [tl.to] ease "steps(1)" has no Framesmith curve; refusing the call site
- `compositions/coin-flash.html`:42 [tl.fromTo] ease "steps(1)" has no Framesmith curve; refusing the call site
- `compositions/crt-glitch.html`:47 [tl.fromTo] position "0.02 * i" is an expression; refusing the call site

## Approximated (measured deltas)

- `compositions/boot.html`:46 [placement] element #boot-ground has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/boot.html`:48 [placement] element #boot-stage has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/boot.html`:48 [placement] element #boot-title has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/boot.html`:48 [placement] element #boot-sub has no own left/top; placed by flow approximation at (0, 188) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/boot.html`:48 [container element] element #boot-stage is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/score.html`:52 [placement] element #score-ground has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/score.html`:53 [placement] element #score-num has no own left/top; placed by flow approximation at (90, 204) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/score.html`:56 [placement] element #score-burst has no own left/top; placed by flow approximation at (444, 864) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/score.html`:53 [container element] element #score-hud is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/score.html`:55 [container element] element #score-star is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/score.html`:56 [container element] element #score-burst is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/start.html`:43 [placement] element #start-ground has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/start.html`:44 [placement] element #start-stage has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/start.html`:44 [placement] element #start-text has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/start.html`:44 [placement] element #start-coin has no own left/top; placed by flow approximation at (0, 152) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/start.html`:44 [container element] element #start-stage is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/coin-flash.html`:36 [placement] element #coin-flash-fill has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/crt-glitch.html`:38 [container element] element #crt-glitch-a is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/crt-glitch.html`:39 [container element] element #crt-glitch-b is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/crt-glitch.html`:40 [container element] element #crt-glitch-c is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/scanlines.html`:37 [placement] element #scanlines-fill has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/scanlines.html`:37 [container element] element #scanlines-fill is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `index.html`:44 [frame rate] fps=30 emitted on the composition root delta: frame rate undeclared in source; HyperFrames default 30fps assumed

## Translated

- `compositions/boot.html`:52 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/score.html`:60 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/start.html`:48 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/coin-flash.html`:40 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/crt-glitch.html`:43 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/scanlines.html`:41 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `index.html`:63 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
