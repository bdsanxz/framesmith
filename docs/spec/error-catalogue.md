# The error catalogue: codes, kinds and remedies

Status: in force from Sprint 1. Sources: `ROADMAP.md` line 1.42 and `PLAN.md` section 3 ("a typed error for every refusal, with a remedy"); decisions P2 and A19 of `DECISIONS.md` (the exit codes that follow a kind); `notation.md` section 13 (the wording of the messages for the common mistakes, which says that the code and the remedy of each message belong to this catalogue); `check-command.md` sections 4 and 5 (the form of a report and what the command prints); `CLAUDE.md` rules 1 (a composition carries no code), 5 (no colour is needed to read a log or a message) and 12 (a test is named for what it covers).

Every time Framesmith refuses a project or fails, what it prints has a code, a kind, the file and line where there is one, one sentence on what is wrong and one sentence on what to change. The list of codes is section 4 of this file, and nothing else: the program's own list is held to it by tests (section 6). A calling program acts on the code and the kind, and an author can search for a code; neither depends on the wording of a message.

## 1. What a code is

- A code is `FS-` and four digits: `FS-0007`. Nothing else begins a code.
- A code names a kind of problem, never a value: the same mistake has the same code in every file and every run. The value found, and the place, are in the message.
- A code is never given another meaning and never reused. The meaning of a code is one sentence, in section 4, and the earlier meanings of the list are kept in the record (section 5). A code stays on the list and in the record when the problem it names can no longer happen.
- The numbers follow the order in which codes were added and say nothing more. They are not grouped by kind, by crate or by command.
- Every code is named in one place in the program, the type `Code` of the `notation` crate, whatever part of the program emits it. A code is a value of that type; no other part writes a code as text.

## 2. The three kinds

A kind says who has to act, and it never changes with the command that found the problem.

| Kind | Printed as | Meaning |
|---|---|---|
| Invalid | `invalid` | What the author gave is wrong: a value, a file, a folder or a command line that is not accepted. Changing what was given is the remedy. |
| Unsupported | `unsupported` | What the author gave is known to Framesmith and is not supported in this version. It is named by the support table of `notation.md` section 14; anything else that is not accepted is invalid. The remedy is to remove it, to replace it, or to wait for the sprint that brings it. |
| Failure | `failure` | What the author gave may be right, and Framesmith or the machine could not carry it out: a file that cannot be read, a device that does not answer. The remedy is to put the machine right and run the command again. |

The exit code is derived from the kind and the command (decisions P2 and A19); this file does not state it again. The rule that tells unsupported from invalid is the support table, and the first code of the kind unsupported is added with the work that lists and refuses the table (`DECISIONS.md` A28). No code of that kind is on the list yet.

## 3. A report

A report is one line of plain text. It has these parts, in this order:

```
composition.xml, line 3: FS-0007: invalid: `width` must be an even whole number from 2 to 8192, not 641. Write the size as an even whole number from 2 to 8192.
```

1. **The place**, where there is one: the file and `line N` (`check-command.md` section 4). A report with no file to name, such as a folder that does not exist, begins with the words of its outcome instead (`No project: `), or with `framesmith: ` when the command line cannot be read.
2. **The code**, `FS-` and four digits, followed by a colon.
3. **The kind**, one of the three words of section 2, followed by a colon.
4. **What is wrong**: one sentence, the message of the code with the value found in it. It ends with a full stop; a message that already ends with one gets no second. The messages of the codes of `check` are in `check-command.md` section 4.
5. **What to change**: one sentence, the remedy of the code in section 4, word for word. It is the same for every report of the code, and it is the last sentence of the line.

A report has no colour, no styling and no control character other than the line feed, whatever the terminal, the environment (`NO_COLOR`, `CLICOLOR_FORCE`, `TERM`) or whether the output is a terminal. The code, the kind and the remedy are words in the line, so a report that is copied into a file, a pipe or a message reads the same as on a screen. A report is one line, and begins at its place or at the words of its outcome, so that one report can be told from the next.

## 4. The list of codes

Each row is one code. The **meaning** is the sentence that is recorded (section 5) and never changed. The **remedy** is the sentence the program prints; the program's text is held to this one. Rows are in the order of the numbers.

