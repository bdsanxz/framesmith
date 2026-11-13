# Fidelity report: design-export-to-video → main

- translated: 11
- approximated (with deltas): 18
- refused: 2

## Refused (cause named; conversion refused)

- `compositions/grid.html`:51 [tl.fromTo] position "0.3 + i * 0.1" is outside the numeric grammar (unbound identifier "i" in the numeric grammar (+ - * / parens, numbers, identifiers)); refusing the call site
- `compositions/counter.html`:53 [tl.fromTo] property "v" (value "20") has no KDL motion property; refusing the call site

## Approximated (measured deltas)

- `index.html`:54 [placement] element #bed-wrap has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `index.html`:54 [placement] element #bed has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `index.html`:54 [container element] element #bed-wrap is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/grid.html`:43 [placement] element #grid-cells has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/grid.html`:43 [container element] element #grid-cells is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/title.html`:43 [placement] element #title-text has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/title.html`:43 [placement] element #title-inner has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/title.html`:43 [container element] element #title-text is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/counter.html`:45 [placement] element #counter-row has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/counter.html`:45 [placement] element #counter-label has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/counter.html`:45 [placement] element #counter-num has no own left/top; placed by flow approximation at (0, 64) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/counter.html`:45 [container element] element #counter-row is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/endcard.html`:43 [placement] element #endcard-stack has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/endcard.html`:43 [placement] element #endcard-mark has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/endcard.html`:43 [placement] element #endcard-word has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/endcard.html`:43 [container element] element #endcard-stack is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/endcard.html`:43 [container element] element #endcard-mark is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `index.html`:51 [frame rate] fps=30 emitted on the composition root delta: frame rate undeclared in source; HyperFrames default 30fps assumed

## Translated

- `index.html`:54 [asset img] element #bed data: URI decoded to assets/data-bed.svg (534 bytes, percent-encoded)
- `compositions/grid.html`:47 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/title.html`:47 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/title.html`:49 [tl.fromTo] tl.fromTo on "#title-text" at start=3.1000 duration=0.5000
- `compositions/title.html`:51 [tl.fromTo] tl.fromTo on "#title-inner" at start=3.6000 duration=2.4000
- `compositions/counter.html`:49 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/endcard.html`:47 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/endcard.html`:49 [tl.fromTo] tl.fromTo on "#endcard-mark" at start=9.3000 duration=0.6000
- `compositions/endcard.html`:51 [tl.fromTo] tl.fromTo on "#endcard-word" at start=9.5000 duration=0.5000
- `compositions/endcard.html`:53 [tl.fromTo] tl.fromTo on "#endcard-stack" at start=9.0000 duration=3.0000
- `index.html`:67 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
