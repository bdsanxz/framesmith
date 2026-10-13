# The project file: `framesmith.yaml`

Status: draft, for the proof of concept of the native path (XML compositions drawn by our own renderer). Sources: the product owner's request of 2026-10-12; `FRAMESMITH_SPEED.md` (the decisions for the XML architecture); the concept source's project file, as an idea only: one small file in the project folder that says where things are and how media is treated (its public schema lists a catalog address, three folders, one media switch, and two fields for usage reports).

## What it is

A project is a folder. `framesmith.yaml` in that folder holds the settings that belong to the project and not to one composition: which composition is rendered, where finished files go, and how they are rendered and encoded.

- The file is optional, and so is every key in it. A folder with a `composition.xml` and no project file is a complete project.
- A composition says what the video is (size, frame rate, length, elements, motion, sound). The project file never repeats that. If a setting changes the picture or the sound, it belongs in the composition.
- An option on the command line wins over the project file. The project file wins over the default.
- The file is YAML: a mapping of the keys below. A key that is not listed, or a value of the wrong kind, is an error reported with its line. Nothing is ignored silently.

## The project folder

The folder that holds `framesmith.yaml` is the project folder. Every path in the project file, and every `src` in a composition of the project, is relative to the project folder. A composition may therefore sit in a sub-folder and still name `assets/photo.jpg`.

`render` is given a project folder, a project file, or a composition:

- A folder: its `framesmith.yaml` is read if there is one.
- A file named `framesmith.yaml`: its folder is the project folder.
- A composition (`.xml`): the nearest folder at or above it that holds a `framesmith.yaml` is the project folder, and that file's settings apply, except that the composition given is the one rendered. With no project file above it, the composition's own folder is the project folder.

## Keys

| Key | Kind | Default | Meaning |
|---|---|---|---|
| `name` | text | the project folder's name | The project's name. It names the finished file. |
| `composition` | path | `composition.xml` | The composition that `render` draws. |
| `paths.renders` | path | `renders` | The folder finished files are written to. It is created when missing. |
| `render.output` | path | `<paths.renders>/<name>.mp4` | The finished file. |
| `render.backend` | `auto`, `cpu`, `gpu` | `auto` | The renderer. `cpu` defines the correct picture. `gpu` must match it within the agreed tolerance. `auto` takes the GPU renderer when the build has it and a suitable adapter is found, and the CPU renderer otherwise. |
| `render.quality` | `draft`, `standard`, `high` | `standard` | How hard the video encoder works: `draft` is quick and small, `standard` is the product's normal encoding, `high` keeps more detail and takes longer. |
| `render.encoder` | `hardware`, `software` | `hardware` | Which H.264 encoder writes the picture. `hardware` is the machine's own video encoder, at a quality setting that follows `quality`. `software` is the system's encoder run on the processor, at a bit rate that follows `quality`: it is slower and keeps less of the picture, and is for a machine whose hardware does not encode the film's size. When the machine's hardware does not encode the film, `render` stops and says so; it never falls back by itself. No other program is started for either. |
| `render.codec` | `h264`, `hevc` | `h264` | The codec of the picture. `h264` plays on every player and browser; a machine's hardware encodes it up to a size of its own (4096 by 2304 on the machines tried). `hevc` is for films larger than that, 8K among them, and plays on fewer players. A size the chosen encoder does not take stops `render` before the first frame; the message names this setting when the other codec may take the size. |
| `render.workers` | `auto`, or a whole number from 1 to 16 | `auto` | How many parts of the film are rendered at the same time. Each worker has a renderer, decoders and an encoder of its own, and the parts are joined without encoding again. The pictures are the same for any number. `auto` picks a number that suits the renderer and the machine. |
| `render.audio-bitrate` | whole number, kbit/s | `192` | The bit rate of the encoded sound. |

A complete project file for a project whose composition has the default name can be empty. A typical one is two lines:

```yaml
name: launch-film
composition: film.xml
```

With everything written out:

```yaml
name: launch-film
composition: film.xml
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

## Command-line options that override it

| Option of `render` | Overrides |
|---|---|
| `--output <file>` | `render.output` |
| `--backend <auto, cpu or gpu>` | `render.backend` |
| `--quality <draft, standard or high>` | `render.quality` |
| `--encoder <hardware or software>` | `render.encoder` |
| `--codec <h264 or hevc>` | `render.codec` |
| `--workers <auto or a number>` | `render.workers` |

## What the concept source's file has that this one does not, and why

| Their setting | Here |
|---|---|
| The address of a catalog of ready-made parts, and the folders installed parts go to | No catalog exists in Framesmith. A composition names its files by path, and any folder layout works, so no folder needs declaring. |
| A switch that makes browser-friendly copies of videos in codecs a browser plays badly | Not needed: no browser plays the sources. They are decoded by the system's decoders through frx7, which reads H.264 and HEVC; a source in another codec is refused by name (`native-render-pipeline.md`, section 4). |
| The authoring workflow a project came from, and a log of installed catalog parts, both for usage reports | Not built: Framesmith sends no usage reports. |

## What is left for later

- Fonts shared by every composition of a project. Today a composition names its font files itself with `<font>`.
- The number of frames drawn at once. The proof of concept draws one frame at a time.
- Output formats other than MP4.
