# Page protocol

Status: version 1 (Sprint 1); Sprint 2 ticket T010 added one optional field to a clip of the manifest, `relative_start`, without changing the version. Sources: proposal `s01-first-render` (CAP-013, CAP-014, INT-002, CAP-023); project rule 10 (one versioned contract between the Rust core and the in-page runtime).

This is the contract between Framesmith (the host) and the page it renders. Change it in this order: this document, the tests, then both sides in one change.

## Parts

- **The loader**: one small JavaScript file. It replaces the page's clocks, starts the WebAssembly runtime, collects facts from the page, and applies the values the runtime computes.
- **The runtime**: Rust compiled to WebAssembly. It reads the composition and the motion block, resolves them with the same code the host uses, and computes every clip's visibility and every animated value for a frame.

Both are served to the page from the reserved path `/__framesmith__/` (`loader.js`, `runtime.wasm`). A project may not use that path.

## How the page is opened

1. The host starts its own browser with its own empty profile, sized to the composition, at a device scale factor of 1.
2. The host answers every request the page makes. Files come from the project folder. A request for anything outside the project or to any network address is not sent; it is recorded as a use of a network asset.
3. The host sets `__framesmithBoot` before any document script runs: `{ "protocol": 1 }`, plus test-only fields.
4. The page is the project's `index.html` with two additions at its very start: the loader script and the base style sheet (see `composition.md`). Because the loader is first, it runs before any script of the composition.

## Clocks

The loader replaces, before anything else in the page runs:

| Replaced | Behaviour |
|---|---|
| `Date` (`new Date()`, `Date.now()`) | Composition time in milliseconds, counted from 1 January 1970, 00:00 UTC. At time 0 the date is that instant. |
| `performance.now()` | Composition time in milliseconds. |
| `setTimeout`, `clearTimeout` | A callback runs when composition time reaches its due time, never by real time. |
| `setInterval`, `clearInterval` | The same, repeating. An interval shorter than 1 ms is treated as 1 ms. |
| `requestAnimationFrame`, `cancelAnimationFrame` | Callbacks run once per seek, with the composition time as their argument. A callback registered while callbacks are running waits for the next seek. |

- While the page loads, composition time is 0.
- Before the page reports ready, callbacks due at time 0 are run, so start-up code that defers itself by a zero timeout has finished.
- On a seek to time `T`: timers due at or before `T` run in order of due time (then in order of creation), each seeing its own due time as the current time; then the clock is set to `T`; then animation-frame callbacks run.
- Real time never reaches the composition's scripts. The loader keeps one real timer for itself, to measure the cost of a seek.
- The time zone of the page is UTC.

## `__framesmith`

The loader defines one global object.

| Member | Type | Meaning |
|---|---|---|
| `protocol` | number | 1. |
| `ready` | promise of a string | Resolves once the page is ready, with the ready report as JSON. |
| `begin(config)` | function | Called once by the host after it accepts the ready report. `config` is `{ "fps": 30 }`. Returns `""` or a JSON problem report. |
| `seek(frame)` | function | Puts the page at frame `frame`. Synchronous. Returns `""` when there is nothing to report, otherwise a JSON problem report. |
| `costs()` | function | Returns a JSON list: the real time in milliseconds that each seek took, in call order. |

### Ready

The page is ready when all of these hold:

1. the document and its sub-resources have loaded;
2. the runtime has started;
3. every declared font face has loaded;
4. every `img` element and every CSS background image has loaded and decoded;
5. callbacks due at time 0 have run.

The ready report:

```json
{ "protocol": 1,
  "clocks": true,
  "manifest": { },
  "problems": [ { "code": "FS-E020", "description": "…" } ],
  "unsupported": [ { "code": "FS-U017", "detail": "framesmith.timeline" } ],
  "failures": [ { "kind": "image", "detail": "art/logo.png" } ] }
```

- `clocks` is true only when the loader has confirmed that the replaced clocks are the ones the page sees.
- `manifest` is the composition as the page sees it: name, size, rate, length, clips with their windows, tweens with the number of elements each target matches. A clip whose start is written relative to another clip (`after:<clip id>`, `composition.md`) carries its resolved `start` like any clip, and also `relative_start`: the value as written, offset included (`after:intro+0.5`), spaces at its ends removed. Ticket T011 changed nothing else in the protocol: the offset is part of the same text. A clip whose start is a time has no `relative_start` field. The page resolves the relative start itself, with the same code as the host, from the clips of its own document. It is `null` when the page's composition cannot be resolved; `problems` then says why.
- `failures` lists what stopped the page from being ready in a usable state: `image`, `font`, `file` (any other file the page asked for, such as a style sheet) or `script`, with the file or the error message.

The host waits at most 30 seconds for `ready`.

### What the host does with the report

In this order, before any frame is captured:

1. A protocol number other than 1 stops the run (FS-R013).
2. `clocks` not true stops the run (FS-R009).
3. A request to a network address, which the host refused to send, stops the run (FS-U009, exit code 2).
4. Any failure stops the run (FS-R006 for an image, a font or a file; FS-R007 for a script).
5. Any unsupported use stops the run with exit code 2.
6. Problems the page found in its own composition, when it could not resolve it, stop the run (FS-R010).
7. The manifest is compared with the one read from the files. Any difference stops the run and names the first difference with both values (FS-R010). Neither side wins. For a clip the comparison covers its start, then the way the start is written (a relative start or a time), then its duration. When only the way the start is written differs, the message reads, for example: the way the start of clip 2 (#b) is written is "after:a" in the files and a time in seconds in the page.

The protocol version stays 1 for `relative_start` because the field is optional and left out when absent: a manifest of clips that all have a time as their start is exactly what it was, a reader that does not know the field reads such a manifest unchanged, and the page and the host are built and shipped together from the same source, so they never meet with only one of them knowing the field.

### Seek

`seek(frame)`:

1. advances the clocks to `frame ÷ fps` and runs due callbacks (see Clocks);
2. asks the runtime for the state of that frame;
3. shows or hides every clip, by putting the attribute `fs-shown` on the clips that show and taking it off the others;
4. writes the animated values onto the elements.

The state of a frame depends only on the frame number. Seeking the same frame after any other frame gives the same state.

The problem report returned by `begin` and `seek` has the same `unsupported` and `failures` lists as the ready report, holding only what is new since the last report. A script error inside a timer or animation-frame callback is a failure of kind `script`.

## Capture

After `seek(frame)` returns with nothing to report, the host takes one screenshot of the viewport. That picture is frame `frame`.

## Runtime interface (loader to WebAssembly)

Internal to the two files that are built together; listed for completeness.

- The loader passes the runtime a snapshot of the page: every element that carries a `data-fs-` attribute (tag, id, attributes, enclosing such element), and the text of every motion block.
- The runtime returns the selectors it needs matched. The loader matches each inside the composition element and passes the matches back.
- The runtime returns the ready data (manifest and problems) and, for each clip, which element of the snapshot it is.
- When the run begins, the runtime is given the frame rate and returns the animated elements in the order their numbers will come.
- For a frame, the runtime returns a block of numbers: one visibility flag per clip, and for each animated element a set of flags and the five property values. The loader writes `x` and `y` as the CSS `translate` property, `scale` as `scale`, `rotation` as `rotate`, and `opacity` as the element's own opacity multiplied by the animated value.
