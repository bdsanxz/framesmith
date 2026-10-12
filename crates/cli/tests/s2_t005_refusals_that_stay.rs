//! Sprint 2, ticket T005: what Sprint 2 still defers is still refused, four names stay unknown, and
//! every diagnostic code of Sprint 1 keeps its number, its kind and its meaning.
//!
//! Notes for maintainers
//! ---------------------
//! * The tables in this file are the contract of the ticket. `STAYING` and `UNKNOWN` name, for each
//!   refusal that stays, the fixture folder and the text in the fixture that is at fault; the test
//!   works out the line number from that text, so a fixture that is edited without its table is
//!   caught. `LIFTED` lists the four refusals that Sprint 2 lifts, each with the ticket that lifts it.
//! * **A ticket that lifts one of the four** (T010 for FS-U003, T021 for FS-U004, T026 for FS-U005,
//!   T034 for FS-U006) removes the row from `LIFTED`, the row of the same code from `SPRINT1` and the
//!   row from `docs/spec/support-matrix.md`, and deletes the fixture folders. It never renumbers a
//!   code and never edits another row.
//! * **A refusal that is lifted in parts** (FS-U003: T010 lifted the plain form, T011 the offset, T012 chains,
//!   T013 a named clip with no written duration; T038 lifts the rest) keeps its row in `LIFTED`, with every ticket that still waits on it, and is
//!   listed in `NARROWED` with the meaning its row has now. Each ticket of the part narrows the row
//!   and drops its ticket from the list; the last one removes the row from `LIFTED`, `NARROWED` and
//!   `SPRINT1`, as above. The row's wording is still checked word for word: against `NARROWED`
//!   instead of `SPRINT1`.
//! * `SPRINT1` is the list of diagnostic codes as the Sprint 1 release gave them (the spec of commit
//!   4865055), copied here as text of our own. A code that is still raised has the same kind and the
//!   same sentence of meaning, and is still raised by the same input.
//! * Nothing here starts a real browser or `ffmpeg` for a refused project: stand-in programs note
//!   that they were started, as in `refuse.rs`.

mod common;

use common::*;
use measure::Ran;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------
// The tables
// ---------------------------------------------------------------------

/// A refusal that stays: the fixture folder under `fixtures/unsupported/`, and for each place the
/// code is to be listed at, the file and a piece of text that occurs on exactly one line of it.
struct Case {
    folder: &'static str,
    uses: &'static [(&'static str, &'static str)],
}

const fn case(folder: &'static str, uses: &'static [(&'static str, &'static str)]) -> Case {
    Case { folder, uses }
}

/// Every support-matrix entry that stays a refusal for the whole of Sprint 2, with all its fixtures.
const STAYING: &[(&str, &[Case])] = &[
    ("FS-U001", &[case("fs-u001-video-element", &[("index.html", "<video")])]),
    ("FS-U002", &[case("fs-u002-audio-element", &[("index.html", "<audio")])]),
    (
        "FS-U007",
        &[case("fs-u007-declared-variable", &[("index.html", "data-fs-var-title")])],
    ),
    ("FS-U008", &[case("fs-u008-animated-gif", &[("index.html", "blink.gif")])]),
    (
        "FS-U009",
        &[
            case("fs-u009-network-font-in-stylesheet", &[("style.css", "remote.woff2")]),
            case("fs-u009-network-image", &[("index.html", "https://example.invalid/picture.png")]),
            case(
                "fs-u009-network-image-in-stylesheet",
                &[("style.css", "//example.invalid/picture.png")],
            ),
            case("fs-u009-network-script", &[("index.html", "https://example.invalid/library.js")]),
            case("fs-u009-network-stylesheet", &[("index.html", "https://example.invalid/style.css")]),
        ],
    ),
    (
        "FS-U010",
        &[
            case("fs-u010-keyframes-tween", &[("index.html", "\"keyframes\"")]),
            case("fs-u010-path-tween", &[("index.html", "\"kind\": \"path\"")]),
        ],
    ),
    ("FS-U011", &[case("fs-u011-bounce-easing", &[("index.html", "out-bounce")])]),
    ("FS-U012", &[case("fs-u012-colour-property", &[("index.html", "background-color")])]),
    (
        "FS-U013",
        &[
            case("fs-u013-block-defaults", &[("index.html", "\"defaults\"")]),
            case("fs-u013-repeat-option", &[("index.html", "\"repeat\"")]),
            case("fs-u013-stagger-option", &[("index.html", "\"stagger\"")]),
        ],
    ),
    (
        "FS-U014",
        &[
            case("fs-u014-css-animation-in-style-attribute", &[("index.html", "animation-name")]),
            case(
                "fs-u014-css-animation-in-stylesheet",
                &[("style.css", "animation: slide"), ("style.css", "@keyframes")],
            ),
        ],
    ),
    (
        "FS-U015",
        &[
            case(
                "fs-u015-css-transition-in-style-attribute",
                &[("index.html", "transition-duration")],
            ),
            case("fs-u015-css-transition-in-stylesheet", &[("style.css", "transition: left")]),
        ],
    ),
    (
        "FS-U016",
        &[case("fs-u016-browser-animation-interface", &[("script.js", ".animate(")])],
    ),
    ("FS-U017", &[case("fs-u017-script-interface", &[("script.js", "framesmith.to")])]),
    ("FS-U018", &[case("fs-u018-embedded-document", &[("index.html", "<iframe")])]),
    ("FS-U019", &[case("fs-u019-svg-animation-element", &[("index.html", "<animate ")])]),
    ("FS-U020", &[case("fs-u020-pseudo-class-target", &[("index.html", ":first-child")])]),
    (
        "FS-U021",
        &[case("fs-u021-translate-on-motion-target", &[("index.html", "translate: 10px")])],
    ),
];

/// Every name in the unknown family, with all its fixtures. The first five are Sprint 1's; the four
/// that follow are the names that Sprint 2 deliberately does not accept (see `NOT_ACCEPTED`).
const UNKNOWN: &[(&str, &[Case])] = &[
    (
        "FS-X001",
        &[
            case("fs-x001-unknown-attribute", &[("index.html", "data-fs-strat")]),
            case("fs-x001-unknown-clip-end", &[("index.html", "data-fs-end")]),
            case("fs-x001-unknown-clip-layer", &[("index.html", "data-fs-layer")]),
            case("fs-x001-unknown-registry-opt-out", &[("index.html", "data-fs-no-timeline")]),
            case("fs-x001-unknown-thumbnail", &[("index.html", "data-fs-thumbnail")]),
        ],
    ),
    ("FS-X002", &[case("fs-x002-unknown-tween-field", &[("index.html", "\"eas\"")])]),
    ("FS-X003", &[case("fs-x003-unknown-kind", &[("index.html", "\"kind\": \"tween\"")])]),
    ("FS-X004", &[case("fs-x004-unknown-property", &[("index.html", "opacty")])]),
    ("FS-X005", &[case("fs-x005-unknown-easing", &[("index.html", "out-quadd")])]),
];

