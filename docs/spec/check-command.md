# The `check` command: a composition file

Status: in force from Sprint 1, for the first form of `framesmith check`. Sources: `notation.md` (section 2 for the version, section 3 for the project folder, the root, the closed notation and the rule that every problem is reported at once, section 4 for the value forms, section 5.1 for the frame count, section 6.1 for the root's table, N6 for the size, N7 for ids, section 9 and N5 for shared styles, N8 for times, N9 for numbers, section 7.4 and Q5 for the root's `background`, section 13 for the wording of the messages); `frame-time.md` (the frame grid rule, done by the `timebase` crate); decisions P1 and P2 of `DECISIONS.md` (the version rule, the exit codes) and A29 (the root's name is `id`); `PLAN.md` section 3 (which crate holds what); `CLAUDE.md` rules 1 and 5. `error-catalogue.md` (the codes, their kinds and remedies, and the form of a report); `project-file.md` (the project file, its keys and what is reported for it); `project-location.md` (the three forms of the argument and the project folder). Where this file and `notation.md` say the same thing, `notation.md` is the text and this file only points to it.

This version of `check` reads the project file `framesmith.yaml`, when the project folder has one, and the one composition file the project names (`composition.xml` unless the project file names another, or the composition file the command line gives), and nothing else: the root's attributes, and the form of everything under the root (section 3a). It is the first step of the author's journey: a folder with a `composition.xml` is a project, and the command says whether its settings and its composition are ones a render could use. When they have problems it reports all of them in one run, each with a code, its file and its line (sections 4 and 5).

## 1. The command line

```
framesmith check <path>
```

- **One argument, the path of the project**: a folder, a `framesmith.yaml`, or a composition file. `project-location.md` says how the project folder is found from each (a folder is the project folder; a project file makes its folder the project folder; a composition file belongs to the nearest folder at or above it that holds a `framesmith.yaml`, or to its own folder when none does) and that every path written in the project is relative to the project folder. A folder that holds a `composition.xml` and no project file is a complete project. The project file (`framesmith.yaml`) directly in the project folder is read when there is one, and `project-file.md` says what it holds and what is reported for it.
- **Anything else is a command line that cannot be read**: no word at all, a word other than `check`, `check` with no path, `check` with more than one word after it, or an option. The command prints `framesmith: ` and a statement of the code `FS-0017` (section 4: the code, its kind, one sentence saying what is wrong and the remedy), then the line `usage: framesmith check <path>` (and, for a command line with no command or with a word that is no command, the usage line of `render` after it, which names its options: `render-command.md` section 1), both on standard error, prints nothing on standard output, and exits with code 1. No option is taken yet (`--json` is Sprint 6).

## 2. What is read

- The file `framesmith.yaml` directly in the project folder, when there is one, opened for reading only (`project-file.md` sections 2 and 5; `project-location.md` sections 1 and 3 for the project folder).
- The composition file the command line gives, or else the one the project names with the key `composition`, as a path from the project folder (`project-file.md` section 4), or else `composition.xml` directly in the project folder; opened for reading only.
- Its text must be UTF-8. The first byte that is not is reported with its line (section 4).
- Of the XML, the root element and its attributes are judged by the rules of section 3, and every element under it by the rules of section 3a: whether it is an element the notation has, whether it stands where it may, whether it takes each attribute it writes, and whether the numbers and times it writes are in the forms the notation lists. What an attribute means, the range of values it takes, and what an element needs, are not judged here: each is judged by the part of the product that draws the element, except the forms of the place and size of a `<box>` (section 3b), which are judged here by the code that draws them. The `<style>` elements of the file and the names given by `class` are judged here too, by the rules of section 3c.
- The files a composition shows with `<composition src>` (`notation.md` section 10). Each is opened for reading only and judged by the rules of this section and of section 3a, and its problems are reported with its own name. A file that is not in the project folder, or that is there and cannot be read, is reported on the line of the element that names it. Loops, depth and frame rates of the shown files are judged here too (`FS-0044` to `FS-0048`).

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

## 3a. The form of the whole file

The rules are those of `notation.md` section 3 ("The notation is closed"), the tables of section 6, section 4 for the value forms, and N7 to N9 of section 12. Every element under the root is judged; an element that is reported as unknown, or as a `<span>` outside a text or a `<composition>` without `src`, is not read further, since there is no table to judge what is inside it. Everything else, also an element in a place it may not stand, has its own attributes and children judged by its own table.

| Code | Mistake | Message (after the code) |
|---|---|---|
| `FS-0029` | An element the notation does not have | ``<circle> is not a known element; inside <box> the elements taken are <box>, <rect>, <text>, <image>, <video>, <shape>, <composition src>, <tween> and <set>``. Where the place holds no element: ``<circle> is not a known element; no element is taken inside <tween>``. A `<span>` outside a text or a span: ``<span> is not a known element here; a <span> stands directly in <text> or <span>``. A name is shown as it was written, with its prefix. |
| `FS-0030` | An attribute the element does not take | ``<rect> has no attribute `colour` ``. The element is named as `<composition src>` for a composition shown from another file, and as `<composition>` for the root. An attribute with a prefix (`xml:lang`) is not taken by any element. |
| `FS-0031` | An element in a place it may not stand | ``<box> is not known inside <text>; only <span>, <tween> and <set> are; a <box> stands directly in <composition>, <box> or <rect>``. Where the place holds nothing: ``<tween> is not known inside <font>; <font> holds no element; a <tween> stands directly in <box>, <rect>, <text>, <span>, <image>, <video>, <shape> or <composition src>``. An element that stands only in the root, with the element it was found in: ``<style> is not known inside <box>; a <style> stands only directly in the <composition>, never inside an element`` (`<style>`, `<font>`, `<audio>`). A `<composition>` without `src` inside a composition: ``a <composition> inside a composition needs the attribute `src`; without one it stands only as the root of a file``. |
| `FS-0032` | A value that is not a number | ``` `x="a"` is not a number: an optional sign, digits with an optional point and an optional exponent, such as 12, -4, .5 or 2.5e-1 ```; a number too large to be finite: ``` `x="1e999"` is too large: a number must be finite ```; the `width` or `height` of a `<video>`: ``` `width="10.5"` is not a whole number ```. |
| `FS-0033` | A value that is not a time | `` `at="-1"` is not a time in seconds: digits with an optional point, such as 5 or 1.25 `` |
| `FS-0040` | A `start` without a `duration`, or a `duration` without a `start`, on an element that takes them as a clip | `` <box> needs both start and duration, or neither `` |
| `FS-0058` | A `<tween>` with no `duration`, or a `duration` of 0 | `` <tween> has no `duration`; a tween needs a duration above 0 ``; ``<tween> has `duration="0"`; a tween needs a duration above 0`` |
| `FS-0059` | A `<tween>` with no `from-` or `to-` attribute | `` <tween> changes no property; a tween needs at least one `from-` or `to-` attribute `` |
| `FS-0060` | A `from-` or `to-` attribute of a `<tween>` whose property the element it moves does not take as an attribute (`notation.md` section 8.1: `count` on a `<text>` only, `typed` on a `<text>` or `<span>`, `draw` on a `<shape>`) | ``` `to-count` changes `count`, which <box> does not take; a tween changes only the properties of the element it moves ``` |
| `FS-0061` | A `<set>` with no property | `` <set> changes no property; a set needs at least one property, such as `x` `` |
| `FS-0062` | A property of a `<set>` that the element it moves does not take as an attribute (`notation.md` section 8.1) | ``` `count` changes `count`, which <box> does not take; a set changes only the properties of the element it moves ``` |
| `FS-0063` | An `ease` that is not an easing the notation takes (`notation.md` section 8.4) | ``` `ease="bounce"` is not a known easing; the easings taken are linear, in-quad, out-quad, in-out-quad, in-cubic, out-cubic, in-out-cubic, in-quart, out-quart, in-out-quart, in-quint, out-quint, in-out-quint, in-sine, out-sine, in-out-sine, in-expo, out-expo, in-out-expo and out-back(n), with n a number ``` |
| `FS-0064` | A background layer, size, place or repeat not in the form of `render-command.md` section 4.6 | ``` `background="linear-gradient(90deg, #ff0000 0%)"` is not in the form of a background: a gradient needs two stops or more, each a colour and a place ``` |
| `FS-0065` | A list of background settings of the wrong length | ``` `background-size="10 10, 20 20"` has 2 entries for 3 layers: write one entry for all the layers or one for each ``` |
| `FS-0066` | A background setting on an element with no `background` | ``` `background-size="10 10"` is written on <box>, which has no `background` ``` |
| `FS-0067` | A number that counts written other than as `notation.md` section 7.12 gives it | ``` `count-format="0.0.0"` is not a form of number; the forms are `0`, `0.00`, `0,0`, `0,0.00`, `m:ss` and `m:ss.0`, with up to six decimals ```; ``` `count-format` needs `count` ```; ``a <text> that counts holds no words of its own: its text is the number``; ``` a tween of `count` needs the attribute `count` on its <text> ``` (`a set of` for a `<set>`) |

- **Where the line is.** The line of a report is the line of the element's opening `<` (`FS-0029`, `FS-0031`, `FS-0040`, `FS-0058`, `FS-0059`, `FS-0061`) or of the attribute (`FS-0030`, `FS-0032`, `FS-0033`, `FS-0060`, `FS-0062`, `FS-0063`, `FS-0064`, `FS-0065`, `FS-0066`, and `FS-0067` but for words or a span in a counting text, which is on the text's line), and the order of section 4 holds for them together with the reports of the root.
- **A tween.** A `<tween>` that stands where it may (`FS-0031` otherwise) is judged by `notation.md` section 8.2: it needs a `duration` above 0 (`FS-0058`; a `duration` that is not a time is `FS-0033` alone) and at least one `from-` or `to-` attribute (`FS-0059`; one whose name is not an animated property, such as `to-width`, is `FS-0030` and counts here, so that the mistake is reported once), and each property it changes must be one the element it moves takes as an attribute (`FS-0060`, once for each such attribute). Its `ease`, when written, must be an easing of section 8.4 (`FS-0063`).
- **A set.** A `<set>` that stands where it may (`FS-0031` otherwise) is judged by `notation.md` section 8.3: it needs at least one property (`FS-0061`; one whose name is not an animated property, such as `width`, is `FS-0030` and counts here, so that the mistake is reported once), and each property it changes must be one the element it moves takes as an attribute (`FS-0062`, once for each such attribute).
- **A number that counts.** `count` and `count-format` on an element other than a `<text>` are `FS-0030`, with the message ``` `count` is for a <text>, not a <box> ``` (the element's name in place of `<box>`). On a `<text>`, judged with what its styles give it (section 3c): a `count-format` that is not one of the forms of `notation.md` section 7.12 (`0`, `0,0`, then `.` and one to six `0`s after either; `m:ss`, then `.` and one to six `0`s), on the line of the attribute; a `count-format` on a text with no `count`, on the line of the attribute; a text with `count` that holds words other than white space, or a `<span>`, on the line of the text, once; and a `<tween>` or `<set>` of `count` inside a text with no `count`, on the line of the attribute. Each is `FS-0067`. A `<tween>` or `<set>` of `count` or `typed` inside an element that does not take the property, such as a `<box>`, is `FS-0060` (a tween's property the element does not take), on the line of the attribute.
- **The pair of a clip.** `start` and `duration` are written both or neither on `<box>`, `<rect>`, `<text>`, `<span>`, `<image>`, `<video>` and `<shape>` (`notation.md` sections 5.2 and 6). A `<composition src>` and an `<audio>` are not judged for the pair: their `start` has a default. One written without the other is `FS-0040`, reported on the element's line, once for the element.
- **The attributes that are judged for their form.** A number: `x`, `y`, `scale`, `scale-x`, `rotation`, `skew-y`, `opacity`, `blur`, `brightness`, `rotate-x`, `rotate-y`, `reveal`, `max-width`, `flex-grow`, `gap`, `font-weight`, `font-size`, `border-radius`, `perspective`, `typed`, `count`, `stroke-width`, `draw` wherever an element takes them; every `from-` and `to-` attribute of a `<tween>` and every property of a `<set>`; the `volume` of an `<audio>` and the `value` of a `<volume>`. A whole number: the `width` and `height` of a `<video>`. A time: `start`, `duration`, `media-start` and `at`. The form of the root's `version`, `width`, `height`, `fps`, `duration` and `background` is judged by section 3; the value of an attribute of a `<style>` is judged on the element that takes the style; the forms of a `<box>`'s place and size, margin and position are judged by section 3b; the forms of the other attributes (colours, lists) and the range of any value are judged by the part of the product that draws the element.
- **A time** is read by N8: `.5` is the time 0.5, and `5.`, `.`, an empty value, a sign, an exponent and a unit are not times. **A number** is read by N9: `+5`, `-4`, `1e3`, `2.5E-1`, `.5` and `5.` are numbers; `inf`, `nan`, `1e`, `12px`, `50%`, an empty value and a value too large to be finite are not.
- **An `id`** is a name and is not compared with another: two elements with one `id`, or an element and an `<audio>`, are accepted and nothing is reported (N7). The `id` of a `<style>` is judged by section 3c.
- **No unknown form changes the picture.** A composition that differs from a valid one only by a form reported here has a report, so `check` ends with 1 and no render of it can start: a project is opened for a render by the same reading that `check` does, and a project with a report is not opened.

