# Fidelity report: text-behind-subject → main

- translated: 5
- approximated (with deltas): 9
- refused: 1

## Refused (cause named; conversion refused)

- `compositions/words.html`:42 [tl.fromTo] property "clipPath" (value "\"inset(0 0% 0 0)\"") has no KDL motion property; refusing the call site

## Approximated (measured deltas)

- `index.html`:59 [placement] element #plate-wrap has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `index.html`:60 [placement] element #plate has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `index.html`:66 [placement] element #cutout-wrap has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `index.html`:67 [placement] element #subject has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `index.html`:59 [container element] element #plate-wrap is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `index.html`:66 [container element] element #cutout-wrap is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/words.html`:36 [placement] element #words-drift has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/words.html`:36 [container element] element #words-drift is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `index.html`:57 [frame rate] fps=30 emitted on the composition root delta: frame rate undeclared in source; HyperFrames default 30fps assumed

## Translated

- `index.html`:60 [asset img] element #plate data: URI decoded to assets/data-plate.svg (547 bytes, percent-encoded)
- `index.html`:67 [asset img] element #subject data: URI decoded to assets/data-subject.svg (551 bytes, percent-encoded)
- `compositions/words.html`:40 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/words.html`:43 [tl.fromTo] tl.fromTo on "#words-drift" at start=0.4000 duration=5.6000
- `index.html`:72 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
