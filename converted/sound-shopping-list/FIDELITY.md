# Fidelity report: sound-shopping-list → main

- translated: 12
- approximated (with deltas): 24
- refused: 4

## Refused (cause named; conversion refused)

- `compositions/terminal.html`:66 [tl.set] position "0.3 + (i * 1.05) / cmd.length" is outside the numeric grammar (unsupported token '.' in the numeric grammar (+ - * / parens, numbers, identifiers)); refusing the call site
- `compositions/demos.html`:76 [tl.fromTo] property "skewX" (value "0") has no KDL motion property; refusing the call site
- `compositions/demos.html`:88 [tl.set] position "4.23 + (i * 1.0) / word.length" is outside the numeric grammar (unsupported token '.' in the numeric grammar (+ - * / parens, numbers, identifiers)); refusing the call site
- `compositions/demos.html`:92 [tl.fromTo] property "color" (value "\"#ffffff\"") has no KDL motion property; refusing the call site

## Approximated (measured deltas)

- `compositions/terminal.html`:51 [placement] element #terminal-cmd has no own left/top; placed by flow approximation at (160, 300) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/terminal.html`:51 [placement] element #terminal-typed has no own left/top; placed by flow approximation at (160, 300) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/terminal.html`:52 [placement] element #terminal-note has no own left/top; placed by flow approximation at (160, 300) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/terminal.html`:50 [container element] element #terminal-box is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/terminal.html`:51 [container element] element #terminal-cmd is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/terminal.html`:51 [container element] element #terminal-typed is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/demos.html`:59 [placement] element #demos-whoosh has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/demos.html`:59 [placement] element #demos-whoosh-w has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/demos.html`:61 [placement] element #demos-click has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/demos.html`:62 [placement] element #demos-click-btn has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/demos.html`:66 [placement] element #demos-typing has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/demos.html`:66 [placement] element #demos-typing-w has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/demos.html`:68 [placement] element #demos-impact has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/demos.html`:68 [placement] element #demos-impact-w has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/demos.html`:70 [placement] element #demos-riser has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/demos.html`:70 [placement] element #demos-riser-w has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/demos.html`:59 [container element] element #demos-whoosh is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/demos.html`:61 [container element] element #demos-click is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/demos.html`:62 [container element] element #demos-click-btn is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/demos.html`:66 [container element] element #demos-typing is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/demos.html`:66 [container element] element #demos-typing-w is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/demos.html`:70 [container element] element #demos-riser is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/end.html`:37 [placement] element #end-line has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `index.html`:61 [frame rate] fps=30 emitted on the composition root delta: frame rate undeclared in source; HyperFrames default 30fps assumed

## Translated

- `compositions/terminal.html`:57 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/terminal.html`:69 [tl.fromTo] tl.fromTo on "#terminal-note" at start=1.8500 duration=0.2000
- `compositions/demos.html`:63 [asset svg] inline <svg> extracted to assets/data-demos-cursor.svg (199 bytes; xmlns added: true)
- `compositions/demos.html`:74 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/demos.html`:78 [tl.fromTo] tl.fromTo on "#demos-cursor" at start=4.1200 duration=0.8000
- `compositions/demos.html`:79 [tl.to] tl.to on "#demos-click-btn" at start=5.0390 duration=0.0600
- `compositions/demos.html`:80 [tl.to] tl.to on "#demos-click-btn" at start=5.1100 duration=0.1800
- `compositions/demos.html`:91 [tl.fromTo] tl.fromTo on "#demos-impact" at start=8.0340 duration=1.2000
- `compositions/demos.html`:94 [tl.fromTo] tl.fromTo on "#demos-riser-w" at start=10.0310 duration=3.9690
- `compositions/end.html`:41 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/end.html`:42 [tl.fromTo] tl.fromTo on "#end-line" at start=14.0000 duration=0.2500
- `index.html`:84 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
