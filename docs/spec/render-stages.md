# Render stages

Status: v0 (Sprint 1). Sources: proposal `s01-first-render` (CAP-008, CAP-009, CAP-012, CAP-015, CAP-016, CAP-017, CAP-021, INT-002, INT-003, REC-003 to REC-009); `PLAN.md` decision D4.

## Before the stages

`render` does these in order and stops at the first that fails. Nothing is started and nothing is written until all pass.

1. **Options.** `--fps` must be a whole number from 1 to 240 (FS-R004); a ratio is refused (FS-U004, exit 2).
2. **Check.** The full `check` runs on the project. Errors stop with exit 1; unsupported or unknown uses stop with exit 2. The diagnostics are the ones `check` prints.
3. **Prerequisites.** A browser and `ffmpeg` are looked up. Each missing one is reported; both are reported in one run when both are missing.
4. **Output location.** Missing parent folders are created and a staging file is created beside the output path. If either fails the run stops (FS-R005) and names the path and the reason.

### Finding a browser

1. `FRAMESMITH_BROWSER`, when set, is the browser. If it does not exist or cannot be run, the run stops (FS-R002). There is no fallback.
2. Otherwise, the first of these that exists, on macOS, in `/Applications` and then in `~/Applications`: Google Chrome, Chromium, Google Chrome for Testing, Microsoft Edge, Brave Browser, Vivaldi, Google Chrome Beta, Google Chrome Canary.
3. Otherwise, the first of these names on the search path: `chromium`, `chromium-browser`, `google-chrome`, `google-chrome-stable`, `chrome`, `chrome-headless-shell`.
4. Otherwise, the newest browser build in the caches that browser automation tools keep in the user's home folder (`~/.cache/puppeteer`, `~/Library/Caches/ms-playwright`).
5. Otherwise the run stops (FS-R001).

Framesmith never downloads a browser in this version, and never uses the network.

### Finding ffmpeg

`ffmpeg` is the first program of that name on the search path. It is run as a separate program.

## The five stages

| Stage | What happens | Ends when |
|---|---|---|
| compile | The project's entry page is read and prepared: the loader and the base style sheet are added. The render job is created. | The page is ready to be served. |
| probe | The browser starts. The page is opened and reports ready. The ready report is checked and the page's manifest is compared with the files (see `page-protocol.md`). The frame count is fixed. | The page is accepted. |
| capture | `ffmpeg` starts. Then, for each frame in order: seek, screenshot, hand the picture to `ffmpeg`. | The last frame is handed over. |
| encode | `ffmpeg` finishes the file. The browser is stopped. | `ffmpeg` exits with success. |
| commit | The staging file is moved onto the output path in one step. | The output file is in place. |

### The page's size and the browser's window

The page is always laid out and pictured at exactly the composition's size, at a scale of 1, whatever the size of the display or of the browser's window. The size of the page is given to the browser when the page is opened; it is not taken from the window.

The browser's window is asked to be the composition's size, but never less than 500 in width or in height. A full browser build keeps a strip of its window for itself, so a window as short as a small composition leaves no room for the page; such a browser then gives no picture, and the capture would wait without end. With this lower limit a composition of any allowed size renders, and the pictures are the same as with a browser that has no such strip.

Source: our own observation on 2026-10-11. Google Chrome 155 on macOS gave no picture for compositions 72 pixels high or less (64x36, 160x36, 128x72) and rendered 64x90 and larger; a headless-only build and an older full build rendered every size. With the lower limit all three give identical frames for the same 64x36 composition.

- One worker. Frames are captured and encoded strictly in order.
- No time limit applies to capture. Only the wait for the page to become ready is limited (30 seconds).
- No retry and no resume in this version.

## Output

- Default path: `<project>/out/<project folder name>.mp4`. `--output <path>` replaces it.
- The file is H.264 video in an MP4 container, 8-bit 4:2:0, at the composition size and the run's frame rate, with no audio stream. The index is at the front of the file so playback can start before the whole file is read. Quality is one fixed standard setting.
- The staging file sits in the output folder and is named `.<output name>.<run id>.part`. It becomes the output only by the move in the commit stage.
- A file already at the output path is not touched until that move, which replaces it.
- `render` changes no other file in the project.

## Stopping early

