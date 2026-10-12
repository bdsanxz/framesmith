# Commands

Status: v0 (Sprint 1); Sprint 2 ticket T006 added `framesmith info`, ticket T007 added its `--json` form, ticket T008 added `framesmith timeline`, ticket T009 added its `--json` form, ticket T010 added the line for a relative start to `check` and `info`, ticket T011 added the offset to that line, ticket T012 added the error for starts that form a circle (FS-E053) to what `check`, `info` and `timeline` list, ticket T013 added the start of a clip that follows a clip with no written duration (and the error FS-E054 for an end that cannot be worked out) to what they print, ticket T014 added the lanes to `timeline` (grouped in the text form) and to `timeline --json` (the lane of each clip), and a negative lane (FS-E055) to what `check` lists. Sources: proposal `s01-first-render` (CAP-001, CAP-006, CAP-018, CAP-019, CAP-020); proposal `s02-timing-nesting-preview` (CAP-001 for the line of a relative start, CAP-032 for `info`, CAP-035 for `info --json`, CAP-034 and INT-001 for `timeline`, CAP-035 for `timeline --json`, CAP-004 and CAP-002 for the lanes); project rules 5 (logging) and 10 (other programs integrate by running the command with `--json`).

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
- Each clip whose start is written relative to another clip (`data-fs-start="after:<clip id>"`, see `composition.md`, "Relative start") is listed below the facts, after the lines of worked-out durations, as `  clip #outro  start 3 s (after:intro)`: the clip's name, the start it resolved to in seconds, and the form the author wrote in brackets. Only such clips have this line, so a project whose starts are all times prints what it printed before. `info` prints the same lines, from the same function.
- Warnings, when there are any, are listed before `check passed` and the facts.

For the fixture `relative-start`, where `#outro` follows `#intro` (which starts at 1 s and lasts 2 s) and `#echo`, written before the clip it follows, follows `#last` (0.5 s for 0.5 s):

```text
check passed
  composition  main
  size         320x180
  rate         30 fps
  length       5 s
  frames       150
  clips        4
  tweens       0
  clip #outro  start 3 s (after:intro)
  clip #echo  start 1 s (after:last)
```

With an offset, the line gives the written form with the offset, as written (spaces at its ends removed, spaces inside kept). For the fixture `relative-offset`, where `#intro` starts at 1 s and lasts 2 s (so it ends at 3 s), `#later` is `after:intro+0.5`, `#sooner` is `after:intro-0.5` and `#spaced` is `after: intro + 1`:

```text
check passed
  composition  main
  size         320x180
  rate         30 fps
  length       5 s
  frames       150
  clips        4
  tweens       0
  clip #later  start 3.5 s (after:intro+0.5)
  clip #sooner  start 2.5 s (after:intro-0.5)
  clip #spaced  start 4 s (after: intro + 1)
```

An offset that is not a time (FS-E051) and a start that resolves to a time before zero (FS-E052) are errors listed like any other; `render` refuses them with the same list and exit code 1 (`composition.md`, "Relative start"):

```text
error FS-E051 index.html:15
  The offset "+abc" in the start "after:intro+abc" of the clip #outro is not a time in seconds.
  fix: Write the offset as + or - and a number of seconds with at most nine decimals and no unit, for example after:intro+0.5, or write the start as a time in seconds.

check failed: 1 error, 0 unsupported, 0 unknown, 0 warnings
```

Clips whose relative starts follow one another form a chain, and every link of it has its line (for the fixture `relative-chain`, where `#a` starts at 1 s and lasts 2 s, `#b` is `after:a` and lasts 1.5 s, and `#c` is `after:b+0.5`, the lines are `  clip #b  start 3 s (after:a)` and `  clip #c  start 5 s (after:b+0.5)`). Relative starts that depend on each other in a circle are one error, FS-E053, that names every clip of the circle; for the fixture `relative-cycle-two`, where `#first` is `after:second` and `#second` is `after:first+0.5`, with the first of them on line 16 of `index.html`, `check` exits 1 and prints (`render` prints the same list on standard error, exits 1 and starts nothing; `info` and `timeline` print it as `check` does):

