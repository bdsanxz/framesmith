# Composition attributes

Status: v0 (Sprint 1). Sources: proposal `s01-first-render` (CAP-002, CAP-003, CAP-004); product-owner decisions of 2026-10-11 on filled-in values (the second of them: fill in as the reference does); `PLAN.md` section 2 for our own names; for the rules under "Values that are not written" and the text rule under "Layout and background", what the reference's rendered output shows (ticket T052, measured on 2026-10-11 through its command line with small sample compositions of our own).

## Project

- A project is a folder. Its entry page is `index.html` in that folder.
- Assets (images, stylesheets, scripts, fonts) are files inside the project folder. A reference that starts with `/` is read from the project folder's top level.
- The folder `out/` inside a project is where `render` writes by default. Framesmith never reads it.

## The composition element

Exactly one element in `index.html` is the composition. It carries:

| Attribute | Required | Value | Meaning |
|---|---|---|---|
| `data-fs-composition` | yes | a name: letters, digits, `-`, `_` | Marks the element and names the composition. |
| `data-fs-size` | yes | `WIDTHxHEIGHT`, two whole numbers | Picture size in pixels. Both must be even, from 2 to 16384. |
| `data-fs-fps` | no | a whole number from 1 to 240 | Frame rate. 30 when not written. |
| `data-fs-duration` | no | a time greater than 0 | Length in seconds. Worked out when not written. |

- A written length always has authority.
- When no length is written it is worked out; see "Values that are not written".
- `check` marks a rate or a length that was not written as "filled in".

## Layout and background

- The page is laid out in a viewport of exactly the declared size. One CSS pixel is one output pixel, whatever screen the machine has.
- Framesmith adds a first, lowest-priority style sheet to the page: the page background is black, the body has no margin, text is drawn with geometric precision, and the composition element is a positioned box of the declared size that clips what overflows it. Any rule the author writes wins over these.
- Geometric precision (`text-rendering: geometricPrecision` on the root, inherited by everything) means glyphs are drawn from their outlines as designed. Without it the browser lets the operating system thicken the strokes on some machines, so the same project would give heavier text there. An author who wants the browser's usual text writes their own `text-rendering` value on the root or on any element.
- A composition with no content therefore renders black, or the background the author sets.

## Clips

A clip is an element inside the composition that shows only for part of the time.

| Attribute | Required | Value | Meaning |
|---|---|---|---|
| `data-fs-clip` | yes | none | Marks the element as a clip. |
| `data-fs-start` | yes | a time, 0 or more | When the clip appears, in seconds from the start of the composition. |
| `data-fs-duration` | no | a time greater than 0 | How long it shows. Worked out when not written. |
| `data-fs-track` | no | a whole number | Recorded. It does not change the picture in this version. |

- The window is start-inclusive and end-exclusive, on the frame grid (see `frame-time.md`).
- A clip inside another clip shows only while every enclosing clip also shows.
- A clip whose window runs past the end of the composition is cut at the end.
- When no duration is written it is worked out; see "Values that are not written".
- `check` marks a duration that was not written as "filled in".
- `data-fs-start`, `data-fs-duration` or `data-fs-track` on an element that is not a clip and not the composition is an error.

### Values that are not written

The length of the composition is settled first, then the clips that last until it.

1. **The end of the motion** is the latest end time of any tween. A `set` tween ends at its own time. A motion block whose latest end is 0 (for example, only `set` tweens at 0) counts as no motion.
2. **A clip has its own end** when its duration is written, or when it is an `img` clip: an `img` clip with no duration lasts 3 seconds.
3. **The length**, when not written:
   - with motion, it is the end of the motion. Clips do not lengthen it: a clip that ends later is cut at the end, like any clip that runs past the end.
   - with no motion, it is the latest end among the clips that have their own end.
   - with no motion and no clip that has its own end, there is nothing to work it out from. That is an error.
4. **A clip that is not an image and has no duration** shows from its start to the end of the composition, whether the length was written or worked out. Its duration is the length minus its start.
   - If it starts at or after the end, it never shows. Its duration is then 0, and it gets the same warning as any clip that starts at or after the end.
   - If the length cannot be worked out (the last case of rule 3), the error names each such clip and says to write a duration on it or a length on the composition.

Examples, at 30 frames per second, with one move from 0.5 s to 2.5 s as the only motion:

| Written | Length | The clip shows |
|---|---|---|
| no length; a `div` clip from 1 s, no duration | 2.5 s, 75 frames | from frame 30 to the last frame |
| length 5 s; a `div` clip from 1 s, no duration | 5 s, 150 frames | from frame 30 to the last frame |
| no length; a `div` clip from 0.5 s for 4 s | 2.5 s, 75 frames | from frame 15 to the last frame (cut) |
| no length; an `img` clip from 1 s, no duration | 2.5 s, 75 frames | from frame 30 to the last frame (cut) |
| no length; a `div` clip from 3 s, no duration | 2.5 s, 75 frames | never (warning) |

And with no motion at all:

| Written | Length | The clip shows |
|---|---|---|
| no length; an `img` clip from 1 s, no duration | 4 s, 120 frames | from frame 30 to the last frame |
| no length; a `div` clip from 1 s for 2 s | 3 s, 90 frames | from frame 30 to the last frame |
| no length; a `div` clip from 0 s, no duration, and another from 1 s for 2 s | 3 s, 90 frames | the first on every frame |
| length 5 s; a `div` clip from 1 s, no duration | 5 s, 150 frames | from frame 30 to the last frame |
| no length; a `div` clip from 1 s, no duration | error | nothing is rendered |

### How a clip is hidden

A hidden clip keeps its place in the layout: it is made invisible, not removed, so nothing around it moves when it appears or disappears. Every clip is hidden from the moment the page starts to load, and is shown only on frames inside its window. Elements inside a hidden clip are hidden with it.

## Content

- Text, CSS-styled boxes, inline SVG shapes, and `img` elements that point at local still images.
- Text uses fonts installed on the machine, or a font file inside the project declared with `@font-face`.
- The first frame is captured only after every image and every declared font has finished loading.

## Names that label things in messages

- A clip is named by its `id` as `#id` when it has one; otherwise by its position among the clips, as `number 2 <div>` (counting from 1 in document order).

## Not in this version

Everything in `support-matrix.md` is recognised and refused. Any other attribute whose name starts with `data-fs-` is reported as unknown and also refused.