/// The refusals that Sprint 2 lifts, with the tickets that still wait on each. They are still raised
/// today (FS-U003 only for the forms that are not built); the last ticket removes the row.
const LIFTED: &[(&str, &[&str], &[&str])] = &[
    ("FS-U003", &["T038"], &["fs-u003-nested-composition-start"]),
    ("FS-U004", &["T021"], &["fs-u004-ratio-rate"]),
    ("FS-U005", &["T026"], &["fs-u005-other-page", "fs-u005-second-composition"]),
    ("FS-U006", &["T034"], &["fs-u006-nested-composition"]),
];

/// A lifted refusal that no input raises alone, and the codes that are listed beside it: a clip that
/// follows a nested composition is refused (FS-U003), and the nested composition itself is listed too
/// (FS-U006). Only these.
const COMPANIONS: &[(&str, &[&str])] = &[("FS-U003", &["FS-U006"])];

/// The refusals whose meaning a ticket has narrowed, with the meaning the row has now. The Sprint 1
/// meaning stays in `SPRINT1`; it is the text this one replaces.
const NARROWED: &[(&str, &str)] = &[(
    "FS-U003",
    "A clip start written relative to another clip in a form that is not built yet: naming a nested composition. The plain form `after:<clip id>`, the form with an offset (`after:<clip id>` followed by `+` or `-` and a time), chains of them, in which a clip names a clip whose own start is relative, and a start that names a clip with no written duration (a still image, or a clip that lasts to the end of the composition) are supported.",
)];

/// The four names that are deliberately not accepted: the attribute, whether it is written on a
/// clip or on the composition, the fixture folder, and the sprint the support matrix says it waits for.
const NOT_ACCEPTED: &[(&str, &str, &str)] = &[
    ("data-fs-end", "fs-x001-unknown-clip-end", "clip"),
    ("data-fs-layer", "fs-x001-unknown-clip-layer", "clip"),
    ("data-fs-no-timeline", "fs-x001-unknown-registry-opt-out", "composition"),
    ("data-fs-thumbnail", "fs-x001-unknown-thumbnail", "composition"),
];

/// Every diagnostic code of Sprint 1: code, kind, and the sentence of meaning, as the Sprint 1
/// release (commit 4865055) gave them.
const SPRINT1: &[(&str, &str, &str)] = &[
    ("FS-U001", "unsupported", "A `video` element."),
    ("FS-U002", "unsupported", "An `audio` element."),
    ("FS-U003", "unsupported", "A clip start written relative to another clip: `data-fs-start=\"after:<clip id>\"`, optionally followed by `+` or `-` and a time."),
    ("FS-U004", "unsupported", "A frame rate written as a ratio, such as `30000/1001`, on the composition or with `--fps`."),
    ("FS-U005", "unsupported", "A second composition in the project (in `index.html` or in another HTML file at the top of the project)."),
    ("FS-U006", "unsupported", "A composition nested inside another."),
    ("FS-U007", "unsupported", "A declared variable: any attribute starting `data-fs-var-`."),
    ("FS-U008", "unsupported", "An animated image: a GIF with more than one frame, an animated PNG or WebP, an AVIF sequence, or an SVG file that animates itself."),
    ("FS-U009", "unsupported", "An asset loaded from a network address (`http:`, `https:`, or starting `//`) by an image, stylesheet, script, font or CSS `url()`. Nothing is fetched."),
    ("FS-U010", "unsupported", "A tween kind that is not built: `keyframes`, `path`."),
    ("FS-U011", "unsupported", "An easing outside the ten: the in, out and in-out forms of `quint`, `sine`, `expo`, `circ`, `back`, `elastic`, `bounce`; `steps`; `spring`; `cubic-bezier`."),
    ("FS-U012", "unsupported", "A property outside the five: `scale-x`, `scale-y`, `skew-x`, `skew-y`, `rotation-x`, `rotation-y`, `z`, `width`, `height`, `color`, `background-color`, `border-radius`, `blur`, `x-percent`, `y-percent`."),
    ("FS-U013", "unsupported", "A tween option that is not built: `repeat`, `repeat-delay`, `alternate`, `stagger`, `delay`, `label`; and the block fields `labels` and `defaults`."),
    ("FS-U014", "unsupported", "A CSS animation: an `animation` or `animation-name` property other than `none`, or an `@keyframes` rule."),
    ("FS-U015", "unsupported", "A CSS transition: a `transition` property other than `none`, or a `transition-duration` other than zero."),
    ("FS-U016", "unsupported", "The browser's own animation interface used by a script: `.animate(`, `new Animation`, `KeyframeEffect`, `getAnimations`, `document.timeline`. Also any animation found running in the page during a render."),
    ("FS-U017", "unsupported", "The script interface for building motion: any use of the page global `framesmith`."),
    ("FS-U018", "unsupported", "An embedded document: `iframe`, `embed`, `object`."),
    ("FS-U019", "unsupported", "An animation element inside inline SVG: `animate`, `animateTransform`, `animateMotion`, `set`."),
    ("FS-U020", "unsupported", "A target selector form that `check` cannot read (pseudo-classes, sibling combinators and others outside `motion-block.md`)."),
    ("FS-U021", "unsupported", "`translate`, `rotate` or `scale` set by the author on an element that motion targets."),
    ("FS-X001", "unknown", "An attribute starting `data-fs-` that is not in `composition.md` or in the table above."),
    ("FS-X002", "unknown", "A field in a motion block or in a tween that is not in `motion-block.md` or in the table above."),
    ("FS-X003", "unknown", "A tween `kind` that is not recognised."),
    ("FS-X004", "unknown", "A property name that is not recognised."),
    ("FS-X005", "unknown", "An easing name that is not recognised."),
    ("FS-E001", "error", "The project path does not exist or is not a folder."),
    ("FS-E002", "error", "The project has no `index.html`."),
    ("FS-E003", "error", "No composition in `index.html`."),
    ("FS-E004", "error", "The composition name is empty or has characters outside letters, digits, `-`, `_`."),
    ("FS-E005", "error", "The size is missing."),
    ("FS-E006", "error", "The size is not two whole numbers in range."),
    ("FS-E007", "error", "The width or the height is odd."),
    ("FS-E008", "error", "The frame rate is not a whole number from 1 to 240."),
    ("FS-E009", "error", "The length is not a time."),
    ("FS-E010", "error", "The written length is zero or less."),
    ("FS-E011", "error", "No length is written, and there is no motion and no clip with its own end to work it out from."),
    ("FS-E020", "error", "A clip has no start."),
    ("FS-E021", "error", "A clip start is not a time."),
    ("FS-E022", "error", "A clip start is negative."),
    ("FS-E023", "error", "A clip duration is not a time."),
    ("FS-E024", "error", "A written clip duration is zero or less."),
    ("FS-E025", "error", "A clip has no duration, and the composition has no length for it to last until: none is written and none can be worked out."),
    ("FS-E026", "error", "A clip attribute on an element that is not a clip."),
    ("FS-E027", "error", "A clip outside the composition."),
    ("FS-E028", "error", "A track that is not a whole number."),
    ("FS-E030", "error", "A referenced local file does not exist."),
    ("FS-E031", "error", "A reference points outside the project folder, or into the reserved path `/__framesmith__/`."),
    ("FS-E040", "error", "A motion block is not valid JSON."),
    ("FS-E041", "error", "A motion block or a tween has the wrong shape."),
    ("FS-E042", "error", "A tween lacks a required field."),
    ("FS-E043", "error", "A tween has a negative start or duration."),
    ("FS-E044", "error", "A tween target matches no element in the composition."),
    ("FS-E045", "error", "A tween value is not a number, or is out of range."),
    ("FS-E046", "error", "A motion block names a composition that is not this one."),
    ("FS-E047", "error", "A tween's fields disagree about its kind."),
    ("FS-E048", "error", "A tween target is not a valid selector."),
    ("FS-W001", "warning", "A clip starts at or after the end of the composition and can never show."),
    ("FS-R001", "run failure", "No Chromium-based browser was found."),
    ("FS-R002", "run failure", "`FRAMESMITH_BROWSER` points at something that does not exist or cannot be run."),
    ("FS-R003", "run failure", "`ffmpeg` is not on the search path."),
    ("FS-R004", "run failure", "A command-line value is not acceptable."),
    ("FS-R005", "run failure", "The output location cannot be written."),
    ("FS-R006", "run failure", "An image, a font or another file failed to load in the page."),
    ("FS-R007", "run failure", "A page script threw an error."),
    ("FS-R008", "run failure", "The page did not become ready within 30 seconds."),
    ("FS-R009", "run failure", "The page clocks could not be replaced."),
    ("FS-R010", "run failure", "The page disagrees with the files."),
    ("FS-R011", "run failure", "The browser stopped or reported an error during the run."),
    ("FS-R012", "run failure", "`ffmpeg` stopped or reported an error during the run."),
    ("FS-R013", "run failure", "The page and Framesmith speak different protocol versions."),
    ("FS-R014", "run failure", "`init`: the folder exists and is not empty."),
    ("FS-R015", "run failure", "`init`: the folder cannot be created."),
    ("FS-R016", "run failure", "An internal error."),
];