## 3b. The forms of a place

The attributes that place a `<box>` take the forms that `render` judges them by (`render-command.md` section 3 and section 4.1). `check` judges the same forms by the same code, so that a form `render` refuses for a place is reported by `check` on the same line, with the same code, the same message and the same exit status. The attributes are `left`, `top`, `right`, `bottom`, `width`, `height`, `margin` and `position` of a `<box>`, and those that lay out its children (`gap`, `padding`, `flex-direction`, `justify-content`, `align-items` and `align-self`, `render-command.md` section 4.3), with `columns` (section 4.4), `flex-grow` and `max-width` (sections 4.1 and 4.3), and the codes are:

- a length that is not a length, such as `left="10px"` or `width="5em"`, and a `width` or `height` below 0: `FS-0034`;
- a `margin` that is not one to four numbers, such as `margin="10%"`: `FS-0032`;
- a `position` that is not `static` or `absolute`, such as `position="fixed"`: `FS-0039`;
- a `gap` below 0, or a value of a `padding` below 0, such as `gap="-2"`: `FS-0034`; a `padding` that is not one to four numbers, such as `padding="10%"`: `FS-0032`;
- a word of `flex-direction`, `justify-content`, `align-items` or `align-self` that is not one of its words, such as `align-items="middle"`: `FS-0039`; a `max-width` that is not a number above 0, or a `flex-grow` below 0, such as `max-width="0"`: `FS-0034`; a `columns` that is not a list of column widths, such as `columns="wide 1fr"`: `FS-0054`; a column width of 0 or below, such as `columns="0 1fr"`: `FS-0053`; a `perspective` of 0 or below, such as `perspective="0"`: `FS-0053`, on the attribute's line; a `<box>` with both `columns` and `flex-direction`: `FS-0055`;
- a length in pixels that is a number with a fraction, such as `left="10.5"`, or a whole number beyond the limit of 1000000 pixels, and a place or a size of a box that works out beyond the limit (`render-command.md` section 4.1, rule 6): `FS-0035`, which is of the kind unsupported.

