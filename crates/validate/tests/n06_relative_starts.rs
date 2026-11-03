//! Sprint 6 T002: relative starts resolve to exact effective windows.
//!
//! Validates through the public [`validate_folder`] API only: exact,
//! offset, negative-overlap, and chained windows resolve; self, cycle,
//! unknown, before-zero, and ambiguous starts each fail with one
//! diagnostic naming the chain or values.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n06t002-{tag}-{}-{}",
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

fn starts(dir: &Path) -> Vec<f64> {
    validate_folder(dir)
        .composition
        .expect("valid composition")
        .clips
        .iter()
        .map(|c| c.start())
        .collect()
}

const OPEN: &str =
    "composition name=\"main\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n";

// --- exact and offset windows ------------------------------------------------

#[test]
fn follows_reference_end_exactly() {
    let dir = tmp("exact");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1 id=\"a\"\n\
             text value=\"B\" duration=2 id=\"b\" start-after=\"a\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(starts(&dir), vec![0.0, 1.0]);
}

#[test]
fn positive_offset_shifts_later() {
    let dir = tmp("offset-pos");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1 id=\"a\"\n\
             text value=\"B\" duration=1 start-after=\"a\" start-offset=0.5\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(starts(&dir), vec![0.0, 1.5]);
}

#[test]
fn negative_offset_overlaps_as_authored() {
    let dir = tmp("offset-neg");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=2 id=\"a\"\n\
             text value=\"B\" duration=2 start-after=\"a\" start-offset=-1.5\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(starts(&dir), vec![0.0, 0.5]);
}

#[test]
fn underscore_spellings_resolve() {
    let dir = tmp("underscore");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1 id=\"a\"\n\
             text value=\"B\" duration=1 start_after=\"a\" start_offset=0.25\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(starts(&dir), vec![0.0, 1.25]);
}

#[test]
fn chains_resolve_in_order() {
    let dir = tmp("chain");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1 id=\"a\"\n\
             text value=\"B\" duration=1 id=\"b\" start-after=\"a\"\n\
             text value=\"C\" duration=1 id=\"c\" start-after=\"b\" start-offset=0.5\n\
             box duration=1 id=\"d\" width=4 height=4 start-after=\"c\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(starts(&dir), vec![0.0, 1.0, 2.5, 3.5]);
}

#[test]
fn forward_reference_to_later_clip_resolves() {
    // Document order never constrains the reference graph.
    let dir = tmp("forward");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"B\" duration=1 id=\"b\" start-after=\"a\"\n\
             text value=\"A\" start=0 duration=2 id=\"a\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(starts(&dir), vec![2.0, 0.0]);
}

