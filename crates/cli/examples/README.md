# Framesmith bundled examples (Sprints 1 + 4)

Three minimal folders plus two Sprint 4 reference folders, all using
only v1 elements. All pass
`framesmith check` with no edits and render from a clean checkout —
even on a machine without FFmpeg, where the first render self-provides
the pinned build and announces it.

## `hello`

The v1 front door (Sprint 4, CAP-001): one `composition.kdl`, no
`framesmith.toml`, no font files, no audio inputs. A single `text`
clip over a solid background color (`#1a1a2e`), 640x360, 3 s at
30 fps. No `font` is declared, so the render line announces the
fallback it used (the system font found, or the built-in default
face where a machine has none).

```sh
framesmith check examples/hello
framesmith render examples/hello
```

Expected: exit 0, `examples/hello/out.mp4` (H.264 yuv420p video +
AAC stereo 48 kHz, silent track) showing the greeting over the
background color.

## `text-on-color`

Two `text` clips over a solid background color (`#1a1a2e`), 640x360,
3 s at 30 fps. No `font` is declared, so the CPU renderer uses the
built-in default face and says so in its output. No audible input, so
the MP4 carries the encoder-default silent AAC stereo 48 kHz track.

```sh
framesmith check examples/text-on-color
framesmith render examples/text-on-color
```

Expected: exit 0, `examples/text-on-color/out.mp4` (H.264 yuv420p video +
AAC stereo 48 kHz) showing both text lines over the background color.

## `text-on-video`

A `video` clip (`clip.mp4`, a short self-made 640x360 test pattern with
a 440 Hz tone, committed next to the composition) scaled with
`fit="contain"`, with two `text` clips drawn over it, 640x360, 3 s at
30 fps. The output keeps the source sound as-is (no volume, fade, or
mix in Sprint 1).

```sh
framesmith check examples/text-on-video
framesmith render examples/text-on-video
```

Expected: exit 0, `examples/text-on-video/out.mp4` (H.264 yuv420p video
+ AAC stereo 48 kHz) showing timed video with text over it and the
original tone audible.

## Footage reference (Sprint 5, T010)

`reference-footage`: 6 s 720p30 footage proof — VFR phone-style source
trimmed to `[1,4)` and slowed to half rate as a full-bleed background,
rotated portrait source trimmed and looped as a pillarbox overlay, and
a finite GIF sticker with an inferred 2 s duration. No text: text
parity is Sprint 4 territory, so every pixel exercises footage
behavior. The HyperFrames equivalent is `reference-footage-hf`
(same content and timing via `data-media-start`,
`data-playback-rate`, the native `loop` attribute, and an explicit
`data-duration="2"` where the KDL side infers). The picture plus speed
comparison is re-runnable with one command (see
`evidences/proof-n05-T010/compare.sh`).

```sh
framesmith check examples/reference-footage
framesmith render examples/reference-footage
```

Expected: exit 0; `out.mp4` is 1280x720 H.264 `yuv420p` video plus
AAC stereo 48 kHz audio; check echoes the effective windows and the
inferred GIF length; re-renders are byte-identical at any worker count.

## References (Sprint 4, T002)

Two larger folders proving composed output, each with committed media
under `media/` and entrance/exit animation via `tween`:

- `reference-1-title-card`: 10 s 1080p30 title card — `text` title and
  subtitle, `image` card art, `box` accent bar, `audio` bed. Entrance
  tweens slide/fade/scale in; exit tweens fade out over [9,10).
- `reference-2-lower-third`: 8 s 1080p30 animated lower-third over
  video — `video` background clip with sound, two `box` bars, two
  `text` lines, `audio` sting with fades. The lower-third slides in
  over [0.5,1.5) and slides/fades out over [7,8).

```sh
framesmith check examples/reference-1-title-card
framesmith render examples/reference-1-title-card
framesmith check examples/reference-2-lower-third
framesmith render examples/reference-2-lower-third
```

Expected: exit 0 each time; `out.mp4` files are 1920x1080 H.264
`yuv420p` video plus AAC stereo 48 kHz audio. The HyperFrames
equivalents of both references are benchmark inputs owned by the
Sprint 4 proof tasks, not part of these folders.

## Structured reference (Sprint 6, T011)

`structured-reference`: one project proving the timing/structure
behaviors end to end — relative starts, a twice-used nested bumper
with differing trims, a hidden layer, a canvas preset, inferred
durations, and two compositions. The root `main` (64x64, 30fps,
inferred 4 s) chains `intro` → `bumper-a` (trim [0,1), red half) →
`bumper-b` (trim [1,2), green half) → `outro` via `start-after`, hides
a green `ghost` box over [1,2), and infers the `title` text and `bed`
audio (committed `media/tone.wav`, exactly 4 s) spans to the
composition end. Reusable `bumper` is red [0,1) + green [1,2);
`cutdown` uses canvas preset `square` with an inferred 1 s duration.

```sh
framesmith check examples/structured-reference
framesmith render examples/structured-reference
framesmith render examples/structured-reference --composition cutdown
framesmith render examples/structured-reference --start 1 --end 3
```

Expected: exit 0 each time; `timeline` lists the authored windows
with the hidden flag, `info` reports both canvases and the inferred
durations, root/named/range MP4s carry exact windows with aligned
sound, and every re-render is byte-identical. Six invalid variants
(duplicate id, start-after cycle, ambiguous root, sub-1/60 s length,
circular nest, empty sub-range) refuse before frame one. The
one-command proof is `evidences/proof-n06-T011/prove.sh`
(`tests/cli_n06_t011_reference.rs` covers the same gates in the suite).

## Proof

`tests/cli_examples_fixtures.rs` renders both examples from temp copies,
confirms the container/codec contract with `ffprobe`, confirms drawn
text by decoding frames, confirms the video example's sound with
`volumedetect`, and re-renders to confirm byte-identical CPU frames.
