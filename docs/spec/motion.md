# Motion

Status: v0 (Sprint 1). Sources: `PLAN.md`, section 3 (what `motion` keeps: tweens, keyframes, easings, paths, expressions, the pure evaluator) decision D3 (our own engine, easings written by us) and decision D2 (motion is written in the composition's XML, so its written form belongs to the notation's spec); the principle that animation is put at a time and never played; the easing formulas below, which are the standard power curves written out by us.

This is the behaviour of the motion engine, the `motion` crate: what a tween is, the five properties, the ten easings, and the value of a property at a time. The way motion is written in a composition is left to the notation's spec.

## A tween

| Field | Value |
|---|---|
| target | A name the caller resolves to the elements the tween moves. Each one is animated the same way. |
| kind | One of the four kinds below. |
| from, to | Values of properties: the starting values (from and from-to tweens) and the end values (to, from-to and set tweens). |
| at | Start time from the start of the composition, 0 or more. |
| duration | Length, 0 or more. A set tween has none. |
| ease | One of the ten easing names. `linear` when not otherwise given. |

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
| `rotation` | degrees, clockwise | 0 |
| `opacity` | 0 to 1 | 1 |

A value is a number. `opacity` outside 0 to 1 is not a value of the engine.

## Easing names

`linear`, `in-quad`, `out-quad`, `in-out-quad`, `in-cubic`, `out-cubic`, `in-out-cubic`, `in-quart`, `out-quart`, `in-out-quart`.

With `p` the progress from 0 to 1 and `n` = 2, 3, 4 for quad, cubic, quart:

- linear: `p`
- in: `pⁿ`
- out: `1 − (1 − p)ⁿ`
- in-out: `2ⁿ⁻¹ pⁿ` when `p < 0.5`, otherwise `1 − (2 − 2p)ⁿ ÷ 2`

Every curve gives exactly 0 at `p = 0` and exactly 1 at `p = 1`.

## The value of a property at a time

For one property of one element, take the tweens that touch it, ordered by start time. Tweens with the same start keep the order in which they were given, and the later one wins.

1. Before the first tween starts: if the first tween is a from or from-to tween, the property holds that tween's starting value. Otherwise it holds the element's own value.
2. From a tween's start until the next tween on that property starts: the property follows that tween. During the tween it is `start + (end − start) × ease(progress)`. From the tween's end it holds the end value.
3. A set tween is a tween of no length: its value applies from the frame at its time.

The value depends only on the time and the tweens. It never depends on which frame was shown before.

A tween may run past the end of the composition. The video is not lengthened; the last frame shows the value for its own time.
