# The `check` command: the root of a composition

Status: in force from Sprint 1, for the first form of `framesmith check`. Sources: `notation.md` (section 2 for the version, section 3 for the project folder, the root and the rule that every problem is reported at once, section 4 for the value forms, section 5.1 for the frame count, section 6.1 for the root's table, N6 for the size, N8 for times, N9 for numbers, section 7.4 and Q5 for the root's `background`, section 13 for the wording of the messages); `frame-time.md` (the frame grid rule, done by the `timebase` crate); decisions P1 and P2 of `DECISIONS.md` (the version rule, the exit codes) and A29 (the root's name is `id`); `PLAN.md` section 3 (which crate holds what); `CLAUDE.md` rules 1 and 5. `error-catalogue.md` (the codes, their kinds and remedies, and the form of a report); `project-file.md` (the project file, its keys and what is reported for it); `project-location.md` (the three forms of the argument and the project folder). Where this file and `notation.md` say the same thing, `notation.md` is the text and this file only points to it.

This version of `check` reads the project file `framesmith.yaml`, when the project folder has one, and the root of the one composition file the project names (`composition.xml` unless the project file names another, or the composition file the command line gives), and nothing else. It is the first step of the author's journey: a folder with a `composition.xml` is a project, and the command says whether its settings and its root are ones a render could use. When they have problems it reports all of them in one run, each with a code, its file and its line (sections 4 and 5).

## 1. The command line

```
framesmith check <path>
```

- **One argument, the path of the project**: a folder, a `framesmith.yaml`, or a composition file. `project-location.md` says how the project folder is found from each (a folder is the project folder; a project file makes its folder the project folder; a composition file belongs to the nearest folder at or above it that holds a `framesmith.yaml`, or to its own folder when none does) and that every path written in the project is relative to the project folder. A folder that holds a `composition.xml` and no project file is a complete project. The project file (`framesmith.yaml`) directly in the project folder is read when there is one, and `project-file.md` says what it holds and what is reported for it.
- **Anything else is a command line that cannot be read**: no word at all, a word other than `check`, `check` with no path, `check` with more than one word after it, or an option. The command prints `framesmith: ` and a statement of the code `FS-0017` (section 4: the code, its kind, one sentence saying what is wrong and the remedy), then the line `usage: framesmith check <path>`, both on standard error, prints nothing on standard output, and exits with code 1. No option is taken yet (`--json` is Sprint 6).

## 2. What is read

- The file `framesmith.yaml` directly in the project folder, when there is one, opened for reading only (`project-file.md` sections 2 and 5; `project-location.md` sections 1 and 3 for the project folder).
- The composition file the command line gives, or else the one the project names with the key `composition`, as a path from the project folder (`project-file.md` section 4), or else `composition.xml` directly in the project folder; opened for reading only.
- Its text must be UTF-8. The first byte that is not is reported with its line (section 4).
- Of the XML only the root element and its attributes are judged. The children of the root (`<style>`, `<font>`, `<audio>`, the drawn elements) are not read, not counted and not reported in this version: each is judged by the part of the product that draws it. A root attribute the table of section 3 does not list (including `class`) is neither judged nor reported here; `class` names shared styles, whose attributes the root takes, and that is read with the styles.

## 3. The root's attributes

The table and the rules are those of `notation.md` section 6.1, with N6 for the size. A written value has its surrounding white space removed first (`notation.md` section 4). Each row says what is accepted and what is reported.

| Attribute | Accepted | Not written |
|---|---|---|
| `version` | A whole number whose value is `1`. `1.0` and `1e0` are the whole number 1; `01` is too. | Reported. |
| `width`, `height` | A whole number (N9: a sign and an exponent are allowed when the value has no fraction) that is even and from 2 to 8192. | Reported. |
| `fps` | A whole number from 1 to 240. | 30. |
| `duration` | A time (`notation.md` section 4: digits with an optional point, at most nine digits on each side, no sign, no exponent; `.5` is read, `5.` is not) above 0. | Reported. |
| `background` | One colour, `#rrggbb` or `#rrggbbaa`, upper or lower case (`notation.md` section 7.4: on the root `background` is a single colour). | Black, `#000000`, fully opaque (Q5). |
| `id` | Any text. It is the composition's name (decision A29). | None. |

- **A number** is read as N9 says: an optional `+` or `-`, digits with an optional point (`.5` and `5.` are numbers), an optional exponent; `inf`, `nan` and the like are not numbers. A number is a whole number when its value has no fraction. A value too large for the check to hold is a whole number out of range.
- **The frame count** is the frame grid rule (`frame-time.md`) applied to `duration` at `fps`, at least 1. A `duration` of 5 at the default rate is 150 frames; of 1.01 at 30 frames a second it is 31 (30.3 rounded up). A `duration` of 0.0001 is 1 frame.
- **A frame rate written as a ratio** (`30000/1001`) is not a whole number and is reported like any other value that is not one. The table of unsupported forms (`notation.md` section 14, S03) moves it from "invalid" to "unsupported" with the ticket that builds that table.
- **Every problem.** The attributes of the root are judged one by one, each by its own rule, and every problem found is reported in the same run, whatever the other attributes hold: the order of the checks decides nothing but the order of equal places (section 4). Two cases stop earlier, because nothing else can be judged: a file that is not valid UTF-8 text or not well-formed XML (one report, no other: the XML reader stops at the first fault and says nothing of the rest), and a root element that is not `<composition>` (one report: the attributes of an element that is not a composition are not judged). A `version` that is wrong does not stop the other attributes from being judged by the rules of version 1.

## 4. The reports

**A report** is one line of plain text with six parts, in this order: the file, the line, the code, the kind, the message and the remedy (`error-catalogue.md` section 3).

```
composition.xml, line 3: FS-0007: invalid: `width` must be an even whole number from 2 to 8192, not 641. Write the size as an even whole number from 2 to 8192.
```

- **The file** is the file the problem is in, written as the path from the project folder with `/` (`notation.md` section 13: `file, line N: message`). It is on every report, also when the project has only the one file. It is first on the line, so that a report begins where the next one cannot: every report of a run is one line, every line starts with the file's name, and the code is the first word after the colon.
- **The line** is `line N`, with `N` counted from 1: the line of the attribute, or, for an attribute that is not written, the line of the root's opening `<`.
- **The code** is `FS-` and four digits. It names the kind of problem, never the value: the same mistake has the same code in every file and every run, and a code is never given another meaning. The codes are listed in one place in the program (the type `Code` of the `notation` crate) and in one place in the specs, `error-catalogue.md` section 4, which also gives each code's kind, meaning and remedy. The table below gives the message of each code of the root.
- **The kind** is `invalid` for every code of this version except `FS-0016`, which is `failure` (`error-catalogue.md` section 2). It is the word after the code.
- **The message** says what is wrong and, where the problem is a value, the form that is accepted (`notation.md` section 13). It is one sentence on one line, whatever the value holds: a value is shown as it was written, white space removed; a character that is not printable is shown as `\u{...}` with its code in hexadecimal; a value longer than 80 characters is shown as its first 80 characters followed by `...`. The message is printed with a full stop after it, and the table gives it without.
- **The remedy** is the last sentence of the line: the sentence of the catalogue for the code, word for word (`error-catalogue.md` section 4).

| Code | Mistake | Message (after the code) |
|---|---|---|
| `FS-0001` | Not UTF-8 | `the file is not well-formed XML: it is not valid UTF-8 text`. The line is that of the first byte that is not. |
| `FS-0002` | Not XML | `the file is not well-formed XML: ` and the XML reader's reason. The line is the one the reader gives. |
| `FS-0003` | Root is not `<composition>` | `the root element is <html>, not <composition>` |
| `FS-0004` | No `version` | ``<composition> needs the attribute `version`; the versions read are: 1`` |
| `FS-0005` | A version not read | `` `version="2"` is not a version this program reads; the versions read are: 1 `` (also for a value that is not a whole number) |
| `FS-0006` | No `width` or `height` | ``<composition> needs the attribute `width`, an even whole number from 2 to 8192`` |
| `FS-0007` | Bad size | `` `width` must be an even whole number from 2 to 8192, not 641 `` (`height` the same; for any value that is odd, below 2, above 8192, not a whole number or not a number) |
| `FS-0008` | Bad frame rate | `` `fps` must be a whole number from 1 to 240, not 0 `` |
| `FS-0009` | No `duration` | ``<composition> needs a `duration` above 0`` |
| `FS-0010` | A `duration` that is not a time | `` `duration="-5"` is not a time in seconds: digits with an optional point, such as 5 or 1.25 `` (also for a negative number, a word, an exponent, more than nine digits on a side, an empty value) |
| `FS-0011` | A `duration` that is 0 | `` `duration="0"` must be above 0 `` |
| `FS-0012` | Bad background | `` `background="red"` is not a colour written as #rrggbb or #rrggbbaa `` |

The wording of a time that is not a time, and of a missing `width` or `height`, goes one step beyond `notation.md` section 13 (the clause after the colon, the clause after the comma): the ticket asks for the form to be named in every report, and these two messages have no other way to name it.

**The order of the reports.** The reports of a file are printed in the order of their lines in the file. Two reports on one line are printed in the order of their places on it, left to right; a report for an attribute that is not written is at the root's opening `<`, which comes before every attribute of the root. Two reports are never made for one place and one attribute.

**A file that is not well-formed XML** has one report (`FS-0001` or `FS-0002`) and nothing else is reported for the file.

**A `No project` line** is not a report of a file's problem: there is no file to name. It begins with its outcome, `No project: `, and goes on with the code of its reason, the kind, the sentence of the reason (section 5) and the remedy: ``No project: FS-0013: invalid: the folder `demo` does not exist. Give the path of a folder that exists.`` The codes are `FS-0013` to `FS-0016`, `FS-0020` and `FS-0025` to `FS-0028`.

## 5. What is printed and how it ends

Everything the command reports is plain text: no colour, no control character other than the line feed, no cursor movement, whatever the terminal, the environment variables (`NO_COLOR`, `CLICOLOR_FORCE`, `TERM`) or whether standard output is a terminal. A report reads the same in a file, in a pipe and on a screen, and it shows its code, its kind and its remedy as words. It is told from the next by its line alone: one report is one line and begins with the name of its file (section 4). Nothing is printed on standard error except the command line that cannot be read (section 1) and, when the run log cannot be written, the one line that says so (`run-log.md` section 7).

| Outcome | Standard output | Exit code |
|---|---|---|
| The project file and the root are valid | One line: ``Ready: the project is valid, 150 frames, 1280x720, 30 fps`` (`1 frame` when it is one). The numbers are the frame count, the width, the height and the frame rate in use, written or not, of the composition that was read. | 0 |
| The project file or the root has one problem or more | Every report of section 4, one line each, in the order of section 4 (the reports of the project file first); then the line ``Invalid: the project is not valid (composition.xml)``, with in the brackets the files that have a report, in the order they are printed (``framesmith.yaml, composition.xml``) | 1 |
| Every report is of the kind unsupported | The reports, then the line ``Unsupported: the project uses what this version does not support (framesmith.yaml)``, with the files in the brackets as above | 2 |
| There is no composition to read | One line, ``No project: `` and the code, the kind, the reason and the remedy (section 4), then nothing else (below); after the reports of the project file when it has any | 1 |
| The command line cannot be read | Nothing (see section 1, standard error) | 1 |

After the author has corrected every line a run reported, the next run of `check` on the project is the first row: the same attributes, judged again, give no report, and the exit code is 0. A value is reported and never corrected: the file is not changed (section 6).

The reasons of `No project` name the path as it was written on the command line (the project folder found above a composition file is named by its absolute path). Each has its code and kind (`error-catalogue.md` section 4):

- ``the folder `demo/` does not exist`` (`FS-0013`, invalid): a path written with a closing `/` names a folder (`project-location.md` section 5)
- ```demo/intro.xml/` is not a folder`` (`FS-0014`, invalid): a closing `/` on a file, or a path that is neither a folder nor a file
- ``there is no composition.xml in `demo`, and no framesmith.yaml there names a composition`` (`FS-0015`, invalid): the folder and both files that were looked for
- ```demo/intro.xml` does not exist`` (`FS-0027`, invalid): any other path that is not there
- ```demo/intro.xml` could not be read: `` and the reason the system gives (`FS-0028`, failure): a path that exists and could not be examined, or a composition file given on the command line that could not be read
- ``composition.xml in `demo` could not be read: `` and the reason the system gives (`FS-0016`, failure): the `composition.xml` of a project folder.
- ``framesmith.yaml in `demo` could not be read: `` and the reason the system gives (`FS-0020`, failure).
- ``the composition `scenes/main.xml`, named by `composition` on line 2 of framesmith.yaml, does not exist in `demo` `` (`FS-0025`, invalid), and the same with ``in `demo` could not be read: `` and the reason (`FS-0026`, failure) (`project-file.md` section 4).

The exit code 2 is used for a project that has no report but those of the kind unsupported (decision P2): so far the keys `compositions` and `timeline` of the project file (`FS-0023`). A run that has one report of the kind invalid or failure ends with 1, whatever else it reports.

## 6. It changes nothing of the project

`check` draws nothing. It opens `framesmith.yaml` (when there is one) and the composition for reading, and changes, renames and deletes no file of the project. The one thing a run writes is its run log, `logs/framesmith.log` under the folder the command is run from (`run-log.md` section 1, decision A2). Two cases:

- **Run from another folder** (the usual case): the log is under that folder, and the project folder is left exactly as it was: every file has the same name and the same bytes as before, and no file or folder is new.
- **Run from the project folder itself, or from a folder inside it** (`cd demo && framesmith check .`): the log is under that folder, so the project folder (or the folder inside it) comes to hold a folder `logs` with the file `framesmith.log`, and those two entries are the only ones new. Every other file of the project has the same name and the same bytes as before, `check` reads `framesmith.yaml` and the composition only, and its output and exit code are those of a run from outside. A second run adds lines to the log and changes nothing else.

Apart from the lines added to the log (and the folder `logs` when it is missing), the folder the command is run from is left as it was. A project folder that cannot be written to, and one on a read-only disk, are checked like any other.

## 7. Where the code lives

- `notation` reads the XML and judges the root; it knows nothing of folders or of the command (`PLAN.md` section 3). It holds the value forms (a whole number, a time, a colour), the root's rules, the wording of the messages of section 4, and the codes: the type `Code` is the one place where a code is named, with the list of all of them, so that the catalogue of codes (`error-catalogue.md`) extends it and nothing else: each code has its kind and its remedy there. It gives back every problem of a file, as `Problem`s in the order of section 4; a `Problem` has a code, a line and a message, gives the kind and the remedy of its code, and knows no file.
- `project` knows the project folder: it finds it from the path given (`project-location.md` section 8), reads `framesmith.yaml` when there is one (`project-file.md` section 8), finds the composition it names (`composition.xml` by default), reads it, and gives the root and the settings or the reasons there is no project, or the reports of the files: a `Report` is a `Problem` with the name of the file it is in, and prints as the line of section 4. Its wording is that of the `No project` lines, each with the code of its reason.
- `cli` is the `framesmith` command: it reads the arguments, calls `project`, prints, and sets the exit code.
- The XML reader is the outside crate `roxmltree` (MIT OR Apache-2.0), named in `PLAN.md` section 3. No type of it appears in a public interface of `notation`. The YAML reader is `saphyr` (`project-file.md` section 8).
- The rule for showing a written value in a message (section 4) is one function of `notation`, `shown`, which `project` uses for the project file too.