| Cause | Code | Exit |
|---|---|---|
| The page is not ready: failed image or font, script error, 30 seconds passed | FS-R006, FS-R007, FS-R008 | 1 |
| Clocks not replaced | FS-R009 | 1 |
| The page disagrees with the files | FS-R010 | 1 |
| An unsupported use seen in the page | FS-U… | 2 |
| The browser stopped or reported an error | FS-R011 | 1 |
| `ffmpeg` stopped or reported an error, including an `ffmpeg` that cannot encode H.264 | FS-R012 | 1 |
| The output cannot be written (found up front or during the run) | FS-R005 | 1 |
| Cancelled | none | 130 |

In every case: the staging file is removed, folders Framesmith created for this run are removed when empty, the browser and `ffmpeg` that Framesmith started are stopped, and a file already at the output path is unchanged. The message names the stage, and for the browser or `ffmpeg` the program and the frame reached.

### Cancelling

- An interrupt (Ctrl-C) or a terminate signal cancels the run at any stage.
- Framesmith prints `Render cancelled`, cleans up as above, and exits 130.
- Child programs are started in their own process groups, so an interrupt from the terminal reaches Framesmith alone and Framesmith decides how they stop.

### Stopping child programs

Each child is asked to stop, given 2 seconds, and then forced to stop together with every process it started. Framesmith stops only processes it started itself. Its browser uses a private, temporary profile, which is deleted afterwards; a browser the user is running is never touched.

## Progress

Human-readable progress goes to standard output, or to standard error when `--json` is given.

```text
compile   done
probe     done      1920x1080, 30 fps, 6 s, 180 frames
capture   180/180
encode    done
commit    demo/out/demo.mp4
Rendered 180 frames, 1920x1080, 30 fps, in 14.2 s
```

- The five stage names appear in order. The probe line gives the size, rate, length and frame count the run uses. The capture line counts frames done out of the total. The last line gives the frame count, size, rate used and elapsed time; the commit line gives the output path.
- In a terminal the capture line is rewritten in place. Otherwise a new capture line is printed at the start, about once a second, and at the end.
- No percentage and no remaining-time figure. Nothing depends on colour.
- When a run stops early the last line is `render stopped in <stage>: <cause>`, or `Render cancelled (in <stage>)`.

## The render job

The job is the complete description of one run: the project path, the entry page, the output path, the composition manifest, the frame rate used, the frame count, and the format settings. It can be written as JSON and read back; reading rejects a missing field, an unknown field, or a value of the wrong type, and names the field. It is an internal form, not a public format.

## Maintainer switches

Environment variables for tests and measurement. They are not part of the user interface.

| Variable | Effect |
|---|---|
| `FRAMESMITH_LOG_DIR`, `FRAMESMITH_LOG` | Where the log is written, and its level (see `commands.md`). |
| `FRAMESMITH_DEBUG_FRAMES=<dir>` | Each captured picture is also saved there as `frame-<number>.png`. |
| `FRAMESMITH_DEBUG_APPLICATIONS=<folders>` | The application folders of step 2 of "Finding a browser" are these (a list written like the search path) instead of `/Applications` and `~/Applications`. An empty value is the same as not set. A test names an empty folder here to stand for a machine with no browser installed, or a folder of its own to stand for one that has. |
| `FRAMESMITH_DEBUG_ORDER=reverse` | Frames are captured from the last to the first, then encoded in order. |
| `FRAMESMITH_DEBUG_ORDER=<n>,<n>,…` | The listed frames are captured first, in the order written; every other frame follows from the first to the last. Frames are then encoded in order. A number past the last frame, or written twice, is ignored. |
| `FRAMESMITH_DEBUG_HOLD=<stage>` | The run waits at the start of that stage (up to 20 seconds) and prints `holding in <stage>` to standard error, so a test can interrupt it there. |
| `FRAMESMITH_DEBUG_FAULT=clocks` | The loader skips replacing the clocks, to prove the host notices. |
| `FRAMESMITH_DEBUG_JOB=<file>` | The render job is written there as JSON when it is created. |
| `FRAMESMITH_DEBUG_COSTS=<file>` | When the last frame has been captured, the page's own record of what each seek cost is written there: a JSON list of real milliseconds, one for each seek in capture order (`costs()` in `page-protocol.md`). The seek measurement in `bench/` reads it. |
