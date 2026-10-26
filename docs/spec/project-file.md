# The project file: `framesmith.yaml`

Status: in force from Sprint 1, for the reading and judging of the file by `framesmith check`. Sources: the product owner's request of 2026-10-12 (a project file named `framesmith.yaml`, `PLAN.md` section 3 and decision "Project file" of `DECISIONS.md`); `ROADMAP.md` line 1.10 (the keys, every key optional, an unknown key an error); the text of the ticket for this file (the list of keys, their defaults, the empty file, the two keys left to Sprint 2); `notation.md` section 3 (a project is a folder; every problem is reported at once, each with its file and line) and section 14, S11 (the keys `compositions` and `timeline` wait for Sprint 2); decision P2 of `DECISIONS.md` (the exit codes of `check`); `check-command.md` and `error-catalogue.md` (the form of a report and the codes); `run-log.md` (what is logged); the YAML 1.2.2 specification of yaml.org (chapter 5.2 for the text encoding and the byte order mark; section 3.2.1.1 for the rule that the keys of a mapping are unique; chapter 10.3 for the core schema, which says which plain words are numbers, truth values and null); the documentation of the outside crate `saphyr` (the reader, and the position it gives for each node). The proofs of concept's text of the same name (`_swe/poc/tree/docs/spec/project-file.md`, a record read under decision B4) is the origin of the list of keys, their kinds and their defaults, taken as behaviour and not as text. Where this file and `notation.md` say the same thing, `notation.md` is the text and this file only points to it.

The project file is how an author keeps the settings of a project instead of repeating them. This version reads it and judges every key; what a render setting does is the business of the render commands that take it (decisions of the render tickets), and no key is applied here.

## 1. What it is

- A project is a folder. `framesmith.yaml` directly in that folder holds the settings that belong to the project and not to one composition: which composition is rendered, where finished files go, and how they are rendered and encoded.
- **The file is optional, and so is every key in it.** A folder that holds a `composition.xml` and no project file is a complete project. An empty `framesmith.yaml` is a valid project file and means the same.
- **The project file never changes what the video is.** The size, the frame rate, the length, the elements, the motion and the sound belong to the composition. A setting that would change the picture or the sound is not a key of this file; if it is written, it is an unknown key (section 4).
- **An option on the command line wins over the project file, and the project file wins over the default.** The options are those of `render` (`render-command.md` sections 1 and 1.1): `--output`, `--backend`, `--quality`, `--encoder`, `--codec` and `--workers`, each replacing the one key it names for one run; the file is never written by them. `check` has none.
- **Nothing is ignored in silence.** A key that is not listed, a value of the wrong kind and a value outside the values of its key are each reported with their line (section 5). A key that is known and not yet supported is reported as such.

## 2. Which file is read

`framesmith check <path>` reads `framesmith.yaml` directly in the project folder, opened for reading only. The project folder is the folder given, the folder of a `framesmith.yaml` given, or, for a composition file given, the nearest folder at or above it that holds a `framesmith.yaml` (`project-location.md`). A folder given is never searched upward. A project folder with no `framesmith.yaml` is a project whose every key has its default (section 6).

The file is read as text and must be valid UTF-8. A byte order mark at the very start (`U+FEFF`) is allowed and is not part of the first key; it does not change a line number.

## 3. The keys

A key written in the table with a dot is a key inside a group: `render.workers` is the key `workers` in the group `render`.

```yaml
name: launch-film
composition: scenes/main.xml
paths:
  renders: renders
render:
  output: renders/launch-film.mp4
  backend: auto
  quality: standard
  encoder: hardware
  codec: h264
  workers: auto
  audio-bitrate: 192
```

| Key | Kind | Accepted | Default when not written |
|---|---|---|---|
| `name` | text | Text that is not empty and has no `/`; it names the finished file | The name of the project folder (section 6) |
| `composition` | path | A path (below) of the composition file | `composition.xml` |
| `paths.renders` | path | A path of the folder finished files go to | `renders` |
| `render.output` | path | A path of the finished file | `<paths.renders>/<name>.mp4` |
| `render.backend` | word | `auto`, `cpu`, `gpu` | `auto` |
| `render.quality` | word | `draft`, `standard`, `high` | `standard` |
| `render.encoder` | word | `hardware`, `software` | `hardware` |
| `render.codec` | word | `h264`, `hevc` | `h264` |
| `render.workers` | `auto`, or a whole number | `auto`, or a whole number from 1 to 16 | `auto` |
| `render.audio-bitrate` | whole number, in kbit/s | A whole number from 1 to 4294967295 | `192` |
| `compositions`, `timeline` | | Not supported in this version (section 5.4) | |

The groups are `paths` and `render`. A group holds the keys of the rows above that begin with its name, and nothing else. No key is nested deeper than one group, and a key of a group is not accepted at the top of the file: `render.workers: 8` written at the top is the key `render.workers`, which is not a key (section 5.3), and its message says how to write it.

