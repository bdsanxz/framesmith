# Commands

Status: v0 (Sprint 1). Sources: proposal `s01-first-render` (CAP-001, CAP-006, CAP-018, CAP-019, CAP-020); project rule 5 (logging).

## Exit codes

| Code | Meaning |
|---|---|
| 0 | The command did what was asked. From `render`: a complete file is at the output path. |
| 1 | It failed: invalid project, missing prerequisite, bad option, or a failure during the run. |
| 2 | Refused: the project uses something unsupported or unknown. |
| 130 | Cancelled by the user. |

From `render`, codes 1, 2 and 130 always mean no new file was written.

## `framesmith init <folder>`

- Creates the folder (and missing parents) with a starter project: `index.html` and `style.css`, an animated title card made of text and shapes with clips and a motion block. Every file is written by us; there is no third-party library, font or media and no network address.
- Prints the folder path and the two next commands: `framesmith check <folder>` and `framesmith render <folder>`.
- An existing empty folder is used. An existing folder that is not empty: nothing is written, exit 1 (FS-R014). A folder that cannot be created: the path and the reason, exit 1 (FS-R015).

## `framesmith check <project>`

Reads the project files. Starts no browser and no `ffmpeg`. Changes no file.

On a pass:

```text
check passed
  composition  main
  size         1920x1080
  rate         30 fps
  length       6 s
  frames       180
  clips        3
  tweens       4
```

- A rate or length that was not written is followed by `(filled in)`.
- Each clip whose duration was worked out is listed below the facts as `  clip #card  duration 3 s (filled in)`.
- Warnings, when there are any, are listed before `check passed` and the facts.

On a fail, every diagnostic is listed, then one summary line:

```text
error FS-E007 index.html:3
  The composition is 1921 pixels wide; the width and the height must both be even.
  fix: Write an even width and height, for example data-fs-size="1920x1080".

check failed: 1 error, 0 unsupported, 0 unknown, 0 warnings
```

- The first word of a diagnostic is its kind in words: `error`, `unsupported`, `unknown` or `warning`.
- The summary says `check passed`, `check failed`, or `check refused` (only unsupported or unknown uses). When it did not pass, it counts each kind.
- Exit code: 0 when a render would proceed, 1 when anything is an error, 2 when the only blockers are unsupported or unknown uses.
- Diagnostics and the result go to standard output.

## `framesmith render <project> [--output <path>] [--fps <n>] [--json]`

See `render-stages.md` for what a run does and prints.

- Diagnostics from the check and error messages go to standard error. Progress goes to standard output.
- An error message has the form:

```text
error FS-R003
  ffmpeg was not found on the search path.
  fix: Install ffmpeg and make sure the folder that holds it is on PATH.
```

### `--json`

Standard output carries exactly one JSON document, on one line, and nothing else. Progress and messages go to standard error.

Completed (exit 0):

```json
{ "outcome": "completed", "run_id": "9f2c41d07a3e", "output": "/abs/demo/out/demo.mp4",
  "width": 1920, "height": 1080, "fps": 30, "frames": 180, "duration": 6 }
```

Invalid project (exit 1) or refused (exit 2):

```json
{ "outcome": "refused", "run_id": "…",
  "diagnostics": [ { "code": "FS-U001", "kind": "unsupported", "file": "index.html", "line": 9,
                     "description": "…", "remedy": "…" } ] }
```

An invalid project has `"outcome": "failed"` and the same `diagnostics` list.

Failed for another reason (exit 1):

```json
{ "outcome": "failed", "run_id": "…",
  "error": { "code": "FS-R011", "description": "…", "remedy": "…",
             "program": "browser", "stage": "capture", "frame": 41 } }
```

`program`, `stage` and `frame` are present when they apply. When both prerequisites are missing, `error` is the first and `errors` lists both.

Refused at run time (exit 2): `"outcome": "refused"` with an `error` as above, whose code is an `FS-U` code.

Cancelled (exit 130):

```json
{ "outcome": "cancelled", "run_id": "…", "stage": "capture" }
```

Only a completed document has `output`. `duration` is the length in seconds as a number. `output` is an absolute path.

## The log

- Every command writes to `framesmith.log` in the log folder. The folder is `FRAMESMITH_LOG_DIR` when set; otherwise the per-user log folder of the platform (`~/Library/Logs/framesmith` on macOS). In the development workspace the variable points at `logs/`.
- When the file passes 5 MB it is renamed to `framesmith.log.1` (replacing any older one) and a new file is started.
- Each line: a UTC timestamp, the level, the component, `run=<run id>`, and the message with its fields.
- `FRAMESMITH_LOG` sets the level: `error`, `warn`, `info` (the default), `debug`, `trace`.
- Logged in order for a run: the command with its inputs; the creation of the render job; each stage change; the start and exit of each child program; the result. Errors and warnings carry the stage and the inputs needed to reproduce them.
- The log holds no file contents and no page text.
- One run id is shared by every line of a run and by the `--json` document.
- If the log cannot be written, the command prints `logging is unavailable: <reason>` once to standard error and carries on.
