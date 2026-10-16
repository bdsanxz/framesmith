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

**Differences found on the runners.** The first hosted run of the workflow "other systems" on `main` (run 38863528483, commit `1bfa423`, 2026-10-16) failed in its job "system-free crates on Windows". Runner: Windows (`windows-latest`). Test file: `crates/project/tests/locating_the_project.rs` (the test target `locating_the_project` of the crate `project`). Size: the test target did not compile on that runner, so none of its tests ran there, and no picture of the processor's renderer was compared on Windows; the compiler gave five errors, two of them E0433 ("cannot find unix in os") and three of them E0599 ("no associated function or constant named from_mode"), then "could not compile project (test locating_the_project) due to 5 previous errors". Cause: the file used interfaces that exist only on Unix systems, in tests that every job compiles: the Unix permission bits of `std::fs::Permissions`, the Unix symbolic-link function, and a named pipe made by `/usr/bin/mkfifo`. Fix: those parts are compiled only on Unix (`#[cfg(unix)]`), and the permission helper has a portable form for the other systems; every other test of the file runs on every system, and the Unix tests run on macOS and Linux as before. No limit was changed: the limit of one level of 255 stands.

## 4. Clip windows

- Each box the scene places has a window: the frames it is drawn in (`render-command.md` section 4.2). A box with neither `start` nor `duration` has its parent's window; a box with both has the frames of its clip by the frame-grid rule (`frame-time.md`), cut to its parent's window.
- Each placed box keeps the origin of its children: the exact start of the box itself when the box is a clip, and otherwise the origin of its parent. The `start` of a box counts from the origin of its parent, and the origin of the root is time 0.
- The list of a frame holds the boxes whose window holds that number, in document order. The list of a frame does not depend on the frames asked for before it.
- The windows are worked out once, when the scene is made. A window is not a time: the scene holds frame numbers and exact times, and reads no clock.
