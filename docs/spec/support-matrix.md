# Support matrix and diagnostic codes

Status: v0 (Sprint 1), being revised for Sprint 2; only the parts named here are revised so far. Ticket T005 added the "Waits for" column of the table of refusals and the table "Names that are deliberately not accepted"; every code and its meaning is as in Sprint 1. The four refusals that Sprint 2 lifts (FS-U003, FS-U004, FS-U005, FS-U006) are marked "This sprint"; the ticket named in each row removes the row when it lifts the refusal. Ticket T010 lifted the plain relative start and added the errors FS-E049 and FS-E050. Ticket T011 lifted the offset (`after:<clip id>` followed by `+` or `-` and a time) and added the errors FS-E051 and FS-E052. Ticket T012 lifted chains (a start that names a clip whose own start is relative) and added the error FS-E053 for starts that form a circle, so FS-U003 is narrowed to the relative forms that are not built yet, waits for tickets T013 and T038, and is no longer raised for an offset or for a chain. Sources: proposal `s01-first-render` (CAP-006, CAP-007, the deferred-behaviour table); proposal `s02-timing-nesting-preview` (CAP-037, CAP-008, its deferred-behaviour table, points P2, P3 and P4); the sprint tags of `ROADMAP.md` and the sprint list of `PLAN.md`; project rule that an unsupported feature is reported by `check` and refused by `render`.

## Kinds

| Kind | Meaning | Effect on the exit code |
|---|---|---|
| error | The project is written wrongly. | 1 |
| unsupported | A feature Framesmith knows about and does not build in this version. | 2, unless there is also an error |
| unknown | A name in Framesmith's own namespace that it does not recognise at all. | 2, unless there is also an error |
| warning | Worth knowing; the render proceeds. | none |

Every diagnostic has a code, a kind, a file and line, one sentence of description, and a remedy. `check` lists them all in one run. `render` prints the same list and starts nothing. There is no option to render anyway.

## Supported in this version

One composition; whole-number frame rate; written or worked-out length; clips with absolute starts, and clips that start when another clip of the composition ends, with or without an offset (`data-fs-start="after:<clip id>"`, `data-fs-start="after:<clip id>+0.5"`, `composition.md`, "Relative start"), when the named clip has a written duration, and also when the named clip's own start is relative (a chain of any depth; starts that form a circle are an error, FS-E053); text, CSS boxes, inline SVG shapes, local still images, project font files; a motion block with to, from, from-to and set tweens on `x`, `y`, `scale`, `rotation`, `opacity` with the ten easings; page scripts that do not animate through the browser's own animation features.

## Unsupported: recognised and refused

Every entry has a fixture in `fixtures/unsupported/<code>-<name>/` that `check` lists and `render` refuses with exit code 2.

| Code | Use | Seen in | Waits for |
|---|---|---|---|
| FS-U001 | A `video` element. | HTML | Sprint 3 |
| FS-U002 | An `audio` element. | HTML | Sprint 3 |
| FS-U003 | A clip start written relative to another clip in a form that is not built yet: naming a clip with no written duration (also when that clip is itself a link of a chain), or naming a nested composition. The plain form `after:<clip id>`, the form with an offset (`after:<clip id>` followed by `+` or `-` and a time) and chains of them, in which a clip names a clip whose own start is relative, are supported when each named clip has a written duration. | HTML | This sprint (ticket T013 for a named clip with no written duration, T038 for a nested composition) |
| FS-U004 | A frame rate written as a ratio, such as `30000/1001`, on the composition or with `--fps`. | HTML, command line | This sprint (ticket T021) |
| FS-U005 | A second composition in the project (in `index.html` or in another HTML file at the top of the project). | HTML | This sprint (ticket T026) |
| FS-U006 | A composition nested inside another. | HTML | This sprint (ticket T034) |
| FS-U007 | A declared variable: any attribute starting `data-fs-var-`. | HTML | Sprint 8 |
| FS-U008 | An animated image: a GIF with more than one frame, an animated PNG or WebP, an AVIF sequence, or an SVG file that animates itself. | image file | Sprint 3 |
| FS-U009 | An asset loaded from a network address (`http:`, `https:`, or starting `//`) by an image, stylesheet, script, font or CSS `url()`. Nothing is fetched. | HTML, CSS, run time | Sprint 3 |
| FS-U010 | A tween kind that is not built: `keyframes`, `path`. | motion block | Sprint 4 |
| FS-U011 | An easing outside the ten: the in, out and in-out forms of `quint`, `sine`, `expo`, `circ`, `back`, `elastic`, `bounce`; `steps`; `spring`; `cubic-bezier`. | motion block | Sprint 4 |
| FS-U012 | A property outside the five: `scale-x`, `scale-y`, `skew-x`, `skew-y`, `rotation-x`, `rotation-y`, `z`, `width`, `height`, `color`, `background-color`, `border-radius`, `blur`, `x-percent`, `y-percent`. | motion block | Sprint 4 |
| FS-U013 | A tween option that is not built: `repeat`, `repeat-delay`, `alternate`, `stagger`, `delay`, `label`; and the block fields `labels` and `defaults`. | motion block | Sprint 4 |
| FS-U014 | A CSS animation: an `animation` or `animation-name` property other than `none`, or an `@keyframes` rule. | CSS, style attribute | Sprint 4 |
| FS-U015 | A CSS transition: a `transition` property other than `none`, or a `transition-duration` other than zero. | CSS, style attribute | Sprint 4 |
| FS-U016 | The browser's own animation interface used by a script: `.animate(`, `new Animation`, `KeyframeEffect`, `getAnimations`, `document.timeline`. Also any animation found running in the page during a render. | scripts, run time | Sprint 4 |
| FS-U017 | The script interface for building motion: any use of the page global `framesmith`. | scripts, run time | Sprint 4 |
| FS-U018 | An embedded document: `iframe`, `embed`, `object`. | HTML | Not yet assigned (see note 2) |
| FS-U019 | An animation element inside inline SVG: `animate`, `animateTransform`, `animateMotion`, `set`. | HTML | Not yet assigned (see note 2) |
| FS-U020 | A target selector form that `check` cannot read (pseudo-classes, sibling combinators and others outside `motion-block.md`). | motion block | Not yet assigned (see note 2) |
| FS-U021 | `translate`, `rotate` or `scale` set by the author on an element that motion targets. | style attribute, run time | Not yet assigned (see note 2) |

