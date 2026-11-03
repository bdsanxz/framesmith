# Sprint 3 render-pipeline facts (T001, T002, T003, T008)

Recorded next to the fixtures as the specification requires. Behavior
is fixed; only these numbers and rules may change with a re-proof of
every fixture.

## Backend selection (T001, CAP-001)

- Flag: `render --backend cpu|gpu|auto`; default (no flag) is `auto`.
- `auto` uses the GPU renderer when the capability probe reports one
  available, else the CPU renderer. The chosen backend is named in the
  render output (`[backend=cpu]` / `[backend=gpu]`) and the run log.
- Detection rule (a detected fact, not an author claim): Sprint 3 ships
  software encoding only, so no hardware GPU renderer is present on an
  ordinary machine and the probe reports unavailable; setting
  `FRAMESMITH_GPU_AVAILABLE=1` opts into the portable GPU picture path
  (`render::gpu`), used to exercise the `gpu` backend in tests.
- `check --backend <name>` accepts all three spellings and never
  changes the verdict: the same folder is valid or invalid under every
  backend. Any other spelling is a validation error on both commands.
- Refusal: `--backend gpu` without a GPU renderer refuses before the
  first frame, names the missing capability, suggests `--backend cpu`
  or `--backend auto`, and writes no MP4.

## Quality presets (T002, CAP-002)

Active preset: `--preset draft|standard|high`; `standard` when none is
named. The active preset and the effective encoder settings appear in
the render output (`[backend=…, preset=…, crf=…, x264-preset=…,
video-bitrate=…]`) and the run log.

| Preset | CRF | x264 preset | Extra flags emitted |
|---|---|---|---|
| `draft` | 30 | `veryfast` | `-crf 30 -preset veryfast` |
| `standard` | 23 | encoder default (`medium`) | none (bytes match Sprint 2 exactly) |
| `high` | 18 | `slow` | `-crf 18 -preset slow` |

- `--crf N` (0–51) overrides only the CRF axis of the active preset.
- `--video-bitrate B` (for example `500k`, `2M`) adds only `-b:v B`.
- Both overrides together, an unknown preset name, or a non-numeric
  override refuses before the first frame with no MP4 written.

## Parallel frames (T003, CAP-003)

- Frames are drawn across `FRAMESMITH_WORKERS` workers (1–64, default
  1) with thread-local raster caches and re-emitted in frame order.
- Any worker count draws byte-identical MP4s; a solo frame equals the
  same frame from a full render (exactly on CPU, deterministically on
  the GPU path). A worker failure aborts the render with the underlying
  reason named and no partial MP4 at the target.
- An invalid `FRAMESMITH_WORKERS` value falls back to serial rendering
  and says so in the output and the run log.

## 48 kHz stereo mix (T008, CAP-009 + CAP-010)

- Framesmith builds one 48 kHz stereo mix: every `audio` track is
  decoded to PCM, trimmed, gained (`volume`, silence through +12 dB),
  linearly faded, placed at its written start rounded to the nearest
  sample (at most half a sample of error, far inside the 1 ms bound of
  48 samples), and summed with no limiter and no normalization.
- Gain above +12 dB is refused up front, never limited silently.
- The mix is byte-identical across reruns and worker counts.
- A composition with no audible sound carries a full-duration AAC
  stereo 48 kHz track of exact digital silence.
- Missing, unreadable, corrupt, or undecodable sound refuses before
  the first frame naming the file and the audio stage, with no MP4.
- Video sound passes through untouched until Sprint 3 T010 moves it
  into the mix; compressed sources carry their codec padding (FFmpeg
  decode behavior), so the 1 ms proof uses lossless WAV sources.
