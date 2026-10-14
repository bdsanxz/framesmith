# The comparison tool: a render against the reference's render

Status: in force from Sprint 1. Sources: `PLAN.md`, section 5.1 (the checks and their limits: frame count, frame rate, size and length identical; the picture frame by frame with `pixelmatch`, 99 % of pixels the same or better on average and in every scene; a frame under 99 % allowed only with a named cause) and section 5.4 (the reference is run only through its command line); decisions A3 (the reference lives in `tools/blackbox/` of the workspace root), A17 (the picture rule: "A frame under 99 % is listed; it is shown as excused with a named cause and fails the check without one. A scene is a frame range named in the fixture's record"), A18 (`pixelmatch` run with `bun`, a comparison tool and no dependency) and A29 of `DECISIONS.md`; `CLAUDE.md` rules 2 (what "equivalent" means; a feature is never rendered or reported wrong in silence), 3 (the reference is a black box: its output is compared and its code is not read) and 11 (FFmpeg is a yardstick only; a skipped comparison is not a passed one); the measured passes of the proofs recorded in `POC_PLAN.md`, experiment E9 (99.71 % and 99.69 % of pixels the same by `pixelmatch`); the published method of the comparison of pixels, named in section 4.2; `docs/dev/gates.md`, sections 5 and 11 (yardstick tests, and a skipped comparison is not a passed one). The tool is development only. It is not shipped and it is not a command of `framesmith`.

The tool is the program `parity` (crate `crates/parity`). It is given a Framesmith render and the reference's render of the same paired fixture, and it hands back one report with a pass or a fail for each check. This document fixes the checks, their settings and the words of the report. The tool's code follows it, and the tests of the crate hold the code to it.

## 1. What is compared

Two videos of one fixture, "ours" and "the reference's". Both are MP4 files whose first picture track holds H.264 or HEVC pictures, which the system decodes. In this slice the checks are:

| Check | Passes when |
|---|---|
| frame count | the two picture tracks hold the same number of pictures |
| frame rate | the two picture tracks say the same number of pictures a second |
| size | the pictures of the two are the same width and the same height |
| length | the two picture tracks play for the same time |
| picture | the average over all frames, and the result of every scene, are at 99 % or better (section 4) |
| events | every visible event of every marked element (it appears, disappears, starts or stops moving) happens in ours within 1 frame of the same event in the reference's (section 5) |
| tracking | the tracked element is within 1 pixel of the reference's on every frame; *not applicable* when the fixture marks none (section 5) |

Sound is the next slice's. The report names it as not compared (section 7). The commands of the tool for starting the reference alone (`parity reference`) and the older `parity timing` (which reads its pictures with an outside program and is not the agreed check; the checks `events` and `tracking` of `parity compare` are) go on as they were. The two older commands that read videos with an outside program, `parity facts` (the facts of the streams, compared one by one) and `parity pictures` (a similarity score on eight sampled frames), are replaced by `parity compare` and `parity against`: the sampled score is not the agreed check, which is 99 % of pixels on every frame.

## 2. Reading the two videos

**Without FFmpeg.** Both videos are read with frx7, through its public traits: the container reader and the probe for the facts, the picture decoder of the system (`codec::Pictures`) for the pictures, and frx7's conversion of pictures for the colours. No program is started to read a video: not `ffmpeg`, not `ffprobe`, not any other. This is the rule of `CLAUDE.md` rule 11 for this tool and a test holds the tool to it (section 11).

**The facts** are read without decoding a picture:

- *frame count*: the number of samples of the picture track;
- *frame rate*: the track's pictures a second, as a fraction;
- *size*: the width and height the stream's headers give after cropping, and the container's where the stream has no headers frx7 reads;
- *length*: the length the picture track plays, as a count of ticks of the track's clock and the clock's ticks a second.

