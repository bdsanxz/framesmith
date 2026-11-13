# Fidelity report: projects-folder-open → main

- translated: 5
- approximated (with deltas): 26
- refused: 5

## Refused (cause named; conversion refused)

- `compositions/desktop.html`:96 [tl.fromTo] property "color" (value "\"#ffffff\"") has no KDL motion property; refusing the call site
- `compositions/desktop.html`:98 [tl.fromTo] property "transformPerspective" (value "900") has no KDL motion property; refusing the call site
- `compositions/desktop.html`:100 [tl.to] ease "elastic.out(1, 0.5)" has no Framesmith curve; refusing the call site
- `compositions/desktop.html`:106 [tl.fromTo] position "3.0 + i * 0.15" is outside the numeric grammar (unbound identifier "i" in the numeric grammar (+ - * / parens, numbers, identifiers)); refusing the call site
- `compositions/desktop.html`:109 [tl.to] position "4.1 + i * 0.12" is outside the numeric grammar (unbound identifier "i" in the numeric grammar (+ - * / parens, numbers, identifiers)); refusing the call site

## Approximated (measured deltas)

- `compositions/desktop.html`:74 [placement] element #desktop-doc-1 has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/desktop.html`:75 [placement] element #desktop-doc-2 has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/desktop.html`:76 [placement] element #desktop-doc-3 has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/desktop.html`:80 [placement] element #desktop-label-pill has no own left/top; placed by flow approximation at (0, 792) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/desktop.html`:68 [container element] element #desktop-shadow is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/desktop.html`:70 [container element] element #desktop-folder is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/desktop.html`:71 [container element] element #desktop-back is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/desktop.html`:74 [container element] element #desktop-doc-1 is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/desktop.html`:74 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/desktop.html`:74 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/desktop.html`:74 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/desktop.html`:75 [container element] element #desktop-doc-2 is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/desktop.html`:75 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/desktop.html`:75 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/desktop.html`:75 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/desktop.html`:76 [container element] element #desktop-doc-3 is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/desktop.html`:76 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/desktop.html`:76 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/desktop.html`:76 [container element] element #? is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/desktop.html`:78 [container element] element #desktop-lid-wrap is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/desktop.html`:78 [container element] element #desktop-lid is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/desktop.html`:80 [container element] element #desktop-label is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/desktop.html`:94 [tl.to] tl.to on "#desktop-cursor" at start=2.0000 duration=0.0500 delta: transformOrigin "\"10% 6%\"" snapped to clip origin "left top" (KDL pivot is a 3x3 placement; pivot off by 0.100 x / 0.060 of element size); the clip origin pivots 3D turns only, while KDL 2D scale/rotation pivot at a built-in point, so the GSAP 2D pivot does not fully land
- `compositions/desktop.html`:95 [tl.to] tl.to on "#desktop-cursor" at start=2.2000 duration=0.0500 delta: transformOrigin "\"10% 6%\"" snapped to clip origin "left top" (KDL pivot is a 3x3 placement; pivot off by 0.100 x / 0.060 of element size); the clip origin pivots 3D turns only, while KDL 2D scale/rotation pivot at a built-in point, so the GSAP 2D pivot does not fully land
- `compositions/desktop.html`:99 [tl.fromTo] tl.fromTo on "#desktop-folder" at start=2.3300 duration=0.1200 delta: transformOrigin "\"50% 100%\"" snapped to clip origin "center bottom" (KDL pivot is a 3x3 placement; pivot off by 0.000 x / 0.000 of element size); the clip origin pivots 3D turns only, while KDL 2D scale/rotation pivot at a built-in point, so the GSAP 2D pivot does not fully land
- `index.html`:38 [frame rate] fps=30 emitted on the composition root delta: frame rate undeclared in source; HyperFrames default 30fps assumed

## Translated

- `compositions/desktop.html`:82 [asset svg] inline <svg> extracted to assets/data-desktop-cursor.svg (266 bytes; xmlns added: true)
- `compositions/desktop.html`:86 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/desktop.html`:91 [tl.fromTo] tl.fromTo on "#desktop-cursor" at start=0.1000 duration=1.8000
- `compositions/desktop.html`:112 [tl.to] tl.to on "#desktop-cursor" at start=3.4000 duration=1.2000
- `index.html`:45 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