#[test]
fn composition_name_answers_as_composition_end() {
    let dir = tmp("comp-target");
    write_comp(
        &dir,
        "composition name=\"main\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n\
         text value=\"A\" start=0 duration=3 id=\"a\"\n\
         text value=\"Tail\" duration=1 start-after=\"main\" start-offset=-1\n\
         }\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(starts(&dir), vec![0.0, 3.0]);
}

#[test]
fn anonymous_relative_clip_resolves() {
    // The follower needs no id of its own; only the target is named.
    let dir = tmp("anon");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=1 duration=1 id=\"a\"\n\
             text value=\"B\" duration=1 start-after=\"a\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(starts(&dir), vec![1.0, 2.0]);
}

// --- refusals ----------------------------------------------------------------

#[test]
fn two_cycle_fails_once_naming_the_chain() {
    let dir = tmp("cycle2");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" duration=1 id=\"a\" start-after=\"b\"\n\
             text value=\"B\" duration=1 id=\"b\" start-after=\"a\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(v.diagnostics.len(), 1, "{:?}", v.diagnostics);
    let d = &v.diagnostics[0];
    assert_eq!(d.code, "E_BAD_TIMING");
    assert!(d.message.contains("\"a\""), "{}", d.message);
    assert!(d.message.contains("\"b\""), "{}", d.message);
    assert!(d.message.contains("cycl"), "{}", d.message);
    assert!(d.message.contains("->"), "{}", d.message);
    assert!(!d.fix.is_empty());
    assert!(d.file.ends_with("composition.kdl"));
}

#[test]
fn three_cycle_fails_once_naming_every_link() {
    let dir = tmp("cycle3");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" duration=1 id=\"a\" start-after=\"c\"\n\
             text value=\"B\" duration=1 id=\"b\" start-after=\"a\"\n\
             text value=\"C\" duration=1 id=\"c\" start-after=\"b\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(v.diagnostics.len(), 1, "{:?}", v.diagnostics);
    let d = &v.diagnostics[0];
    assert_eq!(d.code, "E_BAD_TIMING");
    for id in ["\"a\"", "\"b\"", "\"c\""] {
        assert!(d.message.contains(id), "{}", d.message);
    }
}

#[test]
fn follower_of_a_cycle_adds_no_cascade() {
    let dir = tmp("cycle-dep");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" duration=1 id=\"a\" start-after=\"b\"\n\
             text value=\"B\" duration=1 id=\"b\" start-after=\"a\"\n\
             text value=\"C\" duration=1 start-after=\"a\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(codes(&dir), vec!["E_BAD_TIMING"]);
}

#[test]
fn before_zero_fails_naming_values() {
    let dir = tmp("neg");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1 id=\"a\"\n\
             text value=\"B\" duration=1 start-after=\"a\" start-offset=-2\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(v.diagnostics.len(), 1, "{:?}", v.diagnostics);
    let d = &v.diagnostics[0];
    assert_eq!(d.code, "E_BAD_TIMING");
    assert!(d.message.contains("\"a\""), "{}", d.message);
    assert!(d.message.contains("-1"), "{}", d.message);
    assert!(!d.fix.is_empty());
}

#[test]
fn follower_of_before_zero_adds_no_cascade() {
    let dir = tmp("neg-dep");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1 id=\"a\"\n\
             text value=\"B\" duration=1 id=\"b\" start-after=\"a\" start-offset=-2\n\
             text value=\"C\" duration=1 start-after=\"b\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(codes(&dir), vec!["E_BAD_TIMING"]);
}

#[test]
fn absolute_plus_relative_is_ambiguous() {
    let dir = tmp("ambiguous");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1 id=\"a\"\n\
             text value=\"B\" start=1 duration=1 start-after=\"a\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(v.diagnostics.len(), 1, "{:?}", v.diagnostics);
    let d = &v.diagnostics[0];
    assert_eq!(d.code, "E_BAD_TIMING");
    assert!(d.message.contains("`start`"), "{}", d.message);
    assert!(d.message.contains("`start-after`"), "{}", d.message);
    assert_eq!((d.line, d.column), (3, d.column));
    assert!(d.file.ends_with("composition.kdl"));
    assert!(!d.fix.is_empty());
}

#[test]
fn ambiguous_reference_records_no_cascade() {
    // Both present with an unknown target: the ambiguous combination
    // reports once and the unknown target is not chased.
    let dir = tmp("ambiguous-unknown");
    write_comp(
        &dir,
        &format!("{OPEN}text value=\"B\" start=1 duration=1 start-after=\"ghost\"\n}}\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(codes(&dir), vec!["E_BAD_TIMING"]);
}

#[test]
fn lone_offset_without_reference_fails() {
    let dir = tmp("lone-offset");
    write_comp(
        &dir,
        &format!("{OPEN}text value=\"B\" start=0 duration=1 start-offset=0.5\n}}\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(v.diagnostics.len(), 1, "{:?}", v.diagnostics);
    assert_eq!(v.diagnostics[0].code, "E_BAD_TIMING");
}

#[test]
fn non_numeric_offset_fails_singly() {
    let dir = tmp("bad-offset");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1 id=\"a\"\n\
             text value=\"B\" duration=1 start-after=\"a\" start-offset=\"soon\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(v.diagnostics.len(), 1, "{:?}", v.diagnostics);
    assert_eq!(v.diagnostics[0].code, "E_BAD_TIMING");
}

#[test]
fn unknown_reference_with_offset_reports_once() {
    let dir = tmp("unknown-offset");
    write_comp(
        &dir,
        &format!("{OPEN}text value=\"B\" duration=1 start-after=\"ghost\" start-offset=0.5\n}}\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(codes(&dir), vec!["E_UNKNOWN_REF"]);
}

#[test]
fn several_timing_problems_report_together() {
    let dir = tmp("together");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1 id=\"a\"\n\
             text value=\"B\" start=1 duration=1 start-after=\"a\"\n\
             text value=\"C\" duration=1 start-after=\"a\" start-offset=-5\n\
             text value=\"D\" duration=1 id=\"x\" start-after=\"y\"\n\
             text value=\"E\" duration=1 id=\"y\" start-after=\"x\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(v.diagnostics.len(), 3, "{:?}", v.diagnostics);
    assert_eq!(
        codes(&dir),
        vec!["E_BAD_TIMING", "E_BAD_TIMING", "E_BAD_TIMING"]
    );
}

// --- absolute projects unchanged ---------------------------------------------

#[test]
fn absolute_start_projects_render_exactly_as_before() {
    let dir = tmp("absolute");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=2\n\
             text value=\"B\" start=1 duration=1\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(starts(&dir), vec![0.0, 1.0]);
}
