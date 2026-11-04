//! Sprint 11 motion blur (T007) validation: a composition-level
//! `motion-blur` element carries a `shutter` angle (0–360 degrees), a
//! whole-number `samples` budget (1–32), and a `tier` (`auto`, `high`,
//! `medium`, `low`). At most one per composition, attribute-only.
//!
//! Validates through the public [`validate_folder`] API only: the
//! element lands on the composition with its fields; every tier
//! validates with boundary values accepted; out-of-range shutters and
//! sample counts, unknown tiers, missing keys, duplicates, children,
//! and unknown keys fail naming `motion-blur` and the fix.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n11-blur-{tag}-{}-{}",
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

fn codes_messages(dir: &Path) -> Vec<(String, String, String)> {
    validate_folder(dir)
        .diagnostics
        .iter()
        .map(|d| (d.code.to_string(), d.message.clone(), d.fix.clone()))
        .collect()
}

fn has(dir: &Path, code: &str, needles: &[&str]) -> bool {
    codes_messages(dir)
        .iter()
        .any(|(c, m, f)| c == code && needles.iter().all(|n| m.contains(n) || f.contains(n)))
}

/// One still box plus a `motion-blur` element.
fn body(blur: &str) -> String {
    format!(
        "composition name=\"main\" root=true width=64 height=64 duration=2 background=\"#000\" version=1 {{\n box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=2\n {blur}\n}}\n"
    )
}

// --- T007: valid motion-blur ----------------------------------------------

#[test]
fn motion_blur_lands_with_its_fields() {
    let dir = tmp("ok");
    write_comp(
        &dir,
        &body("motion-blur shutter=180 samples=8 tier=\"high\""),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    let b = comp.motion_blur.expect("motion-blur lands");
    assert_eq!(b.shutter, 180.0);
    assert_eq!(b.samples, 8);
    assert_eq!(b.tier, notation::model::MotionBlurTier::High);
}

#[test]
fn every_tier_blurs() {
    for tier in notation::model::MotionBlurTier::ACCEPTED {
        let dir = tmp("tiers");
        write_comp(
            &dir,
            &body(&format!(
                "motion-blur shutter=180 samples=8 tier=\"{tier}\""
            )),
        );
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Valid, "tier {tier}: {:?}", v.diagnostics);
        let comp = v.composition.expect("valid composition");
        assert_eq!(comp.motion_blur.expect("lands").tier.name(), tier);
    }
}

#[test]
fn boundary_values_are_valid() {
    // Shutter endpoints and the sample-count endpoints all validate.
    for (shutter, samples) in [("0", "1"), ("360", "32"), ("0.5", "2"), ("359.9", "31")] {
        let dir = tmp("bounds");
        write_comp(
            &dir,
            &body(&format!(
                "motion-blur shutter={shutter} samples={samples} tier=\"auto\""
            )),
        );
        let v = validate_folder(&dir);
        assert_eq!(
            v.status,
            Status::Valid,
            "shutter {shutter} samples {samples}: {:?}",
            v.diagnostics
        );
    }
}

#[test]
fn blur_free_compositions_carry_no_blur() {
    let dir = tmp("plain");
    write_comp(&dir, &body(""));
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert!(v.composition.expect("valid").motion_blur.is_none());
}

// --- T007: refusals ---------------------------------------------------------

#[test]
fn shutter_outside_0_to_360_fails_naming_the_range() {
    for shutter in ["-1", "361", "720"] {
        let dir = tmp("shutter");
        write_comp(
            &dir,
            &body(&format!(
                "motion-blur shutter={shutter} samples=8 tier=\"high\""
            )),
        );
        let v = validate_folder(&dir);
        assert_eq!(
            v.status,
            Status::Invalid,
            "shutter {shutter}: {:?}",
            v.diagnostics
        );
        assert!(
            has(&dir, "E_BAD_BLUR", &["motion-blur", "shutter", "0 to 360"]),
            "{:?}",
            codes_messages(&dir)
        );
    }
}

