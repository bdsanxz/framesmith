# Fidelity report: apollo-launch-archive → main

- translated: 21
- approximated (with deltas): 52
- refused: 12

## Refused (cause named; conversion refused)

- `compositions/map.html`:90 [tl.fromTo] position "t + i / cps" is an expression; refusing the call site
- `compositions/map.html`:94 [tl.set] property "filter" (value "\"blur(6px)\"") has no KDL motion property; refusing the call site
- `compositions/map.html`:96 [tl.to] property "filter" (value "\"blur(0px)\"") has no KDL motion property; refusing the call site
- `compositions/map.html`:100 [tl.fromTo] selector "#mp-pin svg" is compound; only `#id` and `.class` land
- `compositions/map.html`:82 [typeOn] typeOn selector "sel" is not a string literal; refusing the call site
- `compositions/launch.html`:62 [asset img] element #ln-a-video references remote asset "data:image/svg+xml,%3C!--s01.mp4--%3E%3Csvg xmlns%3D'http://www.w3.org/2000/svg' width%3D'1600' height%3D'900' viewBox%3D'0 0 1600 900'%3E%3Cdefs%3E%3Cpattern id%3D'h' width%3D'56' height%3D'56' patternUnits%3D'userSpaceOnUse' patternTransform%3D'rotate(45)'%3E%3Crect width%3D'2' height%3D'56' fill%3D'%23808080' fill-opacity%3D'0.1'/%3E%3C/pattern%3E%3C/defs%3E%3Crect width%3D'1600' height%3D'900' fill%3D'url(%23h)'/%3E%3Crect x%3D'96' y%3D'720' width%3D'478' height%3D'64' rx%3D'6' fill%3D'%2315181b' fill-opacity%3D'0.78'/%3E%3Ctext x%3D'126' y%3D'763' font-family%3D'monospace' font-size%3D'32' fill%3D'%23e6edf3'%3EVIDEO: footage/s01.mp4%3C/text%3E%3C/svg%3E"; only project-local files are copied
- `compositions/launch.html`:65 [asset img] element #ln-b-video references remote asset "data:image/svg+xml,%3C!--s02.mp4--%3E%3Csvg xmlns%3D'http://www.w3.org/2000/svg' width%3D'1600' height%3D'900' viewBox%3D'0 0 1600 900'%3E%3Cdefs%3E%3Cpattern id%3D'h' width%3D'56' height%3D'56' patternUnits%3D'userSpaceOnUse' patternTransform%3D'rotate(45)'%3E%3Crect width%3D'2' height%3D'56' fill%3D'%23808080' fill-opacity%3D'0.1'/%3E%3C/pattern%3E%3C/defs%3E%3Crect width%3D'1600' height%3D'900' fill%3D'url(%23h)'/%3E%3Crect x%3D'96' y%3D'720' width%3D'478' height%3D'64' rx%3D'6' fill%3D'%2315181b' fill-opacity%3D'0.78'/%3E%3Ctext x%3D'126' y%3D'763' font-family%3D'monospace' font-size%3D'32' fill%3D'%23e6edf3'%3EVIDEO: footage/s02.mp4%3C/text%3E%3C/svg%3E"; only project-local files are copied
- `compositions/stat.html`:61 [asset img] element #? references remote asset "data:image/svg+xml,%3C!--moon.jpg--%3E%3Csvg xmlns%3D'http://www.w3.org/2000/svg' width%3D'1600' height%3D'900' viewBox%3D'0 0 1600 900'%3E%3Cdefs%3E%3Cpattern id%3D'h' width%3D'56' height%3D'56' patternUnits%3D'userSpaceOnUse' patternTransform%3D'rotate(45)'%3E%3Crect width%3D'2' height%3D'56' fill%3D'%23808080' fill-opacity%3D'0.1'/%3E%3C/pattern%3E%3C/defs%3E%3Crect width%3D'1600' height%3D'900' fill%3D'url(%23h)'/%3E%3Crect x%3D'96' y%3D'720' width%3D'744' height%3D'64' rx%3D'6' fill%3D'%2315181b' fill-opacity%3D'0.78'/%3E%3Ctext x%3D'126' y%3D'763' font-family%3D'monospace' font-size%3D'32' fill%3D'%23e6edf3'%3EPHOTO: assets/generated/src/moon.jpg%3C/text%3E%3C/svg%3E"; only project-local files are copied
- `compositions/stat.html`:82 [tl.fromTo] property "v" (value "384400") has no KDL motion property; refusing the call site
- `compositions/statement.html`:55 [asset img] element #? references remote asset "data:image/svg+xml,%3C!--bootprint.jpg--%3E%3Csvg xmlns%3D'http://www.w3.org/2000/svg' width%3D'1600' height%3D'900' viewBox%3D'0 0 1600 900'%3E%3Cdefs%3E%3Cpattern id%3D'h' width%3D'56' height%3D'56' patternUnits%3D'userSpaceOnUse' patternTransform%3D'rotate(45)'%3E%3Crect width%3D'2' height%3D'56' fill%3D'%23808080' fill-opacity%3D'0.1'/%3E%3C/pattern%3E%3C/defs%3E%3Crect width%3D'1600' height%3D'900' fill%3D'url(%23h)'/%3E%3Crect x%3D'96' y%3D'720' width%3D'839' height%3D'64' rx%3D'6' fill%3D'%2315181b' fill-opacity%3D'0.78'/%3E%3Ctext x%3D'126' y%3D'763' font-family%3D'monospace' font-size%3D'32' fill%3D'%23e6edf3'%3EPHOTO: assets/generated/src/bootprint.jpg%3C/text%3E%3C/svg%3E"; only project-local files are copied
- `compositions/statement.html`:68 [tl.fromTo] property "transformOrigin" (value "\"0 50%\"") has no KDL motion property; refusing the call site
- `compositions/statement.html`:71 [tl.fromTo] position "t" is an expression; refusing the call site

