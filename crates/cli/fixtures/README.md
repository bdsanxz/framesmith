# Framesmith Sprint 1 + Sprint 2 (T001/T002) fixtures

Each folder holds one `composition.kdl` (plus the referenced prop file
where the case needs one on disk). Run `framesmith check <folder>` and
`framesmith render <folder>` from `framesmith/` to reproduce the
expected verdict.

## Sprint 2 valid fixture — `check` exits 0, `render` writes an MP4

| Folder | Content |
|---|---|
| `sprint2-still` | A box, two images, written paint order, and text with line height and letter spacing; box and image pixels land in T004/T005, text draws now |

## Invalid fixtures — `check` exits 1, `render` refuses with exit 1

| Folder | Defect | Key diagnostic |
|---|---|---|
| `invalid-unknown-key` | Unknown composition key `bogus` | `E_UNKNOWN_KEY` with file, line, column |
| `invalid-timing` | `text` clip without `start` | `E_BAD_TIMING` (absolute half-open timing) |
| `invalid-missing-video` | `video` points at `media/gone.mp4` | `E_MISSING_FILE` naming the file |
| `invalid-missing-font` | `font` points at `fonts/gone.ttf` | `E_MISSING_FILE`, never silent substitution |
| `invalid-font-format` | `fonts/bad.woff` is junk bytes | `E_BAD_FONT` with the supported-format fix |
| `invalid-text-attr` | `align="diagonal"` | `E_BAD_TIMING` (exactly left/center/right) |
| `invalid-video-fit` | `fit="zoom"` over committed `clip.mp4` | `E_BAD_FIT` (exactly contain/cover/fill) |
| `invalid-box-size` | `box` with zero width | `E_BAD_BOX` (sizes must be positive) |
| `invalid-image-missing` | `image` points at `media/gone.png` | `E_MISSING_FILE` naming the file |
| `invalid-image-corrupt` | `image` file is not a PNG/JPEG picture | `E_BAD_IMAGE` naming the file and reason |
| `invalid-order` | Non-integer paint `order` values | `E_BAD_ORDER` with file, line, column |
| `invalid-spacing` | Negative line height and letter spacing | `E_BAD_SPACING` with file, line, column |

Every diagnostic carries file, line, column, code, and a fix direction,
and `check --json` reports the same set for agents.

## Render-refusal fixture — `check` exits 0, `render` exits 1, no MP4

| Folder | Defect |
|---|---|
| `invalid-corrupt-video` | `clip.mp4` is not video data; render refuses naming the file |

## Later-sprint fixtures — `check` exits 2, `render` refuses before the first frame with exit 2

| Folder | Element | Enabling sprint |
|---|---|---|
| `later-audio` | `audio` with an `effect` key | Sprint 10 |
| `invalid-image-format` | `image` with an animated-GIF source | Later footage sprint (PNG/JPEG only in Sprint 2) |

Boxes and images with PNG/JPEG sources (T002), `tween` and `set` directives (T003), and plain `audio` tracks with trim, volume, and fades (T007) validate as ordinary elements. Each refusal names the element and its
enabling sprint; no partial MP4 is written.

## Proof

`tests/cli_examples_fixtures.rs` runs the whole set: the Sprint 2
still-life folder checks and renders to a standard MP4, invalid folders
get exit 1 with complete located diagnostics in both human and `--json`
forms, later-sprint folders get exit 2 naming element and sprint, the
corrupt-video folder fails render with the file named and no MP4, and
re-renders of the examples decode byte-identically.

## Sprint 3 pipeline facts

`PIPELINE.md` next to this file records the exact encoder numbers
behind each quality preset, the GPU availability probe rule, the
parallel-worker determinism rule, and the 48 kHz mix rules — the facts
the Sprint 3 proof artifacts quote.
