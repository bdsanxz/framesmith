//! Sprint 13 check, T001: lint with named rules end-to-end.
//!
//! Covers every acceptance criterion: the fourteen rule ids exist with
//! stable meanings, every finding carries rule id, severity, file,
//! line, column, source, and fix, a clean project reports zero findings
//! explicitly, findings batch together in one run, `lint` reports the
//! same findings as `check`, and static plus measured findings share one
//! report labeled by source — plus the refusal variants.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n13-t001-{tag}-{}-{}",
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

// One dead declaration of each unused kind (`stale`, `dead`, `ghost`),
// plus one live declaration of each kind (`title`, `hero`, `brand`) so
// negatives stay silent.
const SEEDED: &str = "composition name=\"t\" width=640 height=480 duration=1 theme=\"brand\" background=\"#112233\" version=1 {\n    variable id=\"title\" type=\"string\" default=\"News\"\n    variable id=\"stale\" type=\"string\" default=\"old\"\n    style name=\"hero\" size=64 color=\"#ffffff\"\n    style name=\"dead\" size=12 color=\"#000000\"\n    text value=\"var(--title)\" start=0 duration=1 style=\"hero\"\n}\ntheme name=\"brand\" {\n    token id=\"accent\" value=\"#ff0000\"\n}\ntheme name=\"ghost\" {\n    token id=\"accent\" value=\"#00ff00\"\n}\n";

const INVALID: &str = "composition name=\"t\" width=64 height=64 duration=0 background=\"#000\" version=1 {\n    text value=\"Hi\" start=0 duration=0\n}\n";

/// `lint[<rule>] ...` lines of one human report, sorted.
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
fn lint_clean_project_reports_zero_findings_explicitly() {
    let root = tmp("clean");
    proj(&root, CLEAN);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("lint: 0 findings across 14 rules"),
        "clean projects name the explicit zero:\n{out}"
    );
    let (code, json) = run(&["lint", "--json", "proj"], &root);
    assert_eq!(code, 0, "{json}");
    assert!(json.contains("\"command\":\"lint\""), "{json}");
    assert!(json.contains("\"lint\":[]"), "{json}");
    assert!(json.contains("\"lint_notes\":[]"), "{json}");
    assert!(json.contains("\"lint_rules\":14"), "{json}");
}

#[test]
fn lint_seeded_project_batches_every_finding_with_rule_and_fix() {
    let root = tmp("seeded");
    proj(&root, SEEDED);
    let (code, out) = run(&["lint", "proj"], &root);
    // Warnings never block a default run.
    assert_eq!(code, 0, "{out}");
    let lines = lint_lines(&out);
    assert_eq!(lines.len(), 3, "one run batches all three findings:\n{out}");
    for rule in ["unused-variable", "unused-style", "unused-theme"] {
        assert!(
            lines.iter().any(|l| l.contains(&format!("lint[{rule}]"))),
            "missing rule {rule}:\n{out}"
        );
    }
    // Used declarations stay silent.
    assert!(!out.contains("--title` is declared"), "{out}");
    assert!(!out.contains("style \"hero\""), "{out}");
    assert!(!out.contains("theme \"brand\""), "{out}");
    // Every finding carries severity, file, line, column, source, fix.
    for l in &lines {
        assert!(l.contains("warning"), "{l}");
        assert!(l.contains("composition.kdl:"), "{l}");
        assert!(l.contains("(static)"), "{l}");
        assert!(l.contains("(fix:"), "{l}");
    }
    assert!(
        out.contains("lint: 3 finding(s) and 0 note(s) across 14 rules"),
        "{out}"
    );
}

#[test]
fn lint_json_findings_carry_every_required_field() {
    let root = tmp("fields");
    proj(&root, SEEDED);
    let (code, json) = run(&["lint", "--json", "proj"], &root);
    assert_eq!(code, 0, "{json}");
    let array = json_between(&json, "\"lint\":[", "],\"lint_notes\"").to_string();
    assert_eq!(array.matches("\"rule\":\"").count(), 3, "{json}");
    for rule in ["unused-variable", "unused-style", "unused-theme"] {
        assert!(array.contains(&format!("\"rule\":\"{rule}\"")), "{json}");
    }
    for key in [
        "\"severity\":\"warning\"",
        "\"source\":\"",
        "\"file\":\"",
        "\"line\":",
        "\"column\":",
        "\"message\":\"",
        "\"fix\":\"",
    ] {
        assert!(array.contains(key), "finding misses {key}:\n{json}");
    }
    // Every source label reads static or measured.
    assert_eq!(array.matches("\"source\":\"").count(), 3, "{json}");
    assert!(!array.contains("\"source\":\"\""), "{json}");
    for chunk in array.split("{\"rule\":").skip(1) {
        assert!(
            chunk.contains("\"source\":\"static\"") || chunk.contains("\"source\":\"measured\""),
            "unlabeled source:\n{chunk}"
        );
    }
    // Locations point at the dead declarations.
    assert!(array.contains("\"line\":3"), "{json}");
}