A report of this section is on the line of its attribute, or, for a place that works out beyond the limit, on the line of the `<box>`, and it reads as `render` prints it. A `<box>` that has a report, of this section or of a form `check` does not judge, is not looked into: nothing inside it is reported, as `render` does. A project whose only reports are of the kind unsupported ends with 2 (section 5).

Of the other refusals that `render` makes (`render-command.md` section 3: an element or an attribute this version does not draw, a colour that is not opaque, and the settings of the project file), `check` makes none in this version beyond those of sections 3 and 3a and the one of section 3c: an attribute that a `<style>` gives and this version does not draw, or a colour or a time that a style gives and this version does not take. It makes one more for a `<text>`: an element inside it, and an attribute of it that this version does not draw, are reported with the code, the message and the exit status that `render` gives them (`text.md`, sections 1 and 8). A project with any other of them can end with 0 here. This is not settled: it is for the owner to decide.

## 3c. The shared styles

The rules are those of `notation.md` section 9 and N5. A style belongs to its file, and `check` reads the whole file, so the styles of the file and the names given by `class` are judged here, by the reader that judges the elements. What a style gives an element is judged on that element (section 3a).

- **A style** is a `<style>` that stands directly in the root; anywhere else it is `FS-0031` (section 3a). It needs an `id`, which is its name (`FS-0041` without one). Of two styles with one `id`, the second is reported (`FS-0042`). A style holds no `class` and no element.
- **A `class`** names one or more styles of the file, separated by spaces. Each name is looked up among the styles of the file. A name that is no style is `FS-0043`, reported once for the attribute: the first name in the list that has no style. A `class` on a `<tween>`, `<set>`, `<volume>` or `<style>` is `FS-0030`, since none of them takes one (`notation.md` section 9).
- **What a style gives.** Each attribute of a style is taken by every element that names the style, as if it were written on that element, and its value is judged by the rules of that element (section 3a). The `id` of a style is its name, and no element is given it; a `class` is not given either. An attribute that the element does not take is `FS-0030`, and a value that is not in the form the element's rule gives is `FS-0032` or `FS-0033`: both are on the element's line, and the `FS-0030` message names the style. Where an element is given one attribute by more than one source, only the value that counts is judged: its own value, or else the value of the last style named in its `class`.
- **The root.** The root's `class` names styles in the same way, and the attributes a style gives the root are judged on the root (section 3), with its reports on the root's line.
- **Motion.** The movement attributes of N5 may be given by a style. A `<tween>` or a `<set>` is not shared, and a `<tween>` written inside a style is `FS-0031`.
- **A refusal that a style gives.** An attribute that a style gives an element or the root, and that this version does not draw, is refused by `render` as `FS-0035`, on the element's line. `check` reports it as well, with the same code, the same message and the same exit status: the message is the one `render-command.md` section 3 gives the attribute, with the name of the style that gives it added. A movement attribute of N5 given to a `<box>` or a `<text>` by a style is taken, as one written on the element is (`render-command.md` section 4.5); given to an element this version does not move, it is refused in this way. The same holds for a `background` that a style gives a `<box>` or the root and that is not one opaque colour.
- **A style that holds something.** A `<style>` with a `class` is `FS-0030`, on the `class`. Each element that a `<style>` holds is `FS-0031`, on that element's line, and that element is not read further (section 3a).

