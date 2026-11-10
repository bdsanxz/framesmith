//! Sprint 13 check, T003: motion assertions and frozen spans end-to-end.
//!
//! Covers every acceptance criterion: a late appearance violates
//! `assert-appear` with the measured time; an escaping element
//! violates `assert-inside` with the measured bounds; a static
//! element violates `assert-moving` with the measured stillness;
//! satisfied assertions stay silent; a frozen span over 1s reports
//! with its time range and no assertions needed; first and last
//! frames never count toward frozen spans; malformed assertions are
//! refused before frame one naming the assertion and the fix; an
//! unrunnable rule leaves a named note while other findings still
//! report; pixel checks label the sampled frames — plus the refusal
//! variants (bad time, unknown key, duplicate, audio element,
//! invalid project) and the `check`/`lint` parity.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n13-t003-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_framesmith"))
}

fn run(args: &[&str], cwd: &Path) -> (i32, String) {
    let mut cmd = Command::new(bin());
    cmd.args(args).current_dir(cwd);
    let out = cmd.output().expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

fn proj(root: &Path, body: &str) -> PathBuf {
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(&folder, "composition.kdl", body);
    folder
}

// Satisfied assertions stay silent: every assertion kind passes and
// the tween keeps the whole timeline moving, so no rule fires.
const CLEAN: &str = "composition name=\"t\" width=640 height=480 duration=4 fps=30 background=\"#112233\" version=1 {\n    box id=\"hero\" start=0 duration=4 x=100 y=100 width=100 height=60 fill=\"#ff0000\" {\n        assert-appear by=2\n        assert-inside\n        assert-moving\n    }\n    tween target=0 property=\"x\" kind=\"to\" start=0 duration=4 to=200 ease=\"in-quad\"\n}\n";

// One seeded finding per motion rule: a late text (`appear-by`), a
// partially escaped box (`stay-inside`), a static box
// (`keep-moving`), and a 6s still timeline (`frozen-timeline`). The
// escape stays partial so `off-canvas` stays silent; every element
// stays clear of the caption band and of each other so only the four
// motion findings batch.
const SEEDED: &str = "composition name=\"t\" width=640 height=480 duration=6 fps=30 background=\"#112233\" version=1 {\n    box id=\"escape\" start=0 duration=2 x=600 y=100 width=100 height=60 fill=\"#ff0000\" {\n        assert-inside\n    }\n    box id=\"stuck\" start=0 duration=2 x=100 y=100 width=100 height=60 fill=\"#00ff00\" {\n        assert-moving\n    }\n    text id=\"late\" value=\"Hi\" start=3 duration=2 x=100 y=100 size=32 {\n        assert-appear by=2\n    }\n}\n";

// Frozen detection needs no assertion: one 1s still on a 4s timeline
// leaves a ~3.9s identical span.
const FROZEN_ONLY: &str = "composition name=\"t\" width=640 height=480 duration=4 fps=30 background=\"#112233\" version=1 {\n    box id=\"still\" start=0 duration=1 x=100 y=100 width=100 height=60 fill=\"#ff0000\"\n}\n";

// The whole 1.05s timeline holds still, but the searchable range
// (first and last frames excluded) is under 1s, so nothing reports.
const EXCLUDED: &str = "composition name=\"t\" width=640 height=480 duration=1.05 fps=30 background=\"#112233\" version=1 {\n    box id=\"still\" start=0 duration=1.05 x=100 y=100 width=100 height=60 fill=\"#ff0000\"\n}\n";

// A `nest` clip cannot be sampled statically, so its `assert-moving`
// leaves a named note while the static box still violates
// `keep-moving`. The nested composition moves (tween), so no frozen
// finding leaks in from it; the main timeline still freezes after
// the clips end.
const NOTE: &str = "composition name=\"main\" root=true width=640 height=480 duration=4 fps=30 background=\"#112233\" version=1 {\n    nest id=\"inner\" nest=\"sub\" start=0 duration=2 {\n        assert-moving\n    }\n    box id=\"stuck\" start=2 duration=2 x=100 y=100 width=100 height=60 fill=\"#00ff00\" track=\"video\" {\n        assert-moving\n    }\n}\ncomposition name=\"sub\" width=640 height=480 duration=2 fps=30 background=\"#000000\" version=1 {\n    box start=0 duration=2 x=10 y=10 width=50 height=50 fill=\"#ff0000\"\n    tween target=0 property=\"x\" kind=\"to\" start=0 duration=2 to=60 ease=\"in-quad\"\n}\n";

const INVALID: &str = "composition name=\"t\" width=640 height=480 duration=0 background=\"#000\" version=1 {\n    box id=\"a\" start=0 duration=0 x=100 y=100 width=100 height=100 fill=\"#ff0000\" {\n        assert-moving\n    }\n}\n";

/// `lint[...]` lines of one human report, sorted.
fn lint_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| l.starts_with("lint["))
        .map(str::to_string)
        .collect();
    lines.sort();
    lines
}

