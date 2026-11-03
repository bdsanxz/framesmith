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

## Proof

`tests/cli_examples_fixtures.rs` renders both examples from temp copies,
confirms the container/codec contract with `ffprobe`, confirms drawn
text by decoding frames, confirms the video example's sound with
`volumedetect`, and re-renders to confirm byte-identical CPU frames.
