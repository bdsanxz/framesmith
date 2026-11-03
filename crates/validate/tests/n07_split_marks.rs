//! Sprint 7 motion (T002 + T003): split/typewriter/count text effects
//! and mark times with `start-after` references.
//!
//! Validates through the public [`validate_folder`] API only: every
//! split mode lands on the text clip, typewriter reveals need a split,
//! count ranges interpolate with declared decimals, contradictory and
//! non-text effects fail, marks name composition times that
//! `start-after` accepts, and unknown or out-of-time marks fail.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n07-split-marks-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_comp(dir: &Path, body: &str) {
    std::fs::write(dir.join("composition.kdl"), body).unwrap();
}

const OPEN: &str =
    "composition name=\"main\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n";
const CLIP: &str = "    text value=\"Hi\" start=0 duration=4\n";

fn comp_with(body: &str) -> String {
    format!("{OPEN}{CLIP}{body}}}\n")
}

fn codes_messages(dir: &Path) -> Vec<(String, String, String)> {
    validate_folder(dir)
        .diagnostics
        .iter()
        .map(|d| (d.code.to_string(), d.message.clone(), d.fix.clone()))
        .collect()
}

// --- split / typewriter / count (T002, CAP-009) ----------------------------

#[test]
fn each_split_mode_validates_and_lands_on_the_clip() {
    for mode in ["chars", "words", "lines"] {
        let dir = tmp(&format!("split-{mode}"));
        write_comp(
            &dir,
            &comp_with(&format!(
                "    text value=\"ab cd\" start=0 duration=1 split=\"{mode}\"\n"
            )),
        );
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Valid, "{mode}: {:?}", v.diagnostics);
        let comp = v.composition.expect("valid composition");
        assert_eq!(comp.clips.len(), 2);
        match &comp.clips[1] {
            notation::model::Clip::Text(tc) => {
                assert_eq!(tc.split.map(|m| m.name()), Some(mode));
                assert!(!tc.typewriter);
                assert_eq!((tc.count_from, tc.count_to, tc.decimals), (None, None, 0));
            }
            other => panic!("expected a text clip, got {other:?}"),
        }
    }
}