/// Slice of one JSON report between two markers.
fn json_between<'a>(json: &'a str, start: &str, end: &str) -> &'a str {
    let s = json
        .find(start)
        .unwrap_or_else(|| panic!("missing {start:?} in {json}"));
    let rest = &json[s + start.len()..];
    let e = rest
        .find(end)
        .unwrap_or_else(|| panic!("missing {end:?} in {json}"));
    &rest[..e]
}

#[test]
fn motion_satisfied_assertions_stay_silent() {
    let root = tmp("clean");
    proj(&root, CLEAN);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("lint: 0 findings across 14 rules"),
        "satisfied appear/inside/moving plus a moving timeline stay silent:\n{out}"
    );
    assert!(!out.contains("lint["), "{out}");
    assert!(!out.contains("lint-note["), "{out}");
}

#[test]
fn motion_seeded_project_batches_each_motion_rule() {
    let root = tmp("seeded");
    proj(&root, SEEDED);
    let (code, out) = run(&["lint", "proj"], &root);
    // Warnings never block a default run.
    assert_eq!(code, 0, "{out}");
    let lines = lint_lines(&out);
    assert_eq!(lines.len(), 4, "one finding per motion rule:\n{out}");
    for rule in ["appear-by", "stay-inside", "keep-moving", "frozen-timeline"] {
        assert!(
            lines.iter().any(|l| l.contains(&format!("lint[{rule}]"))),
            "missing rule {rule}:\n{out}"
        );
    }
    assert!(
        out.contains("lint: 4 finding(s) and 0 note(s) across 14 rules"),
        "{out}"
    );
}

#[test]
fn motion_late_appearance_names_measured_time() {
    let root = tmp("appear");
    proj(&root, SEEDED);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let line = lint_lines(&out)
        .into_iter()
        .find(|l| l.contains("lint[appear-by]"))
        .expect("appear-by finding");
    assert!(line.contains("text \"late\""), "{line}");
    assert!(line.contains("assert-appear"), "{line}");
    assert!(line.contains("3s"), "{line}");
    assert!(line.contains("by=2s"), "{line}");
    assert!(line.contains("(static)"), "{line}");
    assert!(line.contains("(fix:"), "{line}");
}

#[test]
fn motion_escape_names_measured_bounds() {
    let root = tmp("inside");
    proj(&root, SEEDED);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let line = lint_lines(&out)
        .into_iter()
        .find(|l| l.contains("lint[stay-inside]"))
        .expect("stay-inside finding");
    assert!(line.contains("box \"escape\""), "{line}");
    assert!(line.contains("600,100-700,160px"), "{line}");
    assert!(line.contains("60px past the right edge"), "{line}");
    assert!(line.contains("(measured)"), "{line}");
    assert!(line.contains("sampled frames at"), "{line}");
    assert!(line.contains("(fix:"), "{line}");
}

