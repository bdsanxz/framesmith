//! Sprint 2, ticket T007: `framesmith info <project> --json` gives the facts of a project, or its
//! diagnostics, as exactly one JSON document, and nothing else on standard output.
//!
//! Notes for maintainers
//! ---------------------
//! * The JSON and the text form are made from one set of facts (`src/facts.rs`). These tests still
//!   read both outputs of the built binary and compare them value by value, so a change to either
//!   form is caught here.
//! * The test projects are small and written for this file. The values they must give are stated in
//!   the `Known` table; every test reads its expectations from there.
//! * A clip that starts at or after the end of the composition is the warning FS-W001 and the project
//!   still passes: `warned()` makes such a project, and the warning test and the c3 runs use it.
//! * Scratch folders: every test gives each project a scratch key of its own (tests of a file run in
//!   parallel, and a key names a folder that is emptied when it is made). The helpers that make projects
//!   take the key from the test.
//! * None of these tests needs a browser or `ffmpeg`: `info` starts neither.

mod common;

use common::*;
use measure::Ran;
use serde_json::Value;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A project of this file: how it is written and what `info --json` is to say about it.
struct Known {
    name: &'static str,
    attrs: &'static str,
    /// The clips as (start, duration written), in seconds.
    clips: &'static [(&'static str, Option<&'static str>)],
    tweens: &'static str,
    width: u64,
    height: u64,
    fps: u64,
    fps_filled_in: bool,
    duration: f64,
    duration_filled_in: bool,
    frames: u64,
    clip_count: u64,
    tween_count: u64,
    /// Every clip as (label, duration, filled in), in document order.
    clip_durations: &'static [(&'static str, f64, bool)],
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
    clip_count: 2,
    tween_count: 1,
    clip_durations: &[("#c0", 0.5, false), ("#c1", 0.5, false)],
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
    clip_count: 2,
    tween_count: 1,
    clip_durations: &[("#c0", 0.5, false), ("#c1", 0.5, false)],
};

/// The length is left out and worked out from the clips and the tweens; a clip with no duration runs
/// to the end.
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
    clip_count: 2,
    tween_count: 1,
    clip_durations: &[("#c0", 0.5, false), ("#c1", 1.5, true)],
};

const ALL: [&Known; 3] = [&WRITTEN, &RATE_FILLED, &LENGTH_WORKED_OUT];

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

/// A project that passes with two warnings: a clip that starts at or after the end of the composition
/// (the length is written as 2 s) never shows (FS-W001).
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

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

fn info_json(dir: &Path) -> Ran {
    framesmith(&["info", text(dir), "--json"])
}

fn info_text(dir: &Path) -> Ran {
    framesmith(&["info", text(dir)])
}

fn check(dir: &Path) -> Ran {
    framesmith(&["check", text(dir)])
}

/// Standard output as exactly one JSON document: one line, ended by one line feed, that parses whole
/// (a parser that refuses anything after the document), that holds no control character other than that
/// line feed, and that is what the parsed document serialises to, byte for byte.
fn one_document(ran: &Ran, what: &str) -> Value {
    let out = &ran.stdout;
    let line = out
        .strip_suffix('\n')
        .unwrap_or_else(|| panic!("{what}: no final line feed: {out:?}"));
    assert!(!line.contains('\n') && !line.is_empty(), "{what}: not one line: {out:?}");
    let value: Value = serde_json::from_str(line).unwrap_or_else(|e| panic!("{what}: not one document ({e}): {out:?}"));
    assert!(value.is_object(), "{what}: {out:?}");
    assert_eq!(serde_json::to_string(&value).unwrap(), line, "{what}: bytes outside the document");
    let controls: Vec<u8> = out.bytes().filter(|&b| (b < 0x20 && b != b'\n') || b == 0x7f).collect();
    assert_eq!(controls, Vec::<u8>::new(), "{what}: a raw control character in {out:?}");
    assert_eq!(out.matches('\n').count(), 1, "{what}");
    value
}

