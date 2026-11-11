//! Sprint 13 check, T004: contrast and audio health end-to-end.
//!
//! Covers every acceptance criterion: low-contrast text reports the
//! measured ratio with its bar; large text (size 18 and up) takes
//! the 3:1 bar and reports it; a silent mix reports the measured
//! peak; each discarded-audio cause (hidden, muted, zero volume,
//! fully faded, fully outside) names the clip and the reason;
//! animated (gradient) backgrounds report the midpoint-frame ratio
//! labeled as sampled; unrunnable backgrounds leave named notes
//! while other findings still report; every finding names its
//! element with measured values — plus the refusal variants (`lint`
//! vs `check` parity, invalid-project gating, JSON fields).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n13-t004-{tag}-{}-{}",
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
/// audio fixtures need no encoder and `lint` never decodes: the
/// rules read windows and levels only, never samples.
fn write_tone(folder: &Path) {
    let rate: u32 = 48000;
    let frames: u32 = rate;
    let mut pcm = Vec::with_capacity(frames as usize * 4);
    for i in 0..frames {
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

// White text on the dark background, no audio: nothing to measure
// and nothing authored to sound, so every rule stays silent.
const CLEAN: &str = "composition name=\"t\" width=640 height=480 duration=1 background=\"#112233\" version=1 {\n    text id=\"hi\" value=\"Hi\" start=0 duration=1 x=100 y=50 size=16 color=\"#ffffff\"\n}\n";

// One finding per contrast shape: a dim normal text (4.5:1 bar), a
// dark large text (3:1 bar), a dark text over a solid black box
// (effective background), the same dark text over the bare
// background (a different ratio proves the box was read), and a
// white text over a black-to-white gradient (sampled midpoint
// ratio). A passing large text (3.6:1 clears 3:1) and a passing
// white text stay silent. Boxes ride a second track so overlap
// stays out of the batch; every text clears the caption band.
const CONTRAST: &str = "composition name=\"t\" width=640 height=480 duration=1 background=\"#112233\" version=1 {\n    text id=\"dim\" value=\"Hello\" start=0 duration=1 x=100 y=50 size=16 color=\"#777777\"\n    text id=\"big\" value=\"Hello\" start=0 duration=1 x=100 y=120 size=32 color=\"#555555\"\n    text id=\"okbig\" value=\"Hello\" start=0 duration=1 x=100 y=200 size=32 color=\"#777777\"\n    text id=\"white\" value=\"Hello\" start=0 duration=1 x=100 y=260 size=16 color=\"#ffffff\"\n    box id=\"panel\" start=0 duration=1 x=390 y=40 width=68 height=36 fill=\"#000000\" track=\"video\"\n    text id=\"boxbg\" value=\"Hello\" start=0 duration=1 x=400 y=50 size=16 color=\"#333333\"\n    text id=\"boxbg2\" value=\"Hello\" start=0 duration=1 x=400 y=120 size=16 color=\"#333333\"\n    box id=\"shade\" start=0 duration=1 x=390 y=190 width=110 height=70 fill=\"#000000\" track=\"video\" {\n        linear-gradient {\n            stop offset=0 color=\"#000000\"\n            stop offset=1 color=\"#ffffff\"\n        }\n    }\n    text id=\"grad\" value=\"Hello\" start=0 duration=1 x=420 y=215 size=16 color=\"#ffffff\"\n}\n";

// One finding per discarded-audio cause: hidden, zero volume,
// fully faded, fully outside, and muted by a full-window zero
// duck. The voice stays audible, so the mix itself is not silent.
const DISCARDED: &str = "composition name=\"t\" width=640 height=480 duration=1 background=\"#112233\" version=1 {\n    audio id=\"dead-hidden\" path=\"tone.wav\" start=0 duration=1 hidden=true\n    audio id=\"dead-quiet\" path=\"tone.wav\" start=0 duration=1 volume=\"-inf\"\n    audio id=\"dead-fade\" path=\"tone.wav\" start=0 duration=1 fade-in=0.6 fade-out=0.5\n    audio id=\"dead-past\" path=\"tone.wav\" start=5 duration=1\n    audio id=\"vox\" path=\"tone.wav\" start=0 duration=1\n    audio id=\"dead-duck\" path=\"tone.wav\" start=0 duration=1 duck-under=\"vox\" duck-amount=0\n}\n";

// Every clip discarded and nothing else to sound: two
// discarded-audio findings plus the silent mix with its peak.
const SILENT: &str = "composition name=\"t\" width=640 height=480 duration=1 background=\"#112233\" version=1 {\n    audio id=\"gone-quiet\" path=\"tone.wav\" start=0 duration=1 volume=\"-inf\"\n    audio id=\"gone-hidden\" path=\"tone.wav\" start=0 duration=1 hidden=true\n}\n";

// Unmeasurable backgrounds leave notes: a text over another text
// (decoded-shapes pixels) and a text half off a solid box (two
// backgrounds at once), while a dim text still reports its ratio.
// Crossed tracks keep overlap out of the batch.
const NOTE: &str = "composition name=\"t\" width=640 height=480 duration=1 background=\"#112233\" version=1 {\n    text id=\"base\" value=\"Hello\" start=0 duration=1 x=100 y=50 size=16 color=\"#ffffff\" track=\"caption\"\n    text id=\"over\" value=\"Hello\" start=0 duration=1 x=120 y=55 size=16 color=\"#ffffff\"\n    box id=\"half\" start=0 duration=1 x=300 y=50 width=40 height=40 fill=\"#ffffff\" track=\"video\"\n    text id=\"over2\" value=\"Hello\" start=0 duration=1 x=320 y=55 size=16 color=\"#ffffff\"\n    text id=\"dim\" value=\"Hello\" start=0 duration=1 x=100 y=150 size=16 color=\"#777777\"\n}\n";

const INVALID: &str = "composition name=\"t\" width=640 height=480 duration=0 background=\"#000\" version=1 {\n    text id=\"dim\" value=\"Hello\" start=0 duration=0 x=100 y=50 size=16 color=\"#777777\"\n}\n";

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
fn contrast_audio_clean_project_reports_zero_findings() {
    let root = tmp("clean");
    proj(&root, CLEAN);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("lint: 0 findings across 16 rules"),
        "white text on dark with no audio stays silent:\n{out}"
    );
    assert!(!out.contains("lint["), "{out}");
    assert!(!out.contains("lint-note["), "{out}");
}

#[test]
fn contrast_low_text_reports_measured_ratio_and_bar() {
    let root = tmp("ratio");
    proj(&root, CONTRAST);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let line = lint_lines(&out)
        .into_iter()
        .find(|l| l.contains("lint[contrast]") && l.contains("text \"dim\""))
        .expect("dim contrast finding");
    // Measured values: the ratio, the background, the bar, bounds.
    assert!(line.contains("3.6:1"), "{line}");
    assert!(line.contains("against #112233"), "{line}");
    assert!(line.contains("4.5:1"), "{line}");
    assert!(line.contains("100,50-148,66px"), "{line}");
    assert!(line.contains("(measured)"), "{line}");
    assert!(line.contains("(fix:"), "{line}");
}

#[test]
fn contrast_large_text_uses_three_to_one_bar() {
    let root = tmp("large");
    proj(&root, CONTRAST);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let line = lint_lines(&out)
        .into_iter()
        .find(|l| l.contains("lint[contrast]") && l.contains("text \"big\""))
        .expect("big contrast finding");
    assert!(line.contains("2.2:1"), "{line}");
    assert!(line.contains("needs 3:1"), "{line}");
    assert!(line.contains("large"), "{line}");
    // The same gray at size 32 clears the 3:1 bar, so it stays
    // silent and proves the bar was selected, not skipped.
    assert!(
        !lint_lines(&out)
            .iter()
            .any(|l| l.contains("text \"okbig\"")),
        "okbig clears the large-text bar:\n{out}"
    );
}

#[test]
fn contrast_seeded_project_batches_each_shape() {
    let root = tmp("seeded");
    proj(&root, CONTRAST);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let lines = lint_lines(&out);
    assert_eq!(lines.len(), 5, "dim, big, boxbg, boxbg2, grad:\n{out}");
    for line in &lines {
        assert!(line.contains("lint[contrast]"), "{line}");
    }
    // The solid box is read as the effective background: the same
    // dark text measures 1.7:1 over the box, 1.3:1 over the bare
    // composition background.
    let boxed = lines
        .iter()
        .find(|l| l.contains("text \"boxbg\"") && !l.contains("boxbg2"))
        .expect("boxbg finding");
    assert!(boxed.contains("1.7:1"), "{boxed}");
    assert!(boxed.contains("against #000000"), "{boxed}");
    let bare = lines
        .iter()
        .find(|l| l.contains("text \"boxbg2\""))
        .expect("boxbg2 finding");
    assert!(bare.contains("1.3:1"), "{bare}");
    assert!(bare.contains("against #112233"), "{bare}");
    assert!(
        out.contains("lint: 5 finding(s) and 0 note(s) across 16 rules"),
        "{out}"
    );
}

#[test]
fn contrast_animated_background_reports_sampled_midpoint_ratio() {
    let root = tmp("sampled");
    proj(&root, CONTRAST);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let line = lint_lines(&out)
        .into_iter()
        .find(|l| l.contains("text \"grad\""))
        .expect("grad contrast finding");
    assert!(line.contains("4.1:1"), "{line}");
    assert!(line.contains("against #7d7d7d"), "{line}");
    assert!(line.contains("sampled"), "{line}");
    assert!(line.contains("midpoint"), "{line}");
    assert!(line.contains("mid=0.5s"), "{line}");
    assert!(line.contains("(measured)"), "{line}");
}

#[test]
fn audio_each_discarded_cause_names_clip_and_reason() {
    let root = tmp("discarded");
    let folder = proj(&root, DISCARDED);
    write_tone(&folder);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let lines = lint_lines(&out);
    assert_eq!(lines.len(), 5, "one finding per discarded cause:\n{out}");
    for (id, reason) in [
        ("dead-hidden", "hidden"),
        ("dead-quiet", "zero volume"),
        ("dead-fade", "fully faded"),
        ("dead-past", "fully outside"),
        ("dead-duck", "muted"),
    ] {
        let line = lines
            .iter()
            .find(|l| l.contains(&format!("\"{id}\"")))
            .unwrap_or_else(|| panic!("missing finding for {id}:\n{out}"));
        assert!(line.contains("lint[discarded-audio]"), "{line}");
        assert!(line.contains(reason), "{id} names its cause:\n{line}");
        assert!(line.contains("(static)"), "{line}");
        assert!(line.contains("(fix:"), "{line}");
    }
    // The voice stays audible, so the mix itself is not silent.
    assert!(
        !out.contains("lint[silent-audio]"),
        "an audible voice keeps the mix alive:\n{out}"
    );
    assert!(
        out.contains("lint: 5 finding(s) and 0 note(s) across 16 rules"),
        "{out}"
    );
}

#[test]
fn audio_silent_mix_reports_measured_peak() {
    let root = tmp("silent");
    let folder = proj(&root, SILENT);
    write_tone(&folder);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let lines = lint_lines(&out);
    assert_eq!(lines.len(), 3, "two discarded plus the silent mix:\n{out}");
    let line = lines
        .iter()
        .find(|l| l.contains("lint[silent-audio]"))
        .expect("silent-audio finding");
    // Measured values: the peak and the bar.
    assert!(line.contains("-inf dBFS"), "{line}");
    assert!(line.contains("-60 dBFS"), "{line}");
    assert!(line.contains("all 2 audio-bearing clips"), "{line}");
    assert!(line.contains("(measured)"), "{line}");
    assert!(line.contains("(fix:"), "{line}");
    assert!(
        out.contains("lint: 3 finding(s) and 0 note(s) across 16 rules"),
        "{out}"
    );
}

#[test]
fn contrast_unrunnable_backgrounds_leave_named_notes() {
    let root = tmp("note");
    proj(&root, NOTE);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    // Two backgrounds cannot be sampled — text over text, text half
    // off a box — while the dim text still reports its ratio.
    assert!(out.contains("lint-note[contrast]"), "{out}");
    assert!(out.contains("text \"over\""), "{out}");
    assert!(out.contains("text \"over2\""), "{out}");
    let lines = lint_lines(&out);
    assert_eq!(lines.len(), 1, "only the dim text reports:\n{out}");
    assert!(lines[0].contains("text \"dim\""), "{lines:?}");
    assert!(lines[0].contains("3.6:1"), "{lines:?}");
    assert!(
        out.contains("lint: 1 finding(s) and 2 note(s) across 16 rules"),
        "{out}"
    );
}

#[test]
fn contrast_audio_json_findings_carry_rule_source_location_and_fix() {
    let root = tmp("json");
    let folder = proj(&root, SILENT);
    write_tone(&folder);
    let (code, json) = run(&["lint", "--json", "proj"], &root);
    assert_eq!(code, 0, "{json}");
    let array = json_between(&json, "\"lint\":[", "],\"lint_notes\"").to_string();
    // Static and measured sources are labeled per rule.
    assert!(
        array.contains(
            "\"rule\":\"discarded-audio\",\"severity\":\"warning\",\"source\":\"static\""
        ),
        "{json}"
    );
    assert!(
        array
            .contains("\"rule\":\"silent-audio\",\"severity\":\"warning\",\"source\":\"measured\""),
        "{json}"
    );
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
    assert!(json.contains("-inf dBFS"), "{json}");
    assert!(json.contains("-60 dBFS"), "{json}");
    assert!(json.contains("zero volume"), "{json}");
    // Notes carry their own shape: rule, location, cause, fix.
    let notes = json_between(&json, "\"lint_notes\":[", "],\"lint_rules\"").to_string();
    assert!(notes.is_empty() || !notes.contains("\"rule\":"), "{notes}");

    let root = tmp("json-contrast");
    proj(&root, NOTE);
    let (code, json) = run(&["lint", "--json", "proj"], &root);
    assert_eq!(code, 0, "{json}");
    let array = json_between(&json, "\"lint\":[", "],\"lint_notes\"").to_string();
    assert!(
        array.contains("\"rule\":\"contrast\",\"severity\":\"warning\",\"source\":\"measured\""),
        "{json}"
    );
    let notes = json_between(&json, "\"lint_notes\":[", "],\"lint_rules\"").to_string();
    assert_eq!(notes.matches("\"rule\":\"contrast\"").count(), 2, "{notes}");
    assert!(notes.contains("\"cause\":"), "{notes}");
    assert!(notes.contains("\"workaround\":"), "{notes}");
    assert!(
        json.contains("sampled") || json.contains("cannot be measured"),
        "{json}"
    );
}

#[test]
fn contrast_audio_reports_the_same_findings_as_check() {
    let root = tmp("parity");
    proj(&root, CONTRAST);
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
fn contrast_audio_invalid_project_batches_diagnostics_without_contrast_or_audio() {
    let root = tmp("invalid");
    proj(&root, INVALID);
    let (code_l, lint_out) = run(&["lint", "proj"], &root);
    let (code_c, check_out) = run(&["check", "proj"], &root);
    assert_eq!((code_l, code_c), (1, 1), "{lint_out}\n{check_out}");
    assert!(lint_out.contains("error["), "{lint_out}");
    assert!(lint_out.contains("lint:"), "{lint_out}");
    // Broken windows would name garbage: an invalid project refuses
    // through diagnostics and reports no contrast or audio findings.
    for rule in [
        "lint[contrast]",
        "lint[silent-audio]",
        "lint[discarded-audio]",
    ] {
        assert!(
            !lint_out.contains(rule),
            "contrast/audio fires on invalid:\n{lint_out}"
        );
    }
    assert_eq!(
        lint_lines(&lint_out),
        lint_lines(&check_out),
        "{lint_out}\n{check_out}"
    );
}

#[test]
fn contrast_audio_every_finding_names_elements_with_measured_values() {
    let root = tmp("values");
    let folder = proj(&root, DISCARDED);
    write_tone(&folder);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    for line in lint_lines(&out) {
        // Every finding names its rule, severity, location, source,
        // the element, a measured value, and the fix.
        assert!(line.contains("warning"), "{line}");
        assert!(line.contains("composition.kdl:"), "{line}");
        assert!(line.contains("audio \""), "{line}");
        assert!(line.contains("(fix:"), "{line}");
    }
    let root = tmp("values-contrast");
    proj(&root, CONTRAST);
    let (code, out) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    for line in lint_lines(&out) {
        assert!(line.contains("warning"), "{line}");
        assert!(line.contains("composition.kdl:"), "{line}");
        assert!(line.contains("text \""), "{line}");
        assert!(line.contains(":1"), "{line}");
        assert!(line.contains("(measured)"), "{line}");
        assert!(line.contains("(fix:"), "{line}");
    }
}
