# Motion

Status: v0 (Sprint 1). Sources: `PLAN.md`, section 3 (what `motion` keeps: tweens, keyframes, easings, paths, expressions, the pure evaluator) decision D3 (our own engine, easings written by us) and decision D2 (motion is written in the composition's XML, so its written form belongs to the notation's spec); the principle that animation is put at a time and never played; the easing formulas below, which are the standard power curves written out by us.

This is the behaviour of the motion engine, the `motion` crate: what a tween is, the seven properties, the easings, and the value of a property at a time. The way motion is written in a composition is left to the notation's spec.

## A tween

| Field | Value |
|---|---|
| target | A name the caller resolves to the elements the tween moves. Each one is animated the same way. |
| kind | One of the four kinds below. |
| from, to | Values of properties: the starting values (from and from-to tweens) and the end values (to, from-to and set tweens). |
| at | Start time from the start of the composition, 0 or more. |
| duration | Length, 0 or more. A set tween has none. |
| ease | One of the nineteen easing names, or `out-back(n)`. `linear` when not otherwise given. |

## The four kinds

| Kind | Values | Runs |
|---|---|---|
| to | `to` | from the value in effect at its start, to the given value |
| from | `from` | from the given value, to the value in effect at its start |
| from-to | `from` and `to` | between the two given values |
| set | `to` | the given value from `at` onward |

"The value in effect at its start" is what earlier tweens on that property of that element give at that time, or the element's own value when there is no earlier tween.

## Properties

| Name | Unit | The element's own value |
|---|---|---|
| `x` | pixels, to the right | 0 |
| `y` | pixels, downward | 0 |
| `scale` | a factor, the same on both axes | 1 |
| `scale-x` | a factor on the horizontal axis alone, applied with `scale` | 1 |
| `rotation` | degrees, clockwise | 0 |
| `skew-y` | degrees: points right of the transform origin move down for a positive angle | 0 |
| `opacity` | 0 to 1 | 1 |

A value is a number. `opacity` outside 0 to 1 is not a value of the engine.

The engine gives each property's value at a time. How the values move the element's picture (the transform origin, the order scale, slant, turn, move, and fading as one picture) is the notation's rule (`notation.md` section 8).

## Easing names

The nineteen names: `linear`; `in-quad`, `out-quad`, `in-out-quad`; `in-cubic`, `out-cubic`, `in-out-cubic`; `in-quart`, `out-quart`, `in-out-quart`; `in-quint`, `out-quint`, `in-out-quint`; `in-sine`, `out-sine`, `in-out-sine`; `in-expo`, `out-expo`, `in-out-expo`. And `out-back(n)`, with `n` a finite number. How a name is written, and what is reported for one that is not taken, is the notation's (`notation.md` section 8.4).

With `p` the progress from 0 to 1 (a value outside is taken as the nearer end), each curve is written by us from these definitions (`notation.md` section 8.4 states the same formulas):

- `linear`: `p`.
- The power curves, with `n` = 2, 3, 4, 5 for quad, cubic, quart, quint: in `pⁿ`; out `1 − (1 − p)ⁿ`; in-out `2ⁿ⁻¹ pⁿ` when `p < 0.5`, otherwise `1 − (2 − 2p)ⁿ ÷ 2`.
- sine: in `1 − cos(p π ÷ 2)`; out `sin(p π ÷ 2)`; in-out `(1 − cos(p π)) ÷ 2`.
- expo: in 0 at `p = 0`, otherwise `2^(10p − 10)`; out 1 at `p = 1`, otherwise `1 − 2^(−10p)`; in-out 0 at `p = 0` and 1 at `p = 1`, otherwise `2^(20p − 10) ÷ 2` when `p < 0.5` and `(2 − 2^(10 − 20p)) ÷ 2` from there.
- `out-back(n)`: 0 at `p = 0`; otherwise, with `q = p − 1`, `q² ((n + 1) q + n) + 1`.

Every curve gives exactly 0 at `p = 0` and exactly 1 at `p = 1`: the ends are given as those numbers, not worked out. Elsewhere a curve as computed agrees with its definition to within 1e-12 (the tolerance of decision A29 for this ticket).

### Worked values

The values at a quarter, a half and three quarters, worked from the definitions by exact arithmetic (`√2` the square root of 2). The tests hold each curve to these within 1e-12.

| Curve | `p = 1/4` | `p = 1/2` | `p = 3/4` |
|---|---|---|---|
| `linear` | 1/4 | 1/2 | 3/4 |
| `in-quad` | 1/16 | 1/4 | 9/16 |
| `out-quad` | 7/16 | 3/4 | 15/16 |
| `in-out-quad` | 1/8 | 1/2 | 7/8 |
| `in-cubic` | 1/64 | 1/8 | 27/64 |
| `out-cubic` | 37/64 | 7/8 | 63/64 |
| `in-out-cubic` | 1/16 | 1/2 | 15/16 |
| `in-quart` | 1/256 | 1/16 | 81/256 |
| `out-quart` | 175/256 | 15/16 | 255/256 |
| `in-out-quart` | 1/32 | 1/2 | 31/32 |
| `in-quint` | 1/1024 | 1/32 | 243/1024 |
| `out-quint` | 781/1024 | 31/32 | 1023/1024 |
| `in-out-quint` | 1/64 | 1/2 | 63/64 |
| `in-sine` | 1 − √(2 + √2) ÷ 2 | 1 − √2 ÷ 2 | 1 − √(2 − √2) ÷ 2 |
| `out-sine` | √(2 − √2) ÷ 2 | √2 ÷ 2 | √(2 + √2) ÷ 2 |
| `in-out-sine` | (2 − √2) ÷ 4 | 1/2 | (2 + √2) ÷ 4 |
| `in-expo` | √2 ÷ 256 | 1/32 | √2 ÷ 8 |
| `out-expo` | 1 − √2 ÷ 8 | 31/32 | 1 − √2 ÷ 256 |
| `in-out-expo` | 1/64 | 1/2 | 63/64 |

### The shape of the curves

- **Mirrors.** Each `in-` curve is the mirror of its `out-` curve: `in(p) = 1 − out(1 − p)` for every `p`, within 1e-12.
- **Middles.** Each `in-out-` curve is 1/2 at `p = 1/2`, within 1e-12.
- **`out-back(n)`.** For `n` above 0 the curve rises above 1 before its end and comes back to exactly 1 at it. Its largest value is `1 + 4n³ ÷ (27 (n + 1)²)`, at `p = 1 − 2n ÷ (3 (n + 1))`; for `n = 1.7` that is `1 + 19.652 ÷ 196.83` (about 1.0998425), at `p = 47/81`. For `n = 0` the curve is `out-cubic`, which never rises above 1. The largest value is worked out by setting the slope of `q² ((n + 1) q + n)` to 0, which gives `q = −2n ÷ (3 (n + 1))`.

## The value of a property at a time

For one property of one element, take the tweens that touch it, ordered by start time. Tweens with the same start keep the order in which they were given, and the later one wins.

1. Before the first tween starts: if the first tween is a from or from-to tween, the property holds that tween's starting value. Otherwise it holds the element's own value.
2. From a tween's start until the next tween on that property starts: the property follows that tween. During the tween it is `start + (end − start) × ease(progress)`. From the tween's end it holds the end value.
3. A set tween is a tween of no length: its value applies from the frame at its time.

The value depends only on the time and the tweens. It never depends on which frame was shown before.

A tween may run past the end of the composition. The video is not lengthened; the last frame shows the value for its own time.
