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
| sound length | the sound of ours is as long as the picture of ours, to the sample (section 12) |
| loudness | the integrated loudness of the two sounds is within 0.5 LU (section 12) |
| peak | the peaks of the two sounds are within 0.5 dB (section 12) |
| alignment | the two sounds are in step within 1 ms (section 12) |

How a mix sounds is not judged: only the four measured sound checks. The report names that as not compared (section 7). The commands of the tool for starting the reference alone (`parity reference`) and the older `parity timing` (which reads its pictures with an outside program and is not the agreed check; the checks `events` and `tracking` of `parity compare` are) go on as they were. The two older commands that read videos with an outside program, `parity facts` (the facts of the streams, compared one by one) and `parity pictures` (a similarity score on eight sampled frames), are replaced by `parity compare` and `parity against`: the sampled score is not the agreed check, which is 99 % of pixels on every frame.

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

The report ends its checks with the lines of what it did not compare, so that a pass is never read wider than it is: how the sound sounds (only its length, loudness, peak and alignment are measured, section 12). It is not a check and does not change the result.

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
  sound length      not run         needs the reference's render
  loudness          not run         needs the reference's render
  peak              not run         needs the reference's render
  alignment         not run         needs the reference's render
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

## 12. The sound checks

Sources: `PLAN.md`, section 5.1 (sound length equal to the picture's, to the sample; integrated loudness within 0.5 LU; peak within 0.5 dB; alignment within 1 ms); Recommendation ITU-R BS.1770-4 (2015), "Algorithms to measure audio programme loudness and true-peak audio level": Annex 1 (the frequency weighting, its two filters and their coefficients at 48 kHz in Tables 1 and 2; the weights of the channels in Table 3; the gating of blocks of 400 ms overlapping by 75 %, with the absolute gate at -70 LKFS and the relative gate 10 LU under, in clause 5.1 and the formulas of Annex 1); the analogue parameters of the two filters that give those coefficients at any rate by the bilinear transform, as published by B. De Man, "Evaluation of implementations of the EBU R128 loudness measurement", AES convention paper 10035 (2018); decision A29 of `DECISIONS.md` (the methods called "stated" are fixed here, open to the owner's change). LUFS and LKFS are the same unit; this document writes LUFS.

### 12.1 Reading the sound

