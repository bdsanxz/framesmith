# Preview

Status: v0 (Sprint 2); ticket T060 wrote this file: `framesmith preview <project>` in the foreground, the address it prints, the preview page with frame 0, the files it serves, and the network behaviour of the whole binary. Later preview tickets extend it. Sources: proposal `s02-timing-nesting-preview` (CAP-022, CAP-023 for the frame-0 half, CAP-037; journey steps J-05 and J-06; states STATE-01 and STATE-02; points P10 and P19); the sprint plan (`PLAN.md`, section 8: the crate `server` is the preview and Studio server; the in-page runtime is the same file for preview and render); project rule 10 (one runtime file, two hand-written JavaScript files, an in-page behaviour is tested in a real browser) and rule 5 (logging); the facts taken from the research on the concept source are that its preview listens on the loopback address only and runs the same in-page runtime and the same seek as its render. The wording, the address line, the page and the codes below are our own.

## What the command does

`framesmith preview <project>` shows a project in a browser. It reads the project, starts a small web server on this machine, prints the address of the server, and keeps running until it is stopped. The author opens the address in a browser of their own choosing; Framesmith starts no browser (point P10: opening one is a later option, `--open`).

Order of work:

1. **Words.** The command takes the path of one project folder. An option it does not have, a second path, or no path is refused as the other commands refuse them: FS-R004, exit code 1, nothing started. Every option of `preview`, and the help lines for it, are read and written in `crates/cli/src/preview.rs` and the folder `crates/cli/src/preview/`; no other file of the command changes when an option is added.
2. **The check.** The project is read by the same check as `check`, `render` and `info`. Until ticket T074 a project that does not pass the check is not previewed. The command prints the diagnostics and the result line exactly as `render` does (on standard error), starts nothing, and exits with the code of the check: 1 when anything is an error (a folder that does not exist, FS-E001, is one), 2 when the only blockers are unsupported or unknown uses. Nothing listens in this case. (Ticket T074 turns this into a preview that starts and lists the problems.) Warnings are printed on standard error and do not stop the preview.
3. **The server.** The server listens on the loopback address `127.0.0.1` and on no other address of the machine. The port is chosen by the system among the free ones (ticket T079 replaces this with the search from the default port). When listening is not possible the command stops with FS-P001, exit code 1.
4. **The address line.** Once the server answers, the command prints one line on standard output, and keeps running:

```text
Preview at http://127.0.0.1:41873/  main  1280x720  30 fps  5 s
```

The line is: the word `Preview at`, the address (always `http://127.0.0.1:<port>/`, with the port actually used), then the composition's name, size, rate and length, separated by two spaces, written as the other commands write them (`check` and `timeline`). Nothing else is printed on standard output. The line is complete and flushed before the command waits, so a program that reads it can open the address at once: the server already answers.
5. **Running.** The command waits. It starts no other program: no `ffmpeg` and no browser is looked for, started or needed, so the preview starts on a machine that has neither. Stopping (Ctrl-C, a terminate signal) is ticket T061; until then the process is stopped by the signal's default effect, which ends it and frees the port.

Exit codes follow `commands.md`; a foreground preview that was stopped normally exits 0 (ticket T061).

## The network, in the whole binary

Until Sprint 2 the binary held no network code. From ticket T060 `preview` listens on a loopback address and no other command does. The rule that replaces "the binary contains no network code" is a rule about behaviour, checked by running: with every use of the network denied by the operating system, `init`, `check` and `render` give the same results as with it allowed, the same exit codes, the same words, the same files and the same frames (the same holds for the commands added later that read and print, `info` and `timeline`). `preview` is the one command that needs the network of its own machine, and with the network denied it stops with FS-P001.

The preview does not reach another machine and no other machine reaches it: there is no option to change the address it listens on.

## What the server answers

The server speaks plain HTTP/1.1 on the one socket. It answers `GET` and nothing else (any other method: status 405, header `Allow: GET`). Each answer closes the connection. Every answer carries `Cache-Control: no-store`, so that a page shown after a change is never an older one, and `X-Content-Type-Options: nosniff`. A request that cannot be read (no request line, a target that does not begin with `/`, a head longer than 16 KiB) is answered with status 400 (or 431 for a long head). A connection that sends nothing for 5 seconds is closed.

| Path | Answer |
|---|---|
| `/` | The preview page (below). |
| `/index.html` | The project's entry page, prepared for the browser in the way a render prepares it: the loader script and the base style sheet are put first, straight after the document type (`composition.md`, "Layout and background"; `page-protocol.md`, "How the page is opened"). The file is read at each request. |
| `/__framesmith__/loader.js`, `/__framesmith__/runtime.wasm` | The two runtime files, the very bytes a render serves (`page-protocol.md`, "Parts"). |
| `/__framesmith__/preview.js` | The preview page's script. |
| any other path under `/__framesmith__/` | Status 404. The path is reserved. |
| any other path | The file of that name in the project folder, with a content type from its extension, when it is a file inside the project folder; otherwise status 404. |