The "Waits for" column names the sprint that is to build the feature, taken from the sprint tags of `ROADMAP.md` and the sprint list of `PLAN.md`. A row that says "This sprint" is a refusal that Sprint 2 lifts; it is still in force today and is removed, with its fixture, by the ticket it names. FS-U003 names two tickets because the plain relative start (T010), the offset (T011) and chains (T012) are already lifted and the forms left are lifted one by one; its row is removed, with its fixture, by the last of the two, and each of the two narrows the row to what is still refused. The fixture of the row is a follower of a clip with no written duration, a form that waits for T013. Every other row stays a refusal for the whole of Sprint 2, and its fixtures stay.

1. Where a row names a sprint, the feature is listed under that sprint: video, audio and animated images in Sprint 3 (roadmap line 3.1.1b); assets from a network address in Sprint 3, which the Sprint 1 proposal grouped with media and the plan describes as remote assets fetched when a project is compiled; variables in Sprint 8; the wider tween vocabulary, properties, easings and options, CSS animations and transitions, the browser's animation interface and the script interface in Sprint 4 (roadmap lines 2.5.1e, 2.5.2, 2.5.3, and 2.6.2 for transitions). Two rows are only partly settled by those lines: the plan names animated GIF alone for Sprint 3, so the other animated forms in FS-U008 follow the roadmap's wording "animated images"; and roadmap line 5.1.3c splits scripts and fonts loaded from the network across Sprints 3, 8 and 11 without saying which part is which, so FS-U009 follows the Sprint 1 proposal.
2. "Not yet assigned" means that no line of the roadmap or the plan names the feature, so no sprint is given here and none is guessed. The product owner decides. FS-U018 is near roadmap line 2.5.10 (a sandboxed frame for external films, Sprint 11), which is a different feature. FS-U019, FS-U020 and FS-U021 have no matching line at all.

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

### Names that are deliberately not accepted

Four things an author who knows another tool may well write in Framesmith's own `data-fs-` namespace are not accepted in Sprint 2. Each is reported as unknown (FS-X001, kind unknown, naming the attribute, with file and line), `check` refuses the project, and `render` exits with code 2 and starts nothing. Each has a fixture in `fixtures/unsupported/` whose folder name starts `fs-x001-`. The four names below are the ones an author would most plausibly write beside `data-fs-start`, `data-fs-track`, `data-fs-duration` and `data-fs-fps`; they are not a promise that Framesmith will accept these exact spellings later, and the sprint that builds a feature settles its name.

| Name | Written on | What it would be | Reported as | Waits for |
|---|---|---|---|---|
| `data-fs-end` | a clip | The time a clip ends, written as an end instead of a duration. | FS-X001, unknown | Not targeted |
| `data-fs-layer` | a clip | A stacking number for the clip, written beside or instead of its track. | FS-X001, unknown | Not targeted |
| `data-fs-no-timeline` | the composition | A mark that keeps the composition out of the registry of timelines that scripts fill. | FS-X001, unknown | Sprint 4 |
| `data-fs-thumbnail` | the composition | The picture a timeline shows for the composition. | FS-X001, unknown | Sprint 14 |

Why each is left out:

- **End and layer.** A clip's end is always its start plus its duration, and its place among other clips is its track. Other tools keep an end time and a layer only so that their own older projects still open; the plan excludes inputs kept for that reason (`PLAN.md`, section 5), so there is one way to write a clip's timing and no second way that could disagree with it. The product owner closed both as "not targeted" on 2026-10-11 (roadmap lines 2.3.5 and 15.6a, proposal point P2). A project that writes either is refused, never rendered as if the attribute were absent.
- **The timeline-registry opt-out.** The mark only has a meaning once a script can put motion into the registry, which is the script interface of Sprint 4. A composition with no motion needs no mark: it already renders without waiting for anything. Moved to Sprint 4 by the product owner on 2026-10-11 (roadmap line 2.2.4, proposal point P3).
- **The thumbnail.** It belongs to the timeline of the Studio editor, which is Sprint 14 (roadmap line 2.2.5, proposal deferred-behaviour table).

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
| FS-E049 | A clip's relative start names the clip itself. |
| FS-E050 | A clip's relative start names an id that no clip of the composition has: no element has it, or the element that has it is not a clip. |
| FS-E051 | The offset of a clip's relative start (the part after the id, `+0.5` in `after:intro+0.5`) is not a time. |
| FS-E052 | A clip's relative start, with its offset, resolves to a time before zero. The clip is not moved to zero. |
| FS-E053 | The relative starts of two or more clips depend on each other in a circle. One error for the circle, naming every clip of it; none of their starts can be worked out. |

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