#[test]
fn typewriter_with_a_split_and_counts_validate() {
    let dir = tmp("tw-count");
    write_comp(
        &dir,
        &comp_with(
            "    text value=\"abcd\" start=0 duration=2 split=\"chars\" typewriter=true\n    text value=\"n\" start=2 duration=2 count-from=0 count-to=100 decimals=0\n    text value=\"f\" start=0 duration=2 count-from=0 count-to=1 decimals=2\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    match &comp.clips[1] {
        notation::model::Clip::Text(tc) => {
            assert!(tc.typewriter);
            assert_eq!(tc.split.map(|m| m.name()), Some("chars"));
        }
        other => panic!("expected a text clip, got {other:?}"),
    }
    match &comp.clips[2] {
        notation::model::Clip::Text(tc) => {
            assert_eq!(
                (tc.count_from, tc.count_to, tc.decimals),
                (Some(0.0), Some(100.0), 0)
            );
        }
        other => panic!("expected a text clip, got {other:?}"),
    }
    match &comp.clips[3] {
        notation::model::Clip::Text(tc) => {
            assert_eq!(
                (tc.count_from, tc.count_to, tc.decimals),
                (Some(0.0), Some(1.0), 2)
            );
        }
        other => panic!("expected a text clip, got {other:?}"),
    }
}

#[test]
fn unknown_split_modes_fail_naming_the_accepted_modes() {
    let dir = tmp("badmode");
    write_comp(
        &dir,
        &comp_with("    text value=\"Hi\" start=0 duration=1 split=\"letters\"\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    let found = codes_messages(&dir);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "E_BAD_MOTION");
    assert!(
        found[0].1.contains("chars, words, lines"),
        "{:?}",
        found[0].1
    );
}

#[test]
fn contradictory_text_modes_fail() {
    for (tag, body, needle) in [
        (
            "typewriter-without-split",
            "    text value=\"Hi\" start=0 duration=1 typewriter=true\n",
            "needs a `split`",
        ),
        (
            "half-count",
            "    text value=\"Hi\" start=0 duration=1 count-from=0\n",
            "both `count-from` and `count-to`",
        ),
        (
            "split-plus-count",
            "    text value=\"Hi\" start=0 duration=1 split=\"chars\" count-from=0 count-to=9\n",
            "cannot both `split`",
        ),
        (
            "typewriter-plus-count",
            "    text value=\"Hi\" start=0 duration=1 split=\"chars\" typewriter=true count-from=0 count-to=9\n",
            "cannot both `typewriter`",
        ),
        (
            "decimals-alone",
            "    text value=\"Hi\" start=0 duration=1 decimals=2\n",
            "needs a count range",
        ),
        (
            "bad-typewriter-shape",
            "    text value=\"Hi\" start=0 duration=1 split=\"chars\" typewriter=\"yes\"\n",
            "`true` or `false`",
        ),
    ] {
        let dir = tmp(&format!("contra-{tag}"));
        write_comp(&dir, &comp_with(body));
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Invalid, "{tag}: {:?}", v.diagnostics);
        let found = codes_messages(&dir);
        assert!(
            found.iter().all(|(c, _, _)| c == "E_BAD_MOTION"),
            "{tag}: {found:?}"
        );
        assert!(
            found.iter().any(|(_, m, _)| m.contains(needle)),
            "{tag}: refusals must name the contradiction: {found:?}"
        );
    }
}

#[test]
fn splits_on_non_text_fail_naming_the_input() {
    for (tag, body) in [
        (
            "box",
            "    box x=1 y=1 width=10 height=10 fill=\"#ff0000\" start=0 duration=1 split=\"chars\"\n",
        ),
        (
            "image",
            "    image path=\"p.png\" start=0 duration=1 typewriter=true\n",
        ),
        (
            "video",
            "    video path=\"c.mp4\" start=0 duration=1 count-from=0 count-to=1\n",
        ),
        (
            "audio",
            "    audio path=\"s.wav\" start=0 duration=1 split=\"words\"\n",
        ),
        (
            "nest",
            "    nest nest=\"main\" start=0 duration=1 split=\"lines\"\n",
        ),
    ] {
        let dir = tmp(&format!("nontext-{tag}"));
        write_comp(&dir, &comp_with(body));
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Invalid, "{tag}: {:?}", v.diagnostics);
        let found = codes_messages(&dir);
        assert!(
            found.iter().all(|(c, _, _)| c == "E_BAD_MOTION"),
            "{tag}: {found:?}"
        );
        assert!(
            found
                .iter()
                .any(|(_, m, _)| m.contains("apply to text clips only")),
            "{tag}: refusal must name the text-only rule: {found:?}"
        );
    }
}

// --- marks and start-after (T003, CAP-003) ----------------------------------

#[test]
fn marks_validate_and_start_after_resolves_to_the_marked_instant() {
    let dir = tmp("marks-ok");
    write_comp(
        &dir,
        &comp_with(
            "    mark name=\"beat\" at=1.5\n    text value=\"B\" start-after=\"beat\" duration=1\n    text value=\"C\" start-after=\"beat\" start-offset=0.5 duration=1\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!(comp.marks.len(), 1);
    assert_eq!(
        (comp.marks[0].name.as_str(), comp.marks[0].at),
        ("beat", 1.5)
    );
    assert!(
        (comp.clips[1].start() - 1.5).abs() < 1e-9,
        "follows the mark"
    );
    assert!(
        (comp.clips[2].start() - 2.0).abs() < 1e-9,
        "mark plus offset"
    );
}

#[test]
fn mark_at_the_composition_end_is_inside_time() {
    let dir = tmp("mark-edge");
    write_comp(&dir, &comp_with("    mark name=\"end\" at=4\n"));
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
}

#[test]
fn unknown_marks_fail_naming_the_reference() {
    let dir = tmp("mark-unknown");
    write_comp(
        &dir,
        &comp_with("    text value=\"B\" start-after=\"ghost\" duration=1\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    let found = codes_messages(&dir);
    assert!(
        found
            .iter()
            .any(|(c, m, _)| c == "E_UNKNOWN_REF" && m.contains("\"ghost\"")),
        "{found:?}"
    );
}

#[test]
fn marks_outside_composition_time_fail() {
    let dir = tmp("mark-outside");
    write_comp(&dir, &comp_with("    mark name=\"late\" at=9\n"));
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    let found = codes_messages(&dir);
    assert!(
        found.iter().any(|(c, m, _)| c == "E_BAD_TIMING"
            && m.contains("\"late\"")
            && m.contains("outside composition time")),
        "{found:?}"
    );
}

#[test]
fn duplicate_mark_names_fail_as_duplicates() {
    let dir = tmp("mark-dup");
    write_comp(
        &dir,
        &comp_with("    text value=\"A\" start=0 duration=1 id=\"x\"\n    mark name=\"x\" at=1\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    let found = codes_messages(&dir);
    assert!(found.iter().any(|(c, _, _)| c == "E_DUP_ID"), "{found:?}");
}

#[test]
fn marks_from_another_composition_do_not_resolve() {
    use std::fmt::Write as _;
    let dir = tmp("mark-cross");
    let mut body = String::from(
        "composition name=\"a\" width=64 height=64 duration=4 background=\"#000\" version=1 root=true {\n    text value=\"A\" start=0 duration=4\n    mark name=\"beat\" at=1\n}\n",
    );
    let _ = write!(
        body,
        "composition name=\"b\" width=64 height=64 duration=4 background=\"#000\" version=1 {{\n    text value=\"B\" start-after=\"beat\" duration=1\n}}\n"
    );
    write_comp(&dir, &body);
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    let found = codes_messages(&dir);
    assert!(
        found.iter().any(|(c, m, _)| c == "E_UNKNOWN_REF"
            && m.contains("\"beat\"")
            && m.contains("mark in composition")),
        "{found:?}"
    );
}

#[test]
fn malformed_marks_fail_naming_the_input() {
    for (tag, body) in [
        ("missing-name", "    mark at=1\n"),
        ("bad-name", "    mark name=\"has space\" at=1\n"),
        ("missing-at", "    mark name=\"m\"\n"),
        ("negative-at", "    mark name=\"m\" at=-1\n"),
        ("extra-key", "    mark name=\"m\" at=1 speed=2\n"),
    ] {
        let dir = tmp(&format!("mark-bad-{tag}"));
        write_comp(&dir, &comp_with(body));
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Invalid, "{tag}: {:?}", v.diagnostics);
        let found = codes_messages(&dir);
        assert!(!found.is_empty(), "{tag}: every malformed mark reports");
        assert!(
            found.iter().all(
                |(c, _, _)| ["E_BAD_ID", "E_BAD_TIMING", "E_UNKNOWN_KEY"].contains(&c.as_str())
            ),
            "{tag}: {found:?}"
        );
    }
}