/// Like `Ran`, but the output as raw bytes: the colour switches set, `NO_COLOR` taken out.
fn json_forcing_colour(dir: &Path) -> Ran {
    let output = Command::new(BIN)
        .args(["info", text(dir), "--json"])
        .current_dir(dir.parent().unwrap())
        .env("CLICOLOR_FORCE", "1")
        .env("FORCE_COLOR", "1")
        .env("TERM", "xterm-256color")
        .env_remove("NO_COLOR")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    Ran {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8(output.stdout).expect("standard output is UTF-8"),
        stderr: String::from_utf8(output.stderr).expect("standard error is UTF-8"),
    }
}

/// The rows of the text form as label -> value, and its `clip` lines.
fn read_facts(printout: &str) -> (BTreeMap<String, String>, Vec<String>) {
    let (mut rows, mut clips) = (BTreeMap::new(), Vec::new());
    for line in printout.lines() {
        let line = line.trim_start();
        if line.starts_with("clip ") {
            clips.push(line.to_string());
        } else if line.len() > 13 && line.is_char_boundary(13) && line[..13].ends_with(' ') {
            rows.insert(line[..13].trim_end().to_string(), line[13..].to_string());
        }
    }
    (rows, clips)
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

fn keys(value: &Value) -> Vec<&str> {
    value.as_object().unwrap().keys().map(String::as_str).collect()
}

fn same(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

const PASS_KEYS: [&str; 17] = [
    "clip_durations",
    "clips",
    "composition",
    "compositions",
    "diagnostics",
    "duration",
    "duration_filled_in",
    "fps",
    "fps_filled_in",
    "frames",
    "height",
    "outcome",
    "output",
    "project",
    "run_id",
    "tweens",
    "width",
];

/// Everything the document of a project that passes holds, against the text form of the same project
/// (parsed) and against the known values (if given).
fn assert_document_equals_text(dir: &Path, known: Option<&Known>, what: &str) {
    assert_document_equals_text_with(dir, known, what, 0);
}

/// The same, for a project that passes with `warnings` warnings: the text form prints their blocks before
/// the facts, and the document carries them in `diagnostics`.
fn assert_document_equals_text_with(dir: &Path, known: Option<&Known>, what: &str, warnings: usize) {
    let json = info_json(dir);
    let told = info_text(dir);
    assert_eq!((json.code, told.code), (0, 0), "{what}: {}{}", json.all(), told.all());
    assert!(json.stderr.is_empty(), "{what}: {}", json.stderr);
    let doc = one_document(&json, what);
    assert_eq!(keys(&doc), PASS_KEYS, "{what}");
    let blocks: String = doc["diagnostics"].as_array().unwrap().iter().map(block).collect();
    assert_eq!(doc["diagnostics"].as_array().unwrap().len(), warnings, "{what}");
    assert!(told.stdout.starts_with(&blocks), "{what}: {}", told.stdout);
    let (rows, clip_lines) = read_facts(&told.stdout[blocks.len()..]);

    // Types: numbers are numbers, flags are flags, the rest is text.
    for name in ["width", "height", "fps", "frames", "compositions", "clips", "tweens"] {
        assert!(doc[name].is_u64(), "{what}: {name} is not a whole number: {}", doc[name]);
    }
    assert!(doc["duration"].is_number(), "{what}: {}", doc["duration"]);
    for name in ["fps_filled_in", "duration_filled_in"] {
        assert!(doc[name].is_boolean(), "{what}: {name}");
    }
    for name in ["outcome", "run_id", "project", "composition", "output"] {
        assert!(doc[name].is_string(), "{what}: {name}");
    }
    assert_eq!(doc["outcome"], "passed", "{what}");

    // Each fact of the text form, field by field.
    assert_eq!(doc["project"].as_str().unwrap(), rows["project"], "{what}: project");
    assert_eq!(doc["composition"].as_str().unwrap(), rows["composition"], "{what}: composition");
    assert_eq!(format!("{}x{}", doc["width"], doc["height"]), rows["size"], "{what}: size");
    let (rate, rate_filled) = number_and_mark(&rows["rate"], " fps");
    assert_eq!(doc["fps"].as_u64().unwrap() as f64, rate, "{what}: rate");
    assert_eq!(doc["fps_filled_in"].as_bool().unwrap(), rate_filled, "{what}: rate mark");
    let (length, length_filled) = number_and_mark(&rows["length"], " s");
    assert!(
        same(doc["duration"].as_f64().unwrap(), length),
        "{what}: length {} / {length}",
        doc["duration"]
    );
    assert_eq!(doc["duration_filled_in"].as_bool().unwrap(), length_filled, "{what}: length mark");
    for (label, name) in [
        ("frames", "frames"),
        ("compositions", "compositions"),
        ("clips", "clips"),
        ("tweens", "tweens"),
    ] {
        assert_eq!(doc[name].to_string(), rows[label], "{what}: {label}");
    }
    assert_eq!(doc["output"].as_str().unwrap(), rows["output"], "{what}: output");

    // The `clip` lines of the text form are the entries of `clip_durations` that are flagged.
    let list = doc["clip_durations"].as_array().unwrap();
    assert_eq!(list.len() as u64, doc["clips"].as_u64().unwrap(), "{what}: one entry for each clip");
    let mut flagged = Vec::new();
    for entry in list {
        assert_eq!(keys(entry), ["duration", "duration_filled_in", "label"], "{what}");
        assert!(entry["label"].is_string() && entry["duration"].is_number() && entry["duration_filled_in"].is_boolean());
        if entry["duration_filled_in"].as_bool().unwrap() {
            flagged.push((entry["label"].as_str().unwrap().to_string(), entry["duration"].as_f64().unwrap()));
        }
    }
    let mut lines = Vec::new();
    for line in &clip_lines {
        let rest = line.strip_prefix("clip ").unwrap();
        let (label, duration) = rest.split_once("  duration ").unwrap();
        let (seconds, filled) = number_and_mark(duration, " s");
        assert!(filled, "{what}: {line}");
        lines.push((label.to_string(), seconds));
    }
    assert_eq!(flagged.len(), lines.len(), "{what}: {flagged:?} / {lines:?}");
    for ((a, x), (b, y)) in flagged.iter().zip(&lines) {
        assert!(a == b && same(*x, *y), "{what}: {flagged:?} / {lines:?}");
    }

    if let Some(known) = known {
        assert_eq!(doc["width"], known.width, "{what}");
        assert_eq!(doc["height"], known.height, "{what}");
        assert_eq!(doc["fps"], known.fps, "{what}");
        assert_eq!(doc["fps_filled_in"], known.fps_filled_in, "{what}");
        assert!(
            same(doc["duration"].as_f64().unwrap(), known.duration),
            "{what}: {}",
            doc["duration"]
        );
        if known.duration.fract() == 0.0 {
            // A whole number of seconds has no decimal point.
            assert!(doc["duration"].is_u64(), "{what}: {}", doc["duration"]);
        }
        assert_eq!(doc["duration_filled_in"], known.duration_filled_in, "{what}");
        assert_eq!(doc["frames"], known.frames, "{what}");
        assert_eq!(doc["compositions"], 1, "{what}");
        assert_eq!(doc["clips"], known.clip_count, "{what}");
        assert_eq!(doc["tweens"], known.tween_count, "{what}");
        assert_eq!(doc["composition"], "main", "{what}");
        let expected: Vec<(String, f64, bool)> = known
            .clip_durations
            .iter()
            .map(|(label, duration, filled)| (label.to_string(), *duration, *filled))
            .collect();
        let got: Vec<(String, f64, bool)> = list
            .iter()
            .map(|e| {
                (
                    e["label"].as_str().unwrap().to_string(),
                    e["duration"].as_f64().unwrap(),
                    e["duration_filled_in"].as_bool().unwrap(),
                )
            })
            .collect();
        assert_eq!(got, expected, "{what}: clip_durations");
        assert_eq!(
            doc["output"].as_str().unwrap(),
            dir.join("out/known.mp4").display().to_string(),
            "{what}"
        );
        assert_eq!(doc["project"].as_str().unwrap(), dir.display().to_string(), "{what}");
    }
}

// ---------------------------------------------------------------------
// c1
// ---------------------------------------------------------------------

#[test]
fn s2_t007_c1_a_valid_project_gives_one_document_with_every_fact_of_the_text_form_and_exit_0() {
    for known in ALL {
        let dir = known.make(&format!("s2_t007_c1_{}", known.name));
        assert_document_equals_text(&dir, Some(known), known.name);
    }
    // Fixtures of the repository: one with everything written, one with a rate and a length left out.
    for name in ["title-card", "title-card-unwritten"] {
        assert_document_equals_text(&fixture(&format!("s2_t007_c1_{name}"), name), None, name);
    }
}

#[test]
fn s2_t007_c1_the_filled_in_flags_are_right_in_both_directions() {
    let flags = |known: &Known| -> (bool, bool, Vec<bool>) {
        let dir = known.make(&format!("s2_t007_c1_flags_{}", known.name));
        let ran = info_json(&dir);
        assert_eq!(ran.code, 0, "{}", ran.all());
        let doc = one_document(&ran, known.name);
        (
            doc["fps_filled_in"].as_bool().unwrap(),
            doc["duration_filled_in"].as_bool().unwrap(),
            doc["clip_durations"]
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
    // A rate written as 30 is written: it is not flagged.
    let dir = project(
        "s2_t007_c1_thirty",
        "known",
        &page("data-fs-size=\"64x36\" data-fs-fps=\"30\" data-fs-duration=\"1\"", "", "", ""),
    );
    let doc = one_document(&info_json(&dir), "thirty");
    assert_eq!((doc["fps"].as_u64(), doc["fps_filled_in"].as_bool()), (Some(30), Some(false)));
    // Not a mark inside a string: the values are bare numbers.
    assert!(doc["fps"].is_u64() && doc["duration"].is_u64());
}

#[test]
fn s2_t007_c1_a_length_that_is_not_whole_is_a_decimal_number() {
    let dir = project(
        "s2_t007_c1_fraction",
        "known",
        &page("data-fs-size=\"64x36\" data-fs-fps=\"20\" data-fs-duration=\"1.25\"", "", "", ""),
    );
    let doc = one_document(&info_json(&dir), "fraction");
    assert_eq!(doc["duration"], serde_json::json!(1.25));
    assert_eq!(doc["frames"], 25);
}

#[test]
fn s2_t007_c1_the_option_stands_before_or_after_the_folder_and_the_text_form_is_unchanged() {
    let dir = LENGTH_WORKED_OUT.make("s2_t007_c1_order");
    let after = one_document(&info_json(&dir), "after");
    let before_ran = framesmith(&["info", "--json", text(&dir)]);
    let before = one_document(&before_ran, "before");
    let twice = one_document(&framesmith(&["info", text(&dir), "--json", "--json"]), "twice");
    let strip = |mut doc: Value| {
        doc.as_object_mut().unwrap().remove("run_id");
        doc
    };
    assert_eq!(strip(before.clone()), strip(after.clone()));
    assert_eq!(strip(twice), strip(after));
    assert_eq!(before_ran.code, 0);
    // Without the option the text form is what it was: not a document.
    let told = info_text(&dir);
    assert_eq!(told.code, 0);
    assert!(
        told.stdout.starts_with("project      ") && !told.stdout.contains('{'),
        "{}",
        told.stdout
    );
    assert!(told.stdout.contains("length       2 s (filled in)\n"), "{}", told.stdout);
    // The relative form of the folder gives the full path in the document, as in the text form.
    let parent = std::fs::canonicalize(dir.parent().unwrap()).unwrap();
    let ran = run_in(&parent, &["info", "known", "--json"], &[]);
    let doc = one_document(&ran, "relative");
    assert_eq!(doc["project"].as_str().unwrap(), parent.join("known").display().to_string());
    assert_eq!(
        doc["output"].as_str().unwrap(),
        parent.join("known/out/known.mp4").display().to_string()
    );
}

#[test]
fn s2_t007_c1_the_command_is_logged_with_json_asked_for_and_the_document_carries_the_run_id() {
    let dir = RATE_FILLED.make("s2_t007_c1_log");
    let logs = scratch("s2_t007_c1_log_folder");
    let env = [("FRAMESMITH_LOG_DIR", logs.as_os_str()), ("FRAMESMITH_LOG", OsStr::new("info"))];
    let ran = run_in(dir.parent().unwrap(), &["info", text(&dir), "--json"], &env);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let doc = one_document(&ran, "logged");
    let log = std::fs::read_to_string(logs.join("framesmith.log")).unwrap();
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 2, "{log}");
    assert!(
        lines[0].contains("command started") && lines[0].contains("command=\"info\""),
        "{log}"
    );
    assert!(lines[0].contains("json=true"), "{log}");
    assert!(lines[1].contains("command finished") && lines[1].contains("exit=0"), "{log}");
    let run = |line: &str| line.split_whitespace().find(|word| word.starts_with("run=")).map(String::from);
    assert!(run(lines[0]).is_some() && run(lines[0]) == run(lines[1]), "{log}");
    assert_eq!(run(lines[0]).unwrap(), format!("run={}", doc["run_id"].as_str().unwrap()), "{log}");
    assert!(!log.contains("data-fs"), "{log}");
    // The text form says it did not ask for the document.
    let logs = scratch("s2_t007_c1_log_text");
    let env = [("FRAMESMITH_LOG_DIR", logs.as_os_str()), ("FRAMESMITH_LOG", OsStr::new("info"))];
    run_in(dir.parent().unwrap(), &["info", text(&dir)], &env);
    let log = std::fs::read_to_string(logs.join("framesmith.log")).unwrap();
    assert!(log.lines().next().unwrap().contains("json=false"), "{log}");
}

#[test]
fn s2_t007_c1_a_warning_on_a_passing_project_is_carried_and_printed_before_the_facts() {
    let dir = warned("s2_t007_c1_warning");
    let json = info_json(&dir);
    let told = info_text(&dir);
    let checked = check(&dir);
    assert_eq!((json.code, told.code, checked.code), (0, 0, 0), "{}{}", json.all(), told.all());
    assert_eq!(json.stderr, "");
    let doc = one_document(&json, "warning");
    assert_eq!(doc["outcome"], "passed");
    assert_eq!(keys(&doc), PASS_KEYS);
    let list = doc["diagnostics"].as_array().unwrap();
    assert_eq!(list.len(), 2, "{doc}");
    for d in list {
        assert_eq!(keys(d), ["code", "description", "file", "kind", "line", "remedy"]);
        assert_eq!(
            (d["code"].as_str(), d["kind"].as_str(), d["file"].as_str()),
            (Some("FS-W001"), Some("warning"), Some("index.html"))
        );
        assert!(d["line"].as_u64().unwrap() > 0);
        assert!(!d["description"].as_str().unwrap().is_empty() && !d["remedy"].as_str().unwrap().is_empty());
    }
    assert!(list[0]["description"].as_str().unwrap().contains("#late"), "{doc}");
    assert!(list[1]["description"].as_str().unwrap().contains("#later"), "{doc}");
    // The text form of `info` prints the same warnings, text for text, before the facts; `check` prints
    // them before its result and facts.
    let blocks: String = list.iter().map(block).collect();
    assert!(told.stdout.starts_with(&format!("{blocks}project      ")), "{}", told.stdout);
    assert!(checked.stdout.starts_with(&format!("{blocks}check passed\n")), "{}", checked.stdout);
    // The facts after the warnings equal those of `check`, and the document equals the text form.
    let (told_rows, told_clips) = read_facts(&told.stdout[blocks.len()..]);
    let (checked_rows, checked_clips) = read_facts(&checked.stdout[blocks.len() + "check passed\n".len()..]);
    assert_eq!(checked_rows.len(), 7, "{checked_rows:?}");
    for (label, value) in &checked_rows {
        assert_eq!(told_rows.get(label), Some(value), "{label}");
    }
    assert_eq!(told_clips, checked_clips);
    assert_document_equals_text_with(&dir, None, "warning", 2);
    // A written value is not flagged, whatever the warnings.
    assert_eq!(
        (doc["fps_filled_in"].as_bool(), doc["duration_filled_in"].as_bool()),
        (Some(false), Some(false))
    );
}

// ---------------------------------------------------------------------
// c2
// ---------------------------------------------------------------------

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

/// The document of a project that does not pass, against `check` and the text form of `info`.
/// Returns the document.
fn assert_failure_equals_text(dir: &Path, code: i32, what: &str) -> Value {
    let json = info_json(dir);
    let told = info_text(dir);
    let checked = check(dir);
    assert_eq!(checked.code, code, "{what}: {}", checked.all());
    assert_eq!((json.code, told.code), (code, code), "{what}: {}{}", json.all(), told.all());
    assert!(json.stderr.is_empty(), "{what}: {}", json.stderr);
    let doc = one_document(&json, what);
    assert_eq!(keys(&doc), ["diagnostics", "outcome", "run_id"], "{what}");
    assert_eq!(doc["outcome"], if code == 2 { "refused" } else { "failed" }, "{what}");
    assert!(doc["run_id"].is_string());
    let list = doc["diagnostics"].as_array().unwrap();
    assert!(!list.is_empty(), "{what}");
    let kinds = ["error", "unsupported", "unknown", "warning"];
    for d in list {
        assert_eq!(keys(d), ["code", "description", "file", "kind", "line", "remedy"], "{what}");
        assert!(d["code"].as_str().unwrap().starts_with("FS-"), "{what}");
        assert!(kinds.contains(&d["kind"].as_str().unwrap()), "{what}");
        assert!(d["file"].is_string() && d["line"].is_u64(), "{what}");
        for text in ["description", "remedy"] {
            assert!(!d[text].as_str().unwrap().is_empty(), "{what}: {text}");
        }
    }
    // Each diagnostic is, text for text, what `check` and the text form of `info` print; and the
    // summary line follows from the same list.
    let blocks: String = list.iter().map(block).collect();
    let count = |kind: &str| list.iter().filter(|d| d["kind"] == kind).count();
    let summary = format!(
        "check {}: {} {}, {} unsupported, {} unknown, {} {}\n",
        if code == 2 { "refused" } else { "failed" },
        count("error"),
        if count("error") == 1 { "error" } else { "errors" },
        count("unsupported"),
        count("unknown"),
        count("warning"),
        if count("warning") == 1 { "warning" } else { "warnings" },
    );
    assert_eq!(checked.stdout, format!("{blocks}{summary}"), "{what}");
    assert_eq!(told.stdout, checked.stdout, "{what}");
    // The facts of a project that passes are not in the document.
    for name in ["project", "output", "fps", "width", "frames"] {
        assert!(doc.get(name).is_none(), "{what}: {name}");
    }
    doc
}

#[test]
fn s2_t007_c2_a_project_with_an_error_gives_one_document_with_its_diagnostic_and_exit_1() {
    let dir = project(
        "s2_t007_c2_error",
        "faulty",
        &page("data-fs-size=\"65x36\" data-fs-duration=\"1\"", "", "", ""),
    );
    let doc = assert_failure_equals_text(&dir, 1, "an error");
    let list = doc["diagnostics"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["code"], "FS-E007");
    assert_eq!(list[0]["kind"], "error");
    assert_eq!(list[0]["file"], "index.html");
    assert!(list[0]["line"].as_u64().unwrap() > 0);
    assert!(list[0]["description"].as_str().unwrap().contains("65"));
    assert!(list[0]["remedy"].as_str().unwrap().contains("even"));
}

#[test]
fn s2_t007_c2_a_project_with_several_errors_gives_them_all_in_the_order_of_check() {
    let dir = project(
        "s2_t007_c2_errors",
        "faulty",
        &page("data-fs-size=\"65x36\" data-fs-fps=\"23.976\" data-fs-duration=\"-1\"", "", "", ""),
    );
    let doc = assert_failure_equals_text(&dir, 1, "three errors");
    let list = doc["diagnostics"].as_array().unwrap();
    assert_eq!(list.len(), 3);
    assert!(list.iter().all(|d| d["kind"] == "error"));
    let mut codes: Vec<&str> = list.iter().map(|d| d["code"].as_str().unwrap()).collect();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), 3, "{codes:?}");
}

#[test]
fn s2_t007_c2_a_folder_that_does_not_exist_or_has_no_page_gives_a_document_and_exit_1() {
    let root = scratch("s2_t007_c2_places");
    let doc = assert_failure_equals_text(&root.join("nowhere"), 1, "a missing folder");
    assert_eq!(doc["diagnostics"][0]["code"], "FS-E001");
    assert_eq!(doc["diagnostics"][0]["line"], 0);
    std::fs::create_dir_all(root.join("bare")).unwrap();
    write(&root, "bare/notes.txt", "no page here");
    let doc = assert_failure_equals_text(&root.join("bare"), 1, "a folder with no page");
    assert_eq!(doc["diagnostics"][0]["code"], "FS-E002");
}

#[test]
fn s2_t007_c2_an_unsupported_feature_gives_a_document_that_is_refused_with_exit_2() {
    for (name, code) in [
        ("unsupported/fs-u001-video-element", "FS-U001"),
        ("unsupported/fs-u014-css-animation-in-stylesheet", "FS-U014"),
        ("unsupported/fs-u009-network-image", "FS-U009"),
    ] {
        let dir = fixture(&format!("s2_t007_c2_{code}"), name);
        let doc = assert_failure_equals_text(&dir, 2, name);
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
fn s2_t007_c2_an_unknown_attribute_gives_a_document_that_is_refused_with_exit_2() {
    let dir = fixture("s2_t007_c2_unknown", "unsupported/fs-x001-unknown-attribute");
    let doc = assert_failure_equals_text(&dir, 2, "an unknown attribute");
    assert!(doc["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["code"] == "FS-X001" && d["kind"] == "unknown"));
}

#[test]
fn s2_t007_c2_a_command_line_that_cannot_be_read_gives_a_failed_document_as_render_does() {
    let dir = WRITTEN.make("s2_t007_c2_usage");
    let message = |what: &str| format!("error FS-R004\n  {what}\n  fix: Run `framesmith --help` to see the commands and their options.\n");
    let cases: [(&[&str], &str); 5] = [
        (&["info", "--json"], "`framesmith info` needs the path of a project folder."),
        (
            &["info", "--json", "--json"],
            "`framesmith info` needs the path of a project folder.",
        ),
        (&["info", "--bogus", "--json"], "`framesmith info` has no option --bogus."),
        (
            &["info", "known", "--output", "x.mp4", "--json"],
            "`framesmith info` takes only the path of a project folder; --output is one word too many.",
        ),
        (
            &["info", "known", "second", "--json"],
            "`framesmith info` takes only the path of a project folder; second is one word too many.",
        ),
    ];
    for (args, expected) in cases {
        let ran = run_in(dir.parent().unwrap(), args, &[]);
        assert_eq!(ran.code, 1, "{args:?}: {}", ran.all());
        let doc = one_document(&ran, &format!("{args:?}"));
        assert_eq!(keys(&doc), ["error", "outcome", "run_id"], "{args:?}");
        assert_eq!(doc["outcome"], "failed");
        assert_eq!(keys(&doc["error"]), ["code", "description", "remedy"], "{args:?}");
        assert_eq!(doc["error"]["code"], "FS-R004");
        assert_eq!(doc["error"]["description"], expected);
        // The same error, in words, on standard error: the one case where it is not empty.
        assert_eq!(ran.stderr, message(expected), "{args:?}");
    }
    // The same command line in `render --json` gets the same shape.
    let render = run_in(dir.parent().unwrap(), &["render", "--bogus", "--json"], &[]);
    let render_doc = one_document(&render, "render");
    assert_eq!(keys(&render_doc), ["error", "outcome", "run_id"]);
    assert_eq!(keys(&render_doc["error"]), ["code", "description", "remedy"]);
    assert_eq!(render_doc["error"]["remedy"], doc_remedy());
    // Without the option: standard output stays empty and the error is on standard error, as before.
    let ran = run_in(dir.parent().unwrap(), &["info", "--bogus"], &[]);
    assert_eq!((ran.code, ran.stdout.as_str()), (1, ""));
    assert!(ran.stderr.starts_with("error FS-R004\n"));
    // `check` still has no `--json`.
    let ran = run_in(dir.parent().unwrap(), &["check", "known", "--json"], &[]);
    assert_eq!((ran.code, ran.stdout.as_str()), (1, ""));
    assert!(ran.stderr.contains("one word too many"), "{}", ran.stderr);
    // Nothing was written into the project.
    assert!(!dir.join("out").exists());
}

fn doc_remedy() -> &'static str {
    "Run `framesmith --help` to see the commands and their options."
}

// ---------------------------------------------------------------------
// c3
// ---------------------------------------------------------------------

/// A project whose error message holds a quote, a tab, a backslash and a letter beyond ASCII: the
/// text form prints them raw, and the document must escape them.
fn awkward_text(test: &str) -> PathBuf {
    let dir = scratch(&format!("{test}_awkward")).join("awkward");
    write(
        &dir,
        "index.html",
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>t</title></head><body>\n\
<div data-fs-composition=\"ma&quot;in&#9;\\é\" data-fs-size=\"64x36\" data-fs-duration=\"1\">\n\
<div id=\"c0\" data-fs-clip data-fs-start=\"0\"></div>\n</div>\n</body></html>\n",
    );
    dir
}

/// Every run of this file's projects, with the exit code it must give.
fn runs(test: &str) -> Vec<(String, PathBuf, i32)> {
    let mut all = Vec::new();
    for known in ALL {
        all.push((known.name.to_string(), known.make(&format!("{test}_{}", known.name)), 0));
    }
    all.push((
        "error".to_string(),
        project(
            &format!("{test}_error"),
            "faulty",
            &page("data-fs-size=\"65x36\" data-fs-duration=\"1\"", "", "", ""),
        ),
        1,
    ));
    all.push((
        "errors".to_string(),
        project(
            &format!("{test}_errors"),
            "faulty",
            &page("data-fs-size=\"65x36\" data-fs-fps=\"23.976\" data-fs-duration=\"-1\"", "", "", ""),
        ),
        1,
    ));
    all.push((
        "unsupported".to_string(),
        fixture(&format!("{test}_u"), "unsupported/fs-u001-video-element"),
        2,
    ));
    all.push((
        "unknown".to_string(),
        fixture(&format!("{test}_x"), "unsupported/fs-x001-unknown-attribute"),
        2,
    ));
    all.push(("missing".to_string(), scratch(&format!("{test}_missing")).join("nowhere"), 1));
    all.push(("awkward".to_string(), awkward_text(test), 1));
    all.push(("warning".to_string(), warned(&format!("{test}_warning")), 0));
    all
}

#[test]
fn s2_t007_c3_nothing_but_the_document_is_on_standard_output_and_standard_error_is_empty() {
    for (name, dir, code) in runs("s2_t007_c3_all") {
        let ran = info_json(&dir);
        assert_eq!(ran.code, code, "{name}: {}", ran.all());
        // One line, one document, no byte outside it, no raw control character.
        let doc = one_document(&ran, &name);
        assert!(doc["outcome"].is_string(), "{name}");
        // Standard error holds nothing in a normal run, whatever the outcome of the project.
        assert_eq!(ran.stderr, "", "{name}");
    }
}

#[test]
fn s2_t007_c3_the_colour_switches_change_nothing() {
    let strip = |mut doc: Value| {
        doc.as_object_mut().unwrap().remove("run_id");
        doc
    };
    for (name, dir, code) in runs("s2_t007_c3_colour") {
        let plain = info_json(&dir);
        let coloured = json_forcing_colour(&dir);
        assert_eq!(coloured.code, code, "{name}: {}", coloured.all());
        let (a, b) = (one_document(&plain, &name), one_document(&coloured, &name));
        assert!(!coloured.stdout.contains('\u{1b}'), "{name}: {:?}", coloured.stdout);
        assert_eq!(strip(a), strip(b), "{name}");
        assert_eq!(coloured.stderr, "", "{name}");
        // Also with the colour switches the other way round: NO_COLOR set.
        let quiet = run_in(
            dir.parent().unwrap(),
            &["info", text(&dir), "--json"],
            &[("NO_COLOR", OsStr::new("1"))],
        );
        one_document(&quiet, &name);
        assert_eq!(quiet.stderr, "", "{name}");
    }
}

#[test]
fn s2_t007_c3_characters_that_need_escaping_are_escaped_and_come_back_whole() {
    let dir = awkward_text("s2_t007_c3_escape");
    let told = info_text(&dir);
    // The text form carries the raw tab; the document must not.
    assert!(told.stdout.contains("ma\"in\t\\é"), "{:?}", told.stdout);
    let ran = info_json(&dir);
    assert_eq!(ran.code, 1);
    assert!(ran.stdout.contains("\\t") && ran.stdout.contains("\\\""), "{:?}", ran.stdout);
    let doc = one_document(&ran, "awkward");
    let description = doc["diagnostics"][0]["description"].as_str().unwrap();
    assert!(description.contains("ma\"in\t\\é"), "{description:?}");
    assert!(told.stdout.contains(description), "the same text as the text form");
}

#[test]
fn s2_t007_c3_a_document_run_changes_nothing_in_the_project() {
    for (name, dir, _) in runs("s2_t007_c3_unchanged") {
        if !dir.exists() {
            continue;
        }
        let before = tree(&dir);
        info_json(&dir);
        assert_eq!(tree(&dir), before, "{name}");
        assert!(!dir.join("out").exists(), "{name}");
    }
}