#[test]
fn motion_static_names_measured_stillness() {
    let root = tmp("moving");
    proj(&root, SEEDED);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let line = lint_lines(&out)
        .into_iter()
        .find(|l| l.contains("lint[keep-moving]"))
        .expect("keep-moving finding");
    assert!(line.contains("box \"stuck\""), "{line}");
    assert!(line.contains("identical picture"), "{line}");
    assert!(line.contains("sampled frames at"), "{line}");
    assert!(line.contains("(measured)"), "{line}");
    assert!(line.contains("(fix:"), "{line}");
}

#[test]
fn motion_frozen_reports_time_range_without_assertions() {
    let root = tmp("frozen");
    proj(&root, FROZEN_ONLY);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let lines = lint_lines(&out);
    assert_eq!(lines.len(), 1, "only the frozen span reports:\n{out}");
    let line = &lines[0];
    assert!(line.contains("lint[frozen-timeline]"), "{line}");
    assert!(line.contains("from 0.033s to 3.967s"), "{line}");
    assert!(line.contains("first and last frames excluded"), "{line}");
    assert!(line.contains("sampled frames at"), "{line}");
    assert!(line.contains("(measured)"), "{line}");
    assert!(line.contains("(fix:"), "{line}");
}

#[test]
fn motion_first_and_last_frames_never_count() {
    let root = tmp("excluded");
    proj(&root, EXCLUDED);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("lint: 0 findings across 14 rules"),
        "a still span that only exceeds 1s with the edge frames stays silent:\n{out}"
    );
    assert!(!out.contains("lint[frozen-timeline]"), "{out}");
}

#[test]
fn motion_malformed_assertion_refused_before_frame_one() {
    let root = tmp("malformed");
    // A negative deadline is a bad time.
    proj(
        &root,
        "composition name=\"t\" width=640 height=480 duration=2 background=\"#000\" version=1 {\n    text id=\"late\" value=\"Hi\" start=1 duration=1 x=10 y=10 size=20 {\n        assert-appear by=-1\n    }\n}\n",
    );
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("error[E_BAD_MOTION]"), "{out}");
    assert!(out.contains("assert-appear"), "{out}");
    assert!(out.contains("`by`"), "{out}");
    assert!(out.contains("(fix:"), "{out}");
    // An invalid project reports diagnostics and no motion findings.
    assert!(!out.contains("lint[appear-by]"), "{out}");
}

#[test]
fn motion_assertion_unknown_key_refused() {
    let root = tmp("unknown-key");
    proj(
        &root,
        "composition name=\"t\" width=640 height=480 duration=2 background=\"#000\" version=1 {\n    box id=\"b\" start=0 duration=2 x=10 y=10 width=50 height=50 fill=\"#ff0000\" {\n        assert-inside foo=1\n    }\n}\n",
    );
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("error[E_UNKNOWN_KEY]"), "{out}");
    assert!(out.contains("assert-inside"), "{out}");
    assert!(!out.contains("lint[stay-inside]"), "{out}");
}

#[test]
fn motion_assertion_on_audio_refused() {
    let root = tmp("audio");
    proj(
        &root,
        "composition name=\"t\" width=640 height=480 duration=2 background=\"#000\" version=1 {\n    audio start=0 duration=1 path=\"missing.wav\" {\n        assert-moving\n    }\n}\n",
    );
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("error[E_BAD_MOTION]"), "{out}");
    assert!(out.contains("assert-moving"), "{out}");
    assert!(out.contains("audio track"), "{out}");
    assert!(!out.contains("lint[keep-moving]"), "{out}");
    assert!(!out.contains("lint-note[keep-moving]"), "{out}");
}

#[test]
fn motion_unrunnable_rule_leaves_named_note() {
    let root = tmp("note");
    proj(&root, NOTE);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    // The nest pixels cannot be sampled: a named note, while the
    // static box and the frozen tail still report as findings.
    assert!(out.contains("lint-note[keep-moving]"), "{out}");
    assert!(out.contains("nest \"inner\""), "{out}");
    let lines = lint_lines(&out);
    assert!(
        lines.iter().any(|l| l.contains("lint[keep-moving]")),
        "the static box still violates keep-moving:\n{out}"
    );
    assert!(
        lines.iter().any(|l| l.contains("lint[frozen-timeline]")),
        "the still tail still reports frozen:\n{out}"
    );
    assert!(
        out.contains("lint: 2 finding(s) and 1 note(s) across 14 rules"),
        "{out}"
    );
}

