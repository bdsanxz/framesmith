# Support matrix and diagnostic codes

Status: v0 (Sprint 1). Sources: proposal `s01-first-render` (CAP-006, CAP-007, the deferred-behaviour table); project rule that an unsupported feature is reported by `check` and refused by `render`.

## Kinds

| Kind | Meaning | Effect on the exit code |
|---|---|---|
| error | The project is written wrongly. | 1 |
| unsupported | A feature Framesmith knows about and does not build in this version. | 2, unless there is also an error |
| unknown | A name in Framesmith's own namespace that it does not recognise at all. | 2, unless there is also an error |
| warning | Worth knowing; the render proceeds. | none |

Every diagnostic has a code, a kind, a file and line, one sentence of description, and a remedy. `check` lists them all in one run. `render` prints the same list and starts nothing. There is no option to render anyway.

## Supported in this version

One composition; whole-number frame rate; written or worked-out length; clips with absolute starts; text, CSS boxes, inline SVG shapes, local still images, project font files; a motion block with to, from, from-to and set tweens on `x`, `y`, `scale`, `rotation`, `opacity` with the ten easings; page scripts that do not animate through the browser's own animation features.

## Unsupported: recognised and refused

Every entry has a fixture in `fixtures/unsupported/<code>-<name>/` that `check` lists and `render` refuses with exit code 2.

| Code | Use | Seen in |
|---|---|---|
| FS-U001 | A `video` element. | HTML |
| FS-U002 | An `audio` element. | HTML |
| FS-U003 | A clip start written relative to another clip: `data-fs-start="after:<clip id>"`, optionally followed by `+` or `-` and a time. | HTML |
| FS-U004 | A frame rate written as a ratio, such as `30000/1001`, on the composition or with `--fps`. | HTML, command line |
| FS-U005 | A second composition in the project (in `index.html` or in another HTML file at the top of the project). | HTML |
| FS-U006 | A composition nested inside another. | HTML |
| FS-U007 | A declared variable: any attribute starting `data-fs-var-`. | HTML |
| FS-U008 | An animated image: a GIF with more than one frame, an animated PNG or WebP, an AVIF sequence, or an SVG file that animates itself. | image file |
| FS-U009 | An asset loaded from a network address (`http:`, `https:`, or starting `//`) by an image, stylesheet, script, font or CSS `url()`. Nothing is fetched. | HTML, CSS, run time |
| FS-U010 | A tween kind that is not built: `keyframes`, `path`. | motion block |
| FS-U011 | An easing outside the ten: the in, out and in-out forms of `quint`, `sine`, `expo`, `circ`, `back`, `elastic`, `bounce`; `steps`; `spring`; `cubic-bezier`. | motion block |
| FS-U012 | A property outside the five: `scale-x`, `scale-y`, `skew-x`, `skew-y`, `rotation-x`, `rotation-y`, `z`, `width`, `height`, `color`, `background-color`, `border-radius`, `blur`, `x-percent`, `y-percent`. | motion block |
| FS-U013 | A tween option that is not built: `repeat`, `repeat-delay`, `alternate`, `stagger`, `delay`, `label`; and the block fields `labels` and `defaults`. | motion block |
| FS-U014 | A CSS animation: an `animation` or `animation-name` property other than `none`, or an `@keyframes` rule. | CSS, style attribute |
| FS-U015 | A CSS transition: a `transition` property other than `none`, or a `transition-duration` other than zero. | CSS, style attribute |
| FS-U016 | The browser's own animation interface used by a script: `.animate(`, `new Animation`, `KeyframeEffect`, `getAnimations`, `document.timeline`. Also any animation found running in the page during a render. | scripts, run time |
| FS-U017 | The script interface for building motion: any use of the page global `framesmith`. | scripts, run time |
| FS-U018 | An embedded document: `iframe`, `embed`, `object`. | HTML |
| FS-U019 | An animation element inside inline SVG: `animate`, `animateTransform`, `animateMotion`, `set`. | HTML |
| FS-U020 | A target selector form that `check` cannot read (pseudo-classes, sibling combinators and others outside `motion-block.md`). | motion block |
| FS-U021 | `translate`, `rotate` or `scale` set by the author on an element that motion targets. | style attribute, run time |