## Approximated (measured deltas)

- `compositions/map.html`:63 [placement] element #mp-cam has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/map.html`:64 [placement] element #mp-map has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/map.html`:65 [placement] element #mp-hl has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/map.html`:71 [placement] element #mp-vig has no own left/top; placed by flow approximation at (1480, 903) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/map.html`:75 [placement] element #mp-name has no own left/top; placed by flow approximation at (120, 452) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/map.html`:76 [placement] element #mp-sub has no own left/top; placed by flow approximation at (120, 452) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/map.html`:63 [container element] element #mp-cam is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/map.html`:66 [container element] element #mp-pin is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/map.html`:67 [container element] element #mp-glow is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/map.html`:71 [container element] element #mp-vig is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/map.html`:74 [container element] element #mp-label is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/map.html`:102 [typeOn] typeOn "#mp-name" at 2.7000 for 12 chars at 14.0000 chars/s lands as split="chars" typewriter on the text clip (authored reveal 0.8571s) delta: reveal paced over the clip window 4.3300s gives 2.7714 chars/s instead of 14.0000 chars/s
- `compositions/map.html`:104 [typeOn] typeOn "#mp-sub" at 3.4500 for 22 chars at 40.0000 chars/s lands as split="chars" typewriter on the text clip (authored reveal 0.5500s) delta: reveal paced over the clip window 4.3300s gives 5.0808 chars/s instead of 40.0000 chars/s
- `compositions/launch.html`:61 [placement] element #ln-a has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/launch.html`:62 [placement] element #ln-a-video has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/launch.html`:64 [placement] element #ln-b has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/launch.html`:65 [placement] element #ln-b-video has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/launch.html`:68 [placement] element #ln-tint has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/launch.html`:69 [placement] element #ln-vig has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/launch.html`:61 [container element] element #ln-a is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/launch.html`:64 [container element] element #ln-b is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/launch.html`:69 [container element] element #ln-vig is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/stat.html`:61 [placement] element #st-bg has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/stat.html`:62 [placement] element #st-tint has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/stat.html`:63 [placement] element #st-shade has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/stat.html`:66 [placement] element #st-kicker has no own left/top; placed by flow approximation at (140, 250) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/stat.html`:67 [placement] element #st-num has no own left/top; placed by flow approximation at (140, 250) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/stat.html`:68 [placement] element #st-unit has no own left/top; placed by flow approximation at (140, 250) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/stat.html`:69 [placement] element #st-source has no own left/top; placed by flow approximation at (140, 250) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/stat.html`:61 [container element] element #st-bg is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/stat.html`:63 [container element] element #st-shade is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/stat.html`:65 [container element] element #st-block is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/statement.html`:55 [placement] element #sm-bg has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/statement.html`:56 [placement] element #sm-tint has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/statement.html`:58 [placement] element #sm-tick has no own left/top; placed by flow approximation at (140, 420) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/statement.html`:59 [placement] element #sm-line has no own left/top; placed by flow approximation at (140, 420) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/statement.html`:59 [placement] element #sm-w1 has no own left/top; placed by flow approximation at (140, 420) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/statement.html`:59 [placement] element #sm-w2 has no own left/top; placed by flow approximation at (140, 420) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/statement.html`:59 [placement] element #sm-w3 has no own left/top; placed by flow approximation at (140, 420) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/statement.html`:59 [placement] element #sm-w4 has no own left/top; placed by flow approximation at (140, 420) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/statement.html`:59 [placement] element #sm-w5 has no own left/top; placed by flow approximation at (140, 420) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/statement.html`:59 [placement] element #sm-w6 has no own left/top; placed by flow approximation at (140, 420) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/statement.html`:62 [placement] element #sm-black has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/statement.html`:55 [container element] element #sm-bg is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/statement.html`:57 [container element] element #sm-block is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/statement.html`:59 [container element] element #sm-line is a layout container; carried as an empty text anchor for motion targeting delta: container background and flex layout not painted; anchor carries no pixels
- `compositions/subs.html`:40 [placement] element #sub-1 has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/subs.html`:41 [placement] element #sub-2 has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/subs.html`:42 [placement] element #sub-3 has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/subs.html`:43 [placement] element #sub-4 has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `compositions/subs.html`:44 [placement] element #sub-5 has no own left/top; placed by flow approximation at (0, 0) delta: CSS flow layout not simulated; absolute offsets derived from the container origin plus stacked sibling heights
- `index.html`:74 [frame rate] fps=30 emitted on the composition root delta: frame rate undeclared in source; HyperFrames default 30fps assumed

