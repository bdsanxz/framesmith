//! Sprint 24 T004: slideshow reference manifest through the
//! validation gate — the same verdict `check` reports.
//!
//! The one-command proof (`tools/n24-slideshow-proof.sh`) stages
//! the reference to a workdir copy and points this test at it with
//! `FRAMESMITH_N24_PROOF_REF`, so the manifest gate runs on exactly
//! the staged fixture bytes. Without the variable the test falls
//! back to the checkout reference, so `cargo test` alone still
//! proves the same property on the same files.
//!
//! Covers the T001 acceptance criteria at the gate: one manifest
//! per project holding slides, notes, fragments, branches,
//! hotspots, and autoplay; slides reference compositions by id;
//! unknown compositions, broken branch and hotspot targets,
//! empty decks, and zero autoplay each refuse naming the file
//! and ref.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Diagnostic, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n24slideshow-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn reference_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("FRAMESMITH_N24_PROOF_REF") {
        PathBuf::from(dir)
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/slideshow-reference")
    }
}

fn reference_kdl() -> String {
    let dir = reference_dir();
    std::fs::read_to_string(dir.join("composition.kdl"))
        .unwrap_or_else(|_| panic!("reference composition.kdl missing in {}", dir.display()))
}

fn validate_body(body: &str) -> validate::validate::Verdict {
    let dir = tmp("case");
    std::fs::write(dir.join("composition.kdl"), body).unwrap();
    validate_folder(&dir)
}

fn assert_refusal(ds: &[Diagnostic], code: &str, needles: &[&str]) {
    let hit = ds.iter().any(|d| {
        d.code == code && needles.iter().all(|n| d.message.contains(n))
    });
    assert!(
        hit,
        "expected {code} naming {needles:?}; got: {:?}",
        ds.iter()
            .map(|d| (d.code.to_string(), d.message.clone()))
            .collect::<Vec<_>>()
    );
    assert!(
        ds.iter().any(|d| d.file.ends_with("composition.kdl")),
        "refusal must name the file; got: {:?}",
        ds.iter().map(|d| &d.file).collect::<Vec<_>>()
    );
}

#[test]
fn proof_reference_validates_with_full_manifest() {
    let verdict = validate_folder(&reference_dir());
    assert_eq!(
        verdict.status as i32,
        Status::Valid as i32,
        "reference must validate; got: {:?}",
        verdict
            .diagnostics
            .iter()
            .map(|d| (d.code.to_string(), d.message.clone()))
            .collect::<Vec<_>>()
    );
    let deck = verdict.slideshow.expect("reference declares a manifest");
    assert_eq!(deck.autoplay, Some(1.0), "autoplay carries through");
    let ids: Vec<&str> = deck.slides.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["s1", "s2", "s3"], "slide order mirrors the manifest");
    let (s1, s2, s3) = (&deck.slides[0], &deck.slides[1], &deck.slides[2]);
    assert_eq!(s1.composition, "intro");
    assert_eq!(s2.composition, "agenda");
    assert_eq!(s3.composition, "detail");
    assert_eq!(s1.notes.as_deref(), Some("welcome"));
    assert_eq!(s2.notes.as_deref(), Some("topics"));
    assert_eq!(s3.notes.as_deref(), Some("wrap up"));
    assert_eq!(s1.fragments.len(), 1, "s1 carries one fragment");
    assert_eq!(s1.fragments[0].at, 0.5);
    assert!(s1.branches.is_empty(), "s1 branches by nothing");
    assert_eq!(s1.hotspots.len(), 1, "s1 carries one hotspot");
    assert_eq!(s1.hotspots[0].id, "h1");
    assert_eq!(s1.hotspots[0].target, "s3");
    assert_eq!(s2.branches.len(), 1, "s2 carries one branch");
    assert_eq!(s2.branches[0].to, "s3");
    assert_eq!(deck.first_slide().map(|s| s.id.as_str()), Some("s1"));
}

#[test]
fn proof_unknown_slide_composition_refuses_naming_the_ref() {
    let body = reference_kdl().replacen(
        "composition=\"intro\"",
        "composition=\"missing_comp\"",
        1,
    );
    let verdict = validate_body(&body);
    assert_eq!(verdict.status as i32, Status::Invalid as i32);
    assert_refusal(&verdict.diagnostics, "E_UNKNOWN_REF", &["missing_comp"]);
}

#[test]
fn proof_broken_branch_and_hotspot_refuse_naming_the_targets() {
    let body = reference_kdl().replacen("branch to=\"s3\"", "branch to=\"gone\"", 1);
    let verdict = validate_body(&body);
    assert_eq!(verdict.status as i32, Status::Invalid as i32);
    assert_refusal(&verdict.diagnostics, "E_UNKNOWN_REF", &["gone"]);

    let body = reference_kdl().replacen("target=\"s3\"", "target=\"gone\"", 1);
    let verdict = validate_body(&body);
    assert_eq!(verdict.status as i32, Status::Invalid as i32);
    assert_refusal(&verdict.diagnostics, "E_UNKNOWN_REF", &["gone"]);
}

#[test]
fn proof_empty_deck_and_bad_autoplay_refuse_with_lint() {
    let empty =
        "composition name=\"solo\" root=true width=160 height=90 fps=10 duration=1 background=\"#101010\" version=1 {\n    text id=\"t\" value=\"Solo\" start=0 duration=1 x=10 y=30 size=16 color=\"#ffffff\"\n}\nslideshow autoplay=1 {\n}\n";
    let verdict = validate_body(empty);
    assert_eq!(verdict.status as i32, Status::Invalid as i32);
    assert_refusal(&verdict.diagnostics, "E_EMPTY_DECK", &["no slides"]);
    assert!(
        verdict
            .lint_findings
            .iter()
            .any(|f| f.rule == "empty-deck"),
        "empty-deck lint must fire; got: {:?}",
        verdict.lint_findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );

    let bad = reference_kdl().replacen("autoplay=1", "autoplay=0", 1);
    let verdict = validate_body(&bad);
    assert_eq!(verdict.status as i32, Status::Invalid as i32);
    assert_refusal(&verdict.diagnostics, "E_BAD_AUTOPLAY", &["autoplay"]);
    assert!(
        verdict
            .lint_findings
            .iter()
            .any(|f| f.rule == "bad-autoplay"),
        "bad-autoplay lint must fire; got: {:?}",
        verdict.lint_findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
}

#[test]
fn proof_missing_manifest_reads_as_none_but_stays_valid() {
    let body =
        "composition name=\"solo\" root=true width=160 height=90 fps=10 duration=1 background=\"#101010\" version=1 {\n    text id=\"t\" value=\"Solo\" start=0 duration=1 x=10 y=30 size=16 color=\"#ffffff\"\n}\n";
    let verdict = validate_body(body);
    assert_eq!(verdict.status as i32, Status::Valid as i32);
    assert!(
        verdict.slideshow.is_none(),
        "no `slideshow` node reads as no manifest; present refuses on that"
    );
}

#[test]
fn proof_reference_reference_dir_is_a_real_folder() {
    let dir = reference_dir();
    assert!(
        dir.join("composition.kdl").is_file(),
        "reference composition.kdl missing in {}",
        dir.display()
    );
    let _ = Path::new(&dir);
}