```text
error FS-E053 index.html:16
  The starts of #first and #second depend on each other in a circle: #first starts after #second and #second starts after #first, so none of them can be worked out.
  fix: Break the circle: write the start of one of the clips of the circle as a time in seconds, or make it start after a clip that is not in the circle.

check failed: 1 error, 0 unsupported, 0 unknown, 0 warnings
```

A clip may follow a clip that has no written duration (`composition.md`, "Sprint 2 rules for values that are not written", rule 3). After a still image with none (the fixture `follow-still-image`, where `#photo` starts at 1 s) the follower starts three seconds later, and `check` marks the image's duration as filled in:

```text
  clip #photo  duration 3 s (filled in)
  clip #next  start 4 s (after:photo)
```

After a clip that lasts to the end of the composition (the fixture `follow-to-the-end`, length 4 s, where `#never` is `after:base`) the follower starts at the end and can never show; `check` lists the warning FS-W001 for it and passes, and `render` proceeds:

```text
warning FS-W001 index.html:16
  The clip #never starts at 4 s, at or after the end of the composition (4 s), so it never shows.
  fix: Give the clip an earlier start, lengthen the composition, or remove the clip.
```

When the composition has no length for such a clip to last until (the fixture `follow-unknown-end`), the named clip has its error FS-E025 and the clip that follows it has FS-E054, on the line of its `data-fs-start`; `check` exits 1, `render` prints the same list on standard error, exits 1 and starts nothing, and `info` and `timeline` print it as `check` does:

```text
error FS-E054 index.html:15
  The clip #follower starts after #base, which has no duration and lasts until the end of the composition, but the composition has no length for it to last until: none is written, and there is no motion and no clip with its own end to work one out from, so the start of #follower cannot be worked out.
  fix: Write a duration on #base, for example data-fs-duration="2", or a length on the composition, for example data-fs-duration="6", or write the start of #follower as a time in seconds.
```

On a fail, every diagnostic is listed, then one summary line. For a page whose composition element is written on line 8 of `index.html` with `data-fs-size="1921x1080"`:

```text
error FS-E007 index.html:8
  The composition is 1921x1080; the width and the height must both be even.
  fix: Write an even width and an even height, for example data-fs-size="1920x1080".

check failed: 1 error, 0 unsupported, 0 unknown, 0 warnings
```

- The first word of a diagnostic is its kind in words: `error`, `unsupported`, `unknown` or `warning`.
- The summary says `check passed`, `check failed`, or `check refused` (only unsupported or unknown uses). When it did not pass, it counts each kind.
- Exit code: 0 when a render would proceed, 1 when anything is an error, 2 when the only blockers are unsupported or unknown uses.
- Diagnostics and the result go to standard output.

## `framesmith info <project> [--json]`

Reads the project files and prints what Framesmith understands the project to be: the answer to "what will a render of this be?", without rendering. It starts no other program (no browser, no `ffmpeg`), writes nothing into the project (not even the output folder) and uses no network. It takes the project folder and one option, `--json` (see below); any other option or word is refused as `check` and `render` refuse an option they do not have (FS-R004, exit 1). The result is the same whether or not a browser or `ffmpeg` is installed.

On a project that passes the check, every line is a `<label>` padded to 13 columns and then the value (the `clip` lines at the end are the one exception: see below). Nothing is indented, and there is no colour and no control character other than the line feed:

```text
project      /abs/path/demo
composition  main
size         320x180
rate         30 fps (filled in)
length       2 s (filled in)
frames       60
compositions 1
clips        3
tweens       4
output       /abs/path/demo/out/demo.mp4
clip #card  duration 1 s (filled in)
```