// ---------------------------------------------------------------------
// Reading the spec
// ---------------------------------------------------------------------

fn matrix() -> String {
    spec("support-matrix.md")
}

/// The lines of the part of `text` that starts at the heading line `heading` and runs to the next
/// heading of the same or a higher level.
fn part(text: &str, heading: &str) -> Vec<String> {
    let level = heading.chars().take_while(|c| *c == '#').count();
    let mut lines = text.lines();
    assert!(
        lines.by_ref().any(|line| line.trim_end() == heading),
        "the heading {heading:?} is not in the support matrix"
    );
    let mut found = Vec::new();
    for line in lines {
        let deeper = line.chars().take_while(|c| *c == '#').count();
        if line.starts_with('#') && deeper <= level {
            break;
        }
        found.push(line.to_string());
    }
    found
}

/// The cells of every table row in `lines` whose first cell, without backticks, starts with `start`.
fn rows(lines: &[String], start: &str) -> Vec<Vec<String>> {
    lines
        .iter()
        .filter(|line| line.starts_with('|'))
        .map(|line| {
            line.trim()
                .trim_matches('|')
                .split('|')
                .map(|cell| cell.trim().to_string())
                .collect::<Vec<_>>()
        })
        .filter(|cells| cells[0].trim_matches('`').starts_with(start))
        .collect()
}

/// What the spec says each `FS-U` row waits for, by code, in the order written.
fn spec_unsupported() -> Vec<(String, String, String)> {
    let text = matrix();
    rows(&part(&text, "## Unsupported: recognised and refused"), "FS-U")
        .into_iter()
        .map(|cells| {
            assert_eq!(cells.len(), 4, "a row of the table of refusals has four cells: {cells:?}");
            (cells[0].clone(), cells[1].clone(), cells[3].clone())
        })
        .collect()
}

fn spec_unknown_codes() -> BTreeSet<String> {
    let text = matrix();
    rows(&part(&text, "## Unknown: never ignored"), "FS-X")
        .into_iter()
        .map(|cells| cells[0].clone())
        .collect()
}

/// The names not accepted as the spec lists them: name, reported as, waits for.
fn spec_names() -> Vec<(String, String, String)> {
    let text = matrix();
    rows(&part(&text, "### Names that are deliberately not accepted"), "data-fs-")
        .into_iter()
        .map(|cells| {
            assert_eq!(cells.len(), 5, "a row of the table of names has five cells: {cells:?}");
            (cells[0].trim_matches('`').to_string(), cells[3].clone(), cells[4].clone())
        })
        .collect()
}

/// Every table row of the spec that starts with a diagnostic code: code to its second cell.
fn spec_meanings() -> BTreeMap<String, String> {
    let text = matrix();
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut found = BTreeMap::new();
    for cells in rows(&lines, "FS-") {
        found.insert(cells[0].clone(), cells[1].clone());
    }
    found
}

// ---------------------------------------------------------------------
// Running the product
// ---------------------------------------------------------------------

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/unsupported")
}

fn folders() -> BTreeSet<String> {
    std::fs::read_dir(fixtures_root())
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
}

/// The code a fixture folder stands for: `fs-u001-video-element` is `FS-U001`.
fn code_of(folder: &str) -> String {
    let mut parts = folder.splitn(3, '-');
    format!("{}-{}", parts.next().unwrap(), parts.next().unwrap()).to_ascii_uppercase()
}

/// One printed diagnostic header: kind word, code, and the place (file, with a line when there is one).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Listed {
    kind: String,
    code: String,
    file: String,
    line: Option<u32>,
}

