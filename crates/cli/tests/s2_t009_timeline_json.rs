//! Sprint 2, ticket T009: `framesmith timeline <project> --json` gives the timeline of a project, or its
//! diagnostics, as exactly one JSON document, and nothing else on standard output.
//!
//! Notes for maintainers
//! ---------------------
//! * The text form and the document are written from one set of resolved clip windows
//!   (`src/facts.rs`, `Timeline`). These tests still read both outputs of the built binary and compare
//!   them number with number, so a change to either form is caught here. The printed times are parsed
//!   out of the text, never taken from the manifest.
//! * The test projects are small and written for this file. The values they must give are stated in the
//!   `Known` table; every test reads its expectations from there.
//! * "Equal" for a time means the same number: the printed text read as a number and the number of the
//!   document are compared with `==` on 64-bit numbers, never with a tolerance. Up to fifteen
//!   significant digits that is also the same digits (c3 checks that too); the one project with more
//!   digits than a 64-bit number holds is `extreme()`.
//! * No time of a clip can have more than nine decimals, because the reader refuses a longer written
//!   time; `s2_t009_c3_a_time_with_more_than_nine_decimals_cannot_be_written` keeps that premise honest.
//!   If it ever failed, the text form (which rounds to six places beyond nine decimals) and the document
//!   could differ, and the meaning of "equals the time printed" would need a new statement in
//!   `docs/spec/commands.md`.
//! * Scratch folders: every test gives each project a scratch key of its own (tests of a file run in
//!   parallel, and a key names a folder that is emptied when it is made).
//! * None of these tests needs a browser or `ffmpeg`: `timeline` starts neither.

mod common;

