//! Sprint 6 T009: hidden layers skip picture and mix but stay validated.
//!
//! Validates through the public [`validate_folder`] API only: `hidden`
//! parses on every clip kind, defaults to visible, refuses non-boolean
//! values with `E_BAD_HIDDEN`, keeps hidden ids referenceable, and
//! still reports every other error on a hidden clip.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n06hidden-{}-{}-{}",
        tag,
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

fn codes(dir: &Path) -> Vec<String> {
    validate_folder(dir)
        .diagnostics
        .iter()
        .map(|d| d.code.to_string())
        .collect()
}

const OPEN: &str =
    "composition name=\"main\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n";

// --- parsing and defaults ----------------------------------------------------

#[test]
fn hidden_parses_on_every_clip_kind() {
    let dir = tmp("all-kinds");
    write_comp(
        &dir,
        &format!(
            "composition name=\"main\" root=true width=64 height=64 duration=2 background=\"#000\" version=1 {{\n\
             text value=\"A\" start=0 duration=1 hidden=true\n\
             box x=0 y=0 width=8 height=8 fill=\"#ff0000\" start=0 duration=1 hidden=true\n\
             image path=\"p.png\" start=0 duration=1 hidden=true\n\
             audio path=\"s.wav\" start=0 duration=1 hidden=true\n\
             nest nest=\"bumper\" start=0 duration=1 hidden=true\n\
             }}\n\
             composition name=\"bumper\" width=64 height=64 duration=1 background=\"#000\" version=1 {{\n\
             text value=\"B\" start=0 duration=1\n\
             }}\n"
        ),
    );
    // Media files are absent, so the project is invalid — but the only
    // diagnostics must be the missing files, never a key complaint.
    let only = codes(&dir);
    assert!(
        only.iter().all(|c| c == "E_MISSING_FILE"),
        "hidden must parse everywhere, got {only:?}"
    );
    assert_eq!(only.len(), 2, "image + audio missing files, got {only:?}");
}

#[test]
fn hidden_defaults_to_visible_and_accepts_false() {
    let dir = tmp("defaults");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1\n\
             text value=\"B\" start=0 duration=1 hidden=false\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!(comp.clips.len(), 2);
    assert!(!comp.clips[0].hidden(), "absent hidden means visible");
    assert!(!comp.clips[1].hidden(), "hidden=false means visible");
}

#[test]
fn hidden_true_is_stored_on_the_clip() {
    let dir = tmp("stored");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box x=0 y=0 width=8 height=8 fill=\"#ff0000\" start=0 duration=1 hidden=true id=\"h\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert!(comp.clips[0].hidden(), "hidden=true must be stored");
    assert_eq!(comp.clips[0].id().unwrap_or(""), "h");
}

#[test]
fn non_boolean_hidden_refuses_with_location() {
    let dir = tmp("bad-type");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1 hidden=\"yes\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(codes(&dir), vec!["E_BAD_HIDDEN".to_string()]);
    let d = &v.diagnostics[0];
    assert_eq!(d.line, 2, "diagnostic names the clip line, got {d}");
    assert!(d.column > 0, "diagnostic names a column, got {d}");
    assert!(!d.file.is_empty(), "diagnostic names the file, got {d}");
    assert!(!d.fix.is_empty(), "diagnostic names a fix, got {d}");
    assert!(
        d.message.contains("hidden"),
        "message names the key, got {d}"
    );
}

// --- references and full validation ------------------------------------------

#[test]
fn hidden_id_still_answers_start_after() {
    let dir = tmp("ref");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box x=0 y=0 width=8 height=8 fill=\"#ff0000\" start=0 duration=1 hidden=true id=\"base\"\n\
             text value=\"B\" duration=1 start-after=\"base\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert!((comp.clips[1].start() - 1.0).abs() < 1e-9);
}

#[test]
fn hidden_clip_followed_by_hidden_follower_resolves() {
    let dir = tmp("chain");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1 hidden=true id=\"a\"\n\
             text value=\"B\" duration=1 hidden=true start-after=\"a\" id=\"b\"\n\
             text value=\"C\" duration=1 start-after=\"b\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert!((comp.clips[2].start() - 2.0).abs() < 1e-9);
}

#[test]
fn hidden_clip_errors_are_reported_not_skipped() {
    let dir = tmp("errors");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1 hidden=true track=\"bogus\"\n\
             box x=0 y=0 width=8 height=8 fill=\"nope\" start=0 duration=1 hidden=true\n\
             text start=0 duration=1 hidden=true\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    let mut got = codes(&dir);
    got.sort();
    assert_eq!(
        got,
        vec![
            "E_BAD_COLOR".to_string(),
            "E_BAD_TRACK".to_string(),
            "E_MISSING_FIELD".to_string(),
        ],
        "every hidden error reports alongside the rest"
    );
}

#[test]
fn hidden_clip_unknown_reference_still_refuses() {
    let dir = tmp("ghost");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" duration=1 hidden=true start-after=\"ghost\"\n\
             }}\n"
        ),
    );
    assert_eq!(codes(&dir), vec!["E_UNKNOWN_REF".to_string()]);
}
