# Fidelity report: split-flap-board → main

- translated: 3
- approximated (with deltas): 2
- refused: 9

## Refused (cause named; conversion refused)

- `compositions/board.html`:85 [tl.set] target "[ftop, fbot]" is not a DOM selector ("[ftop, fbot]" is runtime state); naming the call site instead of guessing a clip
- `compositions/board.html`:94 [tl.set] position "t" is an expression; refusing the call site
- `compositions/board.html`:95 [tl.set] position "t" is an expression; refusing the call site
- `compositions/board.html`:96 [tl.set] position "t" is an expression; refusing the call site
- `compositions/board.html`:97 [tl.to] duration "STEP / 2" is not a non-negative number; refusing the call site
- `compositions/board.html`:98 [tl.set] position "t + STEP / 2" is an expression; refusing the call site
- `compositions/board.html`:99 [tl.set] position "t + STEP / 2" is an expression; refusing the call site
- `compositions/board.html`:100 [tl.to] duration "STEP / 2" is not a non-negative number; refusing the call site
- `compositions/board.html`:101 [tl.set] position "t + STEP" is an expression; refusing the call site

## Approximated (measured deltas)

- `compositions/board.html`:56 [placement] element #board-grid has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `index.html`:38 [frame rate] fps=30 emitted on the composition root delta: frame rate undeclared in source; HyperFrames default 30fps assumed

## Translated

- `compositions/board.html`:69 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/board.html`:107 [tl.fromTo] tl.fromTo on "#board-grid" at start=0.0000 duration=6.0000
- `index.html`:45 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
