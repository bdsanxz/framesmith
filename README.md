# Framesmith — v1

Framesmith turns a small KDL composition folder into a playable MP4:
H.264 video (`yuv420p`) plus AAC stereo 48 kHz audio, with staged
progress, machine-readable (`--json`) reports, and a pinned
self-provided FFmpeg build when the host has none.

Run the commands below in order. Build steps run from this directory
(`framesmith/`); everything from “Examples” on runs from
`framesmith/crates/cli`, with the freshly built binary on `PATH`.

## Requirements

- macOS (v1 is proven on macOS; see “v1 limits”).
- A Rust toolchain (`cargo`). If `cargo --version` fails, install it
  from [rustup.rs](https://rustup.rs) and re-run the command.
- Network access once, OR FFmpeg 6+ with `libx264` and `aac`
  encoders on `PATH`. On first render with no suitable FFmpeg,
  Framesmith downloads its pinned build itself and announces it; if
  the machine is offline instead, the render refuses with the fix
  named (see limit 4).

## Install

```sh
cargo --version
cargo build --bin framesmith
./target/debug/framesmith --version
export PATH="$PWD/target/debug:$PATH"
framesmith --version
cd crates/cli
```

Expected: `cargo` reports its version, the build finishes, and both
`--version` lines print `framesmith 0.1.0`.

## Examples

`examples/hello` is the v1 front door: one `composition.kdl`, no
`framesmith.toml`, no font files, no audio inputs. It declares no
`font`, so the render line announces the fallback it used — the
system font it found (for example `using system font Arial.ttf`),
or the built-in default face where a machine has no system font.

```sh
framesmith check examples/hello
framesmith render examples/hello
```

Expected: `check` exits 0 with `ok: examples/hello is valid (640x360,
3s, 30fps, 1 clip(s)).`; `render` exits 0, announces the fallback
face in its `rendered ...` line, and writes
`examples/hello/out.mp4` (plus the per-run `logs/framesmith.log`).

If you have `ffprobe` (it ships with a manual FFmpeg install; the
self-provided build is an `ffmpeg` binary only), the container
contract is:

```sh
ffprobe -v error -show_entries stream=codec_name,width,height,pix_fmt,sample_rate,channels -of default=noprint_wrappers=1 examples/hello/out.mp4
```

Expected: `codec_name=h264`, `width=640`, `height=360`,
`pix_fmt=yuv420p`, `codec_name=aac`, `sample_rate=48000`,
`channels=2`.

Two more bundled folders use only v1 elements and render the same way:

```sh
framesmith check examples/text-on-color
framesmith render examples/text-on-color
framesmith check examples/text-on-video
framesmith render examples/text-on-video
```

Expected: exit 0 each time. `text-on-color` draws two text clips over
a solid background (no audible input, so the MP4 carries the
encoder-default silent AAC stereo 48 kHz track). `text-on-video`
scales a committed 640x360 test clip with `fit="contain"`, draws text
over it, and passes the source sound through.

## Commands

```sh
framesmith --version
framesmith --help
framesmith init demo --dir /tmp/readme-probe
framesmith check /tmp/readme-probe/demo
framesmith check --json examples/hello
framesmith errors E_NO_FFMPEG
framesmith errors
```

- `init <name> [--dir <parent>]` creates a minimal project that
  passes `check` untouched. Names are 1–64 ASCII letters, digits,
  `-`/`_`, starting with a letter or digit; anything else is refused
  and nothing is created.
- `check [--json] [--backend cpu|gpu|auto] <folder>` validates
  without rendering and without needing FFmpeg. Exit 0 = valid,
  1 = invalid, 2 = valid but using an element from a later sprint.
  Every problem carries file, line, column, code, and fix.
- `errors [--json] [CODE]` explains every error code.
- `render <folder> [-o <file>] [--backend cpu|gpu|auto]
  [--preset draft|standard|high] [--crf N] [--video-bitrate B]
  [--json]` revalidates, reports the FFmpeg finding (reusing the host
  build, reusing/downloading the pinned build with an announcement, or
  refusing before the first frame), draws frames, mixes the 48 kHz
  track, and writes one MP4 atomically. `--crf` and `--video-bitrate`
  together are refused; `--backend gpu` on a machine whose probe finds
  no GPU renderer is refused — both before the first frame with no MP4
  written.

## v1 limits

Every limit below is observable: run the command, see the refusal.

1. **Invalid folders are refused with located diagnostics (exit 1)
   and no MP4 is written.**
   `framesmith check fixtures/invalid-timing` exits 1 with
   `error[E_BAD_TIMING]` carrying file, line, column, and fix;
   `framesmith render fixtures/invalid-timing` exits 1 and writes no
   MP4.
2. **Later-sprint elements are refused before the first frame
   (exit 2), never mis-rendered.**
   `framesmith check fixtures/later-audio` exits 2 naming the element
   and its sprint; `framesmith render fixtures/later-audio` exits 2
   with `render refused before the first frame` and writes no MP4.
3. **GPU rendering needs a probed GPU renderer.**
   `framesmith render examples/hello --backend gpu -o
   /tmp/readme-probe/gpu.mp4` on a machine without one exits 1 naming
   the missing capability and suggesting `--backend cpu` (or `auto`,
   the default). No MP4 is written.
4. **No FFmpeg and no network is a named refusal, not a hang.**
   With FFmpeg hidden from `PATH`, an empty tool cache, and offline
   forced, `framesmith render <folder>` exits 1 with
   `render refused before the first frame (E_NO_FFMPEG)`, names the
   install-or-reconnect fix, and leaves no MP4 and no partial file.
   `framesmith check <folder>` still exits 0 in the same environment:
   validation never needs FFmpeg. With network available instead, the
   same hidden-FFmpeg render downloads the pinned build, announces
   `downloaded pinned FFmpeg ... (SHA256 verified)`, and exits 0.
5. **The container is always H.264 `yuv420p` plus AAC stereo
   48 kHz.** Even with no audible input the MP4 carries the silent AAC
   track (`mix: no audible sound; silent track`). Audio starts on its
   written sample (48 kHz, ≤ 1 ms).
6. **CPU renders are deterministic.** Two renders of the same folder
   decode byte-identically (frame bytes, not file bytes — the MP4
   wrapper is not canonical). With any `ffmpeg` available:
   `framesmith render examples/hello -o /tmp/readme-probe/a.mp4`,
   `framesmith render examples/hello -o /tmp/readme-probe/b.mp4`,
   decode the first 10 frames of each to `rawvideo`/`rgb24` and `cmp`
   the two outputs: identical.
7. **v1 is a macOS command-line product.** Pinned self-provided builds
   are recorded for macOS (`aarch64`, `x86_64`) only. Other systems
   are not claimed for v1: there the bootstrap refuses with the manual
   `install FFmpeg 6+ ...` fix instead of guessing a download.

## Layout

- `crates/cli/examples/hello` — the minimal example (this README's
  front door).
- `crates/cli/examples/text-on-color`, `text-on-video` — the other
  bundled examples.
- `crates/cli/fixtures/` — invalid folders (exit 1) and later-sprint
  folders (exit 2) used above and by the test suite.
- `crates/cli/src/`, `crates/render/`, `crates/validate/`,
  `crates/glyph/`, `crates/ffmpeg/`, … — the implementation;
  `cargo test --workspace` runs the full suite.
