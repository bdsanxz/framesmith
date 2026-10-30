# The worked-colour strip of the colour-filters fixture (T060, proposal)

`worked-strip.png` is 8 pixels wide and 1 high, all opaque, one worked colour a pixel, left to right:

| x | colour | red | green | blue |
|---|---|---|---|---|
| 0 | warm | 200 | 100 | 50 |
| 1 | cool | 16 | 32 | 64 |
| 2 | white | 255 | 255 | 255 |
| 3 | black | 0 | 0 | 0 |
| 4 | red | 255 | 0 | 0 |
| 5 | green | 0 | 255 | 0 |
| 6 | blue | 0 | 0 | 255 |
| 7 | grey | 128 | 128 | 128 |

It is made, with the implementation, by frx7's test-only picture writer (decision A12) from these values, and
checked in here as `pictures/worked-strip.png`. No third-party media and no file made by another tool.

## What each filtered row must show

Worked by hand from `docs/spec/notation.md` section 7.9 (round half up, clamp 0 to 255 after each step). The
`expected/` pictures are checked in from renders of this composition and held to these values: every pixel of a flat
patch equals its value here (criterion c6 holds the processor's frames to the byte; the card's agreement is criterion
c7, `PLAN.md` section 5.2).

| filter | warm | cool | white | black | red | green | blue | grey |
|---|---|---|---|---|---|---|---|---|
| `brightness(0.5)` | 100,50,25 | 8,16,32 | 128,128,128 | 0,0,0 | 128,0,0 | 0,128,0 | 0,0,128 | 64,64,64 |
| `contrast(2)` | 255,73,0 | 0,0,1 | 255,255,255 | 0,0,0 | 255,0,0 | 0,255,0 | 0,0,255 | 129,129,129 |
| `saturate(0)` | 118,118,118 | 31,31,31 | 255,255,255 | 0,0,0 | 54,54,54 | 182,182,182 | 18,18,18 | 128,128,128 |
| `grayscale(1)` | 118,118,118 | 31,31,31 | 255,255,255 | 0,0,0 | 54,54,54 | 182,182,182 | 18,18,18 | 128,128,128 |
| `brightness(0.5) contrast(2)` | 73,0,0 | 0,0,0 | 129,129,129 | 0,0,0 | 129,0,0 | 0,129,0 | 0,0,129 | 1,1,1 |
| `contrast(2) brightness(0.5)` | 128,37,0 | 0,0,1 | 128,128,128 | 0,0,0 | 128,0,0 | 0,128,0 | 0,0,128 | 65,65,65 |
| `saturate(1) contrast(1) brightness(1)` | 200,100,50 | 16,32,64 | 255,255,255 | 0,0,0 | 255,0,0 | 0,255,0 | 0,0,255 | 128,128,128 |
| `grayscale(0.5)` | 159,109,84 | 23,31,47 | 255,255,255 | 0,0,0 | 155,27,27 | 91,219,91 | 9,9,137 | 128,128,128 |
| no filter | as the strip above |