- `project` is the project folder as an absolute path.
- `composition` is the name of the root composition; `size`, `rate`, `length` and `frames` are its facts. Today a project has exactly one composition and it is the root. `compositions` counts the compositions of the project (the root included); `clips` and `tweens` count those of the project.
- These facts are worked out by the same code as the facts of `check`, so for one project the two commands never differ: the label and the value of `composition`, `size`, `rate`, `length`, `frames`, `clips`, `tweens` and of every `clip` line (the lines of worked-out durations and the lines of relative starts) are the same text as `check` prints (`check` indents each by two spaces; `info` does not). `info` adds `project`, `compositions` and `output`.
- A value that was not written but filled in has the same mark as in `check`: ` (filled in)` after a rate or length that was not written (the rate is then 30 fps; the length is worked out, see `composition.md`, "Values that are not written"). Each clip whose duration was worked out has a line `clip <label>  duration <n> s (filled in)` after `output`, as in `check`. A written value carries no mark.
- `output` is the path `framesmith render <project>` with no `--output` writes the video to (`out/<folder name>.mp4` in the project). It is printed as an absolute path, made the way `render` makes it: the folder is made absolute against the working folder and nothing is resolved further (no symbolic link is followed, `..` stays). It is the same path as the `output` of the `--json` result of that render. It is absolute so that it does not depend on where the command is run from and can be pasted anywhere; `render` in text form shows the path as it was written on the command line on its `commit` line, which `info` does not copy. `info` does not check whether a file is already there.
- Warnings the check finds are printed before the facts, in the form `check` uses for them, each followed by a blank line.

On a project the check would not pass, `info` prints exactly what `check` prints, text for text, and exits with the same code: every diagnostic, a blank line after each, and the summary line (`check failed: ...` or `check refused: ...`). The facts are not printed. Exit code: 0 when a render would proceed, 1 for any error (a folder that does not exist included), 2 when the only blockers are unsupported or unknown uses.

Everything goes to standard output. The command is logged like the others: its input and its result, under the run id; the log line that starts the command says whether `--json` was asked for.

### `info --json`

`framesmith info <project> --json` (the option may stand before or after the project folder) gives the same answer as one JSON document. It is made from the same worked-out facts as the text form, not from the text, so for one project the two never differ. It reads the project and writes nothing, as the text form does.

- Standard output carries exactly one JSON document, on one line and ended by one line feed, and nothing else, in every outcome (a pass, a project that does not pass, a folder that does not exist, a command line that cannot be read). As in the `--json` form of `render`, the document is a single line; the order of its fields is not significant. It holds no control character other than JSON's own escapes, and no colour: `NO_COLOR`, `CLICOLOR_FORCE`, `FORCE_COLOR` and the terminal change nothing.
- Standard error is empty in a normal run, whatever the outcome of the project. A command line that cannot be read is the one case that writes to it (see below).
- The document has no schema yet; one is planned for Sprint 5. Until then the fields below are the contract of this version, and a field may be added.
- Field names are those of the `--json` document of `render` wherever the same thing is meant: `outcome`, `run_id`, `diagnostics`, `output`, `width`, `height`, `fps`, `frames`, `duration`. Numbers are JSON numbers, never strings; truth values are `true` or `false`; text is a string.

A project that passes (exit 0), here the project of the example above (three clips: `#a` and `#b` with a written duration of 1 s, and `#card` with none):

```json
{ "outcome": "passed", "run_id": "9f2c41d07a3e", "project": "/abs/path/demo", "composition": "main",
  "width": 320, "height": 180,
  "fps": 30, "fps_filled_in": true,
  "duration": 2, "duration_filled_in": true,
  "frames": 60, "compositions": 1, "clips": 3, "tweens": 4,
  "output": "/abs/path/demo/out/demo.mp4",
  "clip_durations": [ { "label": "#a", "duration": 1, "duration_filled_in": false },
                      { "label": "#b", "duration": 1, "duration_filled_in": false },
                      { "label": "#card", "duration": 1, "duration_filled_in": true } ],
  "diagnostics": [] }
```