/// The headers (`kind CODE file[:line]`) in what a command printed.
fn listed(output: &str) -> Vec<Listed> {
    output
        .lines()
        .filter(|line| !line.starts_with(' '))
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let (kind, code) = (words.next()?, words.next()?);
            if !matches!(kind, "error" | "unsupported" | "unknown" | "warning") {
                return None;
            }
            if !code.starts_with("FS-") || code.len() != 7 {
                return None;
            }
            let place = words.next().unwrap_or("");
            let (file, line) = match place.rsplit_once(':') {
                Some((file, digits)) if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) => {
                    (file, Some(digits.parse().unwrap()))
                }
                _ => (place, None),
            };
            Some(Listed {
                kind: kind.to_string(),
                code: code.to_string(),
                file: file.to_string(),
                line,
            })
        })
        .collect()
}

/// Stand-ins for the browser and for `ffmpeg` that note being started.
struct Spies {
    dir: PathBuf,
    browser: PathBuf,
    browser_marker: PathBuf,
    ffmpeg_marker: PathBuf,
}

impl Spies {
    fn new(key: &str) -> Spies {
        let dir = scratch(&format!("{key}_spies"));
        let (browser, browser_marker) = spy(&dir, "browser", &common::browser());
        let (_, ffmpeg_marker) = spy(&dir, "ffmpeg", &common::ffmpeg());
        Spies {
            dir,
            browser,
            browser_marker,
            ffmpeg_marker,
        }
    }

    fn run(&self, cwd: &Path, args: &[&str]) -> Ran {
        let path: OsString = format!("{}:{}", self.dir.display(), std::env::var("PATH").unwrap_or_default()).into();
        run_in(
            cwd,
            args,
            &[("FRAMESMITH_BROWSER", self.browser.as_os_str()), ("PATH", path.as_os_str())],
        )
    }

    fn started(&self) -> (bool, bool) {
        (self.browser_marker.exists(), self.ffmpeg_marker.exists())
    }
}

/// What `check` and `render` did with one refused project.
struct Refused {
    check: Ran,
    listed: Vec<Listed>,
}

/// Runs `check`, then `render` (default output), `render --output` to a place of our choosing and
/// `render --json`, with the stand-ins in place, and asserts what holds for every refused project:
/// exit 2 from all, the same list from `check` and `render`, no browser, no `ffmpeg`, no output
/// file or folder anywhere, and the project untouched.
fn refused(key: &str, dir: PathBuf) -> Refused {
    let spies = Spies::new(key);
    let before = tree(&dir);
    let cwd = dir.parent().unwrap();
    let check = run_in(cwd, &["check", text(&dir)], &[]);
    let render = spies.run(cwd, &["render", text(&dir)]);
    let chosen = cwd.join("chosen").join("video.mp4");
    let render_to = spies.run(cwd, &["render", text(&dir), "--output", text(&chosen)]);
    let render_json = spies.run(cwd, &["render", text(&dir), "--json"]);

    assert_eq!(check.code, 2, "{key}: check: {}", check.all());
    assert_eq!(render.code, 2, "{key}: render: {}", render.all());
    assert_eq!(render_to.code, 2, "{key}: render --output: {}", render_to.all());
    assert_eq!(render_json.code, 2, "{key}: render --json: {}", render_json.all());
    assert_eq!(render.stderr, check.stdout, "{key}: render must print the list that check printed");
    assert_eq!(render_to.stderr, check.stdout, "{key}");
    assert_eq!(spies.started(), (false, false), "{key}: a browser or ffmpeg was started");
    assert!(!dir.join("out").exists(), "{key}: the default output folder was created");
    assert!(
        !chosen.exists() && !chosen.parent().unwrap().exists(),
        "{key}: something appeared at the chosen output path"
    );
    assert_eq!(before, tree(&dir), "{key}: the project changed");

    // The structured form lists the same places, with kinds and lines.
    let doc = document(&render_json);
    assert_eq!(doc["outcome"], "refused", "{key}: {doc}");
    let mut from_json: Vec<Listed> = doc["diagnostics"]
        .as_array()
        .unwrap_or_else(|| panic!("{key}: no diagnostics in {doc}"))
        .iter()
        .map(|item| Listed {
            kind: item["kind"].as_str().unwrap().to_string(),
            code: item["code"].as_str().unwrap().to_string(),
            file: item["file"].as_str().unwrap().to_string(),
            line: Some(item["line"].as_u64().unwrap() as u32),
        })
        .collect();
    let mut from_text = listed(&check.stdout);
    from_json.sort();
    from_text.sort();
    assert_eq!(from_json, from_text, "{key}: the JSON list and the printed list differ");
    Refused { check, listed: from_text }
}

/// The line numbers (from 1) of the lines of a fixture file that hold `needle`.
fn lines_with(folder: &str, file: &str, needle: &str) -> Vec<u32> {
    let path = fixtures_root().join(folder).join(file);
    let content = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    content
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains(needle))
        .map(|(index, _)| index as u32 + 1)
        .collect()
}

/// Refuses the fixture and asserts that `check` and `render` list exactly its code, at exactly the
/// places given by the table, with the kind of the code, and nothing else.
fn refused_fixture(test: &str, code: &str, kind: &str, one: &Case) -> Refused {
    let folder = one.folder;
    assert_eq!(code_of(folder), code, "the folder {folder} is filed under the wrong code");
    let result = refused(
        &format!("{test}_{folder}"),
        fixture(&format!("{test}_{folder}"), &format!("unsupported/{folder}")),
    );
    let mut expected: Vec<(String, u32)> = one
        .uses
        .iter()
        .map(|(file, needle)| {
            let hits = lines_with(folder, file, needle);
            assert_eq!(
                hits.len(),
                1,
                "{folder}/{file}: {needle:?} must be on exactly one line, found {hits:?}"
            );
            (file.to_string(), hits[0])
        })
        .collect();
    expected.sort();
    let mut found: Vec<(String, u32)> = Vec::new();
    for item in &result.listed {
        assert_eq!(
            (item.kind.as_str(), item.code.as_str()),
            (kind, code),
            "{folder}: the fixture must trip only its own refusal, but lists {item:?}"
        );
        found.push((
            item.file.clone(),
            item.line.unwrap_or_else(|| panic!("{folder}: {item:?} has no line")),
        ));
    }
    found.sort();
    assert_eq!(found, expected, "{folder}: wrong place");
    assert!(
        result.check.stdout.contains("fix: "),
        "{folder}: every diagnostic carries a remedy:\n{}",
        result.check.stdout
    );
    result
}

