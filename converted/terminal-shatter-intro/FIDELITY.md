# Fidelity report: terminal-shatter-intro → main

- translated: 8
- approximated (with deltas): 26
- refused: 3

## Refused (cause named; conversion refused)

- `compositions/terminal-type.html`:78 [tl.set] position "0.5 + i * 0.06" is outside the numeric grammar (unbound identifier "i" in the numeric grammar (+ - * / parens, numbers, identifiers)); refusing the call site
- `compositions/terminal-shatter.html`:110 [tl.fromTo] position "rand() * 0.12" is outside the numeric grammar (unbound identifier "rand" in the numeric grammar (+ - * / parens, numbers, identifiers)); refusing the call site
- `compositions/kinetic-title.html`:60 [tl.fromTo] position "at" is outside the numeric grammar (unbound identifier "at" in the numeric grammar (+ - * / parens, numbers, identifiers)); refusing the call site

## Approximated (measured deltas)

- `compositions/terminal-type.html`:55 [placement] element #terminal-type-window has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/terminal-type.html`:61 [placement] element #terminal-type-typed has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/terminal-type.html`:61 [placement] element #terminal-type-cursor has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/terminal-type.html`:55 [container element] element #terminal-type-window is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/terminal-type.html`:57 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/terminal-type.html`:58 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/terminal-type.html`:58 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/terminal-type.html`:58 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/terminal-type.html`:61 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/terminal-type.html`:61 [container element] element #terminal-type-typed is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/terminal-type.html`:61 [container element] element #terminal-type-cursor is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/terminal-shatter.html`:62 [placement] element #terminal-shatter-source has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/terminal-shatter.html`:72 [placement] element #terminal-shatter-flash has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/terminal-shatter.html`:62 [container element] element #terminal-shatter-source is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/terminal-shatter.html`:63 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/terminal-shatter.html`:64 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/terminal-shatter.html`:65 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/terminal-shatter.html`:65 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/terminal-shatter.html`:65 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/terminal-shatter.html`:68 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/terminal-shatter.html`:68 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/kinetic-title.html`:45 [placement] element #kinetic-title-l1 has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/kinetic-title.html`:49 [placement] element #kinetic-title-l2 has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/kinetic-title.html`:45 [container element] element #kinetic-title-l1 is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/kinetic-title.html`:49 [container element] element #kinetic-title-l2 is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `index.html`:46 [frame rate] fps=30 emitted on the composition root delta: frame rate undeclared in source; HyperFrames default 30fps assumed

## Translated

- `compositions/terminal-type.html`:66 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/terminal-type.html`:68 [tl.fromTo] tl.fromTo on "#terminal-type-window" at start=0.0000 duration=0.5000
- `compositions/terminal-type.html`:83 [tl.to] tl.to on "#terminal-type-cursor" at start=0.0000 duration=0.2500
- `compositions/terminal-shatter.html`:86 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/terminal-shatter.html`:119 [tl.fromTo] tl.fromTo on flash at start=4.0000 duration=0.2500
- `compositions/kinetic-title.html`:55 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/kinetic-title.html`:63 [tl.fromTo] tl.fromTo on ["#kinetic-title-l1", "#kinetic-title-l2"] at start=6.8000 duration=1.2000
- `index.html`:59 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