**The pictures** are decoded one after the other in the order they are shown (picture 0, 1, 2, and so on), and each is brought to red, green and blue of 8 bits a sample with the colour description the track gives (BT.709 in the narrow range when it gives none, the standard form of frx7's `pixel.md`). The same conversion is used for both videos, so that two videos with the same pictures give the same numbers. Only one picture of each video is in memory at a time besides the one being compared, so that a long video never has to sit in memory whole.

A file that is not a video, that is damaged, or whose pictures the system does not decode is a mistake in how the tool was called, not a failed check: the report is not made, the message names the file, and the exit code is 2 (section 6).

## 3. The four structural checks

Each is a line of the report with the word `pass` or `fail`, and both values, ours first:

| Line | Equal when | The detail shows |
|---|---|---|
| `frame count` | the counts are equal | `ours 60, reference 60` |
| `frame rate` | the two fractions are equal (30/1 and 60000/2000 are equal) | `ours 30/1, reference 30/1` |
| `size` | width and height are both equal | `ours 320x180, reference 320x180` |
| `length` | the two lengths are equal as fractions of a second: ticks of one times the other's clock equal ticks of the other times the first's, in whole numbers, with no margin | `ours 2.000000 s (60000/30000), reference 2.000000 s (60/30)` |

"Identical" is the word of `PLAN.md` 5.1 and it is kept: no margin is added. Two videos with the same count and the same rate have the same length, whatever the clock of their containers, as long as both clocks count a picture in whole ticks, which every container this tool reads does for a steady rate. A length that differs by a rounding of a clock is therefore shown, with both values, as a fail, and the reading maintainer sees it for what it is.

A line shows its values whether it passes or fails, so that a pass can be read as well as a fail.

## 4. The picture check

### 4.1 Pairing

Frames are paired by their number: picture N of ours with picture N of the reference's, counting from 0 in the order shown. Pairing needs the same number of pictures and the same size. When the counts differ, or the sizes differ, the pictures cannot be paired: the line `picture` is a fail, its detail says `frames cannot be paired: ours has 60 frames, reference has 45` (or the sizes), nothing is measured, and the report cannot pass. A frame is never paired with a neighbour to make up the number.

### 4.2 The difference between two pictures

The number of pixels that differ is counted by the method of `pixelmatch` (the program `PLAN.md` 5.1 names, version 8.0.0, run with `bun` as a comparison tool in development and no dependency), written again in this tool from the published method so that the picture check runs with nothing installed:

- a pixel is the same when its two colours are identical, and otherwise when the distance between them is at most **0.1**, which is the setting `threshold` of `pixelmatch` at its default (a distance of 1 is black against white);
- the distance between two colours is the hybrid distance of Abasi and others ("Distance metrics for very large color differences", 2019) in the colour space of Björn Ottosson ("A perceptual color space for image processing", 2020): both colours are turned from sRGB of 8 bits to linear values, to the long, medium and short cone responses, to their cube roots, and to the lightness and the two opponent axes; the lightness is passed through the "toe" of Ottosson's "Okhsv and Okhsl" (with `k1` 0.206, `k2` 0.03 and `k3` (1 + `k1`) / (1 + `k2`)); the distance is the difference of the two lightnesses, taken without sign, plus the length of the difference of the two pairs of opponent axes;
- a pixel that is not the same but is an *anti-aliased* pixel is not counted, by the detector of Vyšniauskas ("Anti-aliased pixel and intensity slope detector", 2009), which is the setting `includeAA` of `pixelmatch` at its default (false): a pixel with a darker and a lighter neighbour in its own picture, where the darkest or the lightest neighbour has three or more equal neighbours of its own in both pictures, and which has no more than two equal neighbours itself, is anti-aliasing in either of the two pictures; the brightness it uses for the neighbours is the luma of the colour (`0.29889531 R + 0.58662247 G + 0.11448223 B`) of the 8-bit numbers.

Every other setting of `pixelmatch` is at its default, and none of them touches opaque pictures. Both pictures are opaque, so no blending against a background takes place.

The values 0.1 and "anti-aliasing not counted" are `pixelmatch`'s own defaults; `PLAN.md` 5.1 says "at the settings recorded in E9", and the record of E9 (`POC_PLAN.md`) names no other. They are fixed here under decision A29 and are the owner's to change.

**Held to the real `pixelmatch`.** This count, and not `pixelmatch` started for every frame, is the measure the picture check judges by (decision B8, 2026-10-14). The count of this tool is the count of `pixelmatch` 8.0.0 on the same two pictures: a yardstick test (`crates/parity/tests/yardstick_pixelmatch_counts.rs`) runs the real `pixelmatch` with `bun` on the same pairs, among them 100,000 pairs of colours, pictures of shapes with anti-aliased edges, and pictures of noise, and requires the two counts to be equal on every pair. The test needs `bun` on the search path and the package `pixelmatch` in the folder the variable `FRAMESMITH_COMPARISON_MODULES` names, or else in `pixelmatch/node_modules` of the comparison folder (`tools/blackbox/pixelmatch/`, where `pixelmatch` 8.0.0 and `pngjs` 7.0.0 are installed with `bun`, pinned, apart from the reference's own modules; decisions A18 and B8), or else in that folder's own `node_modules`; the test hands the pictures to `bun` as raw bytes, so `pngjs` is not needed. Where either is missing the yardstick test is skipped, by name, and counts as nothing compared (`docs/dev/gates.md`, section 5). Where two neighbours are equally dark (or equally light), the detector keeps the one it meets first, going down each column from the left, as `pixelmatch` does; the first run of this tool's code that went along the rows gave 111 against `pixelmatch`'s 109 on one of the pictures of shapes, and the yardstick test is what found it.

### 4.3 The score of a frame

The **score** of a frame is the share of its pixels that are the same, as a percentage: `100 x (pixels - differing) / pixels`. A frame is **low** when more than 1 % of its pixels differ, which is a score under 99 %. This is decided in whole numbers (`differing x 100 > pixels`), not on a rounded score, so that a frame at exactly 99 % is not low.

### 4.4 Scenes

A scene is a range of frames with a name, written in the fixture's record. The tool takes them on the command line, `--scene NAME=FIRST-LAST`, once for each scene; FIRST and LAST are frame numbers from 0, both included (`intro=0-29`; a scene of one frame is `hold=30-30` or `hold=30`). Scene names are not used twice. When none is given, there is one scene, called `all`, with every frame.

The scenes must cover the frames exactly: no frame in two scenes, no frame in none, none past the last frame. A set of scenes that does not is a mistake in how the tool was called (exit code 2), and the message names the first frame it found wrong. The tool does not guess a scene for a frame the record leaves out, because a frame that belongs to no scene would be judged by nothing.

### 4.5 A cause for a low frame

A low frame is allowed only with a **named cause**, recorded by the maintainer for that frame: `--excuse FIRST-LAST=CAUSE` (or `--excuse N=CAUSE` for one frame), once for each cause. The cause is a sentence in the maintainer's words and it is shown in the report. An empty cause is a mistake (exit code 2). A low frame with a cause is **excused**; a low frame without one **fails**. An excuse never changes a score: the frame keeps the score that was measured, it is counted with that score in every average, and it is shown as excused and never as passed. A cause recorded for a frame that is not low is not used, and the report says so in a note, naming the frames. A cause for a frame past the last, or two causes for one frame, is a mistake in how the tool was called (exit code 2); the message names the frame and, for two, both causes.

### 4.6 The results

For every frame the report knows its score and its state: `ok` (not low), `excused` (low, with a cause) or `fail` (low, without one). For every scene and for the whole video it gives the average of the scores of its frames, which is `100 x (1 - total differing / total pixels)` and is compared in whole numbers like a frame (`total differing x 100 <= total pixels` is 99 % or better).

- A **scene passes** when its average is 99 % or better and none of its frames fails. A scene with a frame that fails (a low frame with no cause) fails, even when its average is above 99 %.
- The **picture check passes** only when the average over all frames is 99 % or better and every scene passes. A frame under 99 % that is excused does not make a scene fail, and it does not make the average better.

### 4.7 What the report shows of the pictures

- the `picture` line: pass or fail, the average over all frames (to three decimals), the number of frames compared, the lowest score with its frame number, and how many frames are under 99 % (and how many of them are excused);
- one line for every scene: `scene NAME (frames FIRST-LAST)`, pass or fail, the scene's average, and the lowest frame of it;
- the list "frames under 99 %": every low frame, in the order of their numbers, with the frame number, the score, `fail` or `excused (not passed): CAUSE`. A frame at 100 % is in no list, and the list holds the one word `none` when no frame is low;
- with `--per-frame`, the score of every frame as well, so that "100 % on every frame" can be read frame by frame; the JSON always holds every frame.

With `--sheet FILE.png` the tool writes one picture (a PNG file, written by the tool itself: no program is started and no library is used) with a row for each of the first 12 frames of the list of low frames: ours, the reference's, and a third picture that marks in red every pixel that differs (a marked pixel of the third picture stands for any differing pixel under it, so that a small difference does not fade) over a pale grey of ours. Each row is labelled with the frame number, the score and `fail` or `excused`. When no frame is low, no sheet is written and a note says so.

## 5. Timing and position

Sources: `PLAN.md`, section 5.1 (timing: visible events within 1 frame; position: a tracked element within 1 pixel on every frame); decision A29 of `DECISIONS.md` (the values this section states); `CLAUDE.md` rule 2 (a check that did not judge is never shown as passed). The tool reads both videos as section 2 says; it knows nothing of a composition, so the fixture tells it what to look for.

### 5.1 How a fixture marks an element

An element is marked by **a flat colour of its own**: in both compositions of the pair the element is drawn in one opaque colour that nothing else on the frame uses, over a flat background colour, and the fixture's record names the colour. The tool takes the marks on the command line:

- `--background RRGGBB`: the colour behind the marked elements (needed whenever an element is marked);
- `--element NAME=RRGGBB`, once for each element whose events are compared;
- `--track NAME=RRGGBB`, at most once: the **tracked element**, which is followed frame by frame; its events are compared too.

A name is a word without spaces. Two elements with one name or one colour, or an element with the colour of the background, are a mistake in how the tool was called (exit code 2). Only the marked elements are looked at.

### 5.2 Seeing an element on a frame

On every frame of each video, for each marked element:

- the element is **there** when at least **20** pixels are within **24** of its colour on each of red, green and blue (8-bit values after the conversion of section 2);
- its **place** is its centre, to a fraction of a pixel: the pixels near its colour give its box; in that box widened by 3 pixels on each side, every pixel counts by the share of the element it holds between the background and the element's colour (by brightness when the two differ in brightness by 24 or more, since video keeps colour at half the resolution; otherwise along the line between the two colours, a pixel more than 40 from that line counting nothing), shares under 0.05 not counted; the centre is the mean of the pixels' centres weighed by their shares.

### 5.3 The event check

From what is seen on each frame, the events of an element are, counting frames from 0:

| Event | On the frame on which |
|---|---|
| `appears` | it is there and was not there on the frame before (frame 0 when it is there from the start) |
| `disappears` | it is not there and was there on the frame before |
| `starts moving` | its centre is more than **0.25** pixel from the one of the frame before, and it was not moving: the first frame it is seen elsewhere |
| `stops moving` | its centre is 0.25 pixel or less from the one of the frame before, and it was moving; or it disappears while moving |

The events of each element and kind are paired in order: the first `appears` of ours with the first of the reference's, the second with the second (named `appears (time 2)`), and so on. An event passes when the two frames are at most **1** frame apart, and fails otherwise; an event that happens in one video and not in the other fails. The line `events` passes when every event passes; its detail gives the count, or, when it fails, how many failed and the first of them with the event and both frames (`clip appears: ours frame 12, reference frame 10, 2 frame(s) apart`). The list "events" shows every event with both frames and `pass` or `fail`. When marked elements are seen in neither video, the line fails: nothing was compared. When no element is marked, the line is `not applicable`.

### 5.4 The tracking check

On every frame on which the tracked element is there in either video, the distance between its two centres is measured and **rounded to the nearest tenth of a pixel**, the precision the measure is judged at. A frame passes when the distance is at most **1** pixel; a frame on which the element is there in one video and not in the other fails. The line `tracking` passes when every such frame passes, and shows the largest distance and its frame; when it fails, it shows how many frames are over the limit and the first of them with its frame number, its distance and both centres (`frame 14: 2.0 px apart, ours (41.00, 30.00) reference (40.00, 30.00)`). The list "tracked frames over 1 px" shows every frame over the limit, or `none`; with `--per-frame` the place of every frame is listed too, and the JSON always holds it. When the tracked element is seen in neither video, the line fails.

When the fixture marks no tracked element, the line `tracking` is **`not applicable`**, never `pass`.

When the frames cannot be paired (section 4.1), both lines are `not run`.

### 5.5 The values

The limits 1 frame and 1 pixel are `PLAN.md` 5.1's. The rest are fixed here under decision A29 and are the owner's to change: a pixel near a colour within 24 on each channel, an element there with 20 such pixels, moving above 0.25 pixel a frame, a distance judged at a tenth of a pixel. On the pairs the tests write (flat squares of 20 by 20 pixels at 160 by 90, through the system's H.264 encoder), the centre was measured at the drawn centre to two decimals on every frame, and shifts of 1 and 2 pixels measured 1.0 and 2.0.

## 6. The report

The report is plain text, one line for each check and scene, each with the word `pass`, `fail`, `not run` or `not applicable`, then the lists, then `result:` and one word: `passed`, `failed` or `not run`. A check that is `not applicable` had nothing to judge in this fixture: it is neither a pass nor a fail and does not change the result, which the other checks decide. With `--json` it is one JSON document with the same content, the scores of every frame included. The exit code:

| Exit code | When |
|---|---|
| 0 | every check ran and passed (excused frames are listed, and the line `result: passed` is the same) |
| 1 | a check failed |
| 2 | the tool was called wrongly or a file could not be read (the message names it); no report |
| 3 | something that was needed could not be run: the reference is not installed. Nothing was compared. |

An exit code of 3 or 2 is never 0. The word `passed` is printed only when every check ran and passed.

## 7. Not compared

The report ends its checks with the lines of what it did not compare, so that a pass is never read wider than it is: the sound (the next slice's). It is not a check and does not change the result.

## 8. Calls

```
parity compare <ours.mp4> <reference.mp4> [--scene NAME=FIRST-LAST]... [--excuse FIRST-LAST=CAUSE]...
               [--per-frame] [--sheet FILE.png] [--background RRGGBB [--element NAME=RRGGBB]... [--track NAME=RRGGBB]]
               [--json]
    the checks of section 1, on two videos that exist; exit code 0, 1 or 2

parity against <counterpart-folder> <ours.mp4> --reference-video FILE.mp4 [--fps N] [--quality Q]
               [--blackbox DIR] [--browser PATH] [--scene ...] [--excuse ...] [--per-frame] [--sheet ...] [--json]
    runs the reference on a counterpart composition, writes its video to FILE.mp4 (a place section 10 allows),
    and compares it with ours as `compare` does; exit code 0, 1, 2 or 3
```

`parity reference` runs the reference alone, as it did, and ends with 0 when the video was made, 1 when the reference failed, 3 when it is not installed.

## 9. When the reference is not installed

`parity against` needs the reference. When it is not installed in the folder (`tools/blackbox/` of the workspace root, or the folder `--blackbox` names), or the runtime its command needs is missing, nothing is run and nothing is compared. The report names the reference as not run, with the reason and the folder, and every check of the report is a line of its own with the word `not run`:

```
parity: ours (ours.mp4) against the reference's render of counterpart
  reference render  not run         the reference is not installed under tools/blackbox (nothing was run)
  frame count       not run         needs the reference's render
  frame rate        not run         needs the reference's render
  size              not run         needs the reference's render
  length            not run         needs the reference's render
  picture           not run         needs the reference's render
  events            not run         needs the reference's render
  tracking          not run         needs the reference's render
result: not run
```

The exit code is 3. The words `pass` and `passed` do not appear anywhere in this report. In the tests that start the reference (`yardstick_reference_...`) the same absence is a skipped test, named in the line of the run (`docs/dev/gates.md`, sections 5 and 11).

When the reference is installed and fails to render, the line `reference render` is a `fail` with the reason and the other lines are `not run`; the exit code is 1.

## 10. Nothing of the reference enters the repository

- The counterpart composition is read, never copied. It lives in `tools/blackbox/pairs/` outside both repositories.
- The reference runs with the folder it is installed in as its working folder, and it keeps its own state (a cache of frames, a home folder) in `state/` of that folder (`crates/parity/src/blackbox.rs`).
- The reference's video is written only where `--reference-video` says. The tool **refuses** a place that would add the file to a git repository: a place inside a work tree of git that git would not ignore. The tool finds the repository by the nearest folder at or above the place (a link such as `/var` followed) that holds a `.git`, and asks `git check-ignore` whether git would ignore the place; where git cannot be started or cannot say, the place is refused, never guessed. The refusal is made before the reference is started (and before the absence of the reference is looked at), names the place and the repository, and ends with exit code 2. A place inside a work tree that its `.gitignore` covers is allowed, and so is any place outside a repository. Looking at the place starts `git` and nothing else, and only for a place inside a repository.
- The tool writes no other file of the reference's. It writes the sheet (section 4.7) only where `--sheet` says, and the same refusal applies to it, for `compare` as for `against`.
- The tests start a stand-in for the reference in a repository of their own and require `git status` to show nothing after the run.

## 11. Held to this document

The tests of `crates/parity` and `crates/measure` are the link from each rule above to the code; their names say what they cover. Two classes are not core tests and are named as yardstick tests: those that start `bun` with `pixelmatch` (section 4.2), and those that start the reference. The core tests start no program of the reference's kind and no `ffmpeg` or `ffprobe`: the videos they use are written by `measure::write_video`, which encodes with the operating system's own encoder through frx7, and the picture check is the tool's own.

The test videos are small: 160 x 90 pixels, 12 to 40 frames. The pairs of the event and tracking checks (`crates/parity/tests/event_and_tracking_checks.rs`) are written from pictures with a known shift in time (an element that appears 1 or 2 frames later, a motion that starts 2 frames later) or in place (1 pixel on every frame, 2 pixels on one frame); `cargo run -p parity --example made_pairs -- <new folder>` writes such pairs for trying the tool by hand. A video encoded for lossy storage is not the same to the bit as the pictures that were encoded; the tests that need "equal frames" give the tool a file and a copy of it and require 100 % on every frame, and one more test gives it two files written separately from the same pictures and requires the check to pass (the system's encoder gave 100.000 % on every frame in each run measured), and the tests that need a difference give frames that differ by a large block of pixels, far above the loss of the coding.

The yardstick tests are `crates/parity/tests/yardstick_pixelmatch_counts.rs` (section 4.2) and `crates/parity/tests/yardstick_video_reading.rs`, which holds the facts frx7 reads to `ffprobe`'s and the pictures it decodes to FFmpeg's (the count of pixels that differ by the measure of section 4.2 is at most half a percent of the picture; 0.2 % was the most measured, on hard-edged squares where the two converters round the chroma differently).
