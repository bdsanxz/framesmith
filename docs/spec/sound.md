# Sound placed in the film

Status: in force from Sprint 1. This spec says how an `<audio>` of the composition becomes sound in the film's one sound track (`render-command.md` section 5.2): which sounds are read, where each one begins and ends to the sample, which part of its source is heard, how its channels and rate are brought to the mix, and what is reported before the first frame when a sound cannot be placed. The level of a sound (`volume`, `<volume>`) is not applied in this version; nor are fades, loops or the lowering of a loud mix.

Sources: `notation.md` section 6.10 (the attributes of `<audio>`), section 11 (sound) and the points N3 and N4 of section 12 (the kinds read, a sound shorter than its clip); `render-command.md` section 5.2 (the mix: 48,000 sample frames a second, two channels, as long as the picture to the sample); `frx7/docs/spec/os-codecs.md`, the part on sound files (the kinds frx7 reads, its refusals and the length it gives), and `frx7/docs/spec/audio.md` (one channel brought to two, and the conversion of the rate), at the revision Framesmith pins (`docs/dev/frx7.md`); the proofs' `native-notation-reference.md`, read as a record of behaviour (decision B4): a sound placed by its time and not by the frame, from `media-start`, and a source that ends early followed by silence. The proofs reported a sound file they could not read with no line; here every report has one.

## 1. The sounds read

- An `<audio>` that stands in the root of the project's composition is a sound of the film. It takes `src`, `start`, `duration` and `media-start`, and `id` and `class` (which name nothing and change nothing here; `notation.md` section 12, N7).
- **Not applied in this version, and refused** (`FS-0035`, unsupported, on the line of the attribute or the element): a `volume` on an `<audio>`, a `<volume>` inside one (the level of a sound is a later ticket), and an `<audio>` in a file shown from another file (`<composition src>`), whose time would be that of its clip. A `class` whose style gives an `<audio>` a `volume` is refused the same way.
- The order of the sounds in the file changes nothing in what is heard: sounds that play at the same time are added sample by sample (section 4).

## 2. Where a sound is in the film

The mix counts sample frames from 0, at 48,000 a second (`render-command.md` section 5.2). A time is read exactly as the notation writes it (`timebase`): `1.0001` is ten thousand and one ten-thousandths of a second, never a binary number.

- **Its first sample frame** is the one nearest `start` × 48,000: the whole number nearest that product, a half rounded up. `start="1.0001"` is 48,004.8, so the sound begins at sample frame **48,005**. It is not moved to a frame's edge: the frame at 30 frames a second nearest that time begins at 48,000.
- **Its end** is the sample frame nearest (`start` + `duration`) × 48,000, by the same rule; the sound plays up to that frame and not on it. Its length is the end less the first sample frame: `duration="2"` from `start="1"` is 48,000 to 144,000, **96,000** sample frames. Rounding both ends of the written times, not the length alone, keeps two sounds written end to end with no gap and no overlap.
- **Before its first sample frame and from its end on**, the sound adds nothing: where no other sound plays, the mix is silence (0).
- **The film's end.** The mix is as long as the picture (`render-command.md` section 5.2), whatever the sounds: a sound that would go on past the last sample frame of the film is cut there, and the sound track is still exactly as long as the picture, to the sample.

## 3. What is heard of the source

