# The run log: one log line for every step of a command, under one run id

Status: in force from Sprint 1. Sources: `CLAUDE.md` rule 5 (the log: `tracing`, a rotating file appender, `<cwd>/logs/framesmith.log`, the level from an environment variable, a timestamp, a level, a component name and a run id on every line, no sensitive data, no noise; the log is internal and never evidence; a development build sends nothing) and rule 11 (telemetry is a production feature with its own spec); decisions A2 (the log goes under the folder the command is run from, `<cwd>/logs/framesmith.log`), D11 and D36 (telemetry waits for Sprint 26), P6 (no network use in this work) and A29 (the method of watching a process) of `DECISIONS.md`; `PLAN.md` section 3 (which crate holds what) and the Sprint 1 list ("logging under one run id", `ROADMAP.md` line 1.44); RFC 3339 section 5.6 (the form of the timestamp); the documentation of the `tracing`, `tracing-subscriber` and `tracing-appender` crates; `check-command.md` (the command that is logged) and `error-catalogue.md` (the codes that appear in the log).

The run log is how a maintainer confirms that the expected events happened in the right order. It is written by the `framesmith` command for its own run, for a maintainer; it is not an output of the command, it is never read by the product, and what it holds is never copied into evidence or into the records (rule 5): only the conclusion of reading it is. This version logs the command line and its result, and the steps the project part takes on the way. The events of a render (the job, its stages, the workers, each decoder and encoder) are added by the render work that has them.

## 1. Where the log is kept

- **The file is `logs/framesmith.log` under the folder the command is run from** (decision A2): the working folder at the moment the command starts. The project does not choose it: it is the same place whether the project is in that folder, in another place of the machine or on another disk. From the workspace's root the log is `logs/framesmith.log` of the workspace, which is not under version control.
- **Two cases, for a project folder:**
  1. *The command is run from another folder* (the usual case: the workspace's root, a work folder, a script's folder). The log is under that folder, and nothing in the project folder is created or changed: its files keep their names and bytes and it holds no new file or folder (`check-command.md` section 6).
  2. *The command is run from the project folder itself, or from a folder inside it* (`cd demo && framesmith check .`). The log is `logs/framesmith.log` under that folder, by decision A2, so the project folder (or the folder inside it) comes to hold a folder `logs` with the file `framesmith.log`. That is the one thing added: every other file of the project keeps its name and bytes, `check` reads `framesmith.yaml` and the composition only, and its result (what it prints and its exit code) is the same as from outside. A project that is to stay without a `logs` folder is checked from another folder.
- The folder `logs` is made when it is missing. The file is opened for appending: a run adds lines at the end and changes none that is there. Each line is one write, so that two runs at the same time leave whole lines.
- **The log is opened when the command starts**, before the command line is read, so that a command line that cannot be read is logged as well. A run that writes no line at its level (section 5) still opens the file, which is then empty.
- **Rotation.** When the file is larger than 5 MiB (5,242,880 bytes) at the start of a run, it is renamed `framesmith.log.1` first, `framesmith.log.1` becomes `framesmith.log.2`, `.2` becomes `.3`, and a `framesmith.log.3` that is there is removed; the run then starts a new `framesmith.log`. Three older files are kept, so the log never grows beyond about 20 MiB. A rename that fails does not stop the run: the log goes on in the file that is there.
- The working folder, the project folder and the home folder are never written into the log as such (section 6).

## 2. The run id

Every run makes one run id when it starts, before anything is logged: 16 lowercase hexadecimal digits, such as `3fa94c01d27be8a5`. It is made from the system's random numbers, the time and the process, and from nothing the command reads, so it says nothing about the project. Two runs have different ids (a collision is a chance of one in 2^64). **Every line of the run carries the id of the run**, whichever part wrote it, and no line of another run carries it.

## 3. A line

A line is plain text, on one line, in this order, with one space between the parts:

```
2026-10-14T09:15:02.123456Z INFO  cli run=3fa94c01d27be8a5 command started version="0.1.0" args=["check","demo"]
```

