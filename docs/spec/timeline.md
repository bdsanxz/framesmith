# The `timeline` crate: what is shown at a frame

Status: in force from Sprint 1. Sources: `PLAN.md` section 3 (the table of Framesmith's crates: `timeline` holds what is shown at a frame; a frame is a pure function of the composition and the frame's number), section 5 (the processor's renderer against its checked-in pictures) and decision D29 (system-free crates are tested on hosted Windows and Linux runners); `render-command.md` sections 3, 4, 8 and 9 (what this version draws, the picture, a frame on request, the checked-in pictures); `CLAUDE.md` rules 1 and 10.

## 1. What the crate holds

- The scene: a project opened by `project` is made into a scene, or into the reports of everything this version does not draw or apply (`render-command.md` section 3). The behaviour is the one that section states; this crate is where it lives.
- A frame on request: for a frame's number, the display list of that frame (`draw`), and its picture drawn by any renderer handed to it. A number outside the scene is refused with the count of frames.
- Nothing else: writing a film, encoding and any file of media belong to `render`, which uses this crate.

## 2. No system interface

The crate is built only on `project`, `layout`, `draw` and `timebase` (and `tracing` for its log line). It calls no interface of the operating system, holds no `unsafe` code and depends on no crate of frx7, so it builds and its tests run on any system (`scripts/gates/system-free.txt`).

## 3. The checked-in pictures within one level

On every system, each checked-in picture (`fixtures/<name>/expected/frame-NNNN.png`) is drawn from its fixture on the processor's renderer and compared value by value (red, green, blue and alpha, each of 255 levels). Every value must lie within one level of the checked-in value. The test prints the largest difference found, with the fixture, the frame, the pixel and the channel where it was found. On macOS the exact comparison of `render-command.md` section 9 stands besides this one; the limit of one level is never widened to let a system pass: a difference larger than that is recorded with the runner, the test, its size and its cause.

## 4. Clip windows

- Each box the scene places has a window: the frames it is drawn in (`render-command.md` section 4.2). A box with neither `start` nor `duration` has its parent's window; a box with both has the frames of its clip by the frame-grid rule (`frame-time.md`), cut to its parent's window.
- Each placed box keeps the origin of its children: the exact start of the box itself when the box is a clip, and otherwise the origin of its parent. The `start` of a box counts from the origin of its parent, and the origin of the root is time 0.
- The list of a frame holds the boxes whose window holds that number, in document order. The list of a frame does not depend on the frames asked for before it.
- The windows are worked out once, when the scene is made. A window is not a time: the scene holds frame numbers and exact times, and reads no clock.
