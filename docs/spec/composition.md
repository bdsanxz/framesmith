# Composition attributes

Status: v0 (Sprint 1); Sprint 2 ticket T010 added the relative start without an offset (`after:<clip id>`, see "Relative start"); ticket T011 added the offset (`after:<clip id>+0.5`, `after:<clip id>-0.5`) and its two faults; ticket T012 added chains of relative starts and the fault for a circle (FS-E053). Sources: proposal `s01-first-render` (CAP-002, CAP-003, CAP-004); proposal `s02-timing-nesting-preview` (CAP-001, CAP-002 and point P1 for the relative start, which the proposal gives in the form `after:<clip id>`, optionally followed by `+` or `-` and a time; the facts taken from the research on the reference are that it, too, starts such a clip at the end of the named clip and accepts a plus or minus offset after it; the exact spelling of the offset, the sum and the faults below are our own; the research notes on the reference disagree on whether a circle of relative starts is rejected or worked around, and the proposal settles it as an error, which is what this specification does); product-owner decisions of 2026-10-11 on filled-in values (the second of them: fill in as the reference does); `PLAN.md` section 2 for our own names; for the rules under "Values that are not written" and the text rule under "Layout and background", what the reference's rendered output shows (ticket T052, measured on 2026-10-11 through its command line with small sample compositions of our own). The rules under "Sprint 2 rules for values that are not written (not yet built)" are Sprint 2 rules that nothing implements yet; their source is the sampling of 2026-10-11, ticket T002, recorded in `PROGRESS.md`.

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
| `data-fs-start` | yes | a time, 0 or more, or a relative start (`after:<clip id>`, with or without an offset) | When the clip appears, in seconds from the start of the composition; or the end of another clip, moved by an offset (see "Relative start"). |
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

### Sprint 2 rules for values that are not written (not yet built)

None of the four rules below is built. Each states the Sprint 2 proposal's rule, then what the reference did when it was sampled on 2026-10-11 (ticket T002; recorded in `PROGRESS.md`, small compositions of our own, rendered at 320 by 180, measured on the frames). Where the sampling differs from the rule, the rule is **not changed here**: the difference is recorded beside it as "open: the product owner decides", and no code that depends on it is written until that is settled.