Why animation through CSS or the browser's interface is refused: the browser runs it on its own clock, so a frame would show whatever moment the real clock had reached.

A use that only shows when the page runs (for example a script that builds its call from pieces) is caught by the page during `render`. The run stops with exit code 2 and the same code.

## Unknown: never ignored

| Code | Use |
|---|---|
| FS-X001 | An attribute starting `data-fs-` that is not in `composition.md` or in the table above. |
| FS-X002 | A field in a motion block or in a tween that is not in `motion-block.md` or in the table above. |
| FS-X003 | A tween `kind` that is not recognised. |
| FS-X004 | A property name that is not recognised. |
| FS-X005 | An easing name that is not recognised. |

Attributes outside the `data-fs-` namespace are ordinary HTML and are left alone.

## Errors

| Code | Problem |
|---|---|
| FS-E001 | The project path does not exist or is not a folder. |
| FS-E002 | The project has no `index.html`. |
| FS-E003 | No composition in `index.html`. |
| FS-E004 | The composition name is empty or has characters outside letters, digits, `-`, `_`. |
| FS-E005 | The size is missing. |
| FS-E006 | The size is not two whole numbers in range. |
| FS-E007 | The width or the height is odd. |
| FS-E008 | The frame rate is not a whole number from 1 to 240. |
| FS-E009 | The length is not a time. |
| FS-E010 | The written length is zero or less. |
| FS-E011 | No length is written, and there is no motion and no clip with its own end to work it out from. |
| FS-E020 | A clip has no start. |
| FS-E021 | A clip start is not a time. |
| FS-E022 | A clip start is negative. |
| FS-E023 | A clip duration is not a time. |
| FS-E024 | A written clip duration is zero or less. |
| FS-E025 | A clip has no duration, and the composition has no length for it to last until: none is written and none can be worked out. |
| FS-E026 | A clip attribute on an element that is not a clip. |
| FS-E027 | A clip outside the composition. |
| FS-E028 | A track that is not a whole number. |
| FS-E030 | A referenced local file does not exist. |
| FS-E031 | A reference points outside the project folder, or into the reserved path `/__framesmith__/`. |
| FS-E040 | A motion block is not valid JSON. |
| FS-E041 | A motion block or a tween has the wrong shape. |
| FS-E042 | A tween lacks a required field. |
| FS-E043 | A tween has a negative start or duration. |
| FS-E044 | A tween target matches no element in the composition. |
| FS-E045 | A tween value is not a number, or is out of range. |
| FS-E046 | A motion block names a composition that is not this one. |
| FS-E047 | A tween's fields disagree about its kind. |
| FS-E048 | A tween target is not a valid selector. |

## Warnings

| Code | Situation |
|---|---|
| FS-W001 | A clip starts at or after the end of the composition and can never show. |

## Run failures

These stop a command. They carry a code, a description and a remedy, but no file and line.

| Code | Failure | Exit |
|---|---|---|
| FS-R001 | No Chromium-based browser was found. | 1 |
| FS-R002 | `FRAMESMITH_BROWSER` points at something that does not exist or cannot be run. | 1 |
| FS-R003 | `ffmpeg` is not on the search path. | 1 |
| FS-R004 | A command-line value is not acceptable. | 1 |
| FS-R005 | The output location cannot be written. | 1 |
| FS-R006 | An image, a font or another file failed to load in the page. | 1 |
| FS-R007 | A page script threw an error. | 1 |
| FS-R008 | The page did not become ready within 30 seconds. | 1 |
| FS-R009 | The page clocks could not be replaced. | 1 |
| FS-R010 | The page disagrees with the files. | 1 |
| FS-R011 | The browser stopped or reported an error during the run. | 1 |
| FS-R012 | `ffmpeg` stopped or reported an error during the run. | 1 |
| FS-R013 | The page and Framesmith speak different protocol versions. | 1 |
| FS-R014 | `init`: the folder exists and is not empty. | 1 |
| FS-R015 | `init`: the folder cannot be created. | 1 |
| FS-R016 | An internal error. | 1 |
