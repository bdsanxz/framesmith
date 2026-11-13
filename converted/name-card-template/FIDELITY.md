# Fidelity report: name-card-template → main

- translated: 9
- approximated (with deltas): 8
- refused: 0

## Approximated (measured deltas)

- `index.html`:24 [variable] variable "name" ("string") carried with default "Ada Lovelace" delta: fields [label, maxLength] and type "string" not expressible in KDL variables; no render effect
- `index.html`:24 [variable] variable "role" ("string") carried with default "First programmer" delta: fields [label] and type "string" not expressible in KDL variables; no render effect
- `index.html`:24 [variable] variable "accent" ("color") carried with default "#7c5cff" delta: fields [label] and type "color" not expressible in KDL variables; no render effect
- `compositions/card.html`:54 [placement] element #card-hello has no own left/top; placed by flow approximation at (260, 380) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/card.html`:56 [placement] element #card-name has no own left/top; placed by flow approximation at (260, 438) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/card.html`:57 [placement] element #card-role has no own left/top; placed by flow approximation at (260, 612) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/card.html`:53 [container element] element #card-text is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `index.html`:46 [frame rate] fps=30 emitted on the composition root delta: frame rate undeclared in source; HyperFrames default 30fps assumed

## Translated

- `compositions/card.html`:56 [variable text] text from render-time variable "name"; default baked as var(--name)
- `compositions/card.html`:57 [variable text] text from render-time variable "role"; default baked as var(--role)
- `compositions/card.html`:62 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/card.html`:64 [tl.fromTo] tl.fromTo on "#card-bar" at start=0.2000 duration=0.6000
- `compositions/card.html`:66 [tl.fromTo] tl.fromTo on "#card-hello" at start=0.4000 duration=0.5000
- `compositions/card.html`:68 [tl.fromTo] tl.fromTo on "#card-name" at start=0.5500 duration=0.7000
- `compositions/card.html`:70 [tl.fromTo] tl.fromTo on "#card-role" at start=0.9000 duration=0.5000
- `compositions/card.html`:72 [tl.fromTo] tl.fromTo on "#card-text" at start=1.0000 duration=4.0000
- `index.html`:53 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
