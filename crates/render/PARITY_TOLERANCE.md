# GPU–CPU picture tolerance (Sprint 2, T013/T014)

Both renderers draw only from the same `FrameDescription`. The CPU
blitter (`render::render_composition` / `Prepared::cpu_frame`) and the
GPU picture path (`gpu::render_description` / `gpu::render_frame`) are
compared on raw RGB24 bytes of the same canvas by
`gpu::compare_frames`.

## Numbers

| Bound | Value | Meaning |
|---|---|---|
| `TOL_MAX_PER_CHANNEL_ABS` | `4` | No single color channel of any pixel may differ by more than 4 levels (of 255). |
| `TOL_MEAN_ABS` | `1.0` | The mean absolute per-channel difference over the whole frame may not exceed 1.0 level. |
| `TOL_MIN_FRACTION_WITHIN` | `0.995` | At least 99.5% of pixels must have every channel within the per-pixel bound. |

A fixture passes only inside **all three** bounds. CPU reruns stay
**byte-identical** (distance zero); only the CPU-to-GPU comparison uses
this tolerance.

## Why these numbers

The GPU traversal evaluates the same authored geometry through `f32`
motion uniforms (`gpu::quantize`) while the CPU blitter keeps `f64`,
and both sides share the verified raster primitives. The expected
backend gap is therefore at most one float-ulp of motion plus integer
rounding: sub-1-level differences on blended or transformed pixels,
zero elsewhere. The bounds above admit that gap with headroom and still
reject any real drawing defect (a wrong color, a missing element, a
shifted picture all move channels by tens of levels across thousands
of pixels).

## Reporting a mismatch (REC-003)

`Comparison::report(fixture, frame)` names the fixture, the frame
number, and the measured distance (`max_abs`, `mean_abs`, `within`),
and records that no pass is granted: an outside-tolerance picture is a
renderer defect, never a fixture failure. The composition folder is
unchanged. `compare_frames` on different-sized pictures is a harness
error, also never a pass.

## Scope

- Still-element fixtures (T013): boxes, images, written paint order,
  line height, letter-spacing text, and video stills.
- Motion fixtures (T014): tweened position/scale/rotation/opacity in
  to/from/from-to forms, set holds, and eased tweens on box, image,
  text, and video targets. Eased mid-motion frames agree within
  tolerance; tween start and end frames agree as the still-element
  bounds require (they are exact in practice).
- Frame independence (T015) does **not** use this tolerance: frame N
  alone equals frame N from a full render **exactly** on CPU, and any
  worker count draws **byte-identical** pictures on either renderer.
- Weakening any number above requires a full fixture re-proof.

## No-GPU machines

Backend agreement is proven by fixtures where both renderers run; the
picture paths are portable and need no GPU hardware. A machine without
a usable GPU still checks, renders, and validates exactly as Sprint 1
does, and no composition requires author-side GPU hardware.