#[test]
fn unreadable_shutter_fails_naming_the_range() {
    let dir = tmp("shutter-text");
    write_comp(
        &dir,
        &body("motion-blur shutter=\"wide\" samples=8 tier=\"high\""),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert!(
        has(&dir, "E_BAD_BLUR", &["motion-blur", "shutter", "0 to 360"]),
        "{:?}",
        codes_messages(&dir)
    );
}

#[test]
fn samples_outside_1_to_32_fail_naming_the_range() {
    // Below, above, and non-whole budgets all refuse.
    for samples in ["0", "33", "2.5"] {
        let dir = tmp("samples");
        write_comp(
            &dir,
            &body(&format!(
                "motion-blur shutter=180 samples={samples} tier=\"high\""
            )),
        );
        let v = validate_folder(&dir);
        assert_eq!(
            v.status,
            Status::Invalid,
            "samples {samples}: {:?}",
            v.diagnostics
        );
        assert!(
            has(&dir, "E_BAD_BLUR", &["motion-blur", "samples", "1 to 32"]),
            "{:?}",
            codes_messages(&dir)
        );
    }
}

#[test]
fn unreadable_samples_fail_naming_the_range() {
    let dir = tmp("samples-text");
    write_comp(
        &dir,
        &body("motion-blur shutter=180 samples=\"many\" tier=\"high\""),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert!(
        has(&dir, "E_BAD_BLUR", &["motion-blur", "samples", "1 to 32"]),
        "{:?}",
        codes_messages(&dir)
    );
}

#[test]
fn unknown_tier_fails_naming_the_accepted_set() {
    let dir = tmp("tier");
    write_comp(
        &dir,
        &body("motion-blur shutter=180 samples=8 tier=\"ultra\""),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert!(
        has(
            &dir,
            "E_BAD_BLUR",
            &["motion-blur", "ultra", "auto", "high", "medium", "low"]
        ),
        "{:?}",
        codes_messages(&dir)
    );
}

#[test]
fn missing_keys_fail_naming_the_key() {
    // Each required key refused on its own, naming the element.
    for (element, needle) in [
        ("motion-blur samples=8 tier=\"high\"", "`shutter`"),
        ("motion-blur shutter=180 tier=\"high\"", "`samples`"),
        ("motion-blur shutter=180 samples=8", "`tier`"),
    ] {
        let dir = tmp("missing");
        write_comp(&dir, &body(element));
        assert_eq!(validate_folder(&dir).status, Status::Invalid);
        assert!(
            has(&dir, "E_BAD_BLUR", &["motion-blur", needle]),
            "{element}: {:?}",
            codes_messages(&dir)
        );
    }
}

#[test]
fn duplicate_motion_blur_fails() {
    let dir = tmp("dup");
    write_comp(
        &dir,
        &body("motion-blur shutter=180 samples=8 tier=\"high\"\n motion-blur shutter=90 samples=4 tier=\"low\""),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert!(
        has(&dir, "E_BAD_BLUR", &["two `motion-blur`", "at most one"]),
        "{:?}",
        codes_messages(&dir)
    );
}

#[test]
fn motion_blur_children_fail() {
    let dir = tmp("child");
    write_comp(
        &dir,
        &body("motion-blur shutter=180 samples=8 tier=\"high\" {\n grade\n}"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert!(
        has(&dir, "E_BAD_BLUR", &["motion-blur", "child"]),
        "{:?}",
        codes_messages(&dir)
    );
}

#[test]
fn unknown_motion_blur_keys_fail() {
    let dir = tmp("bad-key");
    write_comp(
        &dir,
        &body("motion-blur shutter=180 samples=8 tier=\"high\" angle=90"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert!(
        has(&dir, "E_UNKNOWN_KEY", &["motion-blur", "angle"]),
        "{:?}",
        codes_messages(&dir)
    );
}