fn kind_of(code: &str) -> &'static str {
    match code.as_bytes()[3] {
        b'U' => "unsupported",
        b'X' => "unknown",
        b'W' => "warning",
        _ => "error",
    }
}

// ---------------------------------------------------------------------
// Criterion 1: every refusal that stays has a fixture that is listed and refused
// ---------------------------------------------------------------------

#[test]
fn s2_t005_c1_every_refusal_that_stays_is_listed_with_its_line_and_refused_without_starting_anything() {
    let mut count = 0;
    for (code, cases) in STAYING {
        assert!(!cases.is_empty(), "{code} has no fixture");
        for one in *cases {
            refused_fixture("s2_t005_c1", code, "unsupported", one);
            count += 1;
        }
    }
    assert!(count >= STAYING.len());
}

#[test]
fn s2_t005_c1_every_unknown_name_fixture_is_listed_with_its_line_and_refused_without_starting_anything() {
    for (code, cases) in UNKNOWN {
        for one in *cases {
            refused_fixture("s2_t005_c1_unknown", code, "unknown", one);
        }
    }
}

#[test]
fn s2_t005_c1_the_four_lifted_refusals_are_still_refused_until_their_tickets_lift_them() {
    for (code, tickets, folders_of) in LIFTED {
        for folder in *folders_of {
            if !fixtures_root().join(folder).is_dir() {
                continue; // the ticket has removed it
            }
            let result = refused(
                &format!("s2_t005_c1_lifted_{folder}"),
                fixture(&format!("s2_t005_c1_lifted_{folder}"), &format!("unsupported/{folder}")),
            );
            let beside: &[&str] = COMPANIONS.iter().find(|row| row.0 == *code).map_or(&[], |row| row.1);
            assert!(
                result
                    .listed
                    .iter()
                    .all(|item| (item.code == *code || beside.contains(&item.code.as_str())) && item.kind == "unsupported")
                    && result.listed.iter().any(|item| item.code == *code),
                "{folder} ({tickets:?}): {:?}",
                result.listed
            );
        }
    }
}