**Where the line is.** `FS-0041` is on the `<style>`'s opening `<`; `FS-0042` on the `id` of the second style; `FS-0043` on the `class`; `FS-0030` for a `class` on a style, on that `class`, and for an attribute a style gives that an element does not take, on the element's opening `<`; `FS-0031` for an element held by a style, on that element's opening `<`; `FS-0032` and `FS-0033` for a value a style gives, on the element's opening `<`; `FS-0035` for an attribute a style gives that this version does not draw, on the element's opening `<`.

| Code | Mistake | Message (after the code) |
|---|---|---|
| `FS-0041` | A `<style>` with no `id` | `<style> needs the attribute `id`: the name elements take it by` |
| `FS-0042` | Two `<style>` elements with one `id` | `two <style> elements have the id `label`` |
| `FS-0043` | A `class` that names no style of the file | `` `class` names the style `big`, and this file has no <style> with that id `` |
| `FS-0030` | A `<style>` with a `class`; an attribute a style gives that the element does not take | ``the style `x` has `class`; a style cannot take another style``; ``<text> has no attribute `object-fit`, which the style `label` gives it`` |
| `FS-0031` | An element inside a `<style>` | ``the style `x` holds <box>, and a style holds attributes only; a <box> stands directly in <composition>, <box> or <rect>`` |
| `FS-0035` | An attribute a style gives that this version does not draw, such as `x` or `opacity` on a `<box>` | ``<box> has an attribute this version does not support: `x`, which the style `moving` gives it`` |

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

