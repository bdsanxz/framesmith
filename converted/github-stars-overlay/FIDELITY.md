# Fidelity report: github-stars-overlay → main

- translated: 5
- approximated (with deltas): 10
- refused: 2

## Refused (cause named; conversion refused)

- `compositions/stat.html`:78 [tl.fromTo] property "v" (value "TARGET") has no KDL motion property; refusing the call site
- `compositions/stat.html`:81 [tl.fromTo] target "[\"#stat-label\", \"#stat-date\"]" is not a DOM selector ("[\"#stat-label\", \"#stat-date\"]" is runtime state); naming the call site instead of guessing a clip

## Approximated (measured deltas)

- `index.html`:39 [background] no concrete background found on html/body/#root rules delta: background defaulted to #000000
- `compositions/stat.html`:57 [placement] element #stat-star has no own left/top; placed by flow approximation at (192, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/stat.html`:59 [placement] element #stat-text has no own left/top; placed by flow approximation at (192, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/stat.html`:60 [placement] element #stat-num has no own left/top; placed by flow approximation at (192, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/stat.html`:61 [placement] element #stat-label has no own left/top; placed by flow approximation at (192, 130) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/stat.html`:62 [placement] element #stat-date has no own left/top; placed by flow approximation at (192, 166) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/stat.html`:55 [container element] element #stat-card is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/stat.html`:57 [container element] element #stat-star is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/stat.html`:59 [container element] element #stat-text is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `index.html`:39 [frame rate] fps=30 emitted on the composition root delta: frame rate undeclared in source; HyperFrames default 30fps assumed

## Translated

- `compositions/stat.html`:70 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/stat.html`:72 [tl.fromTo] tl.fromTo on "#stat-card" at start=0.2000 duration=0.5000
- `compositions/stat.html`:74 [tl.fromTo] tl.fromTo on "#stat-star" at start=0.4500 duration=0.3500
- `compositions/stat.html`:83 [tl.to] tl.to on "#stat-card" at start=5.5000 duration=0.4000
- `index.html`:46 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