use common::*;
use measure::Ran;
use serde_json::Value;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// What a clip must come out as: the name, start, end and duration the document is to give, and whether
/// the duration was filled in.
type ClipRow = (&'static str, f64, f64, f64, bool);

/// A project of this file: how it is written and what `timeline --json` is to say about it.
struct Known {
    name: &'static str,
    attrs: &'static str,
    /// The clips as (start, duration written), in seconds, in document order.
    clips: &'static [(&'static str, Option<&'static str>)],
    tweens: &'static str,
    width: u64,
    height: u64,
    fps: u64,
    fps_filled_in: bool,
    duration: f64,
    duration_filled_in: bool,
    frames: u64,
    /// Every clip, in document order.
    rows: &'static [ClipRow],
}

const TWEEN: &str = "{ \"target\": \"#c0\", \"to\": { \"x\": 10 }, \"at\": 0, \"duration\": 0.5 }";

/// Everything written: nothing is filled in.
const WRITTEN: Known = Known {
    name: "written",
    attrs: "data-fs-size=\"64x36\" data-fs-fps=\"24\" data-fs-duration=\"1\"",
    clips: &[("0", Some("0.5")), ("0.5", Some("0.5"))],
    tweens: TWEEN,
    width: 64,
    height: 36,
    fps: 24,
    fps_filled_in: false,
    duration: 1.0,
    duration_filled_in: false,
    frames: 24,
    rows: &[("#c0", 0.0, 0.5, 0.5, false), ("#c1", 0.5, 1.0, 0.5, false)],
};

/// The rate is left out: 30 is filled in.
const RATE_FILLED: Known = Known {
    name: "rate_filled",
    attrs: "data-fs-size=\"64x36\" data-fs-duration=\"1\"",
    clips: &[("0", Some("0.5")), ("0.5", Some("0.5"))],
    tweens: TWEEN,
    width: 64,
    height: 36,
    fps: 30,
    fps_filled_in: true,
    duration: 1.0,
    duration_filled_in: false,
    frames: 30,
    rows: &[("#c0", 0.0, 0.5, 0.5, false), ("#c1", 0.5, 1.0, 0.5, false)],
};

/// The length is left out and worked out from the motion (2 s); a clip with no duration runs to the end.
const LENGTH_WORKED_OUT: Known = Known {
    name: "length_worked_out",
    attrs: "data-fs-size=\"64x36\" data-fs-fps=\"25\"",
    clips: &[("0", Some("0.5")), ("0.5", None)],
    tweens: "{ \"target\": \"#c0\", \"to\": { \"x\": 10 }, \"at\": 1, \"duration\": 1 }",
    width: 64,
    height: 36,
    fps: 25,
    fps_filled_in: false,
    duration: 2.0,
    duration_filled_in: true,
    frames: 50,
    rows: &[("#c0", 0.0, 0.5, 0.5, false), ("#c1", 0.5, 2.0, 1.5, true)],
};

/// The length is written (3 s) and two clips have no duration: only those clips are flagged.
const DURATIONS_WORKED_OUT: Known = Known {
    name: "durations_worked_out",
    attrs: "data-fs-size=\"64x36\" data-fs-fps=\"10\" data-fs-duration=\"3\"",
    clips: &[("0", Some("1")), ("1", None), ("2.5", None)],
    tweens: TWEEN,
    width: 64,
    height: 36,
    fps: 10,
    fps_filled_in: false,
    duration: 3.0,
    duration_filled_in: false,
    frames: 30,
    rows: &[
        ("#c0", 0.0, 1.0, 1.0, false),
        ("#c1", 1.0, 3.0, 2.0, true),
        ("#c2", 2.5, 3.0, 0.5, true),
    ],
};

/// Times between frame times (at 24 fps a frame is 0.041666... s) and with several decimals: one with four,
/// one with nine, a start a hair before a frame time (1.0416666 against 25/24), and a duration worked out
/// from it.
const BETWEEN: Known = Known {
    name: "between",
    attrs: "data-fs-size=\"64x36\" data-fs-fps=\"24\" data-fs-duration=\"2\"",
    clips: &[("0.1", Some("0.3333")), ("0.123456789", Some("0.000000001")), ("1.0416666", None)],
    tweens: TWEEN,
    width: 64,
    height: 36,
    fps: 24,
    fps_filled_in: false,
    duration: 2.0,
    duration_filled_in: false,
    frames: 48,
    rows: &[
        ("#c0", 0.1, 0.4333, 0.3333, false),
        ("#c1", 0.123456789, 0.12345679, 0.000000001, false),
        ("#c2", 1.0416666, 2.0, 0.9583334, true),
    ],
};

/// Clips whose order in the document is not the order of their starts.
const UNSORTED: Known = Known {
    name: "unsorted",
    attrs: "data-fs-size=\"64x36\" data-fs-fps=\"10\" data-fs-duration=\"4\"",
    clips: &[("2", Some("1")), ("0", Some("1.5")), ("1.25", None)],
    tweens: TWEEN,
    width: 64,
    height: 36,
    fps: 10,
    fps_filled_in: false,
    duration: 4.0,
    duration_filled_in: false,
    frames: 40,
    rows: &[
        ("#c0", 2.0, 3.0, 1.0, false),
        ("#c1", 0.0, 1.5, 1.5, false),
        ("#c2", 1.25, 4.0, 2.75, true),
    ],
};

const ALL: [&Known; 6] = [
    &WRITTEN,
    &RATE_FILLED,
    &LENGTH_WORKED_OUT,
    &DURATIONS_WORKED_OUT,
    &BETWEEN,
    &UNSORTED,
];

impl Known {
    fn make(&self, test: &str) -> PathBuf {
        let body: String = self
            .clips
            .iter()
            .enumerate()
            .map(|(number, (start, duration))| {
                let duration = duration.map_or(String::new(), |d| format!(" data-fs-duration=\"{d}\""));
                format!("<div id=\"c{number}\" data-fs-clip data-fs-start=\"{start}\"{duration}></div>\n")
            })
            .collect();
        project(test, "known", &page(self.attrs, "", &body, &motion(self.tweens)))
    }
}

/// A project that passes with two warnings: a clip that starts at or after the end of the written length
/// never shows (FS-W001, a real warning).
fn warned(test: &str) -> PathBuf {
    let body = "<div id=\"c0\" data-fs-clip data-fs-start=\"0\" data-fs-duration=\"1\"></div>\n\
<div id=\"late\" data-fs-clip data-fs-start=\"5\" data-fs-duration=\"1\"></div>\n\
<div id=\"later\" data-fs-clip data-fs-start=\"2\" data-fs-duration=\"1\"></div>";
    project(
        test,
        "known",
        &page(
            "data-fs-size=\"64x36\" data-fs-fps=\"24\" data-fs-duration=\"2\"",
            "",
            body,
            &motion(TWEEN),
        ),
    )
}

/// The largest times the reader accepts, so that the printed end has nineteen significant digits (more
/// than a 64-bit number holds). The first clip starts after the end of the length: a warning, still a pass.
fn extreme(test: &str) -> PathBuf {
    project(
        test,
        "known",
        &page(
            "data-fs-size=\"64x36\" data-fs-fps=\"30\" data-fs-duration=\"999999999\"",
            "",
            "<div id=\"c0\" data-fs-clip data-fs-start=\"999999999.999999999\" data-fs-duration=\"999999999.999999999\"></div>\n\
<div id=\"c1\" data-fs-clip data-fs-start=\"123456789.123456789\" data-fs-duration=\"0.000000001\"></div>",
            "",
        ),
    )
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

fn timeline_json(dir: &Path) -> Ran {
    framesmith(&["timeline", text(dir), "--json"])
}

fn timeline_text(dir: &Path) -> Ran {
    framesmith(&["timeline", text(dir)])
}

fn info_json(dir: &Path) -> Ran {
    framesmith(&["info", text(dir), "--json"])
}

fn check(dir: &Path) -> Ran {
    framesmith(&["check", text(dir)])
}

/// Standard output as exactly one JSON document: one line, ended by one line feed, that parses whole (a
/// parser that is asked for the next value after the document finds nothing, and has consumed every
/// byte), and that holds no control character other than that line feed. Returns the parsed document;
/// its numbers of more than fifteen digits are not exact (the parser of `serde_json` is not
/// round-trip exact), so `disagreements` reads the times from the text of the line.
fn one_document(ran: &Ran, what: &str) -> Value {
    let out = &ran.stdout;
    let line = out
        .strip_suffix('\n')
        .unwrap_or_else(|| panic!("{what}: no final line feed: {out:?}"));
    assert!(!line.contains('\n') && !line.is_empty(), "{what}: not one line: {out:?}");
    let mut stream = serde_json::Deserializer::from_str(line).into_iter::<Value>();
    let value = stream
        .next()
        .unwrap_or_else(|| panic!("{what}: no document: {out:?}"))
        .unwrap_or_else(|e| panic!("{what}: not one document ({e}): {out:?}"));
    assert_eq!(stream.byte_offset(), line.len(), "{what}: bytes after the document: {out:?}");
    assert!(stream.next().is_none(), "{what}: a second document: {out:?}");
    assert!(value.is_object(), "{what}: {out:?}");
    let controls: Vec<u8> = out.bytes().filter(|&b| (b < 0x20 && b != b'\n') || b == 0x7f).collect();
    assert_eq!(controls, Vec::<u8>::new(), "{what}: a raw control character in {out:?}");
    assert_eq!(out.matches('\n').count(), 1, "{what}");
    value
}

fn keys(value: &Value) -> Vec<&str> {
    value.as_object().unwrap().keys().map(String::as_str).collect()
}

const PASS_KEYS: [&str; 12] = [
    "clips",
    "composition",
    "diagnostics",
    "duration",
    "duration_filled_in",
    "fps",
    "fps_filled_in",
    "frames",
    "height",
    "outcome",
    "run_id",
    "width",
];

/// (ticket T014 added `lane` and `lane_filled_in`)
const CLIP_KEYS: [&str; 7] = ["duration", "duration_filled_in", "end", "lane", "lane_filled_in", "name", "start"];

const DIAGNOSTIC_KEYS: [&str; 6] = ["code", "description", "file", "kind", "line", "remedy"];

/// One clip line of the text form, read back.
#[derive(Debug, PartialEq)]
struct Row {
    name: String,
    start: String,
    end: String,
    duration: String,
    filled_in: bool,
    /// The number of the lane line the clip stands under (ticket T014).
    lane: String,
    /// Whether the line ends in the mark of a lane that was filled in.
    lane_filled_in: bool,
}

/// Reads the printed timeline (after any warnings): the header's five cells (separated by two spaces), the
/// lane lines (`lane 0`) and the clip lines under them. A line that does not have the shape of the layout
/// makes the test fail. The rows come in the order of the text form, which groups the clips by lane; the
/// projects of this file write no lane, so that order is the document order the JSON list keeps.
fn read_timeline(printed: &str) -> ([String; 5], Vec<Row>) {
    let mut lines = printed.lines();
    let header = lines.next().expect("a header line");
    let cells: Vec<String> = header.split("  ").map(String::from).collect();
    let header: [String; 5] = cells.try_into().unwrap_or_else(|c| panic!("the header is not five cells: {c:?}"));
    let mut lane = None;
    let mut rows = Vec::new();
    for line in lines {
        if let Some(number) = line.strip_prefix("lane ") {
            lane = Some(number.to_string());
            continue;
        }
        let lane = lane
            .clone()
            .unwrap_or_else(|| panic!("a clip line comes before any lane line: {line:?}"));
        let body = line
            .strip_prefix("  ")
            .unwrap_or_else(|| panic!("a clip line is indented by two spaces: {line:?}"));
        let (body, lane_filled_in) = match body.strip_suffix(" (lane filled in)") {
            Some(body) => (body, true),
            None => (body, false),
        };
        let (name, rest) = body
            .split_once("  ")
            .unwrap_or_else(|| panic!("no two spaces after the name: {line:?}"));
        let words: Vec<&str> = rest.split_whitespace().collect();
        let (times, filled_in) = match words[..] {
            [a, "s", "to", b, "s", c, "s"] => ([a, b, c], false),
            [a, "s", "to", b, "s", c, "s", "(filled", "in)"] => ([a, b, c], true),
            _ => panic!("a clip line does not have the shape of the layout: {line:?}"),
        };
        rows.push(Row {
            name: name.trim_end().to_string(),
            start: times[0].to_string(),
            end: times[1].to_string(),
            duration: times[2].to_string(),
            filled_in,
            lane,
            lane_filled_in,
        });
    }
    (header, rows)
}

/// `<n> <unit>[ (filled in)]` as (n, filled in).
fn number_and_mark(value: &str, unit: &str) -> (f64, bool) {
    let (value, filled) = match value.strip_suffix(" (filled in)") {
        Some(value) => (value, true),
        None => (value, false),
    };
    let number = value.strip_suffix(unit).unwrap_or_else(|| panic!("{value:?} lacks {unit:?}"));
    (number.parse().unwrap_or_else(|_| panic!("{number:?} is not a number")), filled)
}

/// A diagnostic as `check` prints it: three lines and a blank one.
fn block(d: &Value) -> String {
    let place = match d["line"].as_u64().unwrap() {
        0 => d["file"].as_str().unwrap().to_string(),
        line => format!("{}:{line}", d["file"].as_str().unwrap()),
    };
    format!(
        "{} {} {place}\n  {}\n  fix: {}\n\n",
        d["kind"].as_str().unwrap(),
        d["code"].as_str().unwrap(),
        d["description"].as_str().unwrap(),
        d["remedy"].as_str().unwrap()
    )
}

/// The text of the numbers a key has in the `clips` list of a document line, in clip order. Read from the
/// line itself, so that a number is compared as written and not as a parser rounded it.
fn clip_numbers(line: &str, key: &str) -> Vec<String> {
    let from = line.find("\"clips\":[").expect("a clips list");
    let list = &line[from..];
    let list = &list[..list.find(']').expect("the clips list ends")];
    let pattern = format!("\"{key}\":");
    list.match_indices(&pattern)
        .map(|(at, _)| {
            let rest = &list[at + pattern.len()..];
            rest[..rest.find([',', '}']).unwrap_or(rest.len())].to_string()
        })
        .collect()
}

/// Significant digits of a printed number.
fn significant_digits(printed: &str) -> usize {
    printed
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>()
        .trim_start_matches('0')
        .len()
}

/// Where a document (one line of standard output) and a printed timeline differ, as a list of
/// sentences: the header's facts and, for every clip, the name, the start, the end, the duration and the
/// mark. A time of the printed text and the number written in the document must be the same 64-bit
/// number (`==` on the numbers, no tolerance); and for a time of up to fifteen significant digits a time
/// printed with no decimal point is a whole number in the document, and the other way round. An empty
/// list means they agree.
fn disagreements(line: &str, printed: &str) -> Vec<String> {
    let doc: Value = serde_json::from_str(line).expect("a document");
    let mut found = Vec::new();
    let (header, rows) = read_timeline(printed);
    if doc["composition"].as_str() != Some(header[0].as_str()) {
        found.push(format!("composition {} / {}", doc["composition"], header[0]));
    }
    if format!("{}x{}", doc["width"], doc["height"]) != header[1] {
        found.push(format!("size {}x{} / {}", doc["width"], doc["height"], header[1]));
    }
    let (rate, rate_filled) = number_and_mark(&header[2], " fps");
    if doc["fps"].as_u64().map(|n| n as f64) != Some(rate) {
        found.push(format!("rate {} / {rate}", doc["fps"]));
    }
    if doc["fps_filled_in"].as_bool() != Some(rate_filled) {
        found.push(format!("rate mark {} / {rate_filled}", doc["fps_filled_in"]));
    }
    let (length, length_filled) = number_and_mark(&header[3], " s");
    if doc["duration"].as_f64() != Some(length) {
        found.push(format!("length {} / {length}", doc["duration"]));
    }
    if doc["duration_filled_in"].as_bool() != Some(length_filled) {
        found.push(format!("length mark {} / {length_filled}", doc["duration_filled_in"]));
    }
    let frames: u64 = header[4].split(' ').next().unwrap().parse().unwrap();
    if doc["frames"].as_u64() != Some(frames) {
        found.push(format!("frames {} / {frames}", doc["frames"]));
    }
    let clips = doc["clips"].as_array().expect("clips is a list");
    if clips.len() != rows.len() {
        found.push(format!("{} clips / {} clip lines", clips.len(), rows.len()));
        return found;
    }
    for (key, column) in [("start", 0), ("end", 1), ("duration", 2)] {
        let written = clip_numbers(line, key);
        assert_eq!(written.len(), rows.len(), "{key}: one number for each clip");
        for (number, (written, row)) in written.iter().zip(&rows).enumerate() {
            let printed = [&row.start, &row.end, &row.duration][column];
            let read: f64 = printed.parse().unwrap_or_else(|_| panic!("{printed:?} is not a number"));
            let got: f64 = written.parse().unwrap_or_else(|_| panic!("{written:?} is not a number"));
            if got != read {
                found.push(format!("clip {number}: {key} {written} / {printed}"));
            }
            // Whole or not whole, where the digits can tell.
            let whole = written.bytes().all(|b| b.is_ascii_digit());
            if significant_digits(printed) <= 15 && whole != !printed.contains('.') {
                found.push(format!("clip {number}: {key} {written} is not written like {printed}"));
            }
        }
    }
    for (number, (clip, row)) in clips.iter().zip(&rows).enumerate() {
        if clip["name"].as_str() != Some(row.name.as_str()) {
            found.push(format!("clip {number}: name {} / {}", clip["name"], row.name));
        }
        if clip["duration_filled_in"].as_bool() != Some(row.filled_in) {
            found.push(format!("clip {number}: mark {} / {}", clip["duration_filled_in"], row.filled_in));
        }
        let lane = clip["lane"].as_u64().map_or_else(String::new, |lane| lane.to_string());
        if lane != row.lane {
            found.push(format!("clip {number}: lane {} / {}", clip["lane"], row.lane));
        }
        if clip["lane_filled_in"].as_bool() != Some(row.lane_filled_in) {
            found.push(format!(
                "clip {number}: lane mark {} / {}",
                clip["lane_filled_in"], row.lane_filled_in
            ));
        }
    }
    found
}

/// Everything under a folder, folders and files, as paths relative to it.
fn entries(dir: &Path) -> Vec<PathBuf> {
    fn walk(root: &Path, dir: &Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            found.push(path.strip_prefix(root).unwrap().to_path_buf());
            if path.is_dir() {
                walk(root, &path, found);
            }
        }
    }
    let mut found = Vec::new();
    walk(dir, dir, &mut found);
    found.sort();
    found
}

/// Every file of a folder with the time it was last modified.
fn modified(dir: &Path) -> BTreeMap<PathBuf, std::time::SystemTime> {
    entries(dir)
        .into_iter()
        .map(|entry| {
            let time = std::fs::metadata(dir.join(&entry)).unwrap().modified().unwrap();
            (entry, time)
        })
        .collect()
}

/// The document and the text form of a project that passes, with `warnings` warnings; the document is
/// checked for its key set, its types and its diagnostics, and set against the text form. Returns the
/// document.
fn assert_document_equals_text(dir: &Path, what: &str, warnings: usize) -> Value {
    let json = timeline_json(dir);
    let told = timeline_text(dir);
    assert_eq!((json.code, told.code), (0, 0), "{what}: {}{}", json.all(), told.all());
    assert_eq!(json.stderr, "", "{what}");
    let doc = one_document(&json, what);
    assert_eq!(keys(&doc), PASS_KEYS, "{what}");
    assert_eq!(doc["outcome"], "passed", "{what}");
    for name in ["width", "height", "fps", "frames"] {
        assert!(doc[name].is_u64(), "{what}: {name} is not a whole number: {}", doc[name]);
    }
    assert!(doc["duration"].is_number(), "{what}");
    for name in ["fps_filled_in", "duration_filled_in"] {
        assert!(doc[name].is_boolean(), "{what}: {name}");
    }
    for name in ["outcome", "run_id", "composition"] {
        assert!(doc[name].is_string(), "{what}: {name}");
    }
    for clip in doc["clips"].as_array().unwrap() {
        assert_eq!(keys(clip), CLIP_KEYS, "{what}");
        assert!(clip["name"].is_string() && clip["duration_filled_in"].is_boolean(), "{what}");
        for key in ["start", "end", "duration"] {
            assert!(clip[key].is_number(), "{what}: {key}");
        }
    }
    // The warnings the text form prints before the header are the document's diagnostics.
    let diagnostics = doc["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), warnings, "{what}");
    for d in diagnostics {
        assert_eq!(keys(d), DIAGNOSTIC_KEYS, "{what}");
    }
    let blocks: String = diagnostics.iter().map(block).collect();
    assert!(told.stdout.starts_with(&blocks), "{what}: {}", told.stdout);
    let found = disagreements(json.stdout.trim_end(), &told.stdout[blocks.len()..]);
    assert_eq!(found, Vec::<String>::new(), "{what}");
    doc
}

// ---------------------------------------------------------------------
// c1
// ---------------------------------------------------------------------

#[test]
fn s2_t009_c1_a_valid_project_gives_one_document_with_everything_the_text_form_shows_and_exit_0() {
    for known in ALL {
        let dir = known.make(&format!("s2_t009_c1_{}", known.name));
        let (files, all) = (tree(&dir), entries(&dir));
        let doc = assert_document_equals_text(&dir, known.name, 0);

        // The known values, written out in the table (not read from the code that prints them).
        assert_eq!(doc["composition"], "main", "{}", known.name);
        assert_eq!(doc["width"], known.width, "{}", known.name);
        assert_eq!(doc["height"], known.height, "{}", known.name);
        assert_eq!(doc["fps"], known.fps, "{}", known.name);
        assert_eq!(doc["fps_filled_in"], known.fps_filled_in, "{}", known.name);
        assert_eq!(doc["duration"].as_f64(), Some(known.duration), "{}", known.name);
        assert_eq!(doc["duration_filled_in"], known.duration_filled_in, "{}", known.name);
        assert_eq!(doc["frames"], known.frames, "{}", known.name);
        assert_eq!(doc["diagnostics"], serde_json::json!([]), "{}", known.name);
        let clips = doc["clips"].as_array().unwrap();
        assert_eq!(clips.len(), known.rows.len(), "{}", known.name);
        for (clip, (name, start, end, duration, filled)) in clips.iter().zip(known.rows) {
            assert_eq!(clip["name"], *name, "{}", known.name);
            assert_eq!(clip["start"].as_f64(), Some(*start), "{}: {name}", known.name);
            assert_eq!(clip["end"].as_f64(), Some(*end), "{}: {name}", known.name);
            assert_eq!(clip["duration"].as_f64(), Some(*duration), "{}: {name}", known.name);
            assert_eq!(clip["duration_filled_in"], *filled, "{}: {name}", known.name);
        }

        // The run changed nothing in the project and made no output folder.
        assert_eq!((tree(&dir), entries(&dir)), (files, all), "{}", known.name);
        assert!(!dir.join("out").exists(), "{}", known.name);
    }
    // Fixtures of the repository: one with everything written, one with a rate and a length left out.
    for name in ["title-card", "title-card-unwritten"] {
        assert_document_equals_text(&fixture(&format!("s2_t009_c1_{name}"), name), name, 0);
    }
}

#[test]
fn s2_t009_c1_the_filled_in_flags_are_right_in_both_directions() {
    let flags = |known: &Known| -> (bool, bool, Vec<bool>) {
        let dir = known.make(&format!("s2_t009_c1_flags_{}", known.name));
        let ran = timeline_json(&dir);
        assert_eq!(ran.code, 0, "{}", ran.all());
        let doc = one_document(&ran, known.name);
        (
            doc["fps_filled_in"].as_bool().unwrap(),
            doc["duration_filled_in"].as_bool().unwrap(),
            doc["clips"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e["duration_filled_in"].as_bool().unwrap())
                .collect(),
        )
    };
    // Written values are not flagged.
    assert_eq!(flags(&WRITTEN), (false, false, vec![false, false]));
    // Left out: flagged, and only what was left out.
    assert_eq!(flags(&RATE_FILLED), (true, false, vec![false, false]));
    assert_eq!(flags(&LENGTH_WORKED_OUT), (false, true, vec![false, true]));
    assert_eq!(flags(&DURATIONS_WORKED_OUT), (false, false, vec![false, true, true]));
    // A rate written as 30 is written: it is not flagged.
    let dir = project(
        "s2_t009_c1_thirty",
        "known",
        &page("data-fs-size=\"64x36\" data-fs-fps=\"30\" data-fs-duration=\"1\"", "", "", ""),
    );
    let doc = one_document(&timeline_json(&dir), "thirty");
    assert_eq!((doc["fps"].as_u64(), doc["fps_filled_in"].as_bool()), (Some(30), Some(false)));
    // The mark is a flag of its own, never inside a value: the values are bare numbers and the name is
    // only the name.
    assert!(doc["fps"].is_u64() && doc["duration"].is_u64());
    let dir = LENGTH_WORKED_OUT.make("s2_t009_c1_flags_inside");
    let ran = timeline_json(&dir);
    assert!(!ran.stdout.contains("filled in\""), "{}", ran.stdout);
    assert!(!ran.stdout.contains("(filled"), "{}", ran.stdout);
}

#[test]
fn s2_t009_c1_the_clips_are_in_document_order_and_a_clip_with_no_id_is_named_as_a_message_names_it() {
    // Document order, not the order of the starts.
    let dir = UNSORTED.make("s2_t009_c1_order");
    let doc = one_document(&timeline_json(&dir), "unsorted");
    let names: Vec<&str> = doc["clips"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["#c0", "#c1", "#c2"]);
    let starts: Vec<f64> = doc["clips"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["start"].as_f64().unwrap())
        .collect();
    assert_eq!(starts, [2.0, 0.0, 1.25]);

    // A clip with no id has the name `check` gives it; the key is `name`, and there is no `label`.
    let dir = project(
        "s2_t009_c1_noid",
        "known",
        &page(
            "data-fs-size=\"64x36\" data-fs-fps=\"10\" data-fs-duration=\"2\"",
            "",
            "<div id=\"first\" data-fs-clip data-fs-start=\"0\" data-fs-duration=\"1\"></div>\n\
<div data-fs-clip data-fs-start=\"1\"></div>",
            "",
        ),
    );
    let doc = assert_document_equals_text(&dir, "no id", 0);
    let clips = doc["clips"].as_array().unwrap();
    assert_eq!(clips[0]["name"], "#first");
    let second = clips[1]["name"].as_str().unwrap();
    assert!(second.starts_with("number 2 <"), "{second}");
    assert!(clips.iter().all(|c| c.get("label").is_none()));
    // The name is how `info --json` labels the same clip.
    let info: Value = one_document(&info_json(&dir), "info");
    let labels: Vec<&str> = info["clip_durations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["label"].as_str().unwrap())
        .collect();
    let names: Vec<&str> = clips.iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names, labels);
}

#[test]
fn s2_t009_c1_the_facts_shared_with_info_json_have_the_same_names_and_values() {
    for known in ALL {
        let dir = known.make(&format!("s2_t009_c1_shared_{}", known.name));
        let timeline = one_document(&timeline_json(&dir), known.name);
        let info = one_document(&info_json(&dir), known.name);
        for name in [
            "composition",
            "width",
            "height",
            "fps",
            "fps_filled_in",
            "duration",
            "duration_filled_in",
            "frames",
            "diagnostics",
            "outcome",
        ] {
            assert_eq!(timeline[name], info[name], "{}: {name}", known.name);
        }
        // `info` counts the clips in `clips`; `timeline` lists them there. Its durations are those of
        // `info`'s `clip_durations`, entry for entry.
        assert_eq!(
            info["clips"].as_u64().unwrap() as usize,
            timeline["clips"].as_array().unwrap().len()
        );
        for (entry, clip) in info["clip_durations"]
            .as_array()
            .unwrap()
            .iter()
            .zip(timeline["clips"].as_array().unwrap())
        {
            assert_eq!(entry["label"], clip["name"], "{}", known.name);
            assert_eq!(entry["duration"], clip["duration"], "{}", known.name);
            assert_eq!(entry["duration_filled_in"], clip["duration_filled_in"], "{}", known.name);
        }
    }
}

#[test]
fn s2_t009_c1_a_warning_on_a_passing_project_is_carried_and_printed_before_the_timeline() {
    let dir = warned("s2_t009_c1_warning");
    let json = timeline_json(&dir);
    let told = timeline_text(&dir);
    let checked = check(&dir);
    assert_eq!((json.code, told.code, checked.code), (0, 0, 0), "{}{}", json.all(), told.all());
    assert_eq!(json.stderr, "");
    let doc = one_document(&json, "warning");
    assert_eq!(doc["outcome"], "passed");
    assert_eq!(keys(&doc), PASS_KEYS);
    let list = doc["diagnostics"].as_array().unwrap();
    assert_eq!(list.len(), 2, "{doc}");
    for d in list {
        assert_eq!(keys(d), DIAGNOSTIC_KEYS);
        assert_eq!(
            (d["code"].as_str(), d["kind"].as_str(), d["file"].as_str()),
            (Some("FS-W001"), Some("warning"), Some("index.html"))
        );
        assert!(d["line"].as_u64().unwrap() > 0);
        assert!(!d["description"].as_str().unwrap().is_empty() && !d["remedy"].as_str().unwrap().is_empty());
    }
    assert!(list[0]["description"].as_str().unwrap().contains("#late"), "{doc}");
    assert!(list[1]["description"].as_str().unwrap().contains("#later"), "{doc}");
    // The text form prints the same warnings, text for text, before the header; `check` prints them
    // before its result.
    let blocks: String = list.iter().map(block).collect();
    assert!(told.stdout.starts_with(&format!("{blocks}main  ")), "{}", told.stdout);
    assert!(checked.stdout.starts_with(&format!("{blocks}check passed\n")), "{}", checked.stdout);
    // A clip that starts at or after the end is in the list all the same.
    assert_eq!(doc["clips"].as_array().unwrap().len(), 3);
    assert_document_equals_text(&dir, "warning", 2);
    // A written value is not flagged, whatever the warnings.
    assert_eq!(
        (doc["fps_filled_in"].as_bool(), doc["duration_filled_in"].as_bool()),
        (Some(false), Some(false))
    );
}

#[test]
fn s2_t009_c1_a_composition_with_no_clips_gives_an_empty_list() {
    let dir = project(
        "s2_t009_c1_noclips",
        "known",
        &page("data-fs-size=\"64x36\" data-fs-fps=\"20\" data-fs-duration=\"1.25\"", "", "", ""),
    );
    let doc = assert_document_equals_text(&dir, "no clips", 0);
    assert_eq!(doc["clips"], serde_json::json!([]));
    assert_eq!(doc["duration"], serde_json::json!(1.25));
    assert_eq!(doc["frames"], 25);
    let told = timeline_text(&dir);
    assert_eq!(told.stdout, "main  64x36  20 fps  1.25 s  25 frames\n");
}

#[test]
fn s2_t009_c1_the_option_stands_before_or_after_the_folder_and_the_text_form_is_unchanged() {
    let dir = LENGTH_WORKED_OUT.make("s2_t009_c1_order_of_words");
    let after = one_document(&timeline_json(&dir), "after");
    let before_ran = framesmith(&["timeline", "--json", text(&dir)]);
    let before = one_document(&before_ran, "before");
    let twice = one_document(&framesmith(&["timeline", text(&dir), "--json", "--json"]), "twice");
    let strip = |mut doc: Value| {
        doc.as_object_mut().unwrap().remove("run_id");
        doc
    };
    assert_eq!(strip(before.clone()), strip(after.clone()));
    assert_eq!(strip(twice), strip(after));
    assert_eq!(before_ran.code, 0);
    // Without the option the text form is what it was: not a document.
    let told = timeline_text(&dir);
    assert_eq!(told.code, 0);
    assert_eq!(
        told.stdout,
        "main  64x36  25 fps  2 s (filled in)  50 frames\nlane 0\n  #c0  0 s   to 0.5 s  0.5 s (lane filled in)\n  #c1  0.5 s to 2 s    1.5 s (filled in) (lane filled in)\n"
    );
    // The relative form of the folder works too.
    let parent = std::fs::canonicalize(dir.parent().unwrap()).unwrap();
    let ran = run_in(&parent, &["timeline", "known", "--json"], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    one_document(&ran, "relative");
}

#[test]
fn s2_t009_c1_the_command_is_logged_with_json_asked_for_and_the_document_carries_the_run_id() {
    let dir = RATE_FILLED.make("s2_t009_c1_log");
    let logs = scratch("s2_t009_c1_log_folder");
    let env = [("FRAMESMITH_LOG_DIR", logs.as_os_str()), ("FRAMESMITH_LOG", OsStr::new("info"))];
    let ran = run_in(dir.parent().unwrap(), &["timeline", text(&dir), "--json"], &env);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let doc = one_document(&ran, "logged");
    let log = std::fs::read_to_string(logs.join("framesmith.log")).unwrap();
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 2, "{log}");
    assert!(
        lines[0].contains("command started") && lines[0].contains("command=\"timeline\""),
        "{log}"
    );
    assert!(lines[0].contains("json=true"), "{log}");
    assert!(
        lines[1].contains("command finished") && lines[1].contains("exit=0") && lines[1].contains("clips=2"),
        "{log}"
    );
    let run = |line: &str| line.split_whitespace().find(|word| word.starts_with("run=")).map(String::from);
    assert!(run(lines[0]).is_some() && run(lines[0]) == run(lines[1]), "{log}");
    assert_eq!(run(lines[0]).unwrap(), format!("run={}", doc["run_id"].as_str().unwrap()), "{log}");
    assert!(!log.contains("data-fs"), "{log}");
    // The text form says it did not ask for the document.
    let logs = scratch("s2_t009_c1_log_text");
    let env = [("FRAMESMITH_LOG_DIR", logs.as_os_str()), ("FRAMESMITH_LOG", OsStr::new("info"))];
    run_in(dir.parent().unwrap(), &["timeline", text(&dir)], &env);
    let log = std::fs::read_to_string(logs.join("framesmith.log")).unwrap();
    assert!(log.lines().next().unwrap().contains("json=false"), "{log}");
}

#[test]
fn s2_t009_c1_the_help_lists_the_option() {
    let ran = framesmith(&["--help"]);
    assert_eq!(ran.code, 0);
    assert!(ran.stdout.contains("framesmith timeline <project> [--json]"), "{}", ran.stdout);
    assert!(ran.stdout.contains("Option for info and timeline:"), "{}", ran.stdout);
}

#[test]
fn s2_t009_c1_the_colour_switches_change_nothing_and_no_file_is_written() {
    let dir = LENGTH_WORKED_OUT.make("s2_t009_c1_colour");
    let strip = |mut doc: Value| {
        doc.as_object_mut().unwrap().remove("run_id");
        doc
    };
    let plain = one_document(&timeline_json(&dir), "plain");
    let output = Command::new(BIN)
        .args(["timeline", text(&dir), "--json"])
        .current_dir(dir.parent().unwrap())
        .env("CLICOLOR_FORCE", "1")
        .env("FORCE_COLOR", "1")
        .env("TERM", "xterm-256color")
        .env_remove("NO_COLOR")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let out = String::from_utf8(output.stdout).unwrap();
    assert!(!out.contains('\u{1b}'), "{out:?}");
    assert!(output.stderr.is_empty());
    let coloured = one_document(
        &Ran {
            code: output.status.code().unwrap_or(-1),
            stdout: out,
            stderr: String::new(),
        },
        "coloured",
    );
    assert_eq!(strip(plain), strip(coloured));
    let quiet = run_in(
        dir.parent().unwrap(),
        &["timeline", text(&dir), "--json"],
        &[("NO_COLOR", OsStr::new("1"))],
    );
    one_document(&quiet, "no colour");
    assert_eq!(quiet.stderr, "");
}

// ---------------------------------------------------------------------
// c2
// ---------------------------------------------------------------------

/// The diagnostics `check` prints, read back as (kind, code, file, line, description, remedy). The blocks
/// are read from the text of `check`, not made from the document.
fn checked_diagnostics(printed: &str) -> Vec<[String; 6]> {
    let mut found = Vec::new();
    let lines: Vec<&str> = printed.lines().collect();
    let mut at = 0;
    while at < lines.len() && !lines[at].starts_with("check ") {
        let head: Vec<&str> = lines[at].splitn(3, ' ').collect();
        let [kind, code, place] = head[..] else {
            panic!("not a diagnostic head: {:?}", lines[at])
        };
        let (file, line) = match place.rsplit_once(':') {
            Some((file, line)) if line.chars().all(|c| c.is_ascii_digit()) => (file, line),
            _ => (place, "0"),
        };
        let description = lines[at + 1].strip_prefix("  ").expect("the description is indented");
        let remedy = lines[at + 2].strip_prefix("  fix: ").expect("the remedy starts with fix:");
        assert_eq!(lines[at + 3], "", "a blank line after a diagnostic");
        found.push([kind, code, file, line, description, remedy].map(String::from));
        at += 4;
    }
    found
}

/// The document of a project that does not pass, against `check`. Returns the document.
fn assert_failure_equals_check(dir: &Path, code: i32, what: &str) -> Value {
    let json = timeline_json(dir);
    let told = timeline_text(dir);
    let checked = check(dir);
    assert_eq!(checked.code, code, "{what}: {}", checked.all());
    assert_eq!((json.code, told.code), (code, code), "{what}: {}{}", json.all(), told.all());
    assert_eq!(json.stderr, "", "{what}");
    let doc = one_document(&json, what);
    // No timeline in it: only the outcome, the run id and the diagnostics.
    assert_eq!(keys(&doc), ["diagnostics", "outcome", "run_id"], "{what}");
    assert_eq!(doc["outcome"], if code == 2 { "refused" } else { "failed" }, "{what}");
    assert!(doc["run_id"].is_string(), "{what}");
    let list = doc["diagnostics"].as_array().unwrap();
    assert!(!list.is_empty(), "{what}");
    let read = checked_diagnostics(&checked.stdout);
    assert_eq!(read.len(), list.len(), "{what}");
    let kinds = ["error", "unsupported", "unknown", "warning"];
    for (d, parts) in list.iter().zip(&read) {
        assert_eq!(keys(d), DIAGNOSTIC_KEYS, "{what}");
        assert!(kinds.contains(&d["kind"].as_str().unwrap()), "{what}");
        assert!(d["line"].is_u64() && d["file"].is_string(), "{what}");
        // Each of the six parts is what `check` prints, in the same order.
        assert_eq!(d["kind"], parts[0].as_str(), "{what}: kind");
        assert_eq!(d["code"], parts[1].as_str(), "{what}: code");
        assert_eq!(d["file"], parts[2].as_str(), "{what}: file");
        assert_eq!(d["line"].as_u64().unwrap().to_string(), parts[3], "{what}: line");
        assert_eq!(d["description"], parts[4].as_str(), "{what}: description");
        assert_eq!(d["remedy"], parts[5].as_str(), "{what}: remedy");
        assert!(!parts[4].is_empty() && !parts[5].is_empty(), "{what}");
    }
    // The text form of `timeline` is `check`'s text, and the document's list gives that text back.
    assert_eq!(told.stdout, checked.stdout, "{what}");
    let blocks: String = list.iter().map(block).collect();
    assert!(checked.stdout.starts_with(&blocks), "{what}");
    // No header and no clip in the document.
    for name in ["clips", "composition", "width", "fps", "frames", "duration"] {
        assert!(doc.get(name).is_none(), "{what}: {name}");
    }
    doc
}

#[test]
fn s2_t009_c2_a_project_with_an_error_gives_one_document_with_its_diagnostic_and_exit_1() {
    let dir = project(
        "s2_t009_c2_error",
        "faulty",
        &page("data-fs-size=\"65x36\" data-fs-duration=\"1\"", "", "", ""),
    );
    let (files, all) = (tree(&dir), entries(&dir));
    let doc = assert_failure_equals_check(&dir, 1, "an error");
    let list = doc["diagnostics"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["code"], "FS-E007");
    assert_eq!(list[0]["kind"], "error");
    assert_eq!(list[0]["file"], "index.html");
    assert!(list[0]["line"].as_u64().unwrap() > 0);
    assert!(list[0]["description"].as_str().unwrap().contains("65"));
    assert!(list[0]["remedy"].as_str().unwrap().contains("even"));
    assert_eq!((tree(&dir), entries(&dir)), (files, all));
    assert!(!dir.join("out").exists());
}

#[test]
fn s2_t009_c2_a_project_with_several_errors_gives_them_all_in_the_order_of_check() {
    let dir = project(
        "s2_t009_c2_errors",
        "faulty",
        &page("data-fs-size=\"65x36\" data-fs-fps=\"23.976\" data-fs-duration=\"-1\"", "", "", ""),
    );
    let doc = assert_failure_equals_check(&dir, 1, "three errors");
    let list = doc["diagnostics"].as_array().unwrap();
    assert_eq!(list.len(), 3);
    assert!(list.iter().all(|d| d["kind"] == "error"));
    let mut codes: Vec<&str> = list.iter().map(|d| d["code"].as_str().unwrap()).collect();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), 3, "{codes:?}");
    // An error in a clip and a bad time in the motion, in two places of the file.
    let dir = project(
        "s2_t009_c2_clip_errors",
        "faulty",
        &page(
            "data-fs-size=\"64x36\" data-fs-fps=\"24\" data-fs-duration=\"2\"",
            "",
            "<div id=\"c0\" data-fs-clip data-fs-start=\"-1\" data-fs-duration=\"1\"></div>\n\
<div id=\"c1\" data-fs-clip data-fs-start=\"1\" data-fs-duration=\"abc\"></div>",
            &motion("{ \"target\": \"#c0\", \"to\": { \"x\": 10 }, \"at\": 0, \"duration\": 0.5 }"),
        ),
    );
    let doc = assert_failure_equals_check(&dir, 1, "errors in clips");
    assert_eq!(doc["diagnostics"].as_array().unwrap().len(), 2);
}