#[test]
fn s2_t005_c1_the_stand_ins_do_see_a_browser_and_ffmpeg_when_a_project_renders() {
    // The control for "starts no browser and no ffmpeg": a valid project does start both.
    let spies = Spies::new("s2_t005_c1_control");
    let dir = project("s2_t005_c1_control_project", "ok", &page(OK, "", "<p>x</p>", ""));
    let ran = spies.run(dir.parent().unwrap(), &["render", text(&dir)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(spies.started(), (true, true));
}

#[test]
fn s2_t005_c1_the_codes_of_the_test_tables_and_of_the_spec_are_the_same() {
    let lifted: BTreeSet<&str> = LIFTED.iter().map(|row| row.0).collect();
    let in_spec = spec_unsupported();
    let spec_codes: BTreeSet<String> = in_spec.iter().map(|row| row.0.clone()).collect();
    assert_eq!(spec_codes.len(), in_spec.len(), "a code is listed twice in the spec: {in_spec:?}");

    // Rows the spec marks as lifted this sprint are exactly the rows of LIFTED that are still there.
    let spec_lifted: BTreeSet<&str> = in_spec
        .iter()
        .filter(|row| row.2.starts_with("This sprint"))
        .map(|row| row.0.as_str())
        .collect();
    assert!(
        spec_lifted.is_subset(&lifted),
        "the spec marks {spec_lifted:?} as lifted this sprint, but this test knows only {lifted:?}: add the ticket to LIFTED, or the code to STAYING"
    );
    for row in LIFTED {
        let waits = in_spec.iter().find(|r| r.0 == row.0).map(|r| r.2.as_str());
        for ticket in row.1 {
            assert!(
                waits.is_none_or(|waits| waits.contains(ticket)),
                "{} should say it is lifted by {ticket}, but the spec says {waits:?}",
                row.0
            );
        }
    }

    // The rows that stay are the same in both lists: a code gained or lost on either side fails here.
    let spec_stays: BTreeSet<String> = in_spec
        .iter()
        .filter(|row| !row.2.starts_with("This sprint"))
        .map(|row| row.0.clone())
        .collect();
    let test_stays: BTreeSet<String> = STAYING.iter().map(|row| row.0.to_string()).collect();
    assert_eq!(
        spec_stays, test_stays,
        "the support matrix and the table STAYING of this test list different codes that stay refused"
    );

    // The unknown family is the same in both.
    let test_unknown: BTreeSet<String> = UNKNOWN.iter().map(|row| row.0.to_string()).collect();
    assert_eq!(spec_unknown_codes(), test_unknown);

    // The four names are the same in both.
    let spec_named: BTreeSet<String> = spec_names().into_iter().map(|row| row.0).collect();
    let test_named: BTreeSet<String> = NOT_ACCEPTED.iter().map(|row| row.0.to_string()).collect();
    assert_eq!(spec_named, test_named);
}

#[test]
fn s2_t005_c1_every_fixture_folder_is_covered_by_a_table_and_every_table_folder_exists() {
    let mut covered: BTreeSet<String> = BTreeSet::new();
    for (code, cases) in STAYING.iter().chain(UNKNOWN) {
        for one in *cases {
            assert!(
                fixtures_root().join(one.folder).is_dir(),
                "{} is in a table but is not a folder",
                one.folder
            );
            assert_eq!(&code_of(one.folder), code, "{}", one.folder);
            assert!(covered.insert(one.folder.to_string()), "{} is in the tables twice", one.folder);
        }
    }
    for (_, _, folders_of) in LIFTED {
        for folder in *folders_of {
            assert!(covered.insert(folder.to_string()), "{folder} is in the tables twice");
        }
    }
    let on_disk = folders();
    let uncovered: Vec<&String> = on_disk.iter().filter(|name| !covered.contains(*name)).collect();
    assert!(
        uncovered.is_empty(),
        "fixture folders that no table of this test covers: {uncovered:?}"
    );
    // A folder of a lifted refusal may be gone (its ticket removed it); any other missing folder was caught above.
}

// ---------------------------------------------------------------------
// Criteria 2 and 3: four names that are not accepted
// ---------------------------------------------------------------------

fn name_refused(name: &str, folder: &str, on: &str) {
    let test = format!("s2_t005_names_{folder}");
    let result = refused(&test, fixture(&test, &format!("unsupported/{folder}")));
    assert_eq!(result.listed.len(), 1, "{name}: {:?}", result.listed);
    let item = &result.listed[0];
    assert_eq!((item.kind.as_str(), item.code.as_str()), ("unknown", "FS-X001"), "{name}");
    assert_eq!(item.file, "index.html", "{name}");
    // The line is the line that writes the attribute, on the element the table says it belongs to.
    let hits = lines_with(folder, "index.html", name);
    assert_eq!(hits.len(), 1, "{name}: {hits:?}");
    assert_eq!(item.line, Some(hits[0]), "{name}: wrong line");
    let source = std::fs::read_to_string(fixtures_root().join(folder).join("index.html")).unwrap();
    let written = source.lines().nth(hits[0] as usize - 1).unwrap();
    let element = if on == "clip" { "data-fs-clip" } else { "data-fs-composition" };
    assert!(written.contains(element), "{name} must be written on the {on}: {written}");
    // The message names the attribute and says it is not one that Framesmith knows.
    let said = &result.check.stdout;
    assert!(
        said.contains(&format!("The attribute {name} is not one Framesmith knows.")),
        "{said}"
    );
    assert!(
        said.contains("check refused: 0 errors, 0 unsupported, 1 unknown, 0 warnings"),
        "{said}"
    );
    // The fixture is otherwise a good project: take the attribute away and `check` passes.
    let clean = fixture(&format!("{test}_clean"), &format!("unsupported/{folder}"));
    let edited = std::fs::read_to_string(clean.join("index.html"))
        .unwrap()
        .replace(&format!(" {name}=\"{}\"", value_of(&source, name)), "")
        .replace(&format!(" {name}"), "");
    write(&clean, "index.html", edited);
    let ran = run_in(clean.parent().unwrap(), &["check", text(&clean)], &[]);
    assert_eq!(ran.code, 0, "{name}: the fixture holds another fault: {}", ran.all());
}

/// The value written for an attribute in a page, or an empty string when it has none.
fn value_of(source: &str, name: &str) -> String {
    let start = format!("{name}=\"");
    match source.find(&start) {
        Some(at) => {
            let rest = &source[at + start.len()..];
            rest[..rest.find('"').unwrap()].to_string()
        }
        None => String::new(),
    }
}

#[test]
fn s2_t005_c2_an_end_time_and_a_layer_on_a_clip_are_reported_as_unknown_and_render_exits_2() {
    let clip: Vec<_> = NOT_ACCEPTED.iter().filter(|row| row.2 == "clip").collect();
    assert_eq!(clip.len(), 2);
    for (name, folder, on) in clip {
        name_refused(name, folder, on);
    }
}

#[test]
fn s2_t005_c3_the_timeline_registry_opt_out_and_a_thumbnail_on_a_composition_are_reported_as_unknown_and_render_exits_2() {
    let composition: Vec<_> = NOT_ACCEPTED.iter().filter(|row| row.2 == "composition").collect();
    assert_eq!(composition.len(), 2);
    for (name, folder, on) in composition {
        name_refused(name, folder, on);
    }
}

// ---------------------------------------------------------------------
// Criterion 4: every code of Sprint 1 that is still raised keeps its number, kind and meaning
// ---------------------------------------------------------------------

const OK: &str = "data-fs-size=\"64x36\" data-fs-fps=\"10\" data-fs-duration=\"1\"";

fn small(test: &str, attrs: &str, body: &str, rest: &str) -> PathBuf {
    project(test, "p", &page(attrs, "", body, rest))
}

fn block(inside: &str) -> String {
    format!("<script type=\"application/framesmith-motion+json\">\n{inside}\n</script>")
}

/// A tween on `#a` with a field changed: the body of one project of the motion codes.
fn tween(fields: &str) -> String {
    block(&format!("[ {{ {fields} }} ]"))
}

/// A project whose `check` lists the Sprint 1 code `code`, for the codes of the check stage that are
/// not already raised by a fixture folder.
fn project_for(code: &str) -> Option<PathBuf> {
    let key = format!("s2_t005_c4_{code}");
    let a = "<p id=\"a\">a</p>";
    let good = "\"target\": \"#a\", \"to\": { \"x\": 1 }, \"at\": 0, \"duration\": 1";
    Some(match code {
        "FS-E001" => scratch(&key).join("nothing-here"),
        "FS-E002" => {
            let dir = scratch(&key).join("p");
            write(&dir, "readme.txt", "no page");
            dir
        }
        "FS-E003" => project(
            &key,
            "p",
            "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>t</title></head><body><div class=\"x\"></div></body></html>\n",
        ),
        "FS-E004" => project(
            &key,
            "p",
            &page(OK, "", "", "").replace("data-fs-composition=\"main\"", "data-fs-composition=\"a b\""),
        ),
        "FS-E005" => small(&key, "data-fs-duration=\"1\"", "", ""),
        "FS-E006" => small(&key, "data-fs-size=\"abc\" data-fs-duration=\"1\"", "", ""),
        "FS-E007" => small(&key, "data-fs-size=\"65x36\" data-fs-duration=\"1\"", "", ""),
        "FS-E008" => small(&key, "data-fs-size=\"64x36\" data-fs-fps=\"fast\" data-fs-duration=\"1\"", "", ""),
        "FS-E009" => small(&key, "data-fs-size=\"64x36\" data-fs-duration=\"soon\"", "", ""),
        "FS-E010" => small(&key, "data-fs-size=\"64x36\" data-fs-duration=\"0\"", "", ""),
        "FS-E011" => small(&key, "data-fs-size=\"64x36\"", "<p>x</p>", ""),
        "FS-E020" => small(&key, OK, "<p data-fs-clip data-fs-duration=\"1\">a</p>", ""),
        "FS-E021" => small(&key, OK, "<p data-fs-clip data-fs-start=\"soon\">a</p>", ""),
        "FS-E022" => small(&key, OK, "<p data-fs-clip data-fs-start=\"-1\">a</p>", ""),
        "FS-E023" => small(&key, OK, "<p data-fs-clip data-fs-start=\"0\" data-fs-duration=\"soon\">a</p>", ""),
        "FS-E024" => small(&key, OK, "<p data-fs-clip data-fs-start=\"0\" data-fs-duration=\"0\">a</p>", ""),
        "FS-E025" => small(&key, "data-fs-size=\"64x36\"", "<p data-fs-clip data-fs-start=\"1\">a</p>", ""),
        "FS-E026" => small(&key, OK, "<p data-fs-start=\"1\">a</p>", ""),
        "FS-E027" => small(&key, OK, "", "<p data-fs-clip data-fs-start=\"1\">a</p>"),
        "FS-E028" => small(&key, OK, "<p data-fs-clip data-fs-start=\"0\" data-fs-track=\"x\">a</p>", ""),
        "FS-E030" => small(&key, OK, "<img src=\"missing.png\" alt=\"\">", ""),
        "FS-E031" => small(&key, OK, "<img src=\"../outside.png\" alt=\"\">", ""),
        "FS-E040" => small(&key, OK, a, &block("{ not json")),
        "FS-E041" => small(&key, OK, a, &block("5")),
        "FS-E042" => small(&key, OK, a, &tween("\"to\": { \"x\": 1 }, \"at\": 0, \"duration\": 1")),
        "FS-E043" => small(
            &key,
            OK,
            a,
            &tween("\"target\": \"#a\", \"to\": { \"x\": 1 }, \"at\": -1, \"duration\": 1"),
        ),
        "FS-E044" => small(
            &key,
            OK,
            a,
            &tween("\"target\": \"#nothing\", \"to\": { \"x\": 1 }, \"at\": 0, \"duration\": 1"),
        ),
        "FS-E045" => small(
            &key,
            OK,
            a,
            &tween("\"target\": \"#a\", \"to\": { \"x\": \"far\" }, \"at\": 0, \"duration\": 1"),
        ),
        "FS-E046" => small(
            &key,
            OK,
            a,
            &block(&format!("{{ \"composition\": \"other\", \"tweens\": [ {{ {good} }} ] }}")),
        ),
        "FS-E047" => small(
            &key,
            OK,
            a,
            &tween("\"target\": \"#a\", \"kind\": \"set\", \"to\": { \"x\": 1 }, \"from\": { \"x\": 0 }, \"at\": 0, \"duration\": 1"),
        ),
        "FS-E048" => small(
            &key,
            OK,
            a,
            &tween("\"target\": \"#\", \"to\": { \"x\": 1 }, \"at\": 0, \"duration\": 1"),
        ),
        "FS-W001" => small(&key, OK, "<p data-fs-clip data-fs-start=\"5\" data-fs-duration=\"1\">late</p>", ""),
        _ => return None,
    })
}

/// The first fixture folder of a code, from the tables.
fn first_fixture(code: &str) -> Option<&'static str> {
    STAYING
        .iter()
        .chain(UNKNOWN)
        .find(|row| row.0 == code)
        .map(|row| row.1[0].folder)
        .or_else(|| LIFTED.iter().find(|row| row.0 == code).map(|row| row.2[0]))
}

/// What the product prints when the Sprint 1 code is raised by its input: the kind word and the code.
/// `None` when no cheap input raises it (the run failures that need a browser to go wrong).
fn raised(code: &str) -> Option<Vec<Listed>> {
    let key = format!("s2_t005_c4_{code}");
    if let Some(folder) = first_fixture(code) {
        let dir = fixture(&key, &format!("unsupported/{folder}"));
        return Some(listed(&run_in(dir.parent().unwrap(), &["check", text(&dir)], &[]).stdout));
    }
    if let Some(dir) = project_for(code) {
        return Some(listed(&run_in(dir.parent().unwrap(), &["check", text(&dir)], &[]).stdout));
    }
    // The run failures that are raised before a browser is needed.
    let dir = small(&format!("{key}_good"), OK, "<p>x</p>", "");
    let cwd = dir.parent().unwrap();
    let empty = scratch(&format!("{key}_empty"));
    let missing = cwd.join("no-such-browser");
    let named = common::browser();
    let ran = match code {
        "FS-R002" => run_in(cwd, &["render", text(&dir)], &[("FRAMESMITH_BROWSER", missing.as_os_str())]),
        "FS-R003" => run_in(
            cwd,
            &["render", text(&dir)],
            &[("FRAMESMITH_BROWSER", named.as_os_str()), ("PATH", empty.as_os_str())],
        ),
        "FS-R004" => run_in(cwd, &["render", text(&dir), "--fps", "0"], &[]),
        "FS-R005" => run_in(cwd, &["render", text(&dir), "--output", "/dev/null/x.mp4"], &[]),
        "FS-R014" => {
            let busy = cwd.join("busy");
            write(&busy, "mine.txt", "mine");
            run_in(cwd, &["init", text(&busy)], &[])
        }
        "FS-R015" => {
            write(cwd, "a-file", "not a folder");
            run_in(cwd, &["init", text(&cwd.join("a-file/inside"))], &[])
        }
        _ => return None,
    };
    Some(listed(&ran.stderr))
}

/// Every product source file's text, for the registry of codes: the codes are written out in the
/// crates' sources, there is no other list.
fn product_sources() -> String {
    fn walk(dir: &Path, into: &mut String) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, into);
            } else if path.extension().is_some_and(|e| e == "rs") {
                into.push_str(&std::fs::read_to_string(&path).unwrap());
            }
        }
    }
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut all = String::new();
    for entry in std::fs::read_dir(crates).unwrap() {
        let src = entry.unwrap().path().join("src");
        if src.is_dir() {
            walk(&src, &mut all);
        }
    }
    all
}