What the values mean (for the commands that apply them): `render.backend` names the renderer (`cpu` defines the correct picture, `gpu` must agree with it within the tolerance of `PLAN.md` section 5, `auto` takes the card's renderer when the build has it and a suitable adapter is found); `render.quality` says how hard the video encoder works; `render.encoder` says which encoder writes the picture; `render.codec` is the codec of the picture; `render.workers` is how many parts of the film are rendered at once; `render.audio-bitrate` is the bit rate of the encoded sound. This version applies `render.backend` (`render-command.md` section 1.2), `render.audio-bitrate` (section 5.2), `render.codec` (section 5.3), and `render.encoder` and `render.quality` (section 5.4); it reads `render.workers`, and refuses before the first frame a value of it that it does not apply (`render-command.md` section 3).

### 3.1 The kinds

A written value has the type YAML's core schema gives it, so that a value is judged by what the author wrote and not by a guess. The core schema reads a plain word as: a whole number (`17`, `0x10`), another number (`1.5`), `true` or `false`, `null` or `~` or nothing at all, and otherwise as text. A word in single or double quotes is always text.

| Kind | Accepted |
|---|---|
| text | A word that the schema reads as text, quoted or not. A number, `true`, `false` and `null` written plain are not text; in quotes they are (`name: "2026"`). The text is not empty after white space is removed from its ends |
| path | Text, as above, that is not empty. It is relative to the project folder and written with `/`; a path that begins with `/` is a path of the machine and is taken as it is |
| word | Text equal, letter for letter (capitals matter), to one of the words of the key. In quotes or not: `codec: "hevc"` is `hevc` |
| `auto` or a whole number | The text `auto`, or a whole number written plain, in digits, from 1 to 16: `8`, `08`. `"8"` in quotes is text and not a number; `8.0`, `0x8` and `+8` are not accepted either, as written |
| whole number | A whole number written plain, in digits, in the range of the key |
| group | A mapping of keys. A group with no value (`render:` and nothing under it) holds no key and is the same as not writing it |

A key given a value of another kind is reported with the form the key takes (section 5.5). A key given no value (`name:` and nothing after it, `name: null` or `name: ~`) is reported the same way, for every key but a group.

## 4. The project folder and the composition

- **The composition** is the file named by `composition`, or `composition.xml` when the key is not written. The path is relative to the project folder, so a composition may sit in a sub-folder (`scenes/main.xml`); the `src` of its media remain relative to the project folder, not to the composition (`notation.md` section 3; `project-location.md` section 4). When the command line gives a composition file, that file is the composition and `composition` is not used to choose it (`project-location.md` section 1).
- **The name of a file in a report** is its path as it is written: `framesmith.yaml`, `composition.xml` or `scenes/main.xml`. The reports of the project file are printed before those of the composition.
- **The composition is read whenever it can be known.** That is the case for a project file that is absent, empty, or whose `composition` key is right or is not written, even when other keys of the file are wrong. It is not the case when the file could not be read as YAML or is not one mapping (sections 5.1 and 5.2), or when the value of `composition` is itself wrong: nothing says which file the author means, so none is read and the one report stands for the whole. The default `composition.xml` is never read in place of a composition that was named wrongly.
- **A composition that is named and is not there** is a reason of `No project`, like a folder without a `composition.xml` (`check-command.md` section 5): `FS-0025` when the path does not exist, `FS-0026` when it exists and could not be read, which includes a folder at that path. Their lines name the path and the line of the `composition` key. The default `composition.xml` keeps `FS-0015` and `FS-0016`. When the project file also has reports, they are printed first, then the `No project` line.

## 5. What is reported

Every problem of the project file is reported in the same run, whatever the others are, each with the file `framesmith.yaml`, its line, its code, its kind, one sentence on what is wrong and the remedy of the code (`error-catalogue.md` section 3; `check-command.md` section 4). Two cases stop earlier, because nothing else can be judged: the file cannot be read as text or as YAML (5.1), and the file is not one mapping (5.2). In the other cases every key is judged on its own. The reports of the file are printed in the order of their places in it, a line and then a column. A value shown in a message is shown as it was written, white space removed, one line, with the rules of `check-command.md` section 4 (a control character as `\u{...}`, 80 characters and then `...`).

### 5.1 A file that cannot be read as text or as YAML

- A file that is not valid UTF-8 is `FS-0018`, on the line of the first byte that is not.
- A file that is not YAML is `FS-0019`, **once**, with the reader's reason and the line the reader gives. The reader stops at the first fault and says nothing of the rest, so one fault is one report. Among the faults the reader finds are: a flow list or group not closed, a tab where indentation is, a quote not closed, and a key written twice in one group (the YAML specification does not allow a mapping to repeat a key).
- No other report is made for the file, and the composition is not read (section 4).

### 5.2 A file that is not one mapping of keys

An empty file, a file of comments, and a file of `---` alone hold no document or an empty one, and are valid. A file with a document that is a list or a single value, or with more than one document, is `FS-0021`, on the first line of the list or value, or of the second document. Nothing else is reported for it and the composition is not read.

### 5.3 A key that is not listed

A key that is not in the table of section 3 is `FS-0022`, on the line of the key: at the top, any key but `name`, `composition`, `paths`, `render`, `compositions` and `timeline`; in `paths` or `render`, any key but its own. The message names the key as the author wrote it (`render.colour` for a key `colour` in `render`) and lists the keys. A key that is not a word (a number, a list) is also an unknown key, shown as it was written. The message of a key at the top that is written with a dot and is a key of a group (`render.workers: 8` written flat) says how to write it: ``render.workers` is not a key at the top of the file; write `render` as a group and `workers` under it`. The file is YAML, so the dotted names of the table are written as a group and the key under it, as in section 3; the dotted name is only how this spec and the messages call a key.

### 5.4 A key that is not supported yet

`compositions` and `timeline` are keys of Sprint 2 (`notation.md` section 14, S11). They are `FS-0023`, of the kind unsupported, on the line of the key; their values are not judged, since what they may hold is not specified. A project that has only reports of the kind unsupported is a project whose rest is valid, and `check` ends with the exit code 2 (decision P2), not 1.

### 5.5 A value of the wrong kind or outside its values

A key whose value is not of the kind of the key, or is outside the words or the range of its key, is `FS-0024`, on the line of the value (the line of the key when it has none). The message names the key with its group, says what the key takes, and shows what was written, so that the values taken and the value found are both in the report:

- ``render.codec` must be one of h264, hevc, not vp9`
- ``render.workers` must be auto or a whole number from 1 to 16, not 17`
- ``name` must be text that is not empty and has no `/`, not 2026, which YAML reads as a number; put it in quotes to use it as text`
- ``render` must be a group of keys, not fast`
- ``render.codec` has no value; it must be one of h264, hevc`
- ``render.audio-bitrate` must be a whole number of kbit/s from 1 to 4294967295, not 0x10`
- ``compositions` is not supported in this version; it comes in Sprint 2` (`FS-0023`)
- ``render.colour` is not a key of the project file; the keys are: name, composition, paths.renders, render.output, render.backend, render.quality, render.encoder, render.codec, render.workers, render.audio-bitrate` (`FS-0022`)

A value that is a list or a group where a single value is taken is shown as `a list` or `a group of keys`. The value shown is the one the author wrote, quotes included, so `0x10` is shown as `0x10` and not as 16, and `"8"` as `"8"`.

## 6. The defaults

A key not written takes its default of the table of section 3, and the file never has to repeat a default. `render.output`, when it is not written, is the path `<paths.renders>/<name>.mp4`, where `paths.renders` and `name` are the values in use, written or default. The default `name` is the name of the project folder as the system gives it for the folder (its last component after the path is made absolute and followed to the real folder); for a folder that has no name of its own, such as the root of a disk, it is `project`. The default output is a path relative to the project folder and is not created by `check`.

## 7. What `check` prints and how it ends

The lines and the exit codes are those of `check-command.md` section 5, with the files that have reports named at the end:

| Outcome | Standard output | Exit code |
|---|---|---|
| The project file and the composition are valid (the file absent or empty included) | `Ready: the project is valid, 150 frames, 1280x720, 30 fps` (the root of the composition that was read) | 0 |
| A project file or a composition has a report of the kind invalid, or a failure of the kind failure | Every report, then ``Invalid: the project is not valid (framesmith.yaml, composition.xml)``: the files that have reports, in the order the reports are printed | 1 |
| Every report is of the kind unsupported | Every report, then ``Unsupported: the project uses what this version does not support (framesmith.yaml)`` | 2 |
| `framesmith.yaml` exists and could not be read, or the composition is not there | One `No project:` line (`FS-0020`, `FS-0013` to `FS-0016`, `FS-0025`, `FS-0026`), after the reports of the project file if it has any | 1 |

`check` changes nothing of the project; it opens `framesmith.yaml` and the composition for reading only (`check-command.md` section 6).

## 8. Where the code lives

- `notation` holds the codes and the problems (`Code`, `Problem`) and the rule for showing a written value; it does not know the project file.
- `project` reads `framesmith.yaml` (`file`), holds the settings with their defaults (`Settings`), and opens the project: the file, then the composition it names, then the root of that composition (`Project::open`). It gives every report of both files in the order of section 4.
- `cli` prints, and sets the exit code.
- The YAML reader is the outside crate `saphyr` (MIT OR Apache-2.0), named in `PLAN.md` section 3, with its default features off (they add a reader of other encodings, which the product does not need: the file is UTF-8). No type of it appears in a public interface of `project`.

## 9. What the proofs' text had, and what is not taken

The list of keys, their kinds and their defaults are those of the proofs' text, with `render.audio-bitrate` given a range (1 to 4294967295 kbit/s; what a sound encoder can take is judged when the sound is encoded, by the ticket for the sound's bit rate), and the rules of section 3.1 and section 5 written new. Not taken, and not built: the list of what the concept source's project file has that this one does not, fonts shared by every composition, the number of frames drawn at once, and output formats other than MP4. `compositions` and `timeline` are Sprint 2.
