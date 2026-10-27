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

The exit code is derived from the kind and the command (decisions P2 and A19); this file does not state it again. The rule that tells unsupported from invalid is the support table, and the codes of the kind unsupported for what the notation does not support yet are added with the work that lists and refuses the table (`DECISIONS.md` A28). The codes of that kind on the list so far are `FS-0023`, for the two keys of the project file that wait for Sprint 2 (`project-file.md` section 5.4), `FS-0035`, for what `render` does not draw or apply yet (`render-command.md` section 3), and `FS-0038`, for a card that `render` is asked for and this machine does not have (`render-command.md` section 1.2).

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
| `FS-0014` | invalid | The path given to the command is not a folder. | Give the path of a folder, or of a `framesmith.yaml` or a composition file; the path of a file has no closing `/`. |
| `FS-0015` | invalid | The folder given to the command holds no `composition.xml`. | Put a `composition.xml` in the folder, or give the folder that holds one. |
| `FS-0016` | failure | The `composition.xml` of the folder exists and could not be read. | Make `composition.xml` a file this user may read, then run the command again. |
| `FS-0017` | invalid | The command line cannot be read. | Run `framesmith check <path>` or `framesmith render <path>` with the folder, the project file or the composition file of one project. |
| `FS-0018` | invalid | The project file is not valid UTF-8 text. | Save the project file as UTF-8 text. |
| `FS-0019` | invalid | The project file is not well-formed YAML. | Correct the YAML at the line given: indent with spaces and write each key as `key: value`. |
| `FS-0020` | failure | The `framesmith.yaml` of the folder exists and could not be read. | Make `framesmith.yaml` a file this user may read, then run the command again. |
| `FS-0021` | invalid | The project file is not one mapping of keys. | Make the file one mapping of keys, such as `name: launch-film`, or leave it empty. |
| `FS-0022` | invalid | A key of the project file is not one that the project file has. | Remove the key, or write it as one of the keys the message lists. |
| `FS-0023` | unsupported | A key of the project file is known and is not supported in this version. | Remove the key, or wait for the sprint that brings it. |
| `FS-0024` | invalid | A value in the project file is of the wrong kind or outside the values its key takes. | Write the value in the form the message gives. |
| `FS-0025` | invalid | The composition named by the project file does not exist. | Set `composition` to the path of a composition file that exists, or put the file there. |
| `FS-0026` | failure | The composition named by the project file exists and could not be read. | Make the composition file one this user may read, then run the command again. |
| `FS-0027` | invalid | The path given to the command does not exist. | Give the path of a folder, a `framesmith.yaml` or a composition file that exists. |
| `FS-0028` | failure | The path given to the command exists and could not be read. | Make the path one this user may read, then run the command again. |
| `FS-0029` | invalid | An element of the composition file is not one that the notation has. | Remove the element, or write one of the elements the message names. |
| `FS-0030` | invalid | An element of the composition file has an attribute that the element does not take. | Remove the attribute, or write one the element takes; section 6 of the notation spec lists them. |
| `FS-0031` | invalid | An element of the composition file stands in a place where the notation does not allow it. | Move the element to a place the message names, or remove it. |
| `FS-0032` | invalid | A value in the composition file that must be a number is not one. | Write the value as a plain number, such as 12, -4, .5 or 1e3, with no unit and no percent sign. |
| `FS-0033` | invalid | A value in the composition file that must be a time in seconds is not one. | Write the time as digits with an optional point, such as 5, 0.5 or .5, with no sign, exponent or unit. |
| `FS-0034` | invalid | A length in the composition file is not a number of pixels or a percentage, or is below 0 where 0 or more is taken. | Write the value in a form the attribute takes (`notation.md` section 6), and not below 0 where the message says 0 or more. |
| `FS-0035` | unsupported | The project uses a form that is known and that this version does not draw or apply. | Remove what the message names, or wait for the sprint that brings it. |
| `FS-0036` | failure | The video could not be made. | Put right what the message names (the folder, the disk or the system's video encoder), then run the command again. |
| `FS-0037` | invalid | An option of `render`, or its value, is not one the command takes. | Give one of the options the message lists, with one of the values it names, or leave the option out. |
| `FS-0038` | unsupported | The author asked for the graphics card (`gpu`), and this machine has no card that frx7 can draw with. | Set `render.backend` (or `--backend`) to `auto` or `cpu`, which draw on the processor when no card is there. |
| `FS-0039` | invalid | A value that must be one of a list of words is not one of them. | Write one of the words the message names, in lower case, or leave the attribute out. |
| `FS-0040` | invalid | An element of the composition file has `start` without `duration`, or `duration` without `start`, where the notation writes both or neither. | Write both `start` and `duration` on the element, or neither. |
| `FS-0041` | invalid | A `<style>` of the composition file has no `id`, so no element can take it. | Write an `id` on each `<style>`, the name that elements take it by. |
| `FS-0042` | invalid | Two `<style>` elements of the composition file have the same `id`. | Give each `<style>` of the file its own `id`. |
| `FS-0043` | invalid | An element's `class` names a style that the composition file does not have. | Name a `<style>` of this file in `class`, or write its attributes on the element. |
| `FS-0044` | invalid | A composition shows itself, directly or through the compositions it shows. | Take out the `<composition src>` that closes the loop, so that no composition shows itself. |
| `FS-0045` | invalid | A chain of compositions shown by `<composition src>` is more than 16 compositions deep, the root counted. | Show fewer compositions in one chain: at most 16 with the root, each shown by the one before it. |
| `FS-0046` | invalid | A composition shown by `<composition src>` has another frame rate from the composition that shows it. | Write in the shown composition the same `fps` as in the one that shows it, a missing `fps` counting as 30. |
| `FS-0047` | invalid | A composition shown by `<composition src>` does not exist in the project folder. | Give the path of a composition file in the project folder, or put the file there. |
| `FS-0048` | failure | A composition shown by `<composition src>` is in the project folder and could not be read. | Make the shown composition file readable by this user, then run the command again. |
| `FS-0049` | invalid | A font file that `<font src>` names does not exist, at the path it gives from the project folder or, when it begins with `/`, on the machine. | Give the path of a font file that exists, from the project folder or a path that begins with `/`, or put the file there. |
| `FS-0050` | failure | A font file that `<font src>` names exists and could not be read. | Make the font file readable by this user, then run the command again. |
| `FS-0051` | invalid | A file that `<font src>` names is not a font of a kind this version reads, which are TrueType (`.ttf`) and font collections (`.ttc`). | Give in `src` the path of a TrueType font file (`.ttf`) or a font collection file (`.ttc`). |
| `FS-0052` | invalid | A text attribute's value is not in the form the notation gives for it. | Write the value in the form the message names. |
| `FS-0053` | invalid | A length or size that must be above 0 is 0 or below. | Write the value as a number above 0, or leave the attribute out for its default. |
| `FS-0054` | invalid | A `columns` of a `<box>` is not a list of column widths: each a number of pixels above 0, or a share such as `1fr`. | Write the widths as numbers of pixels, such as 640, or as shares, such as 1fr, separated by spaces. |
| `FS-0055` | invalid | A `<box>` has both `columns` and `flex-direction`. | Remove `columns` to lay the children out in a row or a column, or `flex-direction` to lay them out in columns. |
| `FS-0056` | unsupported | A character of a text whose picture this version does not draw: the font has no PNG picture for it at any size, or frx7 refuses the picture. | Give the text a font that has a PNG picture for the character, or remove the character. |
| `FS-0057` | invalid | A file that `<font src>` names begins as a font of a kind this version reads, and the font reader refuses it as damaged. | Replace the font file with a copy that is not damaged, such as the one the font came in. |
| `FS-0058` | invalid | A `<tween>` has no `duration`, or a `duration` of 0. | Write `duration` on the tween as a number of seconds above 0. |
| `FS-0059` | invalid | A `<tween>` has no `from-` or `to-` attribute. | Write at least one `from-` or `to-` attribute on the tween, such as `to-x="100"`. |
| `FS-0060` | invalid | A `<tween>` changes a property that the element it moves does not take as an attribute. | Move the tween to an element that takes the property, or change a property the element takes. |
| `FS-0061` | invalid | A `<set>` has no property. | Write at least one property on the set, such as `x="300"`. |
| `FS-0062` | invalid | A `<set>` changes a property that the element it moves does not take as an attribute. | Move the set to an element that takes the property, or change a property the element takes. |
| `FS-0063` | invalid | The `ease` of a `<tween>` is not an easing the notation takes. | Write one of the easings the message lists, in lower case, such as `ease="out-cubic"`. |
| `FS-0064` | invalid | A background layer, or a size, place or repeat of one, is not in the form the notation gives for it. | Write the value in the form the message names. |
| `FS-0065` | invalid | A list of background settings has neither one entry nor one for each layer of the background. | Write one entry for all the layers, or one for each layer in the order of `background`. |
| `FS-0066` | invalid | A background setting is written on an element with no background. | Write a `background` on the element, or remove the setting. |
| `FS-0067` | invalid | A `border` is not a width and a colour, or a width, `double` and a colour. | Write the border as a width and a colour, such as `6 #ffffff`, or as a width, `double` and a colour, such as `9 double #ffffff`. |
| `FS-0070` | invalid | A `box-shadow` or `drop-shadow` is not an offset across, an offset down, a blur and a colour, or a `box-shadow` is not a list of them. | Write the shadow as an offset across, an offset down, a blur and a colour, such as `10 10 8 #000000`. |
| `FS-0072` | failure | The output folder of the video cannot be made or written, so `render` refuses it before the first frame. | Give `--output` a path in a folder that can be written, or make the folder writable, then run the command again. |
| `FS-0076` | invalid | An `<audio>` has no `duration`, or a `duration` of 0. | Write `duration` on the `<audio>` as the seconds the sound plays, above 0. |
| `FS-0077` | invalid | The sound file that `<audio src>` names does not exist, or the `<audio>` has no `src`. | Write `src` as the path of a sound file that exists, from the project folder or from `/`. |
| `FS-0078` | failure | The sound file that `<audio src>` names exists and could not be read. | Check that the path names a file, not a folder, and that it may be read. |
| `FS-0079` | unsupported | The file that `<audio src>` names is not sound of a kind this version reads, or is of such a kind in a form it does not read. | Convert the sound to one of the kinds the message lists, such as WAV of 16-bit samples. |
| `FS-0080` | invalid | The sound file that `<audio src>` names is of a kind this version reads and cannot be decoded. | Replace the sound file with a copy that is not damaged. |
| `FS-0081` | invalid | An `<audio>` has a `media-start` at or past the end of its sound file. | Write a `media-start` below the length of the sound file, or leave it out to start at the beginning. |

Notes on the rows:

- `FS-0013` to `FS-0016`, `FS-0020`, `FS-0025` to `FS-0028` are the reasons of a `No project` line (`check-command.md` section 5); the line gets its code from the reason. A folder or a file that cannot be read for a reason of the machine is a failure (`FS-0016`, `FS-0020`, `FS-0026`, `FS-0028`); a path that is wrong is invalid. When each of `FS-0013`, `FS-0014`, `FS-0015`, `FS-0027` and `FS-0028` is used is in `project-location.md` section 5: a path written with a closing `/` names a folder, so a missing one is `FS-0013` and a file is `FS-0014`; any other missing path is `FS-0027`.
- `FS-0018` to `FS-0024` are the codes of the project file (`project-file.md` section 5): `FS-0018`, `FS-0019` and `FS-0021` stand for the whole file, `FS-0022` to `FS-0024` for one key.
- `FS-0017` is for every command line that cannot be read. Its message says which way the line is wrong (`check-command.md` section 1).
- `FS-0027` and `FS-0028` were added with the three forms of the argument (`project-location.md`). `FS-0014` and `FS-0017` kept their meanings and had their remedies improved to name the three forms.
- `FS-0029` to `FS-0033` are the codes of the form of a composition file (`check-command.md` section 3a): `FS-0029` to `FS-0031` stand for an element or an attribute and its place, `FS-0032` and `FS-0033` for a number or a time anywhere in the file. A form that the support table of `notation.md` section 14 lists will get codes of the kind unsupported with the work that lists and refuses that table (decision A28).
- `FS-0034` to `FS-0036` are the codes of `render` (`render-command.md`): `FS-0034` for a length that is wrong, `FS-0035` for a form that is known and is not drawn or applied by this version (it is listed by `render` and, with the support table of `notation.md` section 14, will be listed by `check`), `FS-0036` for a film that could not be made. `FS-0017` kept its meaning and had its remedy improved to name both commands.
- `FS-0037` was added with the options of `render` (`render-command.md` section 1): an option that is not one of them, an option with no value, an option given twice, or a value outside the values of its option. It is not `FS-0017`, whose remedy is about the path of the project and would send the author the wrong way.
- `FS-0038` was added with the choice of the renderer (`render-command.md` section 1.2): `gpu`, from the project file or the command line, on a machine with no card that frx7 can draw with. It is of the kind unsupported, as the card refusals of the support table are (ticket T086), and it is the one code of that kind that is not a form of the composition or of the project file: this machine lacks what the author asked for, and the file is right. `render` refuses it before the first frame and exits with 2, with the last line `Refused:` (`render-command.md` section 7). `auto` never gives it: with no card it draws on the processor and says so.
- `FS-0039` was added with the places of a box (`render-command.md` section 3): a `position` that is neither `static` nor `absolute`. A word out of a list the notation gives is an invalid value, not a form of this version that is unsupported, since the list is the notation's and not this version's.
- `FS-0040` was added with the clip window of a box (`notation.md` section 5.2): `start` and `duration` are written both or neither, on each element that takes them as a clip (`notation.md` section 6). The report is on the line of the element. It is of the kind invalid, since the pair is the notation's own rule and not a form this version leaves out.
- `FS-0041` to `FS-0043` were added with shared styles (`notation.md` section 9 and N5; `check-command.md` section 3c): a `<style>` with no `id`, two styles with one `id`, and a `class` that names no style of the file. `FS-0030` and `FS-0031` keep their meanings for a style, with the messages of `check-command.md` section 3c.
- `FS-0044` to `FS-0048` were added with the compositions shown from other files (`notation.md` sections 10 and 13). `FS-0044` to `FS-0047` are of the kind invalid, since the author wrote a loop, a chain too deep, a frame rate that differs, or a path that is wrong. `FS-0048` is of the kind failure, since the file is there and the machine would not give it, as `FS-0016`, `FS-0020`, `FS-0026` and `FS-0028` are.
- `FS-0056` was added with the emoji of a text (`text.md` section 12.4). It is not `FS-0035`: a text whose font is drawn may still hold one glyph that has no picture this version draws, and that glyph is refused on the line of its `<text>` with a remedy about the font.
- `FS-0049` to `FS-0051` were added with the font files of a text (`text.md` section 2.1): `FS-0049` when the file that `<font src>` names is not there, `FS-0050` when it is there and the machine would not give it (a failure, as `FS-0016` and `FS-0020` are), and `FS-0051` when it is not a font of any kind this version takes, which are TrueType and font collections. A font of a kind this version does not draw is unsupported and is reported as `FS-0035`; both messages name the file as written and the kinds taken. `FS-0057` was added with the damaged font files (`text.md` section 2.1; decision B28): a file whose first four bytes name a kind this version reads, and which the font reader refuses, is damaged, not a file of another kind. It is of the kind invalid, as `FS-0051` is, and its message gives the reason the reader gives.
- `FS-0052` was added with the values of the text attributes that are not in the form the notation gives (`text.md` sections 4 and 6.1): a `line-height` or a `letter-spacing` that is neither of its forms, such as a percentage. `FS-0053` was added with the lengths and sizes that must be above 0 (`notation.md` N6): a `font-size` or a `line-height` of 0 or less, which `FS-0034` (0 or more) does not describe. `FS-0054` and `FS-0055` were added with the sets of columns (`render-command.md` section 4.4): a `columns` that is not a list of column widths, and a `<box>` that has `columns` and `flex-direction` together, which `FS-0039` (a word out of a list) and `FS-0031` (an element in a place the notation does not allow) do not describe.
- `FS-0058` to `FS-0060` were added with the tweens (`notation.md` sections 8.1 and 8.2): a tween with no duration above 0, one that changes no property, and one that changes a property the element it moves does not take, such as `to-count` on a `<box>`. They are of the kind invalid, since each breaks a rule of the notation. `FS-0058` and `FS-0059` are on the line of the tween, `FS-0060` on the line of the attribute. A `from-` or `to-` attribute of a name that is not an animated property, such as `to-width`, is `FS-0030`, as any attribute the tween does not take, and it counts as the tween's property for `FS-0059`, so that the one mistake has one report.
- `FS-0061` and `FS-0062` were added with the sets (`notation.md` section 8.3): a set that changes no property, and one that changes a property the element it moves does not take. They are new codes, not wider meanings of `FS-0059` and `FS-0060`, since a meaning is never changed (section 5). `FS-0061` is on the line of the set, `FS-0062` on the line of the attribute. An attribute of a set that is not an animated property, such as `width`, is `FS-0030`, and counts as the set's property for `FS-0061`.
- `FS-0063` was added with the easings (`notation.md` section 8.4; `motion.md`): an `ease` that is not one of the nineteen names or `out-back(n)`. It is of the kind invalid, on the line of the attribute, and its message lists the names taken.
- `FS-0072` was added with the staged output (`render-command.md` sections 5 and 6): an output folder that `render` cannot make or write, refused before the first frame with exit 2 as decision A19 states. Its kind is failure, as `FS-0016`, `FS-0020` and `FS-0028` are, because the machine, not the value written, refuses the folder. Every failure of a place is reported as a refusal with exit code 2 (A19, B57): an output place that `render` cannot write, and a folder that `init` cannot use. The code is numbered FS-0072 by the lead, who holds FS-0067 to FS-0071 for other tickets.
- `FS-0064` to `FS-0066` were added with the backgrounds of several layers (`render-command.md` section 4.6, rule 10): a layer or setting not in its form, a list of settings of the wrong length, and a setting with no background. They are new codes rather than a wider meaning of `FS-0052`, whose meaning is a text attribute's.
- `FS-0067` was added with borders (`box-edges.md` sections 1 and 6): a `border` in a form that is not `width colour` or `width double colour`. It is of the kind invalid, reported on the line of the attribute.

- `FS-0076` to `FS-0081` were added with the sounds placed in the film (`sound.md` section 5). `FS-0076` and `FS-0077` are found from the file alone, as a font's are; `FS-0078` to `FS-0081` when frx7 opens the sound file before the first frame. A missing file and a file that cannot be decoded are invalid, since the author named the wrong or a damaged file; a file the machine would not give is a failure, as `FS-0050` is; a file of a kind not read is unsupported, and its message names the kinds read.

- `FS-0070` was added with shadows (`render-command.md` section 4.7, rule 8): a `box-shadow` or `drop-shadow` that is not in the form `X Y BLUR COLOUR`. It is of the kind invalid, reported on the line of the attribute. It is a new code, not a wider meaning of `FS-0039` (a word out of a list) or `FS-0052` (a text attribute's form), since a meaning is never changed.
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