Each video's first sound track is read with frx7: the container (`probe`) says whether there is one and how long it plays (its length after the edit list, at its own rate), and the system's decoder (`codec::AudioSource`) gives its samples as 32-bit floating point numbers, from the first sample of real sound (the encoder's lead-in is not given). The decoder gives the last coded block whole; the samples past the length the container says is played are not sound and are left out (`measure::read_sound`). A decoder that gives fewer samples than the container says is an error that names the file (exit code 2). No program is started.

### 12.2 Sound length

Ours only: the sound of our render is as long as its picture. The picture's length is the picture track's length (section 3), *ticks* of a clock of *timescale* a second. The check passes when *sample frames* x *timescale* = *ticks* x *rate*, in whole numbers: no margin, not even one sample. The line gives both lengths in sample frames, and the picture's in seconds:

```
  sound length  fail  ours: sound 47999 sample frames, picture 48000 sample frames (1.000000 s at 48000 Hz)
```

When the picture's length is not a whole number of sample frames it is given to three decimals, and the check fails: no sound can be as long.

### 12.3 Loudness

The integrated loudness of each sound, by BS.1770-4:

1. Each channel is weighted by two filters of second order run one after the other, `y[n] = b0 x[n] + b1 x[n-1] + b2 x[n-2] - a1 y[n-1] - a2 y[n-2]`, from rest. The first is a shelf (analogue parameters: centre 1681.974450955533 Hz, gain 3.999843853973347 dB, quality 0.7071752369554196); the second a high-pass (38.13547087602444 Hz, quality 0.5003270373238773). At a rate *fs*, with *K* = tan(pi f0 / fs):
   - shelf: *Vh* = 10^(gain/20), *Vb* = *Vh*^0.4996667741545416, *a0* = 1 + K/Q + K²; b0 = (Vh + Vb K/Q + K²)/a0, b1 = 2(K² - Vh)/a0, b2 = (Vh - Vb K/Q + K²)/a0, a1 = 2(K² - 1)/a0, a2 = (1 - K/Q + K²)/a0;
   - high-pass: b0 = 1, b1 = -2, b2 = 1, a1 = 2(K² - 1)/(1 + K/Q + K²), a2 = (1 - K/Q + K²)/(1 + K/Q + K²).
   At 48 kHz these are the coefficients of BS.1770-4, Tables 1 and 2, within 1e-12 (a test holds them).
2. The weighted samples are taken in blocks of 400 ms (0.4 x *fs* sample frames, rounded) that start every 100 ms (0.1 x *fs*, rounded), from the first sample; a block that would run past the end is not taken. The power of a block is the sum over the channels of *G* times the mean square of the channel in the block, and its loudness is -0.691 + 10 log10(power). *G* is 1 for each channel; for six channels (left, right, centre, low-frequency effects, left surround, right surround) it is 1, 1, 1, 0, 1.41, 1.41.
3. Blocks of loudness -70 LUFS or less are left out. Of the others, the mean power gives a loudness; blocks at 10 LU under it or less are left out too. The loudness of the sound is that of the mean power of the blocks left.
4. A sound with no block left (silent, or shorter than one block) has no loudness: it is shown as `silent`.

The check passes when the two loudnesses are no more than 0.5 LU apart, compared unrounded; two silent sounds pass, one silent and one not fails. The line gives both values to a hundredth and how far apart they are:

```
  loudness  fail  ours -17.86 LUFS, reference -17.26 LUFS; 0.60 LU apart (limit 0.5 LU)
```

The values the tests hold the measure to (section 12.7): a tone of 997 Hz at full scale in one channel of 48 kHz measures -3.01 LUFS (the figure BS.1770-4 gives for it); a tone of 1000 Hz at -23 dB under full scale in both channels measures -23.00 LUFS; each within 0.01 LU. Two seconds of that stereo tone followed by ten seconds of silence measure -23.34 LUFS within 0.01 LU: the blocks of silence alone are gated out, and the three blocks that hold part of the tone are kept (-23 + 10 log10(18.5/20) = -23.339; without the gates it would be near -30.8). A gain of g dB on a sound changes its loudness by g LU.

### 12.4 Peak

The sample peak of each sound: the largest magnitude of any sample of any channel, as dB under full scale (20 log10 of it; full scale is 1.0). A sound whose samples are all 0 has no peak and is shown as `silent`. This is the peak of the samples, not the true peak between them, which needs oversampling and is not what this check measures.

The check passes when the two peaks are no more than 0.5 dB apart, compared unrounded; as with loudness, two silent sounds pass. The line gives both values to a hundredth and how far apart they are. The values the tests hold the measure to: a single sample of 0.5 in silence measures -6.02 dBFS (20 log10 0.5 = -6.0206); a tone of 997 Hz of amplitude 0.5 at 48 kHz, one second long, measures -6.02 dBFS within 0.001 dB; a gain of g dB changes the peak by g dB.

### 12.5 Alignment

How far ours is shifted against the reference's, in sample frames. The channels of each sound are averaged into one; from each, the first 10 s are taken (or all of it, when shorter). For every shift *s* from -20 ms to +20 ms (0.02 x *fs* sample frames, rounded, each way), the sum of ours[n + s] x reference[n] is taken over the samples both have; the shift is the *s* at which it is largest (a tie keeps the smaller shift). A positive shift means ours is later. The resolution is one sample frame (about 0.02 ms at 48 kHz).

The check passes when the shift is no more than 1 ms either way. The line gives the measured shift in milliseconds to a thousandth and in sample frames:

```
  alignment  fail  measured shift +2.000 ms (+96 sample frames at 48000 Hz): ours is later than the reference; limit 1 ms
```

A shift at the edge of the search (20 ms) fails and says that the shift may be larger. A real shift beyond the search is not measured: the largest sum inside it is then at some other shift (a sound of 50 ms later was measured at +18.188 ms), which fails as well unless it falls within 1 ms; the measure is meant for sounds that are close, as a render and its reference are. Two sounds at different rates fail (they cannot be aligned sample by sample). A sound that is silent in its first 10 s gives nothing to align: the line is `not run`.

### 12.6 No sound, or sound in one

When neither render has a sound track, `loudness`, `peak` and `alignment` are **`not applicable`** ("neither render has sound"), and so is `sound length` ("ours has no sound track"): a render without sound has no sound to be as long as its picture. The length of the two renders is still checked by the structural check `length` (section 3), which runs as always. When only one render has sound, the other three checks fail, saying which one has none; `sound length` is judged when ours has sound and fails when ours has none.

The JSON report has a member `sound` with the rate, channels, sample frames, loudness and peak of each, and the shift in sample frames and milliseconds.

### 12.7 Held to this section

The tests are `crates/parity/tests/sound_measures.rs` (the measures on made signals, section 12.3 and 12.4), `crates/parity/tests/sound_checks.rs` (the four checks on made sounds with a known length, gain and shift) and `crates/parity/tests/sound_comparison.rs` (`parity compare` on MP4 files whose sound is written by `measure::write_video_with_sound`, AAC through the system's encoder). The made sound (`measure::made_sound`) is four tones that swell and fade, ours, of no recording. `cargo run -p parity --example made_sound_pairs -- <new folder>` writes such pairs for trying the tool by hand.

Through the system's AAC encoder at 256,000 bits a second and its decoder, the made sound at 48 kHz in two channels kept its length to the sample (48,000 and 47,999 sample frames read back as written), its shift to the sample (24 and 96 sample frames), its loudness within 0.003 LU and its sample peak within 0.006 dB of the samples before coding (measured on 2026-10-14). A steady tone that starts at full level from silence comes back with its sample peak higher, by 0.12 dB at 1000 Hz and -23 dBFS and by 0.16 dB at 997 Hz and amplitude 0.5, and its loudness within 0.03 LU: the coding spreads the sudden start. This is why the stated values of the measures are held on samples, not on coded files. The tests of files keep the asked differences (0.4 and 0.6 LU or dB, 0.5 and 2 ms) far enough from the limits that this loss does not decide them.

