# The `check` command: the root of a composition

Status: in force from Sprint 1, for the first form of `framesmith check`. Sources: `notation.md` (section 2 for the version, section 3 for the project folder and the root, section 4 for the value forms, section 5.1 for the frame count, section 6.1 for the root's table, N6 for the size, N8 for times, N9 for numbers, section 7.4 and Q5 for the root's `background`, section 13 for the wording of the messages); `frame-time.md` (the frame grid rule, done by the `timebase` crate); decisions P1 and P2 of `DECISIONS.md` (the version rule, the exit codes) and A29 (the root's name is `id`); `PLAN.md` section 3 (which crate holds what); `CLAUDE.md` rules 1 and 5. Where this file and `notation.md` say the same thing, `notation.md` is the text and this file only points to it.

This version of `check` reads the root of one composition file and nothing else. It is the first step of the author's journey: a folder with a `composition.xml` is a project, and the command says whether its root is one a render could use.

## 1. The command line

```
framesmith check <folder>
```

- **One argument, the project folder.** A folder that holds a `composition.xml` and no project file is a complete project. The project file (`framesmith.yaml`) is not read in this version: whether it is there or not changes nothing. A path that is a composition file, and a folder found by searching upward, come with the tickets that read the project file.
- **Anything else is a command line that cannot be read**: no word at all, a word other than `check`, `check` with no folder, `check` with more than one word after it, or an option. The command prints `framesmith: ` and one sentence saying what is wrong, then the line `usage: framesmith check <folder>`, both on standard error, prints nothing on standard output, and exits with code 1. No option is taken yet (`--json` is Sprint 6).

## 2. What is read

- The file `composition.xml` directly in the folder, opened for reading only.
- Its text must be UTF-8. The first byte that is not is reported with its line (section 5).
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
- **The first problem.** This version reports the first problem it finds and stops, looking in this order: the file is read, it is XML, the root is `<composition>`, then `version`, `width`, `height`, `fps`, `duration`, `background`. Reporting every problem of a composition in one run, and giving each report a code, comes with the next ticket; the wording below does not change then.

## 4. The messages

The message of a problem is `line N: ` followed by the words below (`notation.md` section 13). `N` is the line of the attribute, or, for an attribute that is not written, the line of the root's opening `<`. A value is shown as it was written, white space removed; a character that is not printable is shown as `\u{...}` with its code in hexadecimal; a value longer than 80 characters is shown as its first 80 characters followed by `...`, so that a message is always one line.

| Mistake | Message |
|---|---|
| Not XML | `line N: the file is not well-formed XML: ` and the XML reader's reason. `N` is the line the reader gives. |
| Not UTF-8 | `line N: the file is not well-formed XML: it is not valid UTF-8 text`. `N` is the line of the first byte that is not. |
| Root is not `<composition>` | `line N: the root element is <html>, not <composition>` |
| No `version` | ``line N: <composition> needs the attribute `version`; the versions read are: 1`` |
| A version not read | ``line N: `version="2"` is not a version this program reads; the versions read are: 1`` (also for a value that is not a whole number) |
| No `width` or `height` | ``line N: <composition> needs the attribute `width` `` |
| Bad size | ``line N: `width` must be an even whole number from 2 to 8192, not 641`` (`height` the same; for any value that is odd, below 2, above 8192, not a whole number or not a number) |
| Bad frame rate | ``line N: `fps` must be a whole number from 1 to 240, not 0`` |
| No `duration` | ``line N: <composition> needs a `duration` above 0`` |
| A `duration` that is not a time | ``line N: `duration="-5"` is not a time in seconds: digits with an optional point, such as 5 or 1.25`` (also for a negative number, a word, an exponent, more than nine digits on a side, an empty value) |
| A `duration` that is 0 | ``line N: `duration="0"` must be above 0`` |
| Bad background | ``line N: `background="red"` is not a colour written as #rrggbb or #rrggbbaa`` |
| No file to read | ``No project: ...`` (section 5) |

The wording of a time that is not a time goes one step beyond `notation.md` section 13 (the clause after the colon): the ticket asks for the form to be named in the report, and a time has no other message that does.

## 5. What is printed and how it ends

Everything the command reports is plain text: no colour, no control character other than the line feed, no cursor movement, whatever the terminal, the environment variables (`NO_COLOR`, `CLICOLOR_FORCE`, `TERM`) or whether standard output is a terminal. A message reads the same in a file, in a pipe and on a screen. Nothing is printed on standard error except the command line that cannot be read (section 1).

| Outcome | Standard output | Exit code |
|---|---|---|
| The root is valid | One line: ``Ready: the project is valid, 150 frames, 1280x720, 30 fps`` (`1 frame` when it is one). The numbers are the frame count, the width, the height and the frame rate in use, written or not. | 0 |
| The root has a problem | The message of section 4, then the line ``Invalid: the project is not valid (composition.xml)`` | 1 |
| There is no composition to read | One line, ``No project: `` and the reason, then nothing else (below) | 1 |
| The command line cannot be read | Nothing (see section 1, standard error) | 1 |

The reasons of `No project` name the path as it was written on the command line:

- ``the folder `demo` does not exist``
- ```demo` is not a folder``
- ``there is no composition.xml in `demo` ``
- ``composition.xml in `demo` could not be read: `` and the reason the system gives.

The exit code 2 (a project that is valid and uses something unsupported) is not used yet: this version knows no unsupported form.

## 6. It writes nothing

`check` draws nothing and writes no file. It opens `composition.xml` for reading, and creates, changes, renames and deletes nothing: not in the project folder, not under the folder the command is run from, not anywhere else. After a run, every file of the project folder has the same name and the same bytes as before, and no file or folder is new. There is no log yet; the run log is the work of the logging ticket, which fixes where it goes (`DECISIONS.md` A2). A project folder that cannot be written to, and one on a read-only disk, are checked like any other.

## 7. Where the code lives

- `notation` reads the XML and judges the root; it knows nothing of folders or of the command (`PLAN.md` section 3). It holds the value forms (a whole number, a time, a colour), the root's rules and the wording of the messages of section 4.
- `project` knows the project folder: it finds `composition.xml`, reads it, and gives the root or the reason there is no project. Its wording is that of the `No project` lines.
- `cli` is the `framesmith` command: it reads the arguments, calls `project`, prints, and sets the exit code.
- The XML reader is the outside crate `roxmltree` (MIT OR Apache-2.0), named in `PLAN.md` section 3. No type of it appears in a public interface of `notation`.