**1. The length of a composition that holds nested compositions** (proposal `CAP-006`; ticket T040).
- Rule: with no length written, the length is the later of the end of the composition's own motion and the end of the last nested composition placed in it. A written length has authority over both. `check` marks a length worked out this way as filled in.
- Source: sampling of 2026-10-11, ticket T002, at 30 frames per second with no length written on the host. A nested composition written inline, from 2 s with a written duration of 3 s, in a host with no motion: 150 frames, and the nested picture is on screen from frame 60 to the last frame. The same with the host's own motion ending at 1 s: 150 frames. Host motion ending at 6 s with the nested composition ending at 5 s: 180 frames. Host motion ending at 1 s with a plain clip (not a composition) ending at 5 s: 30 frames, the clip is cut and never shows. A nested composition kept in its own file, with its duration also written on the host element: 150 frames. All of these agree with the rule.
- Open: the product owner decides. When the nested composition is kept in its own file and the host element writes no duration, the reference did not use the length written in that file. It used the end of the nested file's own motion, placed at the host element's start: a file with length 3 s and 1 s of motion, starting at 2 s, gave 90 frames (the nested picture cut at 3 s); length 4 s with 2.5 s of motion gave 135 frames; motion lasting the whole 3 s gave 150 frames; a file with no motion added nothing to the length. The rule above (and the ticket for a nested duration taken from the child's length, T039) assumes the child's written length is used. Whether Framesmith follows the written length or, like the reference, the end of the child's motion, is not decided.

**2. The smallest length** (proposal `CAP-006`; ticket T025).
- Rule: a length shorter than one sixtieth of a second, written or worked out, is an error that names the minimum. Exactly one sixtieth of a second is accepted and gives 1 frame at 60 frames per second.
- Source: sampling of 2026-10-11, ticket T002, rendered at 60 frames per second. Written on the composition: 0.1 s gave 6 frames, 0.05 s gave 3, 0.02 s and 0.0167 s gave 2, and 0.016667 s, one sixtieth (0.016666666666666666), 0.016666 s, 0.0166 s, 0.01 s and 0.001 s all gave 1 frame. Every one of them was accepted, a video was made, and the reference's own gate command passed. A length worked out from a clip's duration (no motion) was accepted the same way, down to 0.01 s. A length worked out from the end of motion was accepted at 1 s down to 0.016667 s and at exactly one sixtieth (1 frame); at 0.016666 s, 0.0166 s, 0.01 s and 0.001 s the render was refused with a message that the composition has no length, without naming a minimum.
- Agrees: exactly one sixtieth of a second is accepted and gives 1 frame; a worked-out length from motion below one sixtieth is refused.
- Open: the product owner decides. A written length below one sixtieth, and a length worked out from a clip's duration below one sixtieth, are not refused by the reference; they give 1 frame (the frame-count minimum of `frame-time.md`). The rule above makes them errors. The reference's refusal below one sixtieth does not name the minimum, and the rule requires that it does.

**3. The start of a clip that follows a clip with no end of its own** (proposal `CAP-001`; ticket T013).
- Rule: a clip that follows a still image with no duration starts at the image's start plus 3 s (plus or minus any offset). A clip that follows a clip that lasts to the end of the composition starts at the composition's end and can never show (the Sprint 1 warning for a clip that never shows applies). A clip that follows a clip whose end cannot be worked out is an error that names both clips.
- Source: sampling of 2026-10-11, ticket T002, at 30 frames per second, followers lasting 1 s. Followed clip from 1 s for 2 s (control): followers with no offset, `+ 0.5` and `- 0.5` first showed on frames 90, 105 and 75 (3 s, 3.5 s, 2.5 s). Followed still image from 1 s with no duration, length 10 s: frames 120, 135 and 105 (4 s, 4.5 s, 3.5 s). Both agree with the rule.
- Open: the product owner decides. Followed clip that is not an image, from 1 s with no duration (length 10 s written, or length 6 s worked out from motion): the followers first showed on frames 30, 45 and 15 (1 s, 1.5 s, 0.5 s). The reference starts the follower at the followed clip's start plus the offset, so the follower shows; it does not start at the composition's end and it is not an error. With no length written and a timeline that holds only a zero-length tween at 0, the same followers appeared at the same frames and the video was 2.5 s long (the length came from the followers' own ends); there was no error for an end that cannot be worked out. The rule above states the opposite for both cases.

**4. The track kind of a clip with none written** (proposal `CAP-007`; ticket T015).
- Rule: a clip with no written kind, a still image included, has kind graphics. `check` and `timeline` mark it as filled in. The kind never changes the picture. The attribute that carries a written kind, and its four values (video, audio, captions, graphics), are named by T015.
- Source: sampling of 2026-10-11, ticket T002. In the reference's timeline listing, an `img` clip with no kind written, a `div` clip with none and an `h1` clip with none were all listed under graphics. Clips with a kind written were listed under that kind, an `img` clip included (kind video written on an `img`: video). All nine sample clips were drawn on every frame whatever their kind. Agrees. The reference's listing carries no mark saying the kind was filled in; that mark is ours.

### Relative start

A clip may start when another clip of the same composition ends, moved earlier or later by an offset. This is written in the clip's `data-fs-start`, in place of a time.

**The written form.** `after:<id>` or `after:<id><offset>`, where `<id>` is the `id` of the clip to follow and `<offset>` is a sign and a time.
- The word `after` is written in lower case and is followed at once by a colon. `After:intro` is not a relative start: it is read as a time and refused as one (FS-E021).
- Spaces at the ends of the value are ignored, as for a time. Spaces between the colon and the id are ignored too: `after: intro` is `after:intro`.
- Nothing after the colon (`after:`) is not a relative start and not a time: FS-E021.

**The offset.** An offset is a sign, `+` or `-`, and a time in seconds.
- The time is read by the reader that reads every written time (`frame-time.md`, "Times"): digits with an optional decimal point, at most nine digits after the point, no exponent, no unit. `0.5`, `2`, `.5` and `0.011111111` are times; `1.2.3`, `1.`, `1e3`, `0,5` and `0.5s` are not.
- Spaces may stand before the sign (between the id and the sign) and between the sign and the time, and nowhere else: `after:intro+0.5`, `after:intro +0.5`, `after:intro+ 0.5` and `after:intro + 0.5` are the same offset. A space inside the time (`0. 5`) is not allowed.
- The sign is required: `after:intro 0.5` has none and names no clip (see below, FS-E050). A second sign is not allowed: `after:intro+-0.5`, `after:intro--0.5` and `after:intro++1` are offsets that are not times (FS-E051). The time has no sign of its own.
- `+0` and `-0` (also `+0.0`) are offsets of zero: the clip starts exactly where `after:intro` would start it, and the form as written is kept and printed as written.
- The offset is in seconds only. It is never read as frames or as another unit.

**How the text after `after:` is read.** The text is taken with the spaces at its ends and the spaces after the colon removed. It is read in four steps, in this order, and the first that applies decides. The rule is the same on the host and in the page.
1. **The text is exactly the `id` of a clip** of the composition (compared exactly, upper and lower case included). It is the plain form and names that clip, whatever characters the id holds. `after:scene-2` follows the clip `scene-2`; `after:lower-third` follows `lower-third`; `after:intro-0.5` follows a clip whose id is `intro-0.5`. This step comes first, so an id that holds a sign is never taken for an offset while a clip has it.
2. **Find where an offset would begin.** If the text holds a `+`, an offset begins at the first `+`. Otherwise, if it holds a `-`, an offset begins at the last `-` (only the last sign can begin a time, so `lower-third-0.5` is `lower-third` and `-0.5`). Otherwise the text holds no sign: go to step 4. Signs that stand directly before that sign (only spaces between them) belong to the offset as well. The *offset text* is everything from there to the end; the *base* is the text before it, spaces at its end removed; the *tail* is the offset text without the signs and spaces it begins with. For `intro - 0.5` the base is `intro`, the offset text is `- 0.5` and the tail is `0.5`; for `intro+-0.5` the offset text is `+-0.5`; for `intro+1e-3` the offset text is `+1e-3`. A `+` always begins the offset, so an id that holds a `+` can be followed only in the plain form.
3. **The text is of the offset form** when the base is not empty and the offset text holds a `+` or its tail is a number (digits and decimal points only, at least one digit, so `1.2.3` is a number here and `2b` is not). A `-` followed by anything else is a hyphen in a name, not an offset. For a text of the offset form:
   - The offset is read as described under "The offset". If it is not a time, FS-E051 (below) is reported, showing the offset as written.
   - The base is then looked up as in step 1: it must be exactly the `id` of a clip. If it is not, FS-E050 (below) is reported, with the words for an offset. If it is the clip itself, FS-E049. If it names a nested composition, FS-U003 (below). When the offset is not a time and the base is also missing, both faults are listed, each once.
   - Otherwise the start is resolved as set out under "The resolved start".
4. **Any other text is an id that no clip has**, FS-E050. So `after:lower-third` with no such clip is "no clip with the id `lower-third`", and so are `after:+1`, `after:-1`, `after:a b`, `after:intro-abc`, `after:intro-2b` and `after:intro-0.5s` (a hyphen followed by something that is not a number is a name), `after:intro-` and `after:intro 0.5`.

So `after:scene-2` is the plain form when a clip `scene-2` exists; when none does, it is read as the clip `scene` and the offset `-2`; and when no clip `scene` exists either, it is FS-E050, and the message says that both readings were tried. Likewise `after:intro-0.5` and `after:intro+0.5` are an offset from `#intro` when no clip has the whole text as its id (the case of ticket criterion 1).

**Finding the named clip.** The clip to follow is found among the clips of the same composition, wherever they stand in the file: before the clip that names it, after it, or inside another clip. Its `id` attribute must be exactly the id written. Only a clip can be named: an element with that id that is not marked `data-fs-clip`, or an id that no element has, names nothing (FS-E050 below). When two clips of one composition have the same id, which HTML does not allow, the first in the document is the one named.

**The resolved start.** The follower starts at the end of the clip it names, plus the offset. The end of the named clip is its start plus its written duration; when the named clip's own start is relative, its start is the one it resolves to (see "Chains" below). The start is measured from the start of the composition, like every start, whatever clip the follower or the named clip stands inside. The sum is exact (`frame-time.md`), a fraction and never a binary number: `0.1 + 0.2` is exactly `0.3`, one third of a frame written with nine decimals is added as written, and nothing is rounded on the way from the text to the frame. From there the follower is an ordinary clip: the first frame on which it shows is the frame grid rule applied to its start (`frame-time.md`, "Clip window in frames"). The clip it names is not changed. A resolved start of exactly 0 is accepted; a start before 0 is an error (FS-E052 below) and is never moved to 0.

Examples, at 30 frames per second:
- `#intro` starts at 1 s and lasts 2 s, so it shows from frame 30 to frame 89. A clip `data-fs-start="after:intro"` starts at 3 s and first shows on frame 90.
- `data-fs-start="after:intro+0.5"` starts at 3.5 s and first shows on frame 105. `after:intro-0.5` starts at 2.5 s and first shows on frame 75: it shows together with `#intro` on frames 75 to 89.
- If `#intro` starts at 1.03 s and lasts 2 s, its end is 3.03 s, which falls between frame 90 (3 s) and frame 91 (3.0333 s): the follower first shows on frame 91.
- At 10 frames per second, `#a` starts at 0 and lasts 0.1 s and `after:a+0.2` starts at exactly 0.3 s, which is exactly 3 frames: frame 3. (In binary floating point `0.1 + 0.2` is a hair above `0.3`, so a floating-point sum rounded up as it stands would give frame 4; with the tolerance of the frame grid rule it gives frame 3 too.)
- At 10 frames per second, `#a` starts at 0 and lasts 0.4 s and `after:a+0.0001` starts at exactly 0.4001 s, which is exactly 4.001 frames. That is one thousandth of a frame above frame 4, inside the tolerance of the frame grid rule (0.001 included), so the clip first shows on frame 4. In binary floating point the product is a hair above 4.001, outside the tolerance, and a floating-point sum would give frame 5 even with the tolerance applied; the exact sum does not.

**Chains.** The named clip may itself start relative to a third clip, to any depth: when `#c` follows `#b` and `#b` follows `#a`, the start of `#b` is worked out first, and `#c` starts at the end of `#b` as `#b` now starts. Each clip names exactly one clip, so the clips a clip depends on form a line, and the starts are worked out from the clip that has a time, down the line.
- The order in which the clips are written in the file does not matter, and neither does the order in which a clip is named and written: a follower may stand before, after or inside the clips it follows, and the six orders of three clips give the same three starts.
- An offset may stand on every link, with its own sign: each link adds its own offset to the end of the clip before it. A start is the exact sum of the time that begins the line, the written durations and the offsets (`frame-time.md`), and the frame grid rule is applied once, to that exact sum, never link by link.
- Changing the duration of the first clip by a time moves every clip that follows it, directly or not, by exactly that time. Changing the start of the first clip does the same.
- Only the end of the named clip is used, so only a clip that is named needs a written duration; a clip that is named and has none is a form that waits (see "Forms that wait"), also when the clip is a link of a chain.
- The depth is not limited. A line of 200 clips is worked out like a line of three, and the working out does not depend on how deep the line is.
- A start of a link that resolves to a time before zero is FS-E052 for that link, as for any clip.

Example, at 30 frames per second. `#a` starts at 1 s and lasts 2 s; `#b` is `after:a` and lasts 1.5 s; `#c` is `after:b+0.5`. `#a` ends at 3 s, so `#b` starts at 3 s (frame 90) and ends at 4.5 s, so `#c` starts at 5 s (frame 150). When `#a` lasts 3 s instead, `#b` starts at 4 s (frame 120) and `#c` at 6 s (frame 180): each moved by 1 s, which is 30 frames. The fixture `relative-chain` is this chain.

**Cycles.** When the relative starts of two or more clips depend on each other in a circle (`#a` follows `#b` and `#b` follows `#a`; or `#a` follows `#b`, `#b` follows `#c` and `#c` follows `#a`), none of their starts can be worked out, however the offsets are written. This is one error with its own code, FS-E053, whatever the number of clips in the circle (two or more; a clip that names itself is the circle of one clip, FS-E049, as before). `check` exits 1 and `render` refuses the project and starts nothing, as for any error.
- **One error for a circle, naming every clip of it.** The message lists the clips of the circle and, for each, the clip it starts after.
- **Order of the clips in the message.** The circle is listed from the clip written first in the file, then the clip it starts after, then the clip that one starts after, and so on until the circle closes. The same circle gives the same text however the file is arranged around it and in whatever order it is looked at.
- **The line.** The error is reported on the line of the `data-fs-start` attribute of the clip the message begins with, the one written first in the file.
- **Two circles in one composition** are two errors, one for each, each naming only its own clips.
- **A clip that follows a clip of a circle, or follows a clip that follows one, without being in the circle,** is not named in the error and has no error of its own: its start cannot be known until the circle is broken, and the circle is listed once. The same holds for a clip that follows a clip whose own start has any other fault (FS-E049, FS-E050, FS-E051, FS-E052, FS-E021, FS-E022, or a refusal FS-U003 of the clip itself): the fault is listed with the clip that has it, and the follower is not listed again. A follower of such a clip has no start while the fault stays, and the project is refused.
- **A clip of a circle whose offset is not a time** still names the clip it follows, so it is a link of the circle and is named in FS-E053; its FS-E051 is listed as well, because that fault does not depend on the circle.
- **A clip of a circle that has no written duration** is named in FS-E053 like any other. The circle is the only fault listed for it: the starts of the circle are not known, so the refusal for a clip with no written duration is not reached.

Examples. With `#a` after `#b` (line 5) and `#b` after `#a` (line 6): FS-E053 on line 5, naming `#a` and `#b`. With three clips written in the order `#one`, `#two`, `#three`, where `#one` follows `#three`, `#two` follows `#one` and `#three` follows `#two`: FS-E053 on the line of `#one`, naming `#one`, `#three` and `#two` in that order; written in another order the message starts at whichever of the three is written first and lists the same circle round from there.

**What `check` and `info` print.** For each clip whose start is relative, and for no other clip, one line below the facts: `clip #outro  start 3 s (after:intro)`, and with an offset `clip #b  start 3.5 s (after:intro+0.5)`. It gives the clip's name as the other `clip` lines do, the resolved start in seconds written as every time is written, and the form the author wrote, as written (spaces at its ends removed, spaces inside kept). `check` and `info` print this line from one function, so the two never differ. A project whose starts are all times prints nothing more than it printed before. `timeline` prints the resolved start in its table as it does for every clip (the written form beside it is a later ticket).

**The faults, each an error with its own code.** Each is listed with the file, the line of the `data-fs-start` attribute, one sentence and a remedy, and is reported with every other fault in the same run (`support-matrix.md`). The remedy of each says how to fix it. `{label}` is the clip named as `Names that label things in messages` says.

| Code | The fault | Message |
|---|---|---|
| FS-E049 | The clip names itself. | `The clip #b starts after itself ("after:b"), so its start cannot be worked out.` |
| FS-E050 | No clip of the composition has the id. This covers an id that no element has, and an id that belongs to an element that is not a clip. | `The clip #b starts after "intro", but no clip with the id "intro" is in this composition; only an element marked data-fs-clip can be named.` With an offset that is a time, and a text that could also be an id: `The clip #b starts after "scene-2", but no clip with the id "scene-2" is in this composition; read as the clip "scene" and the offset "-2", no clip with the id "scene" is in it either; only an element marked data-fs-clip can be named.` With an offset: `The clip #b starts after "intro" (the part of "after:intro+0.5" before the offset), but no clip with the id "intro" is in this composition; only an element marked data-fs-clip can be named.` |
| FS-E051 | The offset after the id is not a time. The message shows the offset and the whole start as written. | `The offset "+abc" in the start "after:intro+abc" of the clip #b is not a time in seconds.` |
| FS-E052 | The start resolves to a time before zero. The message gives the resolved time and the end it was worked out from. | `The clip #b starts at -0.5 s ("after:intro-1.5", and #intro ends at 1 s), before the start of the composition.` |
| FS-E053 | The starts of two or more clips depend on each other in a circle. One error for the circle, on the line of the clip written first in it, naming every clip of it and, for each, the clip it starts after. | `The starts of #a and #b depend on each other in a circle: #a starts after #b and #b starts after #a, so none of them can be worked out.` With three clips: `The starts of #a, #b and #c depend on each other in a circle: #a starts after #b, #b starts after #c and #c starts after #a, so none of them can be worked out.` |

The remedies: for FS-E051, `Write the offset as + or - and a number of seconds with at most nine decimals and no unit, for example after:intro+0.5, or write the start as a time in seconds.`; for FS-E052, `Use an offset of -1 s or later, or write the start as a time of 0 or more in seconds. The clip is not moved to 0.` (the number is the end of the named clip); the others as before. The remedy of FS-E053 is `Break the circle: write the start of one of the clips of the circle as a time in seconds, or make it start after a clip that is not in the circle.`

What each doubtful text prints, with a clip `#intro` that starts at 0 and lasts 1 s and no other clip:

| Written | Result |
|---|---|
| `after:intro+0.5`, `after:intro-0.5`, `after: intro + 0.5` | Resolved: 1.5 s, 0.5 s, 1.5 s. |
| `after:intro+0`, `after:intro-0` | Resolved: 1 s. |
| `after:intro+abc`, `after:intro+`, `after:intro+1.2.3`, `after:intro+0.5s`, `after:intro+1e3`, `after:intro+1e-3` | FS-E051 with the offset as written. |
| `after:intro-1.2.3`, `after:intro-1.`, `after:intro-0.1234567891` | FS-E051 (a number after a minus is an offset). |
| `after:intro+-0.5`, `after:intro--0.5`, `after:intro++1`, `after:intro-+1` | FS-E051 (a second sign). |
| `after:intro-abc`, `after:intro-2b`, `after:intro-0.5s`, `after:intro-`, `after:intro 0.5` | FS-E050: no clip with that whole text as its id. |
| `after:intro-2` | Resolved: -1 s, so FS-E052 (the offset is a time; the clip is not moved to 0). |
| `after:scene-2` (no clip `scene-2`, no clip `scene`) | FS-E050, saying both readings were tried. |
| `after:+1`, `after:-1` | FS-E050 (no id before the sign). |
| `after:nothing+1` | FS-E050 for the base `nothing`. |
| `after:nothing+abc` | FS-E051 and FS-E050, both. |
| `after:intro+1` on the clip `#intro` | FS-E049. |

**Forms that wait.** Each is recognised and refused as unsupported, FS-U003 (exit code 2, `support-matrix.md`), with a message that says which form it is; none is ever rendered as if it were another. An offset after the id does not change which form it is: each of these is refused the same with or without one.
- A start that names a clip with no written duration: a clip that lasts to the end of the composition, or a still image whose three seconds were filled in. This holds also when the named clip is itself a link of a chain: the clip is worked out like any other, and the clip that follows it is refused. Ticket T013, which also waits for a decision on the length of such a clip.
- A start that names a nested composition. Ticket T038. (A nested composition is itself refused today, FS-U006.)

A relative start of a form that waits is not worked out; the clip has no start until the form is built or rewritten as a time, and a clip that follows it gets no error of its own (see "Cycles"). A relative start of the forms above whose offset is not a time still gets FS-E051, because that fault does not depend on the named clip.

### How a clip is hidden

A hidden clip keeps its place in the layout: it is made invisible, not removed, so nothing around it moves when it appears or disappears. Every clip is hidden from the moment the page starts to load, and is shown only on frames inside its window. Elements inside a hidden clip are hidden with it.

## Content

- Text, CSS-styled boxes, inline SVG shapes, and `img` elements that point at local still images.
- Text uses fonts installed on the machine, or a font file inside the project declared with `@font-face`.
- The first frame is captured only after every image and every declared font has finished loading.

## Names that label things in messages

- A clip is named by its `id` as `#id` when it has one; otherwise by its position among the clips, as `number 2 <div>` (counting from 1 in document order).

## Not in this version

Everything in `support-matrix.md` is recognised and refused. Any other attribute whose name starts with `data-fs-` is reported as unknown and also refused. Four such names are listed by name in `support-matrix.md` ("Names that are deliberately not accepted"): an end time and a layer on a clip, the timeline-registry opt-out and a thumbnail on a composition.