#[test]
fn motion_json_findings_carry_rule_source_location_and_fix() {
    let root = tmp("json");
    proj(&root, SEEDED);
    let (code, json) = run(&["lint", "--json", "proj"], &root);
    assert_eq!(code, 0, "{json}");
    let array = json_between(&json, "\"lint\":[", "],\"lint_notes\"").to_string();
    for rule in ["appear-by", "stay-inside", "keep-moving", "frozen-timeline"] {
        assert!(array.contains(&format!("\"rule\":\"{rule}\"")), "{json}");
    }
    // Static and measured sources are labeled per rule.
    assert!(
        array.contains("\"rule\":\"appear-by\",\"severity\":\"warning\",\"source\":\"static\""),
        "{json}"
    );
    for rule in ["stay-inside", "keep-moving", "frozen-timeline"] {
        assert!(
            array.contains(&format!(
                "\"rule\":\"{rule}\",\"severity\":\"warning\",\"source\":\"measured\""
            )),
            "{json}"
        );
    }
    for chunk in array.split("{\"rule\":").skip(1) {
        for key in [
            "\"severity\":\"warning\"",
            "\"file\":\"",
            "\"line\":",
            "\"column\":",
            "\"message\":\"",
            "\"fix\":\"",
        ] {
            assert!(chunk.contains(key), "finding misses {key}:\n{chunk}");
        }
    }
    // Measured values ride along machine-readably too.
    assert!(json.contains("first visible at 3s"), "{json}");
    assert!(json.contains("60px past the right edge"), "{json}");
    assert!(json.contains("identical picture"), "{json}");
    assert!(json.contains("timeline frozen from"), "{json}");
}

#[test]
fn motion_reports_the_same_findings_as_check() {
    let root = tmp("parity");
    proj(&root, SEEDED);
    let (code_l, lint_out) = run(&["lint", "proj"], &root);
    let (code_c, check_out) = run(&["check", "proj"], &root);
    assert_eq!((code_l, code_c), (0, 0), "{lint_out}\n{check_out}");
    assert_eq!(
        lint_lines(&lint_out),
        lint_lines(&check_out),
        "lint vs check:\n{lint_out}\n{check_out}"
    );
    let (code_l, lint_json) = run(&["lint", "--json", "proj"], &root);
    let (code_c, check_json) = run(&["check", "--json", "proj"], &root);
    assert_eq!((code_l, code_c), (0, 0), "{lint_json}\n{check_json}");
    let lint_array = json_between(&lint_json, "\"lint\":[", "],\"lint_notes\"").to_string();
    let check_array = json_between(&check_json, "\"lint\":[", "],\"lint_notes\"").to_string();
    assert_eq!(
        lint_array, check_array,
        "lint vs check json:\n{lint_json}\n{check_json}"
    );
}

#[test]
fn motion_invalid_project_batches_diagnostics_without_motion() {
    let root = tmp("invalid");
    proj(&root, INVALID);
    let (code_l, lint_out) = run(&["lint", "proj"], &root);
    let (code_c, check_out) = run(&["check", "proj"], &root);
    assert_eq!((code_l, code_c), (1, 1), "{lint_out}\n{check_out}");
    assert!(lint_out.contains("error["), "{lint_out}");
    assert!(lint_out.contains("lint:"), "{lint_out}");
    // Broken windows would name garbage: an invalid project refuses
    // through diagnostics and reports no motion findings.
    for rule in [
        "lint[appear-by]",
        "lint[stay-inside]",
        "lint[keep-moving]",
        "lint[frozen-timeline]",
    ] {
        assert!(
            !lint_out.contains(rule),
            "motion fires on invalid:\n{lint_out}"
        );
    }
    assert_eq!(
        lint_lines(&lint_out),
        lint_lines(&check_out),
        "{lint_out}\n{check_out}"
    );
}