- `outcome` is `passed`. `run_id` is the run id of the log. `project`, `composition`, `width`, `height`, `frames`, `compositions`, `clips`, `tweens` and `output` are the facts of the text form under the same names: `project` and `output` are absolute paths made as described above; `width` and `height` are whole numbers (the text form joins them as `size`); `frames`, `compositions`, `clips` and `tweens` are whole numbers.
- `fps` is the frame rate, a whole number of frames a second (the text form's `rate`). `duration` is the length of the composition in seconds, a number (the text form's `length`): written without a decimal point when it is whole, as the `duration` of a completed render is, and otherwise as a decimal number, which for a length that is not exact in a decimal fraction is not rounded as the text form rounds it.
- A filled-in value is carried by a value and a separate flag, never by a mark in the value. `fps_filled_in` is `true` when the rate was not written (so it is 30) and `false` when it was written, also when it was written as 30. `duration_filled_in` is `true` when the length was not written and was worked out, `false` when it was written. Both are always present.
- `clip_durations` lists every clip of the composition in document order, each as `label` (how a message names the clip, as in the `clip` lines of the text form), `duration` (seconds, a number as above) and `duration_filled_in` (`true` when the duration was worked out). The `clip` lines of the text form are the entries of this list whose flag is `true`.
- `diagnostics` is the list of warnings the check found; it is `[]` when there are none. A warning does not stop a project from passing (for example FS-W001: a clip that starts at or after the end of the composition never shows). The text form prints the same warnings before the facts.

A project that does not pass (exit 1 for any error, a folder that does not exist included; exit 2 when the only blockers are unsupported or unknown uses; the same as `check`), here a project whose composition is written `data-fs-size="161x90"` (the odd width is on line 8 of its `index.html`):

```json
{ "outcome": "failed", "run_id": "9f2c41d07a3e",
  "diagnostics": [ { "code": "FS-E007", "kind": "error", "file": "index.html", "line": 8,
                     "description": "The composition is 161x90; the width and the height must both be even.",
                     "remedy": "Write an even width and an even height, for example data-fs-size=\"1920x1080\"." } ] }
```

- `outcome` is `failed` (exit 1) or `refused` (exit 2). The facts are not in the document, as the text form does not print them for such a project.
- `diagnostics` is the same list, in the same order, as the diagnostics of `check` and of the text form, and is the same structure as the `diagnostics` of the `--json` document of `render`: each entry has `code`, `kind` (`error`, `unsupported`, `unknown` or `warning`), `file`, `line` (0 when the problem has no line), `description` and `remedy`, the same text as the text form prints. Warnings of a project that fails are in the list too.

A command line that cannot be read (an option or a word the command does not have, or no project folder) together with `--json` is answered as `render --json` answers it: exit 1, one document on standard output with `"outcome": "failed"`, the `run_id` and an `error` that holds `code` (`FS-R004`), `description` and `remedy`; and the same error in the text form on standard error. That is the one case where standard error is not empty. Without `--json` the error goes to standard error and standard output is empty, as before. `check --json` does not exist: `check` refuses `--json` as any other option.

## `framesmith timeline <project> [--json]`

Prints the timeline of the project's composition as text: one header line, then the clips grouped by lane, each lane with a line of its own and one line for each clip under it. It reads the project files through the same check as `check` and `info`, so the times it prints are the clip windows that reading resolved (the start and the duration of each clip, and the end that follows from them); the command works out no time of its own. It starts no other program (no browser, no `ffmpeg`), writes nothing into the project (not even the output folder) and uses no network. The result is the same whether or not a browser or `ffmpeg` is installed.

It takes the project folder and one option, `--json` (see below); nothing else in this version. The header and the lines of a project that passes the check, for the project of the `info` example with its clips named `#c0` and `#c1` (a written start of 0 and a duration of 0.5 s for the first; a written start of 0.5 s and no duration for the second; no written length, so one is worked out from the motion; a written rate of 25):

```text
main  64x36  25 fps  2 s (filled in)  50 frames
lane 0
  #c0  0 s   to 0.5 s  0.5 s (lane filled in)
  #c1  0.5 s to 2 s    1.5 s (filled in) (lane filled in)
```

and, for the same project with `data-fs-track="2"` on `#c0` and `data-fs-track="1"` on `#c1`, where the lanes stand in number order and not in document order:

```text
main  64x36  25 fps  2 s (filled in)  50 frames
lane 1
  #c1  0.5 s to 2 s    1.5 s (filled in)
lane 2
  #c0  0 s   to 0.5 s  0.5 s
```

