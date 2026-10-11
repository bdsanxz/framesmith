# Motion block format

Status: v0 (Sprint 1). Sources: proposal `s01-first-render` (CAP-005, CAP-011, CAP-012); `PLAN.md` decisions D2 and D3; the principle that animation is put at a time and never played.

## Where it is written

A motion block is a `script` element in `index.html` with the type `application/framesmith-motion+json`. Its text is JSON. A page may hold several blocks; their tweens are read in document order as one list.

```html
<script type="application/framesmith-motion+json">
{ "composition": "main",
  "tweens": [
    { "target": "#card", "from": { "y": 60, "opacity": 0 }, "at": 1, "duration": 0.5, "ease": "out-cubic" }
  ] }
</script>
```

The block is either an object with a `tweens` list, or just the list. In the object form `composition` is optional; when written it must be the composition's name.

## A tween

| Field | Required | Value |
|---|---|---|
| `target` | yes | A CSS selector. It is matched inside the composition element and may match several elements; each one is animated the same way. |
| `to`, `from`, `set` | one kind | An object of property values (see below). |
| `at` | yes | Start time in seconds from the start of the composition, 0 or more. |
| `duration` | yes, except for `set` | Length in seconds, 0 or more. A `set` tween has no duration. |
| `ease` | no | One of the ten easing names. `linear` when not written. |
| `kind` | no | `to`, `from`, `from-to` or `set`. When written it must agree with the value fields. |

### The four kinds

| Kind | Fields | Runs |
|---|---|---|
| to | `to` | from the value in effect at its start, to the given value |
| from | `from` | from the given value, to the value in effect at its start |
| from-to | `from` and `to` | between the two given values |
| set | `set` | the given value from `at` onward |

"The value in effect at its start" is what earlier tweens on that property of that element give at that time, or the element's own value when there is no earlier tween.

### Properties

| Name | Unit | The element's own value |
|---|---|---|
| `x` | pixels, to the right | 0 |
| `y` | pixels, downward | 0 |
| `scale` | a factor, the same on both axes | 1 |
| `rotation` | degrees, clockwise | 0 |
| `opacity` | 0 to 1 | 1 |

A value is a JSON number. `opacity` outside 0 to 1 is an error.

### Easing names

`linear`, `in-quad`, `out-quad`, `in-out-quad`, `in-cubic`, `out-cubic`, `in-out-cubic`, `in-quart`, `out-quart`, `in-out-quart`.

With `p` the progress from 0 to 1 and `n` = 2, 3, 4 for quad, cubic, quart:

- linear: `p`
- in: `pⁿ`
- out: `1 − (1 − p)ⁿ`
- in-out: `2ⁿ⁻¹ pⁿ` when `p < 0.5`, otherwise `1 − (2 − 2p)ⁿ ÷ 2`

Every curve gives exactly 0 at `p = 0` and exactly 1 at `p = 1`.

## The value of a property at a time

For one property of one element, take the tweens that touch it, ordered by start time. Tweens with the same start keep their document order, and the later one wins.

1. Before the first tween starts: if the first tween is a from or from-to tween, the property holds that tween's starting value. Otherwise it holds the element's own value.
2. From a tween's start until the next tween on that property starts: the property follows that tween. During the tween it is `start + (end − start) × ease(progress)`. From the tween's end it holds the end value.
3. A set tween is a tween of no length: its value applies from the frame at its time.

The value depends only on the time and the motion block. It never depends on which frame was shown before.

A tween may run past the end of the composition. The video is not lengthened; the last frame shows the value for its own time.

## How motion meets the author's CSS

- Position, scale and rotation are added on top of whatever transform the author's CSS gives the element. Motion moves the element first by `x` and `y`, then turns it by `rotation`, then scales it by `scale`, all about the element's own transform origin (its centre unless the author says otherwise). The author's `transform` then applies inside that.
- Motion uses the CSS properties `translate`, `rotate` and `scale` of the elements it targets. An element that motion targets must not have these three set by the author; that case is refused (see `support-matrix.md`).
- `opacity` from motion multiplies the opacity the author's CSS gives the element.
- A property that no tween touches is left exactly as the author wrote it.
- Motion keeps running while a clip is hidden: when the clip shows again, its elements are where the time says.

## Selectors that `check` can read

`check` matches targets against the files without a browser. It understands type, `#id`, `.class`, `[attr]`, `[attr="value"]` and `*`, joined into compound selectors, with descendant (space) and child (`>`) combinators and comma-separated lists. Any other selector form is refused in this version.

## Errors

Not valid JSON; a block that is neither an object with `tweens` nor a list; a tween that is not an object; a missing `target`, value field, `at` or `duration`; a negative `at` or `duration`; a value that is not a number; `kind` that disagrees with the value fields; a `set` tween with a duration; a target that matches no element; `composition` naming another composition. Each is reported with the file and line.

Fields, kinds, properties and easings outside this document are either recognised and refused, or reported as unknown and refused. They are never ignored. The lists are in `support-matrix.md`.
