//! Sprint 10 sound (T007): the composition `autoplay` playback hint.
//!
//! Validates through the public [`validate_folder`] API only: absent
//! means the default still start (`false`); a bare `true`/`false` (or
//! the same quoted) validates and lands on the composition; anything
//! else fails with `E_BAD_FLAG` naming the accepted switch, the file,
//! the line, and the fix. Rendering never reads the flag (covered by
//! the CLI end-to-end byte-identity test); these tests prove the
//! declarations validate.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n08autoplay-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_comp(dir: &std::path::Path, root_attrs: &str) {
    std::fs::write(
        dir.join("composition.kdl"),
        format!(
            "composition name=\"main\" width=64 height=64 duration=2 fps=30 background=\"#000\" version=1{root_attrs} {{\n    text value=\"Hi\" start=0 duration=2\n}}\n"
        ),
    )
    .unwrap();
}

// --- valid declarations ------------------------------------------------------

#[test]
fn autoplay_defaults_to_still_start() {
    let dir = tmp("default");
    write_comp(&dir, "");
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert!(!v.composition.as_ref().unwrap().autoplay);
}

#[test]
fn autoplay_true_and_false_validate() {
    for (attrs, want) in [
        (" autoplay=true", true),
        (" autoplay=false", false),
        (" autoplay=\"true\"", true),
        (" autoplay=\"false\"", false),
    ] {
        let dir = tmp("switch");
        write_comp(&dir, attrs);
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Valid, "{attrs}: {:?}", v.diagnostics);
        assert_eq!(v.composition.as_ref().unwrap().autoplay, want, "{attrs}");
        assert_eq!(v.compositions[0].autoplay, want, "{attrs}");
    }
}

// --- refusals ----------------------------------------------------------------

#[test]
fn bad_autoplay_values_fail_naming_the_switch() {
    for attrs in [
        " autoplay=\"yes\"",
        " autoplay=2",
        " autoplay=1",
        " autoplay=\"\"",
    ] {
        let dir = tmp("bad");
        write_comp(&dir, attrs);
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Invalid, "{attrs}: {:?}", v.diagnostics);
        assert_eq!(v.diagnostics.len(), 1, "{attrs}: {:?}", v.diagnostics);
        let d = &v.diagnostics[0];
        assert_eq!(d.code, "E_BAD_FLAG", "{attrs}");
        assert!(d.message.contains("`autoplay`"), "{attrs}: {}", d.message);
        assert!(
            d.message.contains("`true` or `false`"),
            "{attrs}: {}",
            d.message
        );
        assert!(!d.fix.is_empty(), "{attrs}");
        assert!(d.line >= 1 && d.column >= 1, "{attrs}");
    }
}

#[test]
fn autoplay_survives_alongside_multi_composition_roots() {
    let dir = tmp("multi");
    std::fs::write(
        dir.join("composition.kdl"),
        "composition name=\"a\" width=64 height=64 duration=2 background=\"#000\" version=1 autoplay=true {\n    text value=\"Hi\" start=0 duration=2\n}\ncomposition name=\"b\" width=64 height=64 duration=1 background=\"#000\" version=1 root=true {\n    text value=\"Yo\" start=0 duration=1\n}\n",
    )
    .unwrap();
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let a = v.compositions.iter().find(|c| c.name == "a").unwrap();
    let b = v.compositions.iter().find(|c| c.name == "b").unwrap();
    assert!(a.autoplay);
    assert!(!b.autoplay);
}
