# `init`: the starter project

Status: v1 (Sprint 1, T101). Sources: the ticket T101 (its words on what to build and its scope boundary); `CLAUDE.md` rule 11 (the starter holds no third-party library, font, picture or sound, and no script); `notation.md` section 6.10 (`<audio>`) and the root's `version`; `project-file.md` (the keys of `framesmith.yaml` and their defaults, section 4); `sound.md` section 5 (the kinds of sound file this version reads: linear PCM WAV); `check-command.md` and `render-command.md` (what the starter must pass); `run-log.md` (the log of a command); `error-catalogue.md` and `error-catalogue-record.md` (the codes `FS-0082` and `FS-0083`, which this command adds).

## 1. The command

`framesmith init <folder>` writes a starter project into `<folder>`, the one word the user gives, as written. It takes no option. Any other command line is refused as `command line not read` (`run-log.md` section 4): the usage line is

    usage: framesmith init <folder>

The folder is not read, opened or searched for a project: `init` writes into it and nothing else.

## 2. The folder

| The folder | What `init` does | Exit code |
|---|---|---|
| does not exist, and its parent folder exists and accepts a new folder | creates it and writes the starter (section 3) | 0 |
| exists, is a folder, and has no entry of any kind (a hidden file counts as an entry) | writes the starter into it | 0 |
| exists, is a folder, and has at least one entry | writes nothing; reports `FS-0082` | 1 |
| cannot be created or read as a folder: its parent is missing, a file of that name is in the way, the system refuses it | writes nothing; reports `FS-0083` | 1 |

The folder is created as one folder (its parent must exist). A failure part-way through the writing of the starter (for example a full disk) is reported as `FS-0083` with the path of the file that could not be written and the reason the system gives. The files already written are left as they are.

Where the folder is not empty, no file of it is opened for writing, so its files keep their bytes.

## 3. The starter's files

The starter is these files, relative to the folder, and no others. Each is written by Framesmith, from text that ships with the program or from numbers the program computes (section 3.3).

| File | What it is |
|---|---|
| `composition.xml` | The starter composition (section 3.1). |
| `framesmith.yaml` | The project file (section 3.2). |
| `audio/tone.wav` | The starter's sound (section 3.3). |

No font file, no picture file, no sound made by anyone else, and no script is written. The starter needs no font file: its text names a family of the machine's fonts (`text.md`, section 2.3), which Framesmith reads on macOS.

### 3.1 The composition

The root `<composition>` has `version="1"`, `width="480"`, `height="270"`, `fps="30"`, `duration="3"` and `background="#1b2a3a"`. Its children, in this order:

1. `<audio id="tone" src="audio/tone.wav" start="0" duration="3"/>`: the sound (section 3.3), at the start of the film, as long as the film.
2. `<box id="sky" left="0" top="0" width="480" height="150" background="#2c5f8a"/>`: the picture's upper part, a flat colour.
3. `<box id="sun" left="340" top="24" width="90" height="90" background="#f5c542"/>`: a second flat colour in the picture.
4. `<box id="ball" left="20" top="170" width="60" height="60" background="#e8553d">` with one child, `<tween at="0" duration="3" to-x="340"/>`: the ball moves 340 pixels to the right over the three seconds (motion between two times).
5. `<text id="title" left="30" top="36" font-family="Helvetica, sans-serif" font-size="36" color="#ffffff">Hello, Framesmith</text>`: the text, set in the machine's Helvetica, or in the machine's first sans-serif family when Helvetica is absent (`text.md`, section 2.3).

The picture at a chosen time is therefore a sky, a sun, a ball in its place on that frame, and the title. A composition holds no code.

### 3.2 The project file

`framesmith.yaml` names the composition, and gives the render settings its defaults (`project-file.md`, section 4):

    composition: composition.xml
    render:
      backend: auto
      quality: standard

Each key may be left out, and the values above are the defaults of their keys.

### 3.3 The sound

`audio/tone.wav` is a WAV file of linear PCM: one channel, 16 bits a sample, 48,000 sample frames a second, 144,000 sample frames (three seconds), with the RIFF header of a plain WAV file (`RIFF`, `WAVE`, a `fmt ` chunk of 16 bytes with format code 1, and a `data` chunk). Its samples are a sine wave of 440 Hz at 0.15 of full scale, with a linear fade in and out of 4,800 sample frames (0.1 second) at each end. The sample at frame `n` (from 0) is

    round(32767 × 0.15 × f(n) × sin(2π × 440 × n / 48000))

where `f(n) = min(1, n / 4800, (143999 − n) / 4800)`, and `round` takes the nearest whole number, halves away from zero. The file is therefore the same bytes on every run and every machine.

## 4. What the command prints

On success (exit 0), standard output has these lines, in this order, with `<folder>` as the user wrote it:

    Created the starter project in <folder>.
    Next, run:
      framesmith check <folder>
      framesmith render <folder>

The first line says `Created` in both cases: the folder was made, or the starter was written into an empty folder.

## 5. What the command refuses

A refusal writes one line to standard error, in the form of `error-catalogue.md` (`FS-0082: failure: <what is wrong>. <remedy>`), and exits with code 1.

| What is wrong | Code | The message says | The remedy |
|---|---|---|---|
| the folder exists and has at least one entry | `FS-0082` | `` `<folder>` is not empty, so nothing was written `` | Choose a folder that does not exist or is empty, and run the command again. |
| the folder cannot be created, or a file cannot be written in it | `FS-0083` | `` `<path>` cannot be created: <the system's reason> `` | Put right the cause the system names (the folder's parent, a file of that name, or the folder's permissions), then run the command again. |

`<path>` is the path as the program joined it from the folder the user named (the folder itself, or a file of the starter). `<the system's reason>` is the reason the system gives, without its error number.

## 6. No network

`init` opens no network connection. It reads no file outside its folder and no font, and it calls no interface that connects to another machine.

## 7. The log

Every run logs its start and its end (`run-log.md`, sections 3 and 4). The end line carries one of two new outcomes:

- `initialised` (info, exit 0): the starter was written;
- `not initialised` (error, exit 1, with `code` `FS-0082` or `FS-0083`): nothing was written, or the writing stopped part-way.

No other line is written by `init`, and the log holds no path or file content (`run-log.md`, section 6).

## 8. What the starter must pass

A fresh starter, with no change, passes `framesmith check <folder>` with exit 0 and no report, and `framesmith render <folder>` renders `renders/<folder name>.mp4` with exit 0. The film has 90 frames of 480 by 270 pixels at 30 frames a second. Its frames show the sky, the sun, the title and the ball, and the ball's place on frame 0 differs from its place on frame 89; its sound track is the tone, not silent.
