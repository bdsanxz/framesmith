# Frame time and frame count

Status: v0 (Sprint 1). Sources: proposal `s01-first-render` (CAP-003, CAP-010); product-owner decision of 2026-10-11 on the frame-count rule; the observed frame-count behaviour of the reference.

## Times

- A written time is a decimal number of seconds, for example `1`, `0.5`, `6.01`. At most nine digits may follow the decimal point. No exponent, no sign other than a leading `-`, no units.
- A written time is read exactly, as a fraction. `0.1` is one tenth, not the nearest binary number.
- All comparisons between times use exact arithmetic. A time is turned into a binary number only at the last step, when a tween needs its progress as a number between 0 and 1.

## Frame rate

- In this version a frame rate is a whole number from 1 to 240.
- When the composition writes no rate, the rate is 30.
- `render --fps N` replaces the rate for one run. The length in seconds does not change.

## Frame time

- Frame `i` shows the composition at time `i ÷ fps`, as an exact fraction.
- The first frame is frame 0, at time 0.

## The frame grid rule

One rule turns a written time into a frame number. It is used for the frame count, for the first and last frame of a clip, and for the start and end of a tween.

> Multiply the time by the frame rate. If the result is within 0.001 of a whole number, take that whole number. Otherwise round up to the next whole number.

"Within 0.001" includes exactly 0.001. The rule is applied in exact arithmetic.

Why a tolerance: an author cannot write a time such as 7/60 of a second exactly in decimal. `0.116667` is meant to be frame 7 at 60 frames per second, and the rule makes it so. The tolerance is one thousandth of a frame, which no viewer can see.

## Frame count

- The frame count is the frame grid rule applied to the composition length, with a minimum of 1.
- Examples: 6 s at 24, 30 and 60 gives 144, 180 and 360. 6.01 s at 30 gives 181 (180.3 rounds up). A length that gives 180.0005 frames gives 180.
- The video is never lengthened by a clip or a tween that runs past the end.

## Clip window in frames

- A clip with start `s` and duration `d` has first frame `F(s)` and end frame `F(s + d)`, where `F` is the frame grid rule.
- Frame `i` shows the clip when `F(s) ≤ i < F(s + d)`. The start is included; the end is not.
- For times that are not within the tolerance of a frame time this is the same as: the clip shows when `s ≤ i ÷ fps < s + d`.

## Tween times

- A tween's start and end are moved onto the frame grid only when they are within the tolerance of a frame time; otherwise they are used as written.
- Progress at frame time `t` is `(t − start) ÷ (end − start)`, held to the range 0 to 1.