- **From `media-start`.** The first sample frame heard is the source's sample frame nearest `media-start` × the source's own rate, by the rule of section 2. For a source at 48,000 frames a second, `media-start="0.5"` makes the first sample heard the source's frame **24,000**, which is the sample at 0.5 seconds; each later sample frame of the sound is the next frame of the source. Without `media-start`, the source is heard from its first frame.
- **To the mix's form.** A source is brought to 48,000 frames a second and two channels by frx7 (`audio::Converter`, `frx7/docs/spec/audio.md`): first its channels, then its rate. A source of **one channel is heard on both sides, each side the source's sample times 0.70710678** (3 dB lower, the rule of frx7's spec, so that the sound is as loud in two loudspeakers as it was in one). A source of two channels is heard as it is, left on the left. A source of six channels is brought to two by frx7's rule. A source at another rate keeps its pitch and its length; the rates and channels other than 48,000 and one or two are the business of a later ticket, which states their tests.
- **A source shorter than its clip** is heard to its end and is silent after (`notation.md` N4): a source of 1 second in a sound of 3 seconds is heard for the first 48,000 sample frames of the sound and adds nothing to the other 96,000. Its end is where frx7's reader gives no more sound (frx7 gives the length of the real sound, without the padding of the last packet).
- **The value of a sample** is the source's, as frx7 reads it (32-bit floating point, full scale ±1), through the conversion above, and nothing else in this version: no level, no fade.

## 4. More than one sound

The mix at a sample frame is the sum of the samples that every sound gives at that frame, channel by channel. Nothing is limited or lowered in this version: a sum above full scale is handed to the encoder as it is (lowering a loud mix is a later ticket).

## 5. What is reported, before the first frame

Each `<audio>` is judged, and its file opened through frx7, when the render is prepared: before the first frame is drawn, before the renderer is chosen and before any file is made. A sound that cannot be placed is reported on the line of its element, with the composition file's name, the file as `src` writes it, and the reason; `render` then writes no file, and ends as `render-command.md` section 7 says for its reports (exit code 1 when one report is invalid or a failure, 2 when every report is unsupported). `check` makes the same reports by the same code, with the same exit codes (`check-command.md`).

The judging is in two steps. What is found from the composition alone (no `src`, no `duration` above 0, no file at `src`) is reported with the other problems of the composition, in the order of the lines, as the faults of a `<font>` are. The sound files are opened only when the composition has no other problem: then every sound that cannot be opened is reported, in the order of the lines.

| What is wrong | Code | Kind | The message says |
|---|---|---|---|
| no `src` | `FS-0064` | invalid | `` `<audio>` needs the attribute `src`, the path of a sound file: from the project folder, or a path that begins with `/` `` |
| no `duration`, or a `duration` of 0 | `FS-0063` | invalid | `` `<audio src="<as written>">` needs a `duration` above 0, the seconds the sound plays `` (with the value written, when there is one). An `<audio>` with no `src` has no file to name: it is named `` `<audio>` ``, and its missing `src` is reported on its own (`FS-0064`, the first row) |
| the file is not there, or `src` names a folder | `FS-0064` | invalid | `` `src="<as written>"` names no file: a sound file, from the project folder or a path that begins with `/` `` |
| the file is there and the machine does not give it (no permission) | `FS-0065` | failure | `` `src="<as written>"` could not be read: <the system's reason> `` |
| the file is not sound of a kind frx7 reads, or a kind read in a form that is not (another number of channels, MP3 below 16,000 Hz) | `FS-0066` | unsupported | `` `src="<as written>"` is not sound this version reads: <frx7's reason>. The kinds read are wav, aiff, mp3 and aac-in-mp4 `` |
| the file is of a kind read and cannot be decoded (damaged, cut short) | `FS-0067` | invalid | `` `src="<as written>"` cannot be decoded: <frx7's reason> `` |
| `media-start` at or past the end of the source | `FS-0068` | invalid | `` `media-start="<as written>"` is at or past the end of `src="<as written>"`, which lasts <seconds, three decimals> s `` |

- **The kinds read** are those frx7 lists as read (`codec::SoundKind::READ`, `frx7/docs/spec/os-codecs.md`): `wav` and `aiff` of linear PCM with one, two or six channels, `mp3` with one or two, and `aac-in-mp4` (AAC in an MP4 file, `.m4a`) with one or two. The list in the message is taken from frx7's, in its order, so that it is always the list frx7 reads. The kind is told from the content of the file, never from its name.
- **A missing file and an undecodable one are not unsupported**: the author named the wrong file, or a damaged one, and is told so (exit code 1).
- **The reason** is frx7's own sentence, as frx7 gives it. It names the file by its path on the machine. It is in the report, which the author reads, and never in the log.
- A `media-start` is compared with the source's length only when frx7 gives a length.

## 6. The log

The `render` crate logs, at level debug and under the component `render::sound`, `sound placed` for each sound it opened, with the line of its element, its first sample frame and its length in sample frames, and then `sounds placed` with the number of sounds, when there is at least one (`run-log.md` section 5). No path, no `src` and no reason is logged (`run-log.md` section 6). frx7 logs the start and the end of each sound decoder on its own (`frx7/docs/spec/run-log.md`, section 6).

## 7. Where the code lives

- `timeline` gathers the sounds of the composition with the scene (`Scene::sounds`, and `timeline::sounds` for `check`): for each `<audio>`, its line, its `src` as written and as a path of the machine, its first sample frame and its end, and its `media-start`; it refuses what section 1 refuses and reports the faults found from the composition alone (section 5). It looks for the file, and does not open it.
- `render` opens each sound file through frx7's `codec` registry with the system's backend (`render::open_sounds`), makes the reports of the second step of section 5, and hands the opened sources to the mix. `check` and `render` call it alike.
- `mix` places the sounds: it reads each source from its `media-start`, brings it to the mix's form through frx7's `audio`, and adds it in its sample frames. It too is built without the system's interfaces: it knows a source only by frx7's `codec::AudioSource` trait.
- `timebase` gives the sample frame nearest a time (`Time::nearest_count`).