- **Header.** One line, no indent: the name of the root composition, its size (`<width>x<height>`), its rate, its length and its frame count, each separated from the next by two spaces. The size, rate and length are worded by the same code as the rows `composition`, `size`, `rate` and `length` of `check` and `info`, so they are the same text: the rate carries ` (filled in)` when it was not written (then it is 30 fps), and the length carries ` (filled in)` when it was not written and was worked out. A written value, 30 fps included, carries no mark. The frame count is followed by `frames` (`1 frame` when it is one).
- **Lane lines.** After the header, the lanes in number order (numerically: lane 2 before lane 10), each lane that holds at least one clip once. A lane line has no indent and holds the word `lane`, a space and the number (`lane 0`, `lane 12`), nothing more; the lane's clips follow it, each on a clip line. A lane that holds no clip has no line. A clip with no lane number written is in lane 0 (see `composition.md`, "Lanes"), so a project that writes no lane prints a single `lane 0` line. The header is followed by a lane line whenever the composition has a clip.
- **Clip lines.** One line for each clip of the composition, grouped under the lane the clip is in and, within a lane, in the order the clips stand in the document, indented by two spaces, with four cells: the clip's name, its start, its end and its duration. The name is how a message names the clip (`#card`, or `number 2 <div>` for a clip with no id), the same as the `clip` lines of `check` and `info`. The cells are written `<name>  <start> s to <end> s  <duration> s`: the name, two spaces, the start, a space, the word `to`, a space, the end, two spaces, the duration. The name, the start and the end are each padded on the right with spaces to the width of the widest cell of their column over the whole timeline (the clips of every lane together, the lane lines excepted), so that the columns line up from one lane to the next; the duration is last and is not padded, so no line ends in a space.
- **Times.** Start, end and duration are in seconds, written the way `check` and `info` write a length (an exact decimal when it has at most nine decimals, otherwise rounded to six places, without trailing zeros), followed by ` s`. The end is the start plus the duration. A clip is shown on the frames from its start up to, not including, its end (see `composition.md`); a clip whose window runs past the length of the composition is printed with the end it was given, and the video simply stops at the length.
- **Marks.** A clip whose duration was not written and was worked out (it lasts until the end of the composition) carries ` (filled in)` after its duration, the mark of `check` and `info`. A written duration carries none. A clip with no lane number written carries ` (lane filled in)` at the end of its line, after its duration and the mark of the duration when it has one (`1.5 s (filled in) (lane filled in)`): it is shown in lane 0, and the mark says that 0 was filled in and not written. A clip with a written lane, 0 included, carries no mark. No other cell carries a mark in this version: a start must be written. A start written relative to another clip (`after:<clip id>`) is printed as the time it resolved to, like any start; the form that was written is not shown beside it in this version (a later ticket adds it), and `check` and `info` show it.
- **Warnings.** Warnings the check finds on a project that passes are printed before the header, in the form `check` uses for them, each followed by a blank line, as `info` does. For example a clip that starts at or after the end of the composition (FS-W001) is listed in the timeline all the same.
- **No clips.** A composition with no clips prints the header line only, with no lane line.
- **Two clips in a lane.** Clips that share a lane are all listed under it, also when their windows overlap in time; nothing is printed about it, and the diagnostics, the warnings and the exit code are the same as without the overlap.
- **No effect on a picture.** The lanes group what is printed and nothing else: a clip's start, end and duration are the same in every lane, and a lane never changes a rendered frame (`composition.md`, "Lanes").
- **Nesting.** Clips inside other clips are listed flat in document order (within their lane) with their times from the start of the composition, as the check resolves them, and each clip is in the lane of its own number whatever the lane of the clip around it. Kinds, labels, locks, relative starts, other compositions and the indented listing of a nested composition's clips are later work and are not part of this form.
- **Plain text.** There is no colour and no control character other than the line feed, with or without the colour switches (`NO_COLOR`, `CLICOLOR_FORCE`, `FORCE_COLOR`, the terminal). Everything goes to standard output.

On a project the check would not pass, `timeline` prints exactly what `check` prints, text for text, and exits with the same code (0 only when a render would proceed, 1 for any error, a folder that does not exist included, and 2 when the only blockers are unsupported or unknown uses). The timeline is not printed. This is the failure behaviour of `info` too.