#[test]
fn s2_t009_c2_a_folder_that_does_not_exist_or_has_no_page_gives_a_document_and_exit_1() {
    let root = scratch("s2_t009_c2_places");
    let doc = assert_failure_equals_check(&root.join("nowhere"), 1, "a missing folder");
    assert_eq!(doc["diagnostics"][0]["code"], "FS-E001");
    assert_eq!(doc["diagnostics"][0]["line"], 0);
    assert!(!root.join("nowhere").exists());
    std::fs::create_dir_all(root.join("bare")).unwrap();
    write(&root, "bare/notes.txt", "no page here");
    let doc = assert_failure_equals_check(&root.join("bare"), 1, "a folder with no page");
    assert_eq!(doc["diagnostics"][0]["code"], "FS-E002");
}

#[test]
fn s2_t009_c2_an_unsupported_feature_gives_a_document_that_is_refused_with_exit_2() {
    for (name, code) in [
        ("unsupported/fs-u001-video-element", "FS-U001"),
        ("unsupported/fs-u014-css-animation-in-stylesheet", "FS-U014"),
        ("unsupported/fs-u009-network-image", "FS-U009"),
    ] {
        let dir = fixture(&format!("s2_t009_c2_{code}"), name);
        let doc = assert_failure_equals_check(&dir, 2, name);
        assert!(
            doc["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d["code"] == code && d["kind"] == "unsupported"),
            "{name}: {doc}"
        );
    }
}

#[test]
fn s2_t009_c2_an_unknown_attribute_gives_a_document_that_is_refused_with_exit_2() {
    let dir = fixture("s2_t009_c2_unknown", "unsupported/fs-x001-unknown-attribute");
    let doc = assert_failure_equals_check(&dir, 2, "an unknown attribute");
    assert!(doc["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["code"] == "FS-X001" && d["kind"] == "unknown"));
}

#[test]
fn s2_t009_c2_a_command_line_that_cannot_be_read_gives_a_failed_document_and_the_same_error_on_standard_error() {
    let dir = WRITTEN.make("s2_t009_c2_usage");
    let message = |what: &str| format!("error FS-R004\n  {what}\n  fix: Run `framesmith --help` to see the commands and their options.\n");
    let edit =
        |word: &str| format!("`framesmith timeline` only prints the timeline and has no edit commands; {word} is not one of its words.");
    let cases: Vec<(Vec<&str>, String)> = vec![
        // A word after the folder, with the option before, after or between.
        (vec!["timeline", "known", "move", "--json"], edit("move")),
        (vec!["timeline", "--json", "known", "move"], edit("move")),
        (vec!["timeline", "known", "--json", "trim"], edit("trim")),
        (vec!["timeline", "known", "delete", "#c1", "--json"], edit("delete")),
        // An option the command does not have, and the one that belongs to a later ticket.
        (
            vec!["timeline", "known", "--composition", "main", "--json"],
            "`framesmith timeline` takes only the path of a project folder; --composition is one word too many.".into(),
        ),
        (
            vec!["timeline", "known", "--output", "x.mp4", "--json"],
            "`framesmith timeline` takes only the path of a project folder; --output is one word too many.".into(),
        ),
        (
            vec!["timeline", "--bogus", "--json"],
            "`framesmith timeline` has no option --bogus.".into(),
        ),
        // No folder at all.
        (
            vec!["timeline", "--json"],
            "`framesmith timeline` needs the path of a project folder.".into(),
        ),
        (
            vec!["timeline", "--json", "--json"],
            "`framesmith timeline` needs the path of a project folder.".into(),
        ),
    ];
    for (args, expected) in cases {
        let ran = run_in(dir.parent().unwrap(), &args, &[]);
        assert_eq!(ran.code, 1, "{args:?}: {}", ran.all());
        let doc = one_document(&ran, &format!("{args:?}"));
        assert_eq!(keys(&doc), ["error", "outcome", "run_id"], "{args:?}");
        assert_eq!(doc["outcome"], "failed");
        assert_eq!(keys(&doc["error"]), ["code", "description", "remedy"], "{args:?}");
        assert_eq!(doc["error"]["code"], "FS-R004");
        assert_eq!(doc["error"]["description"], expected.as_str(), "{args:?}");
        assert_eq!(
            doc["error"]["remedy"],
            "Run `framesmith --help` to see the commands and their options."
        );
        // The same error, in words, on standard error: the one case where it is not empty.
        assert_eq!(ran.stderr, message(&expected), "{args:?}");
        // The same words as the text form gives for the command line without the option.
        let plain: Vec<&str> = args.iter().copied().filter(|word| *word != "--json").collect();
        let told = run_in(dir.parent().unwrap(), &plain, &[]);
        assert_eq!((told.code, told.stdout.as_str()), (1, ""), "{plain:?}");
        assert_eq!(told.stderr, ran.stderr, "{plain:?}");
    }
    // The refusal comes before the project is read: a faulty project and a missing folder give the same
    // answer, and nothing is made.
    let root = scratch("s2_t009_c2_usage_places");
    let ran = framesmith(&["timeline", text(&root.join("nowhere")), "move", "--json"]);
    assert_eq!(ran.code, 1);
    let doc = one_document(&ran, "missing and a word");
    assert_eq!(doc["error"]["code"], "FS-R004");
    assert!(!root.join("nowhere").exists());
    // Nothing was written into the project.
    assert!(!dir.join("out").exists());
    // `check --json` does not exist.
    let ran = run_in(dir.parent().unwrap(), &["check", "known", "--json"], &[]);
    assert_eq!((ran.code, ran.stdout.as_str()), (1, ""));
}

// ---------------------------------------------------------------------
// c3
// ---------------------------------------------------------------------

/// Every passing project of this file, with its name.
fn passing(test: &str) -> Vec<(String, PathBuf, usize)> {
    let mut all = Vec::new();
    for known in ALL {
        all.push((known.name.to_string(), known.make(&format!("{test}_{}", known.name)), 0));
    }
    all.push(("warning".to_string(), warned(&format!("{test}_warning")), 2));
    all.push(("extreme".to_string(), extreme(&format!("{test}_extreme")), 1));
    for name in ["title-card", "title-card-unwritten"] {
        all.push((name.to_string(), fixture(&format!("{test}_{name}"), name), 0));
    }
    all
}

#[test]
fn s2_t009_c3_every_time_of_the_document_equals_the_time_printed_for_the_same_clip() {
    let mut compared = 0;
    for (name, dir, warnings) in passing("s2_t009_c3_all") {
        let doc = assert_document_equals_text(&dir, &name, warnings);
        // Counted here too, so that a loop over nothing cannot pass.
        compared += doc["clips"].as_array().unwrap().len() * 3;
    }
    assert!(compared >= 3 * 18, "{compared}");
}

#[test]
fn s2_t009_c3_a_time_between_frame_times_and_one_with_several_decimals_equal_the_printed_text() {
    let dir = BETWEEN.make("s2_t009_c3_between");
    let doc = assert_document_equals_text(&dir, "between", 0);
    let told = timeline_text(&dir);
    let (_, rows) = read_timeline(&told.stdout);
    // The printed text, as printed.
    let printed: Vec<(&str, &str, &str)> = rows
        .iter()
        .map(|r| (r.start.as_str(), r.end.as_str(), r.duration.as_str()))
        .collect();
    assert_eq!(
        printed,
        [
            ("0.1", "0.4333", "0.3333"),
            ("0.123456789", "0.12345679", "0.000000001"),
            ("1.0416666", "2", "0.9583334"),
        ]
    );
    // The first two starts are not frame times at 24 fps. The third is a hair before one (25/24 s,
    // within a thousandth of a frame): it is kept as written, not moved onto the frame time.
    for start in [0.1f64, 0.123456789] {
        assert!((start * 24.0 - (start * 24.0).round()).abs() > 1e-2, "{start}");
    }
    assert_eq!(rows[2].start, "1.0416666");
    // The digits of the document are the printed digits, for every time of up to fifteen significant
    // digits (the number written by `serde_json` is the shortest that reads back as the same number; a
    // time with a small exponent is written in JSON's exponent form).
    let clips = doc["clips"].as_array().unwrap();
    let text_of = |value: &Value| {
        let written = value.to_string();
        let read: f64 = written.parse().unwrap();
        // Normalise the exponent form to plain digits for the comparison of digits.
        format!("{read:.9}").trim_end_matches('0').trim_end_matches('.').to_string()
    };
    for (clip, row) in clips.iter().zip(&rows) {
        assert_eq!(text_of(&clip["start"]), row.start);
        assert_eq!(text_of(&clip["end"]), row.end);
        assert_eq!(text_of(&clip["duration"]), row.duration);
    }
    // The ordinary ones are written plainly.
    assert_eq!(clips[0]["end"].to_string(), "0.4333");
    assert_eq!(clips[2]["duration"].to_string(), "0.9583334");
}

#[test]
fn s2_t009_c3_the_largest_times_keep_every_printed_digit_in_the_text_and_the_same_number_in_the_document() {
    let dir = extreme("s2_t009_c3_extreme");
    let told = timeline_text(&dir);
    let json = timeline_json(&dir);
    assert_eq!((told.code, json.code), (0, 0), "{}{}", told.all(), json.all());
    let doc = one_document(&json, "extreme");
    // The text form prints every digit: the end has nineteen significant digits.
    let blocks: String = doc["diagnostics"].as_array().unwrap().iter().map(block).collect();
    assert_eq!(doc["diagnostics"].as_array().unwrap().len(), 1, "{doc}");
    let text_form = told.stdout.strip_prefix(blocks.as_str()).expect("the warning comes first");
    assert!(
        text_form.contains("999999999.999999999 s to 1999999999.999999998 s  999999999.999999999 s"),
        "{text_form}"
    );
    let one_space = text_form.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        one_space.contains("123456789.123456789 s to 123456789.12345679 s 0.000000001 s"),
        "{text_form}"
    );
    // The document holds the same number as the printed text read as a number, for every time.
    let line = json.stdout.trim_end();
    assert_eq!(disagreements(line, text_form), Vec::<String>::new());
    let ends = clip_numbers(line, "end");
    assert_eq!(ends[0].parse::<f64>().unwrap(), "1999999999.999999998".parse::<f64>().unwrap());
    // A 64-bit number cannot hold all nineteen digits: that is the one place where the digits differ,
    // and what is kept is the nearest number (here a whole 2000000000).
    assert_ne!(ends[0], "1999999999.999999998");
    assert_eq!(ends[0], "2000000000");
}

/// What a written time with too many decimals (or digits) does: it is refused by the reader, so a time
/// the text form would round can not be reached.
#[test]
fn s2_t009_c3_a_time_with_more_than_nine_decimals_cannot_be_written() {
    let tween = |at: &str| {
        motion(&format!(
            "{{ \"target\": \"#c0\", \"to\": {{ \"x\": 10 }}, \"at\": {at}, \"duration\": 0.5 }}"
        ))
    };
    let cases: Vec<(&str, String, &str)> = vec![
        (
            "start",
            page(
                "data-fs-size=\"64x36\" data-fs-fps=\"30\" data-fs-duration=\"2\"",
                "",
                "<div id=\"c0\" data-fs-clip data-fs-start=\"0.1234567891\" data-fs-duration=\"1\"></div>",
                "",
            ),
            "FS-E021",
        ),
        (
            "duration",
            page(
                "data-fs-size=\"64x36\" data-fs-fps=\"30\" data-fs-duration=\"2\"",
                "",
                "<div id=\"c0\" data-fs-clip data-fs-start=\"0\" data-fs-duration=\"0.1234567891\"></div>",
                "",
            ),
            "FS-E023",
        ),
        (
            "length",
            page(
                "data-fs-size=\"64x36\" data-fs-fps=\"30\" data-fs-duration=\"1.0000000001\"",
                "",
                "",
                "",
            ),
            "FS-E009",
        ),
        (
            "motion time",
            page(
                "data-fs-size=\"64x36\" data-fs-fps=\"30\"",
                "",
                "<div id=\"c0\" data-fs-clip data-fs-start=\"0\" data-fs-duration=\"1\"></div>",
                &tween("0.1234567891"),
            ),
            "FS-E045",
        ),
        (
            "ten whole digits",
            page(
                "data-fs-size=\"64x36\" data-fs-fps=\"30\" data-fs-duration=\"2\"",
                "",
                "<div id=\"c0\" data-fs-clip data-fs-start=\"1000000000\" data-fs-duration=\"1\"></div>",
                "",
            ),
            "FS-E021",
        ),
        (
            "an exponent",
            page(
                "data-fs-size=\"64x36\" data-fs-fps=\"30\" data-fs-duration=\"2\"",
                "",
                "<div id=\"c0\" data-fs-clip data-fs-start=\"1e-10\" data-fs-duration=\"1\"></div>",
                "",
            ),
            "FS-E021",
        ),
    ];
    for (number, (what, index, code)) in cases.iter().enumerate() {
        let dir = project(&format!("s2_t009_c3_toolong_{number}"), "known", index);
        let json = timeline_json(&dir);
        assert_eq!(json.code, 1, "{what}: {}", json.all());
        let doc = one_document(&json, what);
        assert_eq!(doc["outcome"], "failed", "{what}");
        assert!(
            doc["diagnostics"].as_array().unwrap().iter().any(|d| d["code"] == *code),
            "{what}: {doc}"
        );
        assert!(doc.get("clips").is_none(), "{what}");
    }
    // Nine decimals is the most that is accepted, and the text form prints it whole.
    let dir = project(
        "s2_t009_c3_nine",
        "known",
        &page(
            "data-fs-size=\"64x36\" data-fs-fps=\"30\" data-fs-duration=\"2\"",
            "",
            "<div id=\"c0\" data-fs-clip data-fs-start=\"0.123456789\" data-fs-duration=\"0.987654321\"></div>",
            "",
        ),
    );
    let doc = assert_document_equals_text(&dir, "nine decimals", 0);
    assert_eq!(doc["clips"][0]["end"].to_string(), "1.11111111");
    assert!(timeline_text(&dir).stdout.contains("0.123456789 s to 1.11111111 s  0.987654321 s"));
}

#[test]
fn s2_t009_c3_the_comparison_sees_a_time_that_is_wrong() {
    let dir = BETWEEN.make("s2_t009_c3_control");
    let ran = timeline_json(&dir);
    let doc = one_document(&ran, "control");
    let line = ran.stdout.trim_end().to_string();
    let printed = timeline_text(&dir).stdout;
    // The real pair agrees.
    assert_eq!(disagreements(&line, &printed), Vec::<String>::new());

    // A printed time that is wrong by one digit at the ninth decimal.
    let wrong_text = printed.replace("0.12345679 s", "0.12345678 s");
    assert_ne!(wrong_text, printed);
    let found = disagreements(&line, &wrong_text);
    assert!(found.iter().any(|f| f.contains("clip 1: end")), "{found:?}");

    // A printed start that is wrong by a hair.
    let wrong_start = printed.replace("  #c0  0.1 s", "  #c0  0.100001 s");
    assert_ne!(wrong_start, printed);
    let found = disagreements(&line, &wrong_start);
    assert!(found.iter().any(|f| f.contains("clip 0: start")), "{found:?}");

    // A time of the document that is one step of a 64-bit number off.
    let end = doc["clips"][0]["end"].as_f64().unwrap();
    let nudged = format!("{}", f64::from_bits(end.to_bits() + 1));
    let wrong_line = line.replace("\"end\":0.4333", &format!("\"end\":{nudged}"));
    assert_ne!(wrong_line, line);
    let found = disagreements(&wrong_line, &printed);
    assert!(found.iter().any(|f| f.contains("clip 0: end")), "{found:?}");

    // A whole number written with a decimal point in the document, a mark that is the wrong way round,
    // a clip missing, a header fact that is wrong.
    let wrong_line = line.replace("\"end\":2,", "\"end\":2.0,");
    assert_ne!(wrong_line, line);
    assert!(disagreements(&wrong_line, &printed).iter().any(|f| f.contains("not written like")));
    let wrong_line = line.replace("\"duration_filled_in\":true", "\"duration_filled_in\":false");
    assert_ne!(wrong_line, line);
    assert!(disagreements(&wrong_line, &printed).iter().any(|f| f.contains("mark")));
    let mut wrong_doc = doc.clone();
    wrong_doc["clips"].as_array_mut().unwrap().pop();
    assert!(disagreements(&wrong_doc.to_string(), &printed)
        .iter()
        .any(|f| f.contains("clip lines")));
    let wrong_line = line.replace("\"frames\":48", "\"frames\":47");
    assert_ne!(wrong_line, line);
    assert!(disagreements(&wrong_line, &printed).iter().any(|f| f.contains("frames")));
    let wrong_line = line.replace("\"duration\":2,", "\"duration\":2.5,");
    assert_ne!(wrong_line, line);
    assert!(disagreements(&wrong_line, &printed).iter().any(|f| f.contains("length")));
}

#[test]
fn s2_t009_c3_nothing_but_the_document_is_on_standard_output_and_standard_error_is_empty() {
    let mut runs: Vec<(String, PathBuf, i32)> = passing("s2_t009_c3_runs")
        .into_iter()
        .map(|(name, dir, _)| (name, dir, 0))
        .collect();
    runs.push((
        "error".into(),
        project(
            "s2_t009_c3_runs_error",
            "faulty",
            &page("data-fs-size=\"65x36\" data-fs-duration=\"1\"", "", "", ""),
        ),
        1,
    ));
    runs.push((
        "unsupported".into(),
        fixture("s2_t009_c3_runs_u", "unsupported/fs-u001-video-element"),
        2,
    ));
    runs.push((
        "unknown".into(),
        fixture("s2_t009_c3_runs_x", "unsupported/fs-x001-unknown-attribute"),
        2,
    ));
    runs.push(("missing".into(), scratch("s2_t009_c3_runs_missing").join("nowhere"), 1));
    for (name, dir, code) in runs {
        let ran = timeline_json(&dir);
        assert_eq!(ran.code, code, "{name}: {}", ran.all());
        let doc = one_document(&ran, &name);
        assert!(doc["outcome"].is_string(), "{name}");
        assert_eq!(ran.stderr, "", "{name}");
        if dir.exists() {
            let before = tree(&dir);
            timeline_json(&dir);
            assert_eq!(tree(&dir), before, "{name}");
            assert!(!dir.join("out").exists(), "{name}");
        }
    }
}

#[test]
fn s2_t009_c3_the_run_changes_no_file_and_the_modification_times_stay() {
    let dir = LENGTH_WORKED_OUT.make("s2_t009_c3_unchanged");
    let root = dir.parent().unwrap();
    let (files, all, times, beside) = (tree(&dir), entries(&dir), modified(&dir), entries(root));
    timeline_json(&dir);
    assert_eq!(tree(&dir), files);
    assert_eq!(entries(&dir), all);
    assert_eq!(modified(&dir), times);
    assert_eq!(entries(root), beside);
}