The exit code 2 is used for a project that has no report but those of the kind unsupported (decision P2): so far the keys `compositions` and `timeline` of the project file (`FS-0023`), and a form of a `<box>`'s place or size that this version does not take (`FS-0035`, section 3b). A run that has one report of the kind invalid or failure ends with 1, whatever else it reports.

## 6. It changes nothing of the project

`check` draws nothing. It opens `framesmith.yaml` (when there is one) and the composition for reading, and changes, renames and deletes no file of the project. The one thing a run writes is its run log, `logs/framesmith.log` under the folder the command is run from (`run-log.md` section 1, decision A2). Two cases:

- **Run from another folder** (the usual case): the log is under that folder, and the project folder is left exactly as it was: every file has the same name and the same bytes as before, and no file or folder is new.
- **Run from the project folder itself, or from a folder inside it** (`cd demo && framesmith check .`): the log is under that folder, so the project folder (or the folder inside it) comes to hold a folder `logs` with the file `framesmith.log`, and those two entries are the only ones new. Every other file of the project has the same name and the same bytes as before, `check` reads `framesmith.yaml` and the composition only, and its output and exit code are those of a run from outside. A second run adds lines to the log and changes nothing else.

Apart from the lines added to the log (and the folder `logs` when it is missing), the folder the command is run from is left as it was. A project folder that cannot be written to, and one on a read-only disk, are checked like any other.

