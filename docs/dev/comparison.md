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

Timing of visible events and the tracked position of a moving element are the next ticket's; sound is the one after. The report names them as not compared (section 6). The older commands of the tool for timing (`parity timing`) and for starting the reference (`parity reference`) go on as they were; `parity timing` still reads its pictures with an outside program until its own ticket moves it to frx7.

## 2. Reading the two videos

**Without FFmpeg.** Both videos are read with frx7, through its public traits: the container reader and the probe for the facts, the picture decoder of the system (`codec::Pictures`) for the pictures, and frx7's conversion of pictures for the colours. No program is started to read a video: not `ffmpeg`, not `ffprobe`, not any other. This is the rule of `CLAUDE.md` rule 11 for this tool and a test holds the tool to it (section 9).

**The facts** are read without decoding a picture:

- *frame count*: the number of samples of the picture track;
- *frame rate*: the track's pictures a second, as a fraction;
- *size*: the width and height the stream's headers give after cropping, and the container's where the stream has no headers frx7 reads;
- *length*: the length the picture track plays, as a count of ticks of the track's clock and the clock's ticks a second.

**The pictures** are decoded one after the other in the order they are shown (picture 0, 1, 2, and so on), and each is brought to red, green and blue of 8 bits a sample with the colour description the track gives (BT.709 in the narrow range when it gives none, the standard form of frx7's `pixel.md`). The same conversion is used for both videos, so that two videos with the same pictures give the same numbers. Only one picture of each video is in memory at a time besides the one being compared, so that a long video never has to sit in memory whole.

A file that is not a video, that is damaged, or whose pictures the system does not decode is a mistake in how the tool was called, not a failed check: the report is not made, the message names the file, and the exit code is 2 (section 7).

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

**Held to the real `pixelmatch`.** The count of this tool is the count of `pixelmatch` 8.0.0 on the same two pictures: a yardstick test (`crates/parity/tests/yardstick_pixelmatch_counts.rs`) runs the real `pixelmatch` with `bun` on the same pairs, among them 100,000 pairs of colours, pictures of shapes with anti-aliased edges, and pictures of noise, and requires the two counts to be equal on every pair. Where `bun` or the packages `pixelmatch` and `pngjs` are not installed in `tools/blackbox/node_modules/` (decision A18) the yardstick test is skipped, by name, and counts as nothing compared (`docs/dev/gates.md`, section 5).

### 4.3 The score of a frame

The **score** of a frame is the share of its pixels that are the same, as a percentage: `100 x (pixels - differing) / pixels`. A frame is **low** when more than 1 % of its pixels differ, which is a score under 99 %. This is decided in whole numbers (`differing x 100 > pixels`), not on a rounded score, so that a frame at exactly 99 % is not low.

### 4.4 Scenes

A scene is a range of frames with a name, written in the fixture's record. The tool takes them on the command line, `--scene NAME=FIRST-LAST`, once for each scene; FIRST and LAST are frame numbers from 0, both included (`intro=0-29`; a scene of one frame is `hold=30-30`). When none is given, there is one scene, called `all`, with every frame.

The scenes must cover the frames exactly: no frame in two scenes, no frame in none, none past the last frame. A set of scenes that does not is a mistake in how the tool was called (exit code 2), and the message names the first frame it found wrong. The tool does not guess a scene for a frame the record leaves out, because a frame that belongs to no scene would be judged by nothing.

### 4.5 A cause for a low frame

A low frame is allowed only with a **named cause**, recorded by the maintainer for that frame: `--excuse FIRST-LAST=CAUSE` (or `--excuse N=CAUSE` for one frame), once for each cause. The cause is a sentence in the maintainer's words and it is shown in the report. An empty cause is a mistake (exit code 2). A low frame with a cause is **excused**; a low frame without one **fails**. An excuse never changes a score: the frame keeps the score that was measured, it is counted with that score in every average, and it is shown as excused and never as passed. A cause recorded for a frame that is not low is not used, and the report says so in a note, naming the frame.

### 4.6 The results

For every frame the report knows its score and its state: `ok` (not low), `excused` (low, with a cause) or `fail` (low, without one). For every scene and for the whole video it gives the average of the scores of its frames, which is `100 x (1 - total differing / total pixels)` and is compared in whole numbers like a frame (`total differing x 100 <= total pixels` is 99 % or better).

- A **scene passes** when its average is 99 % or better and none of its frames fails. A scene with a frame that fails (a low frame with no cause) fails, even when its average is above 99 %.
- The **picture check passes** only when the average over all frames is 99 % or better and every scene passes. A frame under 99 % that is excused does not make a scene fail, and it does not make the average better.

### 4.7 What the report shows of the pictures

- the `picture` line: pass or fail, the average over all frames (to three decimals), the number of frames compared, the lowest score with its frame number, and how many frames are under 99 % (and how many of them are excused);
- one line for every scene: `scene NAME (frames FIRST-LAST)`, pass or fail, the scene's average, and the lowest frame of it;
- the list "frames under 99 %": every low frame, in the order of their numbers, with the frame number, the score, `fail` or `excused`, and the cause of an excused one. A frame at 100 % is in no list, and the list is empty when no frame is low;
- with `--per-frame`, the score of every frame as well, so that "100 % on every frame" can be read frame by frame; the JSON always holds every frame.

With `--sheet FILE` the tool writes one picture (a PNG file) that shows, for up to the first 12 frames of the list of low frames, ours, the reference's and the pixels that differ, each labelled with the frame number and the score.

## 5. The report

The report is plain text, one line for each check and scene, then the lists, then `result:` and one word: `passed`, `failed` or `not run`. With `--json` it is one JSON document with the same content, the scores of every frame included. The exit code:

| Exit code | When |
|---|---|
| 0 | every check ran and passed (excused frames are listed, and the line `result: passed` is the same) |
| 1 | a check failed |
| 2 | the tool was called wrongly or a file could not be read (the message names it); no report |
| 3 | something that was needed could not be run: the reference is not installed. Nothing was compared. |

An exit code of 3 or 2 is never 0. The word `passed` is printed only when every check ran and passed.

## 6. Not compared

The report ends its checks with the lines of what it did not compare, so that a pass is never read wider than it is: the timing of visible events and the tracked position of a moving element (the next ticket's), and the sound (the one after). They are not checks and do not change the result.

## 7. Calls

```
parity compare <ours.mp4> <reference.mp4> [--scene NAME=FIRST-LAST]... [--excuse FIRST-LAST=CAUSE]...
               [--per-frame] [--sheet FILE.png] [--json]
    the five checks of section 1, on two videos that exist

parity against <counterpart-folder> <ours.mp4> --reference-video FILE.mp4 [--fps N] [--quality Q]
               [--blackbox DIR] [--browser PATH] [--scene ...] [--excuse ...] [--per-frame] [--sheet ...] [--json]
    runs the reference on a counterpart composition, writes its video to FILE.mp4, and compares it
    with ours as `compare` does
```

`parity reference` runs the reference alone, as it did, and ends with 0 when the video was made, 1 when the reference failed, 3 when it is not installed.

## 8. When the reference is not installed

`parity against` needs the reference. When it is not installed in the folder (`tools/blackbox/` of the workspace root, or the folder `--blackbox` names), or the runtime its command needs is missing, nothing is run and nothing is compared. The report names the reference as not run, with the reason and the folder, and every check of the report is a line of its own with the word `not run`:

```
parity: ours (ours.mp4) against the reference's render
  reference render  not run  the reference is not installed under tools/blackbox (nothing was run)
  frame count       not run  needs the reference's render
  frame rate        not run  needs the reference's render
  size              not run  needs the reference's render
  length            not run  needs the reference's render
  picture           not run  needs the reference's render
result: not run
```

The exit code is 3. The words `pass` and `passed` do not appear anywhere in this report. In the tests that start the reference (`yardstick_reference_...`) the same absence is a skipped test, named in the line of the run (`docs/dev/gates.md`, sections 5 and 11).

When the reference is installed and fails to render, the line `reference render` is a `fail` with the reason and the other lines are `not run`; the exit code is 1.

## 9. Nothing of the reference enters the repository

- The counterpart composition is read, never copied. It lives in `tools/blackbox/pairs/` outside both repositories.
- The reference runs with the folder it is installed in as its working folder, and it keeps its own state (a cache of frames, a home folder) in `state/` of that folder (`crates/parity/src/blackbox.rs`).
- The reference's video is written only where `--reference-video` says. The tool **refuses** a place that would add the file to a git repository: a place inside a work tree of git that git would not ignore. The refusal is made before the reference is started, names the place and the repository, and ends with exit code 2. A place inside a work tree that its `.gitignore` covers is allowed, and so is any place outside a repository.
- The tool writes no other file of the reference's. It writes the sheet (section 4.7) only where `--sheet` says, and the same refusal applies to it.
- The tests start a stand-in for the reference in a repository of their own and require `git status` to show nothing after the run.

## 10. Held to this document

The tests of `crates/parity` and `crates/measure` are the link from each rule above to the code; their names say what they cover. Two classes are not core tests and are named as yardstick tests: those that start `bun` with `pixelmatch` (section 4.2), and those that start the reference. The core tests start no program of the reference's kind and no `ffmpeg` or `ffprobe`: the videos they use are written by `measure::write_video`, which encodes with the operating system's own encoder through frx7, and the picture check is the tool's own.

The test videos are small: 320 x 180 pixels, 24 to 60 frames. A video encoded for lossy storage is not the same to the bit as the pictures that were encoded; the tests that need "equal frames" give the tool two files with the same pictures and require 100 % on every frame, which the encoder gives for the same pictures with the same settings, and the tests that need a difference give frames that differ by a large block of pixels, far above the loss of the coding.