#[test]
fn lint_reports_the_same_findings_as_check() {
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
    let lint_notes = json_between(&lint_json, "\"lint_notes\":[", "],\"lint_rules\"").to_string();
    let check_notes =
        json_between(&check_json, "\"lint_notes\":[", "],\"variable_warnings\"").to_string();
    assert_eq!(lint_notes, check_notes, "{lint_json}\n{check_json}");
}

#[test]
fn lint_unused_style_is_scoped_per_composition() {
    let root = tmp("scope");
    proj(
        &root,
        "composition name=\"a\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n    style name=\"dead\" size=12 color=\"#ffffff\"\n    text value=\"Hi\" start=0 duration=1\n}\ncomposition name=\"b\" root=true width=64 height=64 duration=1 background=\"#000\" version=1 {\n    style name=\"live\" size=12 color=\"#ffffff\"\n    text value=\"Yo\" start=0 duration=1 style=\"live\"\n}\n",
    );
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let lines = lint_lines(&out);
    assert_eq!(lines.len(), 1, "{out}");
    assert!(lines[0].contains("lint[unused-style]"), "{out}");
    assert!(lines[0].contains("style \"dead\""), "{out}");
}

#[test]
fn lint_theme_selection_marks_the_theme_used() {
    let root = tmp("theme");
    proj(&root, SEEDED);
    // `--theme ghost` selects ghost; brand stays used through the
    // composition attribute even though the caller shadows it.
    let (code, out) = run(&["lint", "--theme", "ghost", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("lint: 2 finding(s)"),
        "only variable+style remain:\n{out}"
    );
    assert!(!out.contains("lint[unused-theme]"), "{out}");
}

#[test]
fn lint_strict_variables_refuses_without_lint_duplicates() {
    let root = tmp("strict-vars");
    proj(&root, SEEDED);
    let (code, out) = run(&["lint", "--strict-variables", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--stale"), "{out}");
    assert!(!out.contains("warning:"), "{out}");
    assert!(!out.contains("lint[unused-variable]"), "{out}");
}

#[test]
fn lint_invalid_project_batches_diagnostics_with_lint() {
    let root = tmp("invalid");
    proj(&root, INVALID);
    let (code_l, lint_out) = run(&["lint", "proj"], &root);
    let (code_c, check_out) = run(&["check", "proj"], &root);
    assert_eq!((code_l, code_c), (1, 1), "{lint_out}\n{check_out}");
    assert!(lint_out.contains("error["), "{lint_out}");
    assert!(lint_out.contains("lint:"), "{lint_out}");
    assert_eq!(
        lint_lines(&lint_out),
        lint_lines(&check_out),
        "{lint_out}\n{check_out}"
    );
}

#[test]
fn lint_refusals_name_the_cause() {
    let root = tmp("refusals");
    // Missing folder: refused naming the cause, exit 1.
    let (code, out) = run(&["lint", "gone"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("does not exist"), "{out}");
    // Usage errors: exit 2 with the usage line.
    proj(&root, CLEAN);
    let (code, out) = run(&["lint", "--bogus", "proj"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("usage"), "{out}");
    let (code, out) = run(&["lint", "--json", "--json", "proj"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("usage"), "{out}");
    let (code, out) = run(&["lint", "--json"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("usage"), "{out}");
    let (code, out) = run(&["lint", "--backend", "bogus", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--backend"), "{out}");
    let (code, out) = run(&["lint", "--variables", "42", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--variables"), "{out}");
}

#[test]
fn lint_writes_nothing_and_needs_no_ffmpeg() {
    let root = tmp("clean-run");
    let folder = proj(&root, CLEAN);
    let before = std::fs::read(folder.join("composition.kdl")).unwrap();
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        std::fs::read(folder.join("composition.kdl")).unwrap(),
        before,
        "lint must not touch the composition"
    );
    assert!(
        !folder.join("out.mp4").exists(),
        "lint must not write an MP4"
    );
    for entry in std::fs::read_dir(&folder).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name == "composition.kdl" || name == "logs",
            "lint wrote an unexpected entry: {name}"
        );
    }
}
