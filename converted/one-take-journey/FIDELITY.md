# Fidelity report: one-take-journey → main

- translated: 11
- approximated (with deltas): 22
- refused: 8

## Refused (cause named; conversion refused)

- `compositions/journey.html`:196 [tl.to] to vars object is not a literal; refusing the call site
- `compositions/journey.html`:207 [tl.to] to vars object is not a literal; refusing the call site
- `compositions/journey.html`:211 [tl.from] selector "#code .ch" names no converted clip
- `compositions/journey.html`:213 [tl.set] position "0.2 + i * 0.055" is outside the numeric grammar (unbound identifier "i" in the numeric grammar (+ - * / parens, numbers, identifiers)); refusing the call site
- `compositions/journey.html`:225 [tl.from] selector ".bar-val, .bar-name" is compound; only `#id` and `.class` land
- `compositions/journey.html`:230 [tl.fromTo] property "svgOrigin" (value "\"4850 760\"") has no KDL motion property; refusing the call site
- `compositions/journey.html`:241 [tl.to] property "f" (value "525") has no KDL motion property; refusing the call site
- `compositions/journey.html`:251 [tl.to] property "t" (value "DUR") has no KDL motion property; refusing the call site

## Approximated (measured deltas)

- `index.html`:42 [variable] variable "accent" ("color") carried with default "#6ee7c8" delta: fields [label] and type "color" not expressible in KDL variables; no render effect
- `index.html`:42 [variable] variable "canvas" ("color") carried with default "#0f1115" delta: fields [label] and type "color" not expressible in KDL variables; no render effect
- `compositions/journey.html`:112 [placement] element #wire has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/journey.html`:115 [placement] element #wave-g has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/journey.html`:115 [placement] element #wave has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/journey.html`:126 [placement] element #bars has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/journey.html`:134 [placement] element #strip has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/journey.html`:141 [placement] element #ticks has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/journey.html`:109 [container element] element #world is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/journey.html`:112 [container element] element #wire is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/journey.html`:115 [container element] element #wave-g is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/journey.html`:115 [container element] element #wave is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/journey.html`:120 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/journey.html`:121 [container element] element #code is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/journey.html`:126 [container element] element #bars is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/journey.html`:134 [container element] element #strip is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/journey.html`:141 [container element] element #ruler is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/journey.html`:141 [container element] element #ticks is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/journey.html`:161 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/journey.html`:177 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/journey.html`:182 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `index.html`:59 [frame rate] fps=30 emitted on the composition root delta: frame rate undeclared in source; HyperFrames default 30fps assumed

## Translated

- `compositions/journey.html`:111 [asset svg] inline <svg> extracted to assets/data-wire-svg.svg (528 bytes; xmlns added: true)
- `compositions/journey.html`:146 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/journey.html`:214 [tl.set] tl.set on "#caret" at start=2.8200 duration=0.5000
- `compositions/journey.html`:217 [tl.to] tl.to on chip at start=3.4000 duration=0.3000
- `compositions/journey.html`:223 [tl.from] tl.from on "#chart-title" at start=5.7200 duration=0.5000
- `compositions/journey.html`:224 [tl.from] tl.from on ".bar-col" at start=6.5600 duration=0.4500
- `compositions/journey.html`:232 [tl.to] tl.to on "#wave-g" at start=9.4100 duration=0.8000
- `compositions/journey.html`:238 [tl.to] tl.to on ".frame-cell" at start=11.4700 duration=0.0800
- `compositions/journey.html`:244 [tl.from] tl.from on "#file" at start=13.0700 duration=0.5000
- `compositions/journey.html`:250 [tl.to] tl.to on "#playhead" at start=0.0000 duration=17.5000
- `index.html`:74 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
