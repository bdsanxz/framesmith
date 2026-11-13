# Fidelity report: text-behind-subject → main

- translated: 3
- approximated (with deltas): 9
- refused: 3

## Refused (cause named; conversion refused)

- `index.html`:60 [asset img] element #plate references remote asset "data:image/svg+xml,%3C!--plate.webm--%3E%3Csvg xmlns%3D'http://www.w3.org/2000/svg' width%3D'1600' height%3D'900' viewBox%3D'0 0 1600 900'%3E%3Cdefs%3E%3Cpattern id%3D'h' width%3D'56' height%3D'56' patternUnits%3D'userSpaceOnUse' patternTransform%3D'rotate(45)'%3E%3Crect width%3D'2' height%3D'56' fill%3D'%23808080' fill-opacity%3D'0.1'/%3E%3C/pattern%3E%3C/defs%3E%3Crect width%3D'1600' height%3D'900' fill%3D'url(%23h)'/%3E%3Crect x%3D'96' y%3D'720' width%3D'649' height%3D'64' rx%3D'6' fill%3D'%2315181b' fill-opacity%3D'0.78'/%3E%3Ctext x%3D'126' y%3D'763' font-family%3D'monospace' font-size%3D'32' fill%3D'%23e6edf3'%3EVIDEO: assets/layers/plate.webm%3C/text%3E%3C/svg%3E"; only project-local files are copied
- `index.html`:67 [asset img] element #subject references remote asset "data:image/svg+xml,%3C!--subject.webm--%3E%3Csvg xmlns%3D'http://www.w3.org/2000/svg' width%3D'1600' height%3D'900' viewBox%3D'0 0 1600 900'%3E%3Cdefs%3E%3Cpattern id%3D'h' width%3D'56' height%3D'56' patternUnits%3D'userSpaceOnUse' patternTransform%3D'rotate(45)'%3E%3Crect width%3D'2' height%3D'56' fill%3D'%23808080' fill-opacity%3D'0.1'/%3E%3C/pattern%3E%3C/defs%3E%3Crect width%3D'1600' height%3D'900' fill%3D'url(%23h)'/%3E%3Crect x%3D'96' y%3D'720' width%3D'687' height%3D'64' rx%3D'6' fill%3D'%2315181b' fill-opacity%3D'0.78'/%3E%3Ctext x%3D'126' y%3D'763' font-family%3D'monospace' font-size%3D'32' fill%3D'%23e6edf3'%3EVIDEO: assets/layers/subject.webm%3C/text%3E%3C/svg%3E"; only project-local files are copied
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

- `compositions/words.html`:40 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/words.html`:43 [tl.fromTo] tl.fromTo on "#words-drift" at start=0.4000 duration=5.6000
- `index.html`:72 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
