# Fidelity report: media-ledger-card → main

- translated: 5
- approximated (with deltas): 6
- refused: 0

## Approximated (measured deltas)

- `compositions/card.html`:52 [placement] element #card-mark has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/card.html`:54 [placement] element #card-text has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/card.html`:55 [placement] element #card-title has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/card.html`:56 [placement] element #card-sub has no own left/top; placed by flow approximation at (0, 164) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/card.html`:54 [container element] element #card-text is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `index.html`:56 [frame rate] fps=30 emitted on the composition root delta: frame rate undeclared in source; HyperFrames default 30fps assumed

## Translated

- `compositions/card.html`:61 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/card.html`:64 [tl.fromTo] tl.fromTo on "#card-mark" at start=0.2000 duration=0.5000
- `compositions/card.html`:67 [tl.fromTo] tl.fromTo on "#card-title" at start=1.5000 duration=0.2000
- `compositions/card.html`:69 [tl.fromTo] tl.fromTo on "#card-sub" at start=2.0000 duration=0.4000
- `index.html`:68 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