#[test]
fn s2_t005_c4_the_sprint1_table_is_well_formed() {
    let mut seen = BTreeSet::new();
    for (code, kind, meaning) in SPRINT1 {
        assert!(seen.insert(*code), "{code} is in the table twice");
        let expected = match code.as_bytes()[3] {
            b'R' => "run failure",
            _ => kind_of(code),
        };
        assert_eq!(*kind, expected, "{code}");
        assert!(meaning.ends_with('.'), "{code}: {meaning}");
    }
    // Every lifted code is one of the Sprint 1 codes, and so is every narrowed one; a narrowed
    // meaning is a sentence of its own, different from the one it replaces.
    for (code, _, _) in LIFTED {
        assert!(seen.contains(code), "{code}");
    }
    for (code, meaning) in NARROWED {
        let original = SPRINT1.iter().find(|row| row.0 == *code).unwrap_or_else(|| panic!("{code}"));
        assert!(meaning.ends_with('.'), "{code}: {meaning}");
        assert_ne!(*meaning, original.2, "{code} is listed as narrowed but its meaning is the old one");
        assert!(
            LIFTED.iter().any(|row| row.0 == *code),
            "{code} is narrowed but not lifted in parts"
        );
    }
}

#[test]
fn s2_t005_c4_every_sprint1_code_that_is_still_in_the_spec_has_the_same_meaning_there() {
    let now = spec_meanings();
    let mut missing = Vec::new();
    for (code, _, meaning) in SPRINT1 {
        // A meaning that a ticket narrowed is the narrowed one, word for word.
        let expected = NARROWED.iter().find(|row| row.0 == *code).map_or(*meaning, |row| row.1);
        match now.get(*code) {
            None => missing.push(*code),
            Some(today) => assert_eq!(
                today, expected,
                "{code}: the meaning in the support matrix is not the one this test expects"
            ),
        }
    }
    // A code may be missing from the spec only when the ticket that lifted it also removed its row
    // here: so a missing code is a failure.
    assert!(
        missing.is_empty(),
        "codes of Sprint 1 that the support matrix no longer lists: {missing:?}"
    );
}

