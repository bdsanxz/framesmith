//! Sprint 13 check, T002: layout geometry rules end-to-end.
//!
//! Covers every acceptance criterion: overlapping opaque clips report
//! both ids and the overlap area; fully off-canvas elements report
//! their out-of-bounds pixels; text touching the bottom-12% band
//! reports with the band named; transparent and hidden clips never
//! trigger overlap; partially visible elements never trigger
//! off-canvas; every finding names its elements with measured
//! geometry — plus the refusal variants.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n13-t002-{tag}-{}-{}",
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

const CLEAN: &str = "composition name=\"t\" width=640 height=480 duration=1 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1\n}\n";

// One seeded finding per layout rule (overlap, off-canvas,
// caption-zone), surrounded by negatives that must stay silent:
// a fully transparent overlapper, a hidden overlapper, a partially
// visible box, touching windows, touching edges, a different-track
// double, an above-band text, and a hidden band text.
const SEEDED: &str = "composition name=\"t\" width=640 height=480 duration=1 background=\"#112233\" version=1 {\n    box id=\"lower\" start=0 duration=1 x=100 y=100 width=200 height=120 fill=\"#ff0000\"\n    box id=\"upper\" start=0.25 duration=0.5 x=200 y=160 width=200 height=120 fill=\"#00ff00\"\n    box id=\"ghost\" start=0 duration=1 x=110 y=110 width=200 height=120 fill=\"#0000ff\" opacity=0\n    box id=\"sneaky\" start=0 duration=1 x=120 y=120 width=200 height=120 fill=\"#0000ff\" hidden=true\n    box id=\"lost\" start=0 duration=1 x=-300 y=100 width=200 height=120 fill=\"#ff00ff\"\n    box id=\"edge\" start=0 duration=1 x=-50 y=300 width=200 height=120 fill=\"#ff00ff\"\n    text id=\"cap\" value=\"Hello\" start=0 duration=0.5 x=100 y=440 size=32\n    text id=\"title\" value=\"Hi\" start=0 duration=0.5 x=420 y=50 size=32\n    text id=\"hushed\" value=\"Hi\" start=0 duration=0.5 x=300 y=440 size=32 hidden=true\n    box id=\"early\" start=0 duration=0.5 x=400 y=300 width=100 height=60 fill=\"#ffffff\"\n    box id=\"late\" start=0.5 duration=0.5 x=400 y=300 width=100 height=60 fill=\"#ffffff\"\n    box id=\"left\" start=0 duration=1 x=500 y=100 width=60 height=60 fill=\"#ffffff\"\n    box id=\"right\" start=0 duration=1 x=560 y=100 width=60 height=60 fill=\"#ffffff\"\n    box id=\"viddy\" start=0 duration=1 x=100 y=100 width=200 height=120 fill=\"#ffffff\" track=\"video\"\n}\n";

// Negatives only: transparent and hidden overlappers, a partially
// visible box, touching windows and edges, a different-track double,
// an above-band text, and a hidden band text — zero findings.
const NEGATIVES: &str = "composition name=\"t\" width=640 height=480 duration=1 background=\"#112233\" version=1 {\n    box id=\"a\" start=0 duration=1 x=100 y=100 width=100 height=100 fill=\"#ff0000\"\n    box id=\"b\" start=0 duration=1 x=150 y=150 width=100 height=100 fill=\"#00ff00\" opacity=0\n    box id=\"c\" start=0 duration=1 x=150 y=150 width=100 height=100 fill=\"#0000ff\" hidden=true\n    box id=\"d\" start=0 duration=1 x=-50 y=300 width=200 height=100 fill=\"#ff00ff\"\n    box id=\"e\" start=0 duration=0.5 x=400 y=300 width=100 height=60 fill=\"#ffffff\"\n    box id=\"f\" start=0.5 duration=0.5 x=400 y=300 width=100 height=60 fill=\"#ffffff\"\n    box id=\"g\" start=0 duration=1 x=500 y=100 width=60 height=60 fill=\"#ffffff\" track=\"video\"\n    text id=\"top\" value=\"Hi\" start=0 duration=1 x=420 y=50 size=32\n    text id=\"hushed\" value=\"Hi\" start=0 duration=1 x=300 y=440 size=32 hidden=true\n}\n";

const INVALID: &str = "composition name=\"t\" width=640 height=480 duration=0 background=\"#000\" version=1 {\n    box id=\"a\" start=0 duration=0 x=100 y=100 width=100 height=100 fill=\"#ff0000\"\n    box id=\"b\" start=0 duration=0 x=150 y=150 width=100 height=100 fill=\"#00ff00\"\n}\n";

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
    let s = json.find(start).unwrap_or_else(|| panic!("missing {start:?} in {json}"));
    let rest = &json[s + start.len()..];
    let e = rest.find(end).unwrap_or_else(|| panic!("missing {end:?} in {json}"));
    &rest[..e]
}

#[test]
fn layout_clean_project_reports_zero_findings() {
    let root = tmp("clean");
    proj(&root, CLEAN);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("lint: 0 findings across 13 rules"),
        "a lone text stays silent:\n{out}"
    );
}

