//! Sprint 13 check, T007: gate releases with strictness end-to-end.
//!
//! Covers every acceptance criterion: default runs keep the six named
//! warnings as warnings; `--strict` stops on each named warning as an
//! error with its rule and fix; `--strict-all` stops on any warning;
//! strictness invents no findings (counts match the default run);
//! strict stops list every elevated finding; nothing is rendered when
//! strict stops — plus the refusal variants.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n13-t007-{tag}-{}-{}",
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

/// One second of 48 kHz stereo s16 440 Hz tone, written by hand so
/// audio fixtures need no encoder: the discarded-audio rules read
/// windows and levels only, never samples.
fn write_tone(folder: &Path) {
    let rate: u32 = 48000;
    let mut pcm = Vec::with_capacity(rate as usize * 4);
    for i in 0..rate {
        let s = (9000.0
            * (2.0 * std::f64::consts::PI * 440.0 * f64::from(i) / f64::from(rate)).sin())
            as i16;
        pcm.extend_from_slice(&s.to_le_bytes());
        pcm.extend_from_slice(&s.to_le_bytes());
    }
    let mut out = Vec::with_capacity(44 + pcm.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    out.extend_from_slice(&pcm);
    std::fs::write(folder.join("tone.wav"), out).unwrap();
}

/// One finding per named-strict rule (off-canvas, contrast,
/// unused-variable, unused-style, unused-theme, discarded-audio)
/// plus one non-strict warning (overlap) as the `--strict` vs
/// `--strict-all` canary. The 1s duration keeps frozen-timeline
/// silent; every clip clears the caption band.
const STRICT: &str = "composition name=\"t\" width=640 height=480 duration=1 theme=\"brand\" background=\"#112233\" version=1 {\n    variable id=\"stale\" type=\"string\" default=\"old\"\n    style name=\"dead\" size=12 color=\"#000000\"\n    box id=\"lost\" start=0 duration=1 x=-300 y=100 width=200 height=120 fill=\"#ff00ff\"\n    box id=\"lower\" start=0 duration=1 x=100 y=100 width=200 height=120 fill=\"#ff0000\"\n    box id=\"upper\" start=0 duration=0.5 x=200 y=160 width=200 height=120 fill=\"#00ff00\"\n    text id=\"dim\" value=\"Hello\" start=0 duration=1 x=100 y=50 size=16 color=\"#777777\"\n    audio id=\"dead-hidden\" path=\"tone.wav\" start=0 duration=1 hidden=true\n    audio id=\"vox\" path=\"tone.wav\" start=0 duration=1\n}\ntheme name=\"brand\" {\n    token id=\"accent\" value=\"#ff0000\"\n}\ntheme name=\"ghost\" {\n    token id=\"accent\" value=\"#00ff00\"\n}\n";

const NAMED_STRICT: [&str; 6] = [
    "off-canvas",
    "contrast",
    "unused-variable",
    "unused-style",
    "unused-theme",
    "discarded-audio",
];

const CLEAN: &str = "composition name=\"t\" width=640 height=480 duration=1 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1\n}\n";

const INVALID: &str = "composition name=\"t\" width=640 height=480 duration=0 background=\"#000\" version=1 {\n    text value=\"Hi\" start=0 duration=0\n}\n";

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

/// Rule id of one `lint[rule] ...` line.
fn lint_rule(line: &str) -> &str {
    let rest = &line["lint[".len()..];
    &rest[..rest.find(']').expect("lint rule closes")]
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
fn strict_default_runs_keep_all_named_warnings_as_warnings() {
    let root = tmp("default");
    let folder = proj(&root, STRICT);
    write_tone(&folder);
    for cmd in ["check", "lint"] {
        let (code, out) = run(&[cmd, "proj"], &root);
        assert_eq!(code, 0, "{cmd} default exits 0:\n{out}");
        let lines = lint_lines(&out);
        assert_eq!(lines.len(), 7, "{cmd} batches all seven:\n{out}");
        for rule in NAMED_STRICT {
            let line = lines
                .iter()
                .find(|l| lint_rule(l) == rule)
                .unwrap_or_else(|| panic!("missing named rule {rule}:\n{out}"));
            assert!(!line.contains("error"), "{line}");
            assert!(line.contains("warning"), "{line}");
            assert!(line.contains("(fix:"), "{line}");
        }
        // The non-strict canary stays a warning too.
        let overlap = lines
            .iter()
            .find(|l| lint_rule(l) == "overlap")
            .expect("overlap canary");
        assert!(overlap.contains("warning"), "{overlap}");
    }
}

#[test]
fn strict_stops_on_each_named_warning_as_error() {
    let root = tmp("strict");
    let folder = proj(&root, STRICT);
    write_tone(&folder);
    for cmd in ["check", "lint"] {
        let (code, out) = run(&[cmd, "--strict", "proj"], &root);
        assert_eq!(code, 1, "{cmd} --strict gates:\n{out}");
        let lines = lint_lines(&out);
        assert_eq!(lines.len(), 7, "{cmd} --strict lists every finding:\n{out}");
        for rule in NAMED_STRICT {
            let line = lines
                .iter()
                .find(|l| lint_rule(l) == rule)
                .unwrap_or_else(|| panic!("missing elevated rule {rule}:\n{out}"));
            assert!(line.contains("error"), "{line}");
            assert!(line.contains("(fix:"), "{line}");
            assert!(line.contains("composition.kdl:"), "{line}");
        }
        // The canary is not in the named set: warning under --strict.
        let overlap = lines
            .iter()
            .find(|l| lint_rule(l) == "overlap")
            .expect("overlap canary");
        assert!(overlap.contains("warning"), "{overlap}");
        assert!(
            out.contains("6 elevated to errors by --strict"),
            "gate names the count and flag:\n{out}"
        );
        assert!(out.contains("strict:"), "{out}");
    }
}

#[test]
fn strict_all_stops_on_any_warning_as_error() {
    let root = tmp("strict-all");
    let folder = proj(&root, STRICT);
    write_tone(&folder);
    for cmd in ["check", "lint"] {
        let (code, out) = run(&[cmd, "--strict-all", "proj"], &root);
        assert_eq!(code, 1, "{cmd} --strict-all gates:\n{out}");
        let lines = lint_lines(&out);
        assert_eq!(
            lines.len(),
            7,
            "{cmd} --strict-all lists every finding:\n{out}"
        );
        for line in &lines {
            assert!(line.contains("error"), "{line}");
            assert!(line.contains("(fix:"), "{line}");
        }
        assert!(
            lines
                .iter()
                .any(|l| lint_rule(l) == "overlap" && l.contains("error")),
            "the canary elevates under --strict-all:\n{out}"
        );
        assert!(
            out.contains("7 elevated to errors by --strict-all"),
            "gate names the count and flag:\n{out}"
        );
    }
}

#[test]
fn strict_invents_no_findings_counts_match_the_default_run() {
    let root = tmp("counts");
    let folder = proj(&root, STRICT);
    write_tone(&folder);
    let mut modes: Vec<Vec<String>> = Vec::new();
    for args in [
        vec!["lint", "proj"],
        vec!["lint", "--strict", "proj"],
        vec!["lint", "--strict-all", "proj"],
    ] {
        let (code, out) = run(&args, &root);
        let _ = code;
        modes.push(
            lint_lines(&out)
                .iter()
                .map(|l| lint_rule(l).to_string())
                .collect(),
        );
    }
    assert_eq!(modes[0], modes[1], "strict adds no rule");
    assert_eq!(modes[0], modes[2], "strict-all adds no rule");
    assert_eq!(modes[0].len(), 7);
    // JSON rule order matches too; only severities move.
    let (_, default_json) = run(&["lint", "--json", "proj"], &root);
    let (_, strict_json) = run(&["lint", "--strict", "--json", "proj"], &root);
    let (_, all_json) = run(&["lint", "--strict-all", "--json", "proj"], &root);
    let rules = |json: &str| {
        let array = json_between(json, "\"lint\":[", "],\"lint_notes\"").to_string();
        assert_eq!(array.matches("\"rule\":\"").count(), 7, "{json}");
        array
            .match_indices("\"rule\":\"")
            .map(|(i, _)| {
                let rest = &array[i + 8..];
                rest[..rest.find('"').unwrap()].to_string()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(rules(&default_json), rules(&strict_json));
    assert_eq!(rules(&default_json), rules(&all_json));
    assert_eq!(default_json.matches("\"severity\":\"warning\"").count(), 7);
    assert_eq!(strict_json.matches("\"severity\":\"error\"").count(), 6);
    assert_eq!(strict_json.matches("\"severity\":\"warning\"").count(), 1);
    assert_eq!(all_json.matches("\"severity\":\"error\"").count(), 7);
}

#[test]
fn strict_check_and_lint_agree_under_the_gate() {
    let root = tmp("parity");
    let folder = proj(&root, STRICT);
    write_tone(&folder);
    for flag in ["--strict", "--strict-all"] {
        let (code_l, lint_out) = run(&["lint", flag, "proj"], &root);
        let (code_c, check_out) = run(&["check", flag, "proj"], &root);
        assert_eq!((code_l, code_c), (1, 1), "{lint_out}\n{check_out}");
        assert_eq!(
            lint_lines(&lint_out),
            lint_lines(&check_out),
            "lint vs check under {flag}:\n{lint_out}\n{check_out}"
        );
        let (code_l, lint_json) = run(&["lint", flag, "--json", "proj"], &root);
        let (code_c, check_json) = run(&["check", flag, "--json", "proj"], &root);
        assert_eq!((code_l, code_c), (1, 1), "{lint_json}\n{check_json}");
        let lint_array = json_between(&lint_json, "\"lint\":[", "],\"lint_notes\"").to_string();
        let check_array = json_between(&check_json, "\"lint\":[", "],\"lint_notes\"").to_string();
        assert_eq!(lint_array, check_array, "{lint_json}\n{check_json}");
    }
}

#[test]
fn strict_json_names_severity_source_fields_and_the_gate() {
    let root = tmp("json");
    let folder = proj(&root, STRICT);
    write_tone(&folder);
    let (code, json) = run(&["check", "--strict", "--json", "proj"], &root);
    assert_eq!(code, 1, "{json}");
    assert!(json.contains("\"status\":\"valid\""), "{json}");
    assert!(json.contains("\"exit\":1"), "{json}");
    assert!(json.contains("\"strict\":\"strict\""), "{json}");
    assert!(json.contains("\"elevated\":6"), "{json}");
    let array = json_between(&json, "\"lint\":[", "],\"lint_notes\"").to_string();
    for rule in NAMED_STRICT {
        assert!(array.contains(&format!("\"rule\":\"{rule}\"")), "{json}");
    }
    for key in [
        "\"severity\":\"error\"",
        "\"source\":\"",
        "\"file\":\"",
        "\"line\":",
        "\"column\":",
        "\"message\":\"",
        "\"fix\":\"",
    ] {
        assert!(array.contains(key), "finding misses {key}:\n{json}");
    }
    // Measured and static sources stay labeled after elevation.
    assert!(array.contains("\"source\":\"measured\""), "{json}");
    assert!(array.contains("\"source\":\"static\""), "{json}");
    let (code, json) = run(&["lint", "--strict-all", "--json", "proj"], &root);
    assert_eq!(code, 1, "{json}");
    assert!(json.contains("\"strict\":\"strict-all\""), "{json}");
    assert!(json.contains("\"elevated\":7"), "{json}");
    assert_eq!(
        json_between(&json, "\"lint\":[", "],\"lint_notes\"")
            .matches("\"severity\":\"error\"")
            .count(),
        7,
        "{json}"
    );
}

#[test]
fn strict_render_stops_before_frame_one_with_nothing_written() {
    let root = tmp("render-gate");
    let folder = proj(&root, STRICT);
    write_tone(&folder);
    for (flag, want) in [("--strict", 6), ("--strict-all", 7)] {
        let target = format!("out-{}.mp4", &flag[2..]);
        let (code, out) = run(&["render", flag, "proj", "-o", &target], &root);
        assert_eq!(code, 1, "render {flag} gates:\n{out}");
        let lines = lint_lines(&out);
        assert_eq!(
            lines.len(),
            want,
            "render {flag} lists every elevated finding:\n{out}"
        );
        for line in &lines {
            assert!(line.contains("error"), "{line}");
            assert!(line.contains("(fix:"), "{line}");
        }
        assert!(
            out.contains("before the first frame"),
            "gate stops before frame one:\n{out}"
        );
        assert!(
            out.contains("Nothing was rendered"),
            "gate renders nothing:\n{out}"
        );
        assert!(out.contains(flag), "gate names the flag:\n{out}");
        assert!(
            !root.join(&target).exists(),
            "render {flag} wrote no output"
        );
    }
    // No output of any shape: the default -o target is absent too.
    let (code, out) = run(&["render", "--strict", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(
        !folder.join("out.mp4").exists(),
        "strict render wrote no default output"
    );
}

#[test]
fn strict_default_render_ships_with_warnings_intact() {
    let root = tmp("render-default");
    let folder = proj(&root, STRICT);
    write_tone(&folder);
    let (code, out) = run(&["render", "proj", "-o", "ship.mp4"], &root);
    assert_eq!(code, 0, "default render ships:\n{out}");
    assert!(
        root.join("ship.mp4").is_file(),
        "default render writes its MP4:\n{out}"
    );
}

#[test]
fn strict_render_json_gates_without_output() {
    let root = tmp("render-json");
    let folder = proj(&root, STRICT);
    write_tone(&folder);
    let (code, out) = run(
        &["render", "--strict", "--json", "proj", "-o", "gated.mp4"],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("\"exit\":1"), "{out}");
    assert!(out.contains("--strict"), "{out}");
    assert!(out.contains("before the first frame"), "{out}");
    assert!(
        !root.join("gated.mp4").exists(),
        "gated JSON render wrote no output"
    );
}

#[test]
fn strict_clean_project_stays_clean_without_a_gate() {
    let root = tmp("clean");
    proj(&root, CLEAN);
    for cmd in ["check", "lint"] {
        for flag in ["--strict", "--strict-all"] {
            let (code, out) = run(&[cmd, flag, "proj"], &root);
            assert_eq!(code, 0, "{cmd} {flag} on clean exits 0:\n{out}");
            assert!(
                out.contains("lint: 0 findings across 16 rules"),
                "{cmd} {flag} stays explicit:\n{out}"
            );
            assert!(
                !out.contains("elevated"),
                "{cmd} {flag} gates nothing:\n{out}"
            );
        }
    }
}

#[test]
fn strict_invalid_project_keeps_its_invalid_exit() {
    let root = tmp("invalid");
    proj(&root, INVALID);
    for cmd in ["check", "lint"] {
        let (code, out) = run(&[cmd, "--strict", "proj"], &root);
        assert_eq!(code, 1, "{cmd} --strict on invalid exits 1:\n{out}");
        assert!(
            out.contains("invalid"),
            "{cmd} --strict stays invalid:\n{out}"
        );
    }
    // Invalid projects carry no lint batch to gate on.
    let (code, out) = run(&["lint", "--strict-all", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(!out.contains("elevated to errors"), "{out}");
}

#[test]
fn strict_refusals_name_the_cause() {
    let root = tmp("refusals");
    let folder = proj(&root, STRICT);
    write_tone(&folder);
    // The two gate flags are mutually exclusive on every command.
    for cmd in ["check", "lint"] {
        let (code, out) = run(&[cmd, "--strict", "--strict-all", "proj"], &root);
        assert_eq!(code, 2, "{cmd} both flags exit 2:\n{out}");
        assert!(out.contains("mutually exclusive"), "{out}");
        assert!(out.contains("usage"), "{out}");
    }
    let (code, out) = run(&["render", "--strict", "--strict-all", "proj"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("mutually exclusive"), "{out}");
    // Duplicates refuse naming the flag.
    let (code, out) = run(&["lint", "--strict", "--strict", "proj"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("duplicate --strict"), "{out}");
    assert!(out.contains("usage"), "{out}");
    let (code, out) = run(&["check", "--strict-all", "--strict-all", "proj"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("duplicate --strict-all"), "{out}");
    let (code, out) = run(&["render", "--strict", "proj", "--strict"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("duplicate --strict"), "{out}");
    // Missing folders still refuse naming the cause under strict.
    let (code, out) = run(&["lint", "--strict", "gone"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("does not exist"), "{out}");
    // Nothing gated was written anywhere.
    assert!(
        !folder.join("out.mp4").exists() && !root.join("out.mp4").exists(),
        "refusals write no output"
    );
    for entry in std::fs::read_dir(&folder).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        assert!(
            name == "composition.kdl" || name == "tone.wav" || name == "logs",
            "refusal wrote an unexpected entry: {name}"
        );
    }
}
