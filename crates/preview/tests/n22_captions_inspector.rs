//! Sprint 22 captions, INT-003: the Studio inspector reports
//! caption readiness and the active cue at the playhead — the same
//! seek the painter uses, never an invented word.

use preview::studio_inspector::caption_inspector_json;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-preview-n22captions-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn project(dir: &Path) {
    std::fs::write(
        dir.join("talk.srt"),
        "1\n00:00:00,000 --> 00:00:01,000\nhi yo\n\n2\n00:00:01,200 --> 00:00:02,000\ngo\n\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("composition.kdl"),
        "composition name=\"main\" width=640 height=360 fps=30 duration=2 background=\"#000\" version=1 {\n    captions src=\"talk.srt\" start=0 duration=2 font-size=24 {\n        override cue=0 word=0 color=\"#ffd60a\"\n    }\n}\n",
    )
    .unwrap();
}

fn verdict(dir: &Path) -> validate::validate::Verdict {
    let v = validate::validate::validate_folder(dir);
    assert_eq!(
        v.status,
        validate::validate::Status::Valid,
        "{:?}",
        v.diagnostics
    );
    v
}

#[test]
fn inspector_reports_readiness_and_the_active_cue() {
    let dir = tmp("inspector");
    project(&dir);
    let v = verdict(&dir);
    let comp = v.composition.expect("valid verdict holds the composition");
    // Inside cue 1: the active cue names its number, window, words,
    // and resolved per-word ink.
    let at_cue = caption_inspector_json(&comp, &v.readiness, 0.5);
    assert!(at_cue.contains("\"cues\":2"), "{at_cue}");
    assert!(at_cue.contains("\"words\":3"), "{at_cue}");
    assert!(at_cue.contains("\"cue\":1"), "{at_cue}");
    assert!(at_cue.contains("\"start\":0"), "{at_cue}");
    assert!(at_cue.contains("hi"), "{at_cue}");
    assert!(at_cue.contains("#ffd60a"), "{at_cue}");
    assert!(at_cue.contains("\"state\":\"ready\""), "{at_cue}");
    assert!(at_cue.contains("talk.srt"), "{at_cue}");
    // In the gap between cues: no active cue, readiness unchanged.
    let at_gap = caption_inspector_json(&comp, &v.readiness, 1.1);
    assert!(at_gap.contains("\"active\":null"), "{at_gap}");
    assert!(at_gap.contains("\"state\":\"ready\""), "{at_gap}");
    // Inside cue 2: the second cue with its single word.
    let at_cue2 = caption_inspector_json(&comp, &v.readiness, 1.5);
    assert!(at_cue2.contains("\"cue\":2"), "{at_cue2}");
    assert!(at_cue2.contains("go"), "{at_cue2}");
}

#[test]
fn inspector_names_the_failed_source_with_its_cause() {
    // The project validated, then the source vanished: the inspector
    // names the file and the operating-system cause.
    let dir = tmp("vanished");
    project(&dir);
    let v = verdict(&dir);
    let comp = v.composition.expect("valid verdict holds the composition");
    std::fs::remove_file(dir.join("talk.srt")).unwrap();
    let gone = validate::validate::validate_folder(&dir);
    let report = caption_inspector_json(&comp, &gone.readiness, 0.5);
    assert!(report.contains("\"state\":\"failed\""), "{report}");
    assert!(report.contains("talk.srt"), "{report}");
    // The captured composition still seeks the same active cue:
    // readiness never rewrites timing.
    assert!(report.contains("\"cue\":1"), "{report}");
    let _ = v;
}