The path of a request is read as the entry page's references are read (`model::classify`): the query and the fragment are dropped, percent escapes are decoded, `.` and empty parts are dropped, and a `..` that would climb out of the folder makes the request refused (status 404, no content). A file that is reached through a link of the file system is served only when the file the link leads to is also inside the project folder. The server never writes: it opens no file for writing and creates no file or folder in the project. Ticket T062 tests the containment in full.

An entry page or a preview page is served only for a project that passes the check at the time of the request. When it does not (it was changed after the preview started), the answer to `/` is a short page that says no picture is shown because the project no longer passes `framesmith check`, and the answer to `/index.html` is status 503. (Tickets T074 and T075 turn this into the list of problems.)

## The preview page

`/` answers a page made of one element for the picture and, beside it, nothing yet (the controls come with ticket T063). The page is written once, in Rust (`crates/server/src/page.rs`), with the composition's facts in attributes of `body`: `data-width`, `data-height`, `data-fps`, `data-frames` and `data-name`. Its script, `crates/server/web/preview.js`, is the second of the two hand-written JavaScript files of the project; it only does what the browser alone can do:

1. **Fit.** The composition is shown in a frame of its own (`iframe`, the address `/index.html`) at its declared size in CSS pixels, then scaled, from its top left corner, by the one factor `min(room width ÷ width, room height ÷ height)` where the room is the area the page leaves for the picture. The factor can be below 1 (a window smaller than the composition) or above 1 (a larger window); it is never different for the two directions, so the proportions are the declared ones. The scaled picture is centred in the room and nothing of it lies outside the room. The factor is worked out again each time the room changes size. The room is the whole window until the controls exist. The picture is clipped to the scaled composition, so the page's own background shows around it and never inside it.
2. **Frame 0.** The script waits for the composition's frame to define the loader's interface (`__framesmith`, `page-protocol.md`), waits for its `ready`, and then does what a render does before its first capture: `begin` with the composition's rate, and `seek(0)`. Only after `seek(0)` has returned with nothing to report is the picture made visible. Until then the frame is hidden, so the author never sees the composition before its first frame is set.
3. **Not a wrong picture.** The script shows no picture when the ready report says the page speaks another protocol version, did not replace the clocks, has no manifest, or lists any problem, unsupported use or failure, or when `begin` or `seek` returns a report of any. In its place the page says "The preview cannot show this project." and lists the reports, one line each, as `<code or kind>: <detail>`. (Tickets T077 and T078 give these the layout of the other problems.)
4. **State for readers other than the eye.** The script sets `data-state` on `body` to `loading`, then `shown` (frame 0 is on screen) or `refused` (item 3), sets `data-frame` to `0` when the frame is shown, and sets `data-scale` to the factor of item 1. A test, or a program that drives the page, waits for `data-state`.

The page makes no request to any other address than the preview's own. It carries a page icon of no content (`data:,`) so that the browser does not ask for one.

### Layout

The page has a dark grey background, the picture area fills the window, and the picture is centred in it. The notice of item 3 is a block of text on the same background, readable at every width from 480 to 2560 CSS pixels. The picture and the notice never overlap.

## Logging

The run log (`commands.md`, "The log") tells a preview in order under one run id: `command started` with the project, `preview listening` with the address, the project's name and the counts of the check, each failure to answer a request that is not a client's mistake, and a refusal at the start (`command finished` with the codes). Requests are logged at the debug level only (the path and the status). The log holds no file content.

## Diagnostic codes of the preview (family FS-P)

A code keeps its number and meaning for ever. The family is used by the preview tickets only.

| Code | Failure | Exit | Remedy |
|---|---|---|---|
| FS-P001 | The preview cannot listen on the loopback address of this machine: the system refused the socket, with its reason. | 1 | Check that this machine lets programs listen on 127.0.0.1 (a firewall, a sandbox or a denied network does not), then run the command again. |

A run failure of the preview is printed as the other run failures are (`commands.md`): the kind word, the code, the description, and `fix:` with the remedy.

## Not yet built (each waits for its ticket)

Stopping with Ctrl-C and the line `Preview stopped` (T061); the containment tests (T062); the controls, scrubbing and the readout of the frame (T063 onward); the notice for a browser that is not Chromium-based (T069); reload on save (T070 onward); problems in place of the picture (T074 onward); the search for a free port and `--port` (T079, T080); `--open`, `--composition`, `--background`, `--status`, `--stop`, `--list`, `--stop-all`, `--json` (later tickets). A word or option of these is refused by item 1 of "What the command does" until its ticket reads it.
