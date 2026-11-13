# Fidelity report: vox-photo-parallax → main

- translated: 3
- approximated (with deltas): 19
- refused: 7

## Refused (cause named; conversion refused)

- `compositions/shot.html`:79 [asset img] element #? references remote asset "data:image/svg+xml,%3C!--plate.png--%3E%3Csvg xmlns%3D'http://www.w3.org/2000/svg' width%3D'1600' height%3D'900' viewBox%3D'0 0 1600 900'%3E%3Cdefs%3E%3Cpattern id%3D'h' width%3D'56' height%3D'56' patternUnits%3D'userSpaceOnUse' patternTransform%3D'rotate(45)'%3E%3Crect width%3D'2' height%3D'56' fill%3D'%23808080' fill-opacity%3D'0.1'/%3E%3C/pattern%3E%3C/defs%3E%3Crect width%3D'1600' height%3D'900' fill%3D'url(%23h)'/%3E%3Crect x%3D'96' y%3D'720' width%3D'820' height%3D'64' rx%3D'6' fill%3D'%2315181b' fill-opacity%3D'0.78'/%3E%3Ctext x%3D'126' y%3D'763' font-family%3D'monospace' font-size%3D'32' fill%3D'%23e6edf3'%3EPHOTO: assets/generated/layers/plate.png%3C/text%3E%3C/svg%3E"; only project-local files are copied
- `compositions/shot.html`:88 [asset img] element #? references remote asset "data:image/svg+xml,%3C!--plane-1-subject.png--%3E%3Csvg xmlns%3D'http://www.w3.org/2000/svg' width%3D'1600' height%3D'900' viewBox%3D'0 0 1600 900'%3E%3Cdefs%3E%3Cpattern id%3D'h' width%3D'56' height%3D'56' patternUnits%3D'userSpaceOnUse' patternTransform%3D'rotate(45)'%3E%3Crect width%3D'2' height%3D'56' fill%3D'%23808080' fill-opacity%3D'0.1'/%3E%3C/pattern%3E%3C/defs%3E%3Crect width%3D'1600' height%3D'900' fill%3D'url(%23h)'/%3E%3Crect x%3D'96' y%3D'720' width%3D'1010' height%3D'64' rx%3D'6' fill%3D'%2315181b' fill-opacity%3D'0.78'/%3E%3Ctext x%3D'126' y%3D'763' font-family%3D'monospace' font-size%3D'32' fill%3D'%23e6edf3'%3EPHOTO: assets/generated/layers/plane-1-subject.png%3C/text%3E%3C/svg%3E"; only project-local files are copied
- `compositions/shot.html`:90 [asset img] element #? references remote asset "data:image/svg+xml,%3C!--plane-2-near.png--%3E%3Csvg xmlns%3D'http://www.w3.org/2000/svg' width%3D'1600' height%3D'900' viewBox%3D'0 0 1600 900'%3E%3Cdefs%3E%3Cpattern id%3D'h' width%3D'56' height%3D'56' patternUnits%3D'userSpaceOnUse' patternTransform%3D'rotate(45)'%3E%3Crect width%3D'2' height%3D'56' fill%3D'%23808080' fill-opacity%3D'0.1'/%3E%3C/pattern%3E%3C/defs%3E%3Crect width%3D'1600' height%3D'900' fill%3D'url(%23h)'/%3E%3Crect x%3D'96' y%3D'720' width%3D'953' height%3D'64' rx%3D'6' fill%3D'%2315181b' fill-opacity%3D'0.78'/%3E%3Ctext x%3D'126' y%3D'763' font-family%3D'monospace' font-size%3D'32' fill%3D'%23e6edf3'%3EPHOTO: assets/generated/layers/plane-2-near.png%3C/text%3E%3C/svg%3E"; only project-local files are copied
- `compositions/shot.html`:95 [asset img] element #? references remote asset "data:image/svg+xml,%3C!--boulder.png--%3E%3Csvg xmlns%3D'http://www.w3.org/2000/svg' width%3D'1600' height%3D'900' viewBox%3D'0 0 1600 900'%3E%3Cdefs%3E%3Cpattern id%3D'h' width%3D'56' height%3D'56' patternUnits%3D'userSpaceOnUse' patternTransform%3D'rotate(45)'%3E%3Crect width%3D'2' height%3D'56' fill%3D'%23808080' fill-opacity%3D'0.1'/%3E%3C/pattern%3E%3C/defs%3E%3Crect width%3D'1600' height%3D'900' fill%3D'url(%23h)'/%3E%3Crect x%3D'96' y%3D'720' width%3D'725' height%3D'64' rx%3D'6' fill%3D'%2315181b' fill-opacity%3D'0.78'/%3E%3Ctext x%3D'126' y%3D'763' font-family%3D'monospace' font-size%3D'32' fill%3D'%23e6edf3'%3EPHOTO: assets/generated/boulder.png%3C/text%3E%3C/svg%3E"; only project-local files are copied
- `compositions/shot.html`:106 [tl.fromTo] property "--cam-x" (value "TRAVEL") has no KDL motion property; refusing the call site
- `compositions/shot.html`:108 [tl.fromTo] property "--dof" (value "\"0px\"") has no KDL motion property; refusing the call site
- `compositions/shot.html`:109 [tl.fromTo] property "--dof" (value "\"0px\"") has no KDL motion property; refusing the call site

## Approximated (measured deltas)

- `index.html`:23 [variable] variable "parallax" ("number") carried with default "1" delta: fields [label] and type "number" not expressible in KDL variables; no render effect
- `compositions/shot.html`:77 [placement] element #vp-cam has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/shot.html`:79 [placement] element #vp-plate has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/shot.html`:81 [placement] element #vp-titleplane has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/shot.html`:83 [placement] element #vp-kicker has no own left/top; placed by flow approximation at (150, 150) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/shot.html`:84 [placement] element #vp-head has no own left/top; placed by flow approximation at (150, 150) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/shot.html`:88 [placement] element #vp-subject has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/shot.html`:90 [placement] element #vp-near has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/shot.html`:92 [placement] element #vp-prop has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/shot.html`:77 [container element] element #vp-cam is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/shot.html`:79 [container element] element #vp-plate is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/shot.html`:81 [container element] element #vp-titleplane is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/shot.html`:82 [container element] element #vp-title is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/shot.html`:88 [container element] element #vp-subject is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/shot.html`:90 [container element] element #vp-near is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/shot.html`:92 [container element] element #vp-prop is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/shot.html`:93 [container element] element #vp-boulder is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/shot.html`:94 [container element] element #vp-boulder-shadow is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `index.html`:41 [frame rate] fps=30 emitted on the composition root delta: frame rate undeclared in source; HyperFrames default 30fps assumed

## Translated

- `compositions/shot.html`:104 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/shot.html`:111 [tl.fromTo] tl.fromTo on "#vp-title" at start=0.3000 duration=1.1000
- `index.html`:48 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