#[test]
fn layout_seeded_project_batches_one_finding_per_rule() {
    let root = tmp("seeded");
    proj(&root, SEEDED);
    let (code, out) = run(&["lint", "proj"], &root);
    // Warnings never block a default run.
    assert_eq!(code, 0, "{out}");
    let lines = lint_lines(&out);
    assert_eq!(lines.len(), 3, "one finding per layout rule:\n{out}");
    for rule in ["overlap", "off-canvas", "caption-zone"] {
        assert!(
            lines.iter().any(|l| l.contains(&format!("lint[{rule}]"))),
            "missing rule {rule}:\n{out}"
        );
    }
    assert!(
        out.contains("lint: 3 finding(s) and 0 note(s) across 13 rules"),
        "{out}"
    );
}

#[test]
fn layout_overlap_names_both_ids_with_the_area() {
    let root = tmp("overlap");
    proj(&root, SEEDED);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let line = lint_lines(&out)
        .into_iter()
        .find(|l| l.contains("lint[overlap]"))
        .expect("overlap finding");
    // Both ids, the track, the shared window, and the measured area.
    assert!(line.contains("box \"lower\""), "{line}");
    assert!(line.contains("box \"upper\""), "{line}");
    assert!(line.contains("track \"graphics\""), "{line}");
    assert!(line.contains("6000px"), "{line}");
    assert!(line.contains("100x60px"), "{line}");
    assert!(line.contains("(fix:"), "{line}");
}

#[test]
fn layout_off_canvas_reports_out_of_bounds_pixels() {
    let root = tmp("off-canvas");
    proj(&root, SEEDED);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let line = lint_lines(&out)
        .into_iter()
        .find(|l| l.contains("lint[off-canvas]"))
        .expect("off-canvas finding");
    assert!(line.contains("box \"lost\""), "{line}");
    assert!(line.contains("640x480"), "{line}");
    assert!(line.contains("100px past the left edge"), "{line}");
    assert!(line.contains("(fix:"), "{line}");
}

#[test]
fn layout_caption_zone_names_the_band() {
    let root = tmp("caption");
    proj(&root, SEEDED);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let line = lint_lines(&out)
        .into_iter()
        .find(|l| l.contains("lint[caption-zone]"))
        .expect("caption-zone finding");
    assert!(line.contains("text \"cap\""), "{line}");
    assert!(line.contains("caption band"), "{line}");
    assert!(line.contains("bottom 12%"), "{line}");
    assert!(line.contains("y>=422.4px"), "{line}");
    assert!(line.contains("(fix:"), "{line}");
}

#[test]
fn layout_negatives_stay_silent() {
    let root = tmp("negatives");
    proj(&root, NEGATIVES);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("lint: 0 findings across 13 rules"),
        "transparent, hidden, partial, touching, split-track, and quiet texts stay silent:\n{out}"
    );
    assert!(!out.contains("lint["), "{out}");
}

#[test]
fn layout_every_finding_names_elements_with_measured_geometry() {
    let root = tmp("geometry");
    proj(&root, SEEDED);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    for line in lint_lines(&out) {
        // Every finding names its rule, severity, location, source,
        // measured pixels, and fix.
        assert!(line.contains("warning"), "{line}");
        assert!(line.contains("composition.kdl:"), "{line}");
        assert!(line.contains("(static)"), "{line}");
        assert!(line.contains("px"), "{line}");
        assert!(line.contains("(fix:"), "{line}");
    }
}

#[test]
fn layout_json_findings_carry_rule_source_location_and_fix() {
    let root = tmp("json");
    proj(&root, SEEDED);
    let (code, json) = run(&["lint", "--json", "proj"], &root);
    assert_eq!(code, 0, "{json}");
    let array = json_between(&json, "\"lint\":[", "],\"lint_notes\"").to_string();
    assert_eq!(array.matches("\"rule\":\"").count(), 3, "{json}");
    for rule in ["overlap", "off-canvas", "caption-zone"] {
        assert!(array.contains(&format!("\"rule\":\"{rule}\"")), "{json}");
    }
    for chunk in array.split("{\"rule\":").skip(1) {
        assert!(chunk.contains("\"source\":\"static\""), "{chunk}");
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
    // The overlap message carries both ids and the area in JSON too.
    assert!(json.contains("box \\\"lower\\\""), "{json}");
    assert!(json.contains("6000px"), "{json}");
}

#[test]
fn layout_reports_the_same_findings_as_check() {
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
fn layout_invalid_project_batches_diagnostics_without_layout() {
    let root = tmp("invalid");
    proj(&root, INVALID);
    let (code_l, lint_out) = run(&["lint", "proj"], &root);
    let (code_c, check_out) = run(&["check", "proj"], &root);
    assert_eq!((code_l, code_c), (1, 1), "{lint_out}\n{check_out}");
    assert!(lint_out.contains("error["), "{lint_out}");
    assert!(lint_out.contains("lint:"), "{lint_out}");
    // Geometry needs resolved windows: an invalid project refuses
    // through diagnostics and reports no layout findings.
    for rule in ["lint[overlap]", "lint[off-canvas]", "lint[caption-zone]"] {
        assert!(!lint_out.contains(rule), "layout fires on invalid:\n{lint_out}");
    }
    assert_eq!(
        lint_lines(&lint_out),
        lint_lines(&check_out),
        "{lint_out}\n{check_out}"
    );
}
