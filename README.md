# Framesmith

Framesmith renders video from compositions written in XML. A composition is a folder of plain files: an XML file in Framesmith's own notation, with the fonts and media it uses. Framesmith draws every frame itself, on the CPU or on the GPU, and writes an MP4 file. It runs on your own machine, with nothing else to install. Its media work (containers, codecs, audio and pixel format conversion) is done by [frx7](https://github.com/bdsanxz/frx7), a media stack written for it in Rust.

## Status

Framesmith is in early development, and the notation and the command line may still change.

- It runs on macOS only. It is developed and tested on macOS 27 on Apple silicon; older versions of macOS have not been tested.
- `framesmith render` draws `<box>`, `<text>` and `<composition src>` elements today, animated with `<tween>` and `<set>`. The output is H.264 video in an MP4 file, with a stereo AAC audio track.
- Images, video, shapes, rectangles and audio (`<image>`, `<video>`, `<shape>`, `<rect>`, `<audio>`) are defined by the notation but not rendered yet, and HEVC output (`--codec hevc`) is not supported yet. `render` rejects a composition or an option that needs them, names what it rejects, and writes no video file. `check` does not yet report these itself.

## Requirements

- macOS on Apple silicon.
- Rust, through [rustup](https://rustup.rs). The toolchain is set by `rust-toolchain.toml` (stable, with `rustfmt` and `clippy`); the minimum Rust version is the workspace's `rust-version` in `Cargo.toml`.
- For the quality gates only: `cargo-deny` 0.20.2 (`cargo install --locked cargo-deny --version 0.20.2`).

Cargo fetches frx7 from its public repository at the revision pinned in `Cargo.toml`. No credentials are needed.

## Build

```
cargo build -p cli
```

The command is `target/debug/framesmith`. Add `--release` for an optimised build in `target/release/`, which renders much faster.

## Quick start

Make a folder `hello` with one file, `hello/composition.xml`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<composition version="1" id="hello" width="640" height="360" fps="30" duration="2" background="#101820">
  <style id="card" background="#e0a030" border-radius="12"/>
  <box class="card" left="40" top="130" width="160" height="100" opacity="0">
    <tween duration="1" from-x="-40" to-x="0" to-opacity="1" ease="out-cubic"/>
  </box>
  <text left="240" top="160" font-family="Helvetica" font-size="36" color="#ffffff">Hello, Framesmith</text>
</composition>
```

Check it:

```
$ framesmith check hello
Ready: the project is valid, 60 frames, 640x360, 30 fps
```

Render it:

```
$ framesmith render hello
Rendered hello/renders/hello.mp4
frames: 60
size: 640x360
frame rate: 30 fps
length: 2.000 s
renderer: card (chosen by auto)
output: renders/hello.mp4 (default)
backend: auto (default)
quality: standard (default)
encoder: hardware (default)
codec: h264 (default)
workers: auto (default)
audio bitrate: 192 kbit/s (default)
elapsed: ...
```

The video is two seconds long. Over the first second, the orange card slides in from the left and fades in, and the text stays in place.

`check` and `render` take the project's folder, its project file, or its composition file. `render` takes these options:

```
framesmith render <path> [--output <path>] [--backend auto|cpu|gpu] [--quality draft|standard|high]
                         [--encoder hardware|software] [--codec h264|hevc] [--workers auto|1-16] [--marks]
```

`--backend cpu` renders on the CPU and `--backend gpu` on the GPU (shown as `card` in the summary). Both renderers give the same image within a small, tested tolerance; the CPU renderer is the reference.

`check` exits with 0 when the project is valid and 1 when something in it is invalid. Every message names the file and line, and carries a code such as `FS-0030`. Each command writes a log to `logs/framesmith.log` under the folder it is run from.

## The notation in brief

- The root element is `<composition>`. It gives the version (`1`), the size in pixels, the frame rate, the length in seconds and the background.
- Drawn elements are placed with `left`, `top`, `right`, `bottom`, `width` and `height`, in pixels or percentages, or are laid out in rows and columns. Where CSS has a name for an attribute, the notation uses that name and its meaning.
- Shared styles are `<style id="...">` elements, taken by `class`. There are no selectors and no cascade.
- `<font src="..." family="...">` declares a font file of the project. The machine's own fonts can be named too.
- Motion is written in the composition itself. A `<tween>` moves a property from one value to another over a time with an easing; a `<set>` changes it at an instant. This version animates `x`, `y`, `scale`, `scale-x`, `rotation`, `skew-y`, `opacity`, `rotate-x` and `rotate-y`.
- A frame depends only on the composition and the frame's number, so any frame can be drawn on its own.

The full definition is in [`docs/spec/notation.md`](docs/spec/notation.md). The other files in [`docs/spec/`](docs/spec) specify the commands, the renderers, text, motion and timing.

## Architecture

One cargo workspace. Each crate under `crates/` is named after its role:

| Crate | Role |
|---|---|
| `cli` | The `framesmith` command. |
| `notation` | Reads a composition's XML and its values, and reports problems with their lines. |
| `project` | Finds and reads the project folder. |
| `timebase` | Exact times, frame rates and the frame grid. |
| `motion` | The motion engine: tweens, easings, and the value of every animated property at any time. |
| `layout` | Layout: positions and sizes, in whole pixels. |
| `text` | Fonts, shaping, line breaking and the glyphs of each line. |
| `timeline` | What is shown at a given frame. |
| `draw` | The list of drawing commands, and the interface both renderers implement. |
| `draw-cpu` | The CPU renderer, the reference for the correct image. |
| `draw-gpu` | The GPU renderer, the default. |
| `mix` | The audio mix: 48 kHz, stereo. |
| `render` | The render job: draws each frame and writes the video file through frx7. |
| `measure`, `parity` | Reading frames back, measuring them, and comparing renderers. Used in development only. |

Framesmith hands the rendered frames and the audio to frx7, which encodes them with the operating system's codecs and writes the MP4 file.

## Development

```
cargo test --workspace            # the tests
scripts/gates/install-hooks.sh    # once after cloning: the gates then run before every push
scripts/gates.sh                  # every gate
```

`scripts/gates.sh` runs formatting, static analysis (`clippy`, warnings denied), the tests, the core suite, the licence check (`cargo deny`) and a check of the frx7 pin. The core suite runs the tests again in a clean environment, with only the system's folders on the search path. `--gate NAME` runs one gate.

- Tests that need the hardware encoder or a graphics card are named `reference_machine_only_...`. Hosted CI lists them as not run, and the local gates run them.
- To build against a local copy of frx7, run `scripts/gates/frx7.sh override /path/to/frx7`. It writes `.cargo/config.toml`, which is not to be committed; `scripts/gates/frx7.sh drop-override` removes it and returns to the pinned revision.

The gates are specified in [`docs/dev/gates.md`](docs/dev/gates.md), and working with frx7 in [`docs/dev/frx7.md`](docs/dev/frx7.md).

## Contributing

Bug reports and feature requests are welcome through [GitHub Issues](https://github.com/bdsanxz/framesmith/issues). As the project is dual-licensed, external code contributions are not accepted at this time.

## Licence

Framesmith is available under two licences:

- the GNU General Public License, version 3 or (at your option) any later version. See [`LICENSE`](LICENSE).
- a commercial licence, for use in closed-source or proprietary work without the GPL's terms. To ask about one, see [github.com/bdsanxz](https://github.com/bdsanxz).