| Code | Kind | Meaning | Remedy |
|---|---|---|---|
| `FS-0001` | invalid | The composition file is not valid UTF-8 text. | Save the file as UTF-8 text. |
| `FS-0002` | invalid | The composition file is not well-formed XML. | Correct the XML at the line given: close every element and put every attribute value in quotes. |
| `FS-0003` | invalid | The root element of the composition file is not `<composition>`. | Make the root element of the file `<composition>`. |
| `FS-0004` | invalid | The root `<composition>` has no `version`. | Write `version="1"` on the root `<composition>`. |
| `FS-0005` | invalid | The root's `version` is not one that this program reads. | Write `version="1"` on the root `<composition>`, the only version read. |
| `FS-0006` | invalid | The root has no `width` or no `height`. | Write `width` and `height` on the root, each an even whole number from 2 to 8192. |
| `FS-0007` | invalid | The root's `width` or `height` is not an even whole number from 2 to 8192. | Write the size as an even whole number from 2 to 8192. |
| `FS-0008` | invalid | The root's `fps` is not a whole number from 1 to 240. | Write `fps` as a whole number from 1 to 240, or leave it out for 30. |
| `FS-0009` | invalid | The root has no `duration`. | Write `duration` on the root as a number of seconds above 0. |
| `FS-0010` | invalid | The root's `duration` is not a time in seconds. | Write `duration` as digits with an optional point, such as 5 or 1.25. |
| `FS-0011` | invalid | The root's `duration` is 0. | Write `duration` as a number of seconds above 0. |
| `FS-0012` | invalid | The root's `background` is not one colour. | Write `background` as one colour, #rrggbb or #rrggbbaa, or leave it out for black. |
| `FS-0013` | invalid | The folder given to the command does not exist. | Give the path of a folder that exists. |
| `FS-0014` | invalid | The path given to the command is not a folder. | Give the folder that holds `composition.xml`, not a file. |
| `FS-0015` | invalid | The folder given to the command holds no `composition.xml`. | Put a `composition.xml` in the folder, or give the folder that holds one. |
| `FS-0016` | failure | The `composition.xml` of the folder exists and could not be read. | Make `composition.xml` a file this user may read, then run the command again. |
| `FS-0017` | invalid | The command line cannot be read. | Run `framesmith check <folder>` with the folder of one project. |

Notes on the rows:

- `FS-0013` to `FS-0016` are the reasons of a `No project` line (`check-command.md` section 5); the line gets its code from the reason. A folder that cannot be read for a reason of the machine is a failure (`FS-0016`); a path that is wrong is invalid.
- `FS-0017` is for every command line that cannot be read. Its message says which way the line is wrong (`check-command.md` section 1).
- A remedy is one sentence that tells the author what to change, not what went wrong. It does not name a value, since it is the same for every report of the code. It has no vertical bar, which would end its cell.

## 5. Keeping the list

- **A new code** is added to section 4 by the work that adds the refusal, in the same change as the program's code for it: its row here, its value in `Code`, and its row in the record. Nothing is emitted that is not on this list, and nothing is listed that the program cannot emit.
- **A meaning is never changed.** A refusal that needs another meaning gets a new code. A wording that only corrects a slip (a typing mistake, not the sense) is made in the list and in the record in one change, and the change says so.
- **The record** is `error-catalogue-record.md` beside this file: for each code that has been in a version of this list, the meaning it had then. A row of the record is never edited or removed. A code that has no row in the record is new; a row is added to the record when the code is first merged to the main line.
- **The remedy and the kind** of a code may be improved in the list, when the program's text is changed with it; they are not recorded, since a better remedy is not another meaning.

## 6. How the list is held

The tests are in `crates/notation/tests/error_catalogue.rs`, and the report is tested through the command in `crates/cli/tests/`.

- Every row of section 4 has a code that is `FS-` and four digits, one of the three kinds, a meaning, and a remedy that is one sentence; a row without a kind or without a remedy fails the test and names the code.
- The codes the program can emit (the list of `Code`, and every `FS-` and four digits written in the source of any crate) and the codes of section 4 are the same set; a code in one and not in the other fails the test and names the code. Each code is emitted by the command on some input.
- The kind and the remedy of every code in the program equal those of section 4.
- Every code in the record is on the list with the meaning it was recorded with; a meaning that differs, or a recorded code that is gone, fails the test and names the code.
- The reports of the command carry the code, the kind and the remedy as words, with the environment of a terminal that asks for colour and with one that asks for none.