1. **The time**: the time of the event in UTC, `YYYY-MM-DDTHH:MM:SS.ffffffZ` (RFC 3339, microseconds, the letter `Z`).
2. **The level**: `ERROR`, `WARN`, `INFO`, `DEBUG` or `TRACE`, left-aligned in five characters (so `INFO` is followed by two spaces).
3. **The component**: the name of the part of the program that wrote the line. It is a crate of the workspace, such as `cli` or `project`, or a module in one, such as `cli::runlog`. It is never empty.
4. **The run id**: `run=` and the id of section 2.
5. **The message**: a short fixed sentence of the program, which says what happened.
6. **The fields**: zero or more `name=value` parts. A number is written as it is. A text is written between double quotes, with a backslash before a double quote and before a backslash, and `\n`, `\r`, `\t` or `\u{...}` for a control character, so that **a line never holds a line feed, and one event is one line**. A list is written between square brackets with its texts quoted and separated by commas.

The line holds no colour or other control character. Its reader needs no tool: the first four parts are separated by spaces and `grep` finds a run by its id.

## 4. What a command logs

Rows are in the order they are written. Everything else in the table of section 5 is the same for all rows.

| Level | Component | Message | Fields | When |
|---|---|---|---|---|
| info | `cli` | `command started` | `version` (the program's version), `args` (the words of the command line after the program's name, each as section 6 allows) | First line of every run, also when the command line cannot be read |
| warn | `cli::runlog` | `the level in FRAMESMITH_LOG is not one of error, warn, info, debug, trace; info is used` | none | Only when the variable is set to a value that is not a level (section 5); the value is not written |
| debug | `project` | `project located` | `form` (`folder`, `project file` or `composition file`), `folders_searched` (how many folders were looked in for a `framesmith.yaml`: 0 for the first two forms) | The path given has been examined and the project folder found (`project-location.md` section 7); before any file of the project is read |
| debug | `project` | `project file read` | `bytes` (its size) | The folder has a `framesmith.yaml` and it was read |
| debug | `project` | `project file judged` | `problems` (how many) | The project file was read as text and judged |
| debug | `project` | `composition file read` | `bytes` (its size) | The composition the project names (`composition.xml` when it names none) exists and was read |
| debug | `project` | `root judged` | `problems` (how many) | The composition file was judged: its root, and the form of everything under it |
| debug | `cli` | `settings chosen` | `options` (how many settings an option gave) | `render` took the settings by the rule of `render-command.md` section 1.1, before the scene is made |
| debug | `render::scene` | `scene made` | `boxes` (how many boxes the scene holds), `frames` | `render` made the scene of an opened project that this version draws (`render-command.md` section 3) |
| info | `render::film` | `render started` | `frames`, `width`, `height`, `fps`, `renderer` (`processor`) | The first frame is about to be drawn |
| debug | `render::film` | `encoder started` | `hardware` (whether the machine's hardware encodes) | The system's encoder was started |
| debug | `render::film` | `sound encoder started` | `sample_frames` (the length of the mix), `bitrate` (bits a second) | The system's AAC encoder was started for the sound track (`render-command.md` section 5.2) |
| debug | `render::film` | `sound encoder finished` | none | The AAC encoder gave its last packets |
| info | `render::film` | `render finished` | `frames`, `bytes` | The file is complete |
| trace | `cli` | `report` | `file` (the name of the file, as the report prints it), `code`, `line` | Once for each report of an invalid or unsupported project, in the order printed |
| info, warn or error | `cli` | `command finished` | `outcome`, `exit` (the exit code), and `problems` (how many reports) for `invalid` and `unsupported`, and `code` for `no project` with `problems` when the project file had reports before it, `frames` for `rendered`, `code` for `video not made` | Last line of every run |

The outcome and the level of the last line:

| `outcome` | Level | `exit` | Meaning |
|---|---|---|---|
| `ready` | info | 0 | `check` found the project valid |
| `rendered` | info | 0 | `render` wrote the film; `frames` is how many frames it holds |
| `video not made` | error | 1 | `render` could not make the film; `code` is `FS-0036` |
| `invalid` | warn | 1 | The project file or the composition has problems (for `render` also `FS-0034`); the reports were printed |
| `unsupported` | warn | 2 | The only reports are of the kind unsupported (`FS-0023`, and `FS-0035` for `render`); they were printed |
| `no project` | warn, or error when the code is of the kind `failure` | 1 | There is no composition to read; `code` is the code of the reason (`FS-0013` to `FS-0016`, `FS-0020`, `FS-0025` to `FS-0028`) |
| `command line not read` | warn | 1 | The words after the program's name are not a command (`FS-0017`) |

So a maintainer finds, for a `check`, a line for the command with its inputs and a line for its result under one run id, at the default level; the steps in between are there at `debug` and `trace`. A line states a code and a count, never a message: the messages of a report quote the file's values and are printed to the author, not kept (section 6).

## 5. The level

- **The environment variable `FRAMESMITH_LOG` sets the level.** Its value is `error`, `warn`, `info`, `debug` or `trace`, in any case, with white space around it removed. A line is written when its level is the chosen level or one that is more serious: at `warn` the `WARN` and `ERROR` lines, at `info` those and `INFO`, at `trace` all. From the quietest to the most detailed the order is `error`, `warn`, `info`, `debug`, `trace`.
- **The default is `info`**: the variable not set, or set to an empty value.
- **A value that is not a level** is read as the default, and the run's log says so in one `warn` line (section 4), which does not write the value, since the value is read from the environment (section 6). It is not told on standard error: the log is the place for it.
- The level applies to every part of the program that writes to the log, not to the command alone.

## 6. What the log never holds

The log is kept on the machine of the author, and it is shared in a bug report more than a file of the project is. It is made so that it holds nothing of the author that a maintainer does not need.

1. **No content of a composition.** Nothing read from a file of a project is written: not a text, not an attribute's value, not the `id`, not a message of a report (which quotes the value that was wrong). The log has the code, the line and the count; the message is printed by the command and is not kept.
2. **No value from the environment.** The log holds no value of any environment variable: not `FRAMESMITH_LOG`, not `HOME`, not `USER`, not `PATH`, not a variable of the runner. The program reads the variables it needs and writes none of their values.
3. **No home-folder name.** A word of the command line that is a path is written with the home folder part replaced: a path under the home folder of the user who runs the command begins `~/`; a path under the home folder of any other user (`/Users/<name>/...` or `/home/<name>/...`) begins `~other/`, so another user's name is never written. Any other path is written as it was given. The working folder is not written at all, and neither is the project folder as the command found it; the argument is written as it was typed (with the replacement above).
4. **Nothing is sent.** See section 8.

## 7. When the log cannot be written

The run log must never stop a command. If the log cannot be opened, or a write to it fails at any time in the run, because the folder cannot be made, `logs` is a file, `logs/framesmith.log` cannot be opened for appending, the disk is full, or the working folder cannot be found:

- the command runs on as if the log were there: it does the same work, prints the same lines on standard output, and ends with the same exit code, whatever the log's state;
- the run writes **one line on standard error**, after the command's own output and once however many writes fail, and no other line about the log:

```
framesmith: the log is unavailable: logs/framesmith.log under the folder you ran from could not be written. The command ran as usual; make that place writable to keep a log.
```

- nothing else is done with the log in that run: no other place is tried, and a line that could not be written is not written later.

The line has no code. A code is for a refusal or a failure of a command (`error-catalogue.md` section 1), and here the command does not refuse or fail; it is a notice about the record of the run, and the exit code is not changed by it. The line does not say why; the log is internal, and the reason a place cannot be written (a read-only folder, a full disk) shows to the person who looks at the place. A run that logs nothing at its level (section 5) and whose place can be opened prints no line.

## 8. No network

A run opens no network connection and sends nothing from the machine (decisions P6, D11): the log is a file on the machine and nothing reads it but a person. The program uses only crates that write to files (`tracing`, `tracing-subscriber`, `tracing-appender`), and no part of it calls a network function. Usage telemetry is a feature of a production build with its own spec (decision D36, Sprint 26) and is not built. The test that guards this finds that the program binds no function of the system's network interface; the check of a run is made by running the command under a system sandbox profile that forbids a network connection (decision A29), with a control that shows the profile stops one.

## 9. Where the code lives

- `cli` starts the log (`runlog`): it reads `FRAMESMITH_LOG`, makes the run id, rotates and opens the file, installs the subscriber that formats a line as section 3 says for the length of the run, writes the lines of the command (section 4), and prints the line of section 7. It is the only part that knows the file's place.
- `project` writes its own `debug` lines (section 4) with the `tracing` macros and knows nothing of the file or the level. A part that writes no line needs nothing from the log.
- Outside crates: `tracing` (MIT), `tracing-subscriber` (MIT) and `tracing-appender` (MIT), with the fewest features the work needs; no type of theirs appears in a public interface of our crates. The file is appended through the appender of `tracing-appender`, which opens the file in append mode; the rotation of section 1 is done by the program before it opens the file, since the appender rotates by the date and the owner's word is one file name (decision A2).
- Every line is made by one function, so that every part's lines have the same form; a part never writes its own timestamp, level or run id.