## 7. Where the code lives

- `notation` reads the XML and judges the root, the form of the whole file and the shared styles (section 3c); it knows nothing of folders or of the command (`PLAN.md` section 3). It holds the value forms (a number, a whole number, a time, a colour), the root's rules, the table of the elements with the attributes each takes and the elements each holds, the wording of the messages of sections 3a, 3c and 4, and the codes: the type `Code` is the one place where a code is named, with the list of all of them, so that the catalogue of codes (`error-catalogue.md`) extends it and nothing else: each code has its kind and its remedy there. It gives back every problem of a file, as `Problem`s in the order of section 4; a `Problem` has a code, a line and a message, gives the kind and the remedy of its code, and knows no file.
- `project` knows the project folder: it finds it from the path given (`project-location.md` section 8), reads `framesmith.yaml` when there is one (`project-file.md` section 8), finds the composition it names (`composition.xml` by default), reads it, and gives the root and the settings or the reasons there is no project, or the reports of the files: a `Report` is a `Problem` with the name of the file it is in, and prints as the line of section 4. Its wording is that of the `No project` lines, each with the code of its reason.
- `timeline` judges the forms of a `<box>`'s place and size, margin and position (section 3b) with the code that `render` uses for them, and gives them to `render` and `check` as `place_reports` (`render-command.md` section 4.1): one judgement for both commands, in one place.
- `cli` is the `framesmith` command: it reads the arguments, calls `project`, and for the forms of section 3b calls `timeline` through `render`, prints, and sets the exit code.
- The XML reader is the outside crate `roxmltree` (MIT OR Apache-2.0), named in `PLAN.md` section 3. No type of it appears in a public interface of `notation`. The YAML reader is `saphyr` (`project-file.md` section 8).
- The rule for showing a written value in a message (section 4) is one function of `notation`, `shown`, which `project` uses for the project file too.