#[test]
fn s2_t005_c4_every_sprint1_code_is_still_written_in_the_product() {
    let sources = product_sources();
    let mut gone = Vec::new();
    for (code, _, _) in SPRINT1 {
        if !sources.contains(&format!("\"{code}\"")) {
            gone.push(*code);
        }
    }
    assert!(
        gone.is_empty(),
        "codes of Sprint 1 that no source of the product raises any more: {gone:?}"
    );
}

#[test]
fn s2_t005_c4_every_sprint1_code_that_can_be_raised_cheaply_is_raised_with_its_kind_by_its_input() {
    let mut raised_by_input = 0;
    let mut by_source_only = Vec::new();
    for (code, kind, _) in SPRINT1 {
        let Some(found) = raised(code) else {
            by_source_only.push(*code);
            continue;
        };
        let word = if kind == &"run failure" { "error" } else { kind };
        assert!(
            found.iter().any(|item| item.code == *code && item.kind == word),
            "{code}: expected `{word} {code}`, got {found:?}"
        );
        raised_by_input += 1;
    }
    // Only the run failures that need a real browser or ffmpeg to go wrong are left to the source check.
    assert!(
        by_source_only.iter().all(|code| code.starts_with("FS-R")),
        "codes with no input in this test: {by_source_only:?}"
    );
    assert!(raised_by_input >= 55, "{raised_by_input}");
}

// ---------------------------------------------------------------------
// Criterion 5: the support matrix lists every refusal and the sprint it waits for
// ---------------------------------------------------------------------

/// A "waits for" entry of a refusal names a sprint: "Sprint" and its number, or "This sprint" with the
/// ticket that lifts the refusal.
fn names_a_sprint(waits: &str) -> bool {
    let sprint = waits
        .strip_prefix("Sprint ")
        .is_some_and(|rest| rest.chars().next().is_some_and(|c| c.is_ascii_digit()));
    sprint || waits.starts_with("This sprint (ticket T")
}

/// The name rows may also say "Not targeted": the product owner closed those two on 2026-10-11.
fn names_a_sprint_or_not_targeted(waits: &str) -> bool {
    names_a_sprint(waits) || waits.starts_with("Not targeted")
}

#[test]
fn s2_t005_c5_every_refusal_in_the_support_matrix_names_what_it_waits_for() {
    let in_spec = spec_unsupported();
    assert!(!in_spec.is_empty());
    for (code, use_, waits) in &in_spec {
        assert!(!use_.is_empty(), "{code}");
        assert!(!waits.is_empty(), "{code} has an empty \"waits for\"");
        assert_ne!(waits, "Not yet assigned", "{code} still says \"Not yet assigned\"");
        assert!(names_a_sprint(waits), "{code}: {waits:?} names no sprint");
    }
    assert!(
        !matrix().contains("Not yet assigned"),
        "the support matrix still says \"Not yet assigned\" somewhere"
    );
    assert!(
        matrix().contains("2026-10-12"),
        "the support matrix does not state the date of the decision that gives FS-U018 to FS-U021 their sprints"
    );
    // Every code that stays is listed.
    let listed_codes: BTreeSet<&str> = in_spec.iter().map(|row| row.0.as_str()).collect();
    for (code, _) in STAYING {
        assert!(listed_codes.contains(code), "{code} is not in the table of refusals");
    }
    // A refusal that waits for a sprint after this one never says "This sprint", and the other way round.
    for (code, _, waits) in &in_spec {
        let lifted = LIFTED.iter().any(|row| row.0 == code);
        assert_eq!(waits.starts_with("This sprint"), lifted, "{code}: {waits:?}");
    }
}

#[test]
fn s2_t005_c5_the_four_names_not_accepted_are_listed_as_unknown_with_the_sprint_they_wait_for() {
    let names = spec_names();
    assert_eq!(names.len(), NOT_ACCEPTED.len(), "{names:?}");
    for (name, reported, waits) in &names {
        assert!(NOT_ACCEPTED.iter().any(|row| row.0 == name), "{name} is not one of the four");
        assert!(reported.starts_with("FS-X001"), "{name}: {reported}");
        assert!(reported.contains("unknown"), "{name}: {reported}");
        assert!(!waits.is_empty(), "{name} has an empty \"waits for\"");
        assert_ne!(waits, "Not yet assigned", "{name} still says \"Not yet assigned\"");
        assert!(names_a_sprint_or_not_targeted(waits), "{name}: {waits:?}");
    }
    let waits_of = |wanted: &str| names.iter().find(|row| row.0 == wanted).unwrap().2.clone();
    // The sprints decided on 2026-10-11 (proposal points P2 and P3, and the thumbnail row).
    assert!(waits_of("data-fs-end").starts_with("Not targeted"));
    assert!(waits_of("data-fs-layer").starts_with("Not targeted"));
    assert_eq!(waits_of("data-fs-no-timeline"), "Sprint 4");
    assert_eq!(waits_of("data-fs-thumbnail"), "Sprint 14");
}

#[test]
fn s2_t005_c5_the_sprint_each_staying_refusal_waits_for_is_the_one_in_the_plan() {
    let in_spec = spec_unsupported();
    let waits = |code: &str| in_spec.iter().find(|row| row.0 == code).unwrap().2.clone();
    // From the roadmap's sprint tags and the plan's sprint list (see the note under the table).
    for code in ["FS-U001", "FS-U002", "FS-U008", "FS-U009"] {
        assert_eq!(waits(code), "Sprint 3", "{code}");
    }
    assert_eq!(waits("FS-U007"), "Sprint 8");
    for code in [
        "FS-U010", "FS-U011", "FS-U012", "FS-U013", "FS-U014", "FS-U015", "FS-U016", "FS-U017",
    ] {
        assert_eq!(waits(code), "Sprint 4", "{code}");
    }
    // No line of the roadmap or the plan names these four; the product owner's decision of 2026-10-12
    // gives them their sprints (note 2 of the matrix).
    assert_eq!(waits("FS-U018"), "Sprint 11");
    for code in ["FS-U019", "FS-U020", "FS-U021"] {
        assert_eq!(waits(code), "Sprint 4", "{code}");
    }
}