## Translated

- `compositions/map.html`:81 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/map.html`:95 [tl.to] tl.to on "#mp-cam" at start=0.0000 duration=0.5000
- `compositions/map.html`:97 [tl.fromTo] tl.fromTo on "#mp-cam" at start=0.2000 duration=2.3000
- `compositions/map.html`:98 [tl.fromTo] tl.fromTo on "#mp-hl" at start=1.6000 duration=0.4000
- `compositions/map.html`:101 [tl.fromTo] tl.fromTo on "#mp-glow" at start=2.6000 duration=0.4000
- `compositions/map.html`:103 [tl.fromTo] tl.fromTo on "#mp-lead" at start=3.4000 duration=0.3500
- `compositions/launch.html`:74 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/launch.html`:76 [tl.fromTo] tl.fromTo on "#ln-a" at start=4.3300 duration=1.9500
- `compositions/launch.html`:77 [tl.fromTo] tl.fromTo on "#ln-b" at start=6.2800 duration=2.1400
- `compositions/launch.html`:79 [tl.fromTo] tl.fromTo on "#ln-tag" at start=6.3300 duration=0.2200
- `compositions/stat.html`:74 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/stat.html`:76 [tl.fromTo] tl.fromTo on "#st-bg" at start=8.4200 duration=3.8900
- `compositions/stat.html`:77 [tl.fromTo] tl.fromTo on "#st-kicker" at start=8.5200 duration=0.3500
- `compositions/stat.html`:78 [tl.fromTo] tl.fromTo on "#st-num" at start=9.0200 duration=0.2000
- `compositions/stat.html`:84 [tl.fromTo] tl.fromTo on "#st-unit" at start=11.2700 duration=0.3500
- `compositions/stat.html`:85 [tl.fromTo] tl.fromTo on "#st-source" at start=11.5200 duration=0.3000
- `compositions/statement.html`:66 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `compositions/statement.html`:67 [tl.fromTo] tl.fromTo on "#sm-bg" at start=12.3100 duration=3.9900
- `compositions/statement.html`:74 [tl.fromTo] tl.fromTo on "#sm-black" at start=15.5600 duration=0.7000
- `compositions/subs.html`:48 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
- `index.html`:102 [gsap.timeline] paused timeline construction; KDL time is driven by the renderer seek