**Words after the project folder.** `timeline` only prints; it has no edit commands. A word after the project folder that is not an option, for example `framesmith timeline demo move`, is refused with error FS-R004 (a word on the command line the command does not have), exit 1, on standard error, with standard output empty. The message names the word and says that `timeline` only prints and has no edit commands:

```text
error FS-R004
  `framesmith timeline` only prints the timeline and has no edit commands; move is not one of its words.
  fix: Run `framesmith --help` to see the commands and their options.
```

No file is opened for writing or changed by it, and the project is not read. An option (a word that begins with `--`) other than `--json` is refused as `info` refuses an option it does not have: `framesmith timeline demo --output x.mp4` gives FS-R004 with `` `framesmith timeline` takes only the path of a project folder; --output is one word too many.``, and an option in place of the folder, or no folder, is refused as `check` refuses it. `--composition`, which belongs to a later ticket, is one of these options for now. The code FS-R004 is kept, and no code of its own is made, because the case is the same as the others it covers (the command line cannot be read) and the message alone carries the difference; the error table of `support-matrix.md` keeps one row for it. With `--json` present, a command line that cannot be read is answered as `info --json` answers it (see `timeline --json`).

The command is logged like the others: the line that starts it holds its input (the project) and whether `--json` was asked for (`json=true` or `json=false`), the line that ends it holds the result (the exit code, the count of each kind of diagnostic, and the number of clips), under the run id.

### `timeline --json`

`framesmith timeline <project> --json` (the option may stand before or after the project folder, as with `info`) gives the same answer as one JSON document. It is made from the same worked-out clip windows as the text form, not from the text: the start, the end, the duration and the marks are taken once, and the text form and the document each write them out, so for one project the two never differ. It reads the project and writes nothing, as the text form does.

- **One document.** Standard output carries exactly one JSON document, on one line and ended by one line feed, and nothing else, in every outcome (a pass, a project that does not pass, a folder that does not exist, a command line that cannot be read), with the same rules as `info --json`: no control character other than JSON's own escapes, no colour (`NO_COLOR`, `CLICOLOR_FORCE`, `FORCE_COLOR` and the terminal change nothing), the order of the fields not significant. Standard error is empty in a normal run, whatever the outcome of the project; the one case that writes to it is a command line that cannot be read (below).
- **No schema yet.** One is planned for Sprint 5. Until then the fields below are the contract of this version, and a field may be added. Later tickets add values to each clip (kinds, labels, locks, metadata, ids, relative forms, nested clips); a program must ignore a key it does not know.
- **Names.** The facts the document shares with `info --json` have the names and the forms they have there: `outcome`, `run_id`, `diagnostics`, `composition`, `width`, `height`, `fps`, `fps_filled_in`, `duration`, `duration_filled_in`, `frames`. Numbers are JSON numbers, never strings; truth values are `true` or `false`; text is a string.

A project that passes (exit 0), here the project of the first example above (a written rate of 25, a length worked out, clips `#c0` and `#c1`), with `data-fs-track="2"` written on `#c0` and no lane written on `#c1`:

```json
{ "outcome": "passed", "run_id": "9f2c41d07a3e", "composition": "main",
  "width": 64, "height": 36,
  "fps": 25, "fps_filled_in": false,
  "duration": 2, "duration_filled_in": true,
  "frames": 50,
  "clips": [ { "name": "#c0", "start": 0,   "end": 0.5, "duration": 0.5, "duration_filled_in": false, "lane": 2, "lane_filled_in": false },
             { "name": "#c1", "start": 0.5, "end": 2,   "duration": 1.5, "duration_filled_in": true,  "lane": 0, "lane_filled_in": true } ],
  "diagnostics": [] }
```

- `outcome` is `passed`; `run_id` is the run id of the log.
- `composition`, `width`, `height`, `fps`, `fps_filled_in`, `duration`, `duration_filled_in` and `frames` are the header of the text form: the name of the root composition, its size (the text form joins `width` and `height` as `<width>x<height>`), its rate (a whole number; `fps_filled_in` is `true` when it was not written and is 30, `false` when it was written, also when written as 30), its length in seconds (`duration`; `duration_filled_in` is `true` when it was not written and was worked out) and its frame count. The mark ` (filled in)` of the text form is never inside a value of the document; it is the flag of its own.
- `clips` is the list of every clip of the composition, in the order the clips stand in the document, and `[]` for a composition with no clips. The list is not grouped: the text form groups the clips by lane and the document gives each clip its `lane`, so a program that wants the groups sorts on it (a stable sort on `lane` keeps the document order within a lane, which is the order of the text form). In this version each entry has exactly these keys: `name` (how a message names the clip, `#card` or `number 2 <div>`, the name column of the text form; the key is `name` and not `label` because a later ticket gives the clip an author's own label as a separate value), `start`, `end` and `duration` (seconds, numbers), `duration_filled_in` (`true` when the duration was not written and was worked out, the clip's mark in the text form; `false` for a written duration), `lane` (the clip's lane, a whole JSON number of 0 or more, never a string: the number written, or 0 when none was) and `lane_filled_in` (`true` when no lane number was written and 0 was filled in, the clip's ` (lane filled in)` mark of the text form; `false` for a written lane, a written 0 included). The end is the start plus the duration. A clip that starts at or after the end of the composition is in the list like any other, as in the text form; a clip whose window runs past the length has the end it was given.
- `diagnostics` is the list of warnings the check found, in the order the text form prints them before the header; `[]` when there are none. A warning does not stop a project from passing. Each entry has the structure of `info --json` (`code`, `kind`, `file`, `line`, `description`, `remedy`).
- The document holds everything the text form shows and nothing it does not: it has no `project` and no `output`, which the text form of `timeline` does not print.

**Times.** Each time of the document is the number the text form prints for the same clip and the same cell. The reader accepts a written time with at most nine decimals (and at most nine digits before the point) and refuses a longer one (FS-E021, FS-E023, FS-E009, FS-E045), and every other time is a sum or a difference of such times, so every time the document holds is an exact decimal fraction with at most nine decimals; the text form writes every one of them exactly, and the rule of the text form that rounds to six places a value with more than nine decimals is never reached by a time of a clip. A time is written in the document as the shortest JSON number that is the same binary 64-bit number as the printed text read as a number; a whole number of seconds has no decimal point (the conversion of `info --json` and `render --json`, which write their numbers the same way). So "equals the time printed" means: the printed text and the number of the document are the same number, and they are the same digits for any time of up to fifteen significant digits (the precision of a binary 64-bit number); beyond it (a start of 999999999.999999999, for example) the printed text keeps every digit and the number of the document is the nearest 64-bit number to it (here the whole number 1000000000), which is the number a program gets by reading the printed text as a number. A very small time is written the way JSON allows (a duration of 0.000000001 s is `1e-9`). This case is far outside any real project (a clip that starts thirty years in), and it is tested (`s2_t009_c3_the_largest_times_...`).

A project that does not pass (exit 1 for any error, a folder that does not exist included; exit 2 when the only blockers are unsupported or unknown uses; the same as `check`), here the project of the `info --json` example:

```json
{ "outcome": "failed", "run_id": "9f2c41d07a3e",
  "diagnostics": [ { "code": "FS-E007", "kind": "error", "file": "index.html", "line": 8,
                     "description": "The composition is 161x90; the width and the height must both be even.",
                     "remedy": "Write an even width and an even height, for example data-fs-size=\"1920x1080\"." } ] }
```

- `outcome` is `failed` (exit 1) or `refused` (exit 2). `diagnostics` is the same list, in the same order and with the same text, as the diagnostics of `check` and of the text form (warnings of a project that fails are in it too), and is the structure of `info --json`. The header and the clips are not in the document, as the text form does not print them for such a project.

**A command line that cannot be read** (an option other than `--json`, a word after the project folder such as `move`, a second folder, or no folder) together with `--json` is answered as `info --json` answers it: exit 1, one document on standard output with `"outcome": "failed"`, the `run_id` and an `error` that holds `code` (`FS-R004`), `description` and `remedy`; and the same error in the text form (the message of the text form for that command line, the edit-word message for a word after the folder included) on standard error. The project is not read. Without `--json` the error goes to standard error and standard output is empty.

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
