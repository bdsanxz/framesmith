//! Sprint 7 motion (T004 + T009): stagger delays and repeat shapes
//! validate, refusals name the bound or form, and window overflow
//! clamps with a reviewable report.
//!
//! Validates through the public [`validate_folder`] API only:
//! stagger origins (both spellings) and grid forms land on tweens and
//! keys; repeat counts 1 to 1000 with yoyo and gaps land too;
//! negative staggers, bad origins, bad grids, endless and over-limit
//! repeats, non-boolean yoyos, and negative gaps fail with
//! `E_BAD_MOTION`; staggered sets fail as unknown keys; every problem
//! reports together; fitting motion stays silent while overflowing
//! motion records one clamp entry per directive and stays valid.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n07-stagger-repeat-{tag}-{}-{}",
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
const CLIP: &str = "    box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4\n";

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

// --- valid stagger + repeat ------------------------------------------------

#[test]
fn stagger_origin_repeat_yoyo_and_gap_validate_on_tweens_and_keys() {
    let dir = tmp("valid");
    write_comp(
        &dir,
        &comp_with(
            "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 stagger=0.5 stagger-from=\"center\" repeat=3 yoyo=true repeat-delay=0.25\n    key target=0 property=\"y\" stagger=0.25 stagger-from=\"end\" repeat=2 {\n        frame at=0 value=0\n        frame at=1 value=60\n    }\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    let tween = &comp.tweens[0];
    assert_eq!(tween.stagger, 0.5);
    assert!(matches!(
        tween.stagger_from,
        notation::model::StaggerOrigin::Center
    ));
    assert!(tween.stagger_grid.is_none());
    assert_eq!(
        (tween.repeat, tween.yoyo, tween.repeat_delay),
        (3, true, 0.25)
    );
    let key = &comp.keys[0];
    assert_eq!(key.stagger, 0.25);
    assert!(matches!(
        key.stagger_from,
        notation::model::StaggerOrigin::End
    ));
    assert_eq!((key.repeat, key.yoyo, key.repeat_delay), (2, false, 0.0));
    // A single-piece box never overflows: no clamp report.
    assert!(v.stagger_clamps.is_empty());
}

#[test]
fn underscore_aliases_and_grid_forms_validate() {
    let dir = tmp("alias");
    write_comp(
        &dir,
        &comp_with(
            "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 stagger_grid=4 stagger_from=\"start\" repeat_delay=0.5\n    key target=0 property=\"y\" stagger-grid=\"4x3\" {\n        frame at=0 value=0\n        frame at=1 value=60\n    }\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!(
        comp.tweens[0].stagger_grid,
        Some(notation::model::StaggerGrid {
            cols: 4,
            rows: None
        })
    );
    assert_eq!(
        comp.keys[0].stagger_grid,
        Some(notation::model::StaggerGrid {
            cols: 4,
            rows: Some(3)
        })
    );
    assert_eq!(comp.tweens[0].repeat_delay, 0.5);
}

#[test]
fn fitting_staggered_split_motion_stays_silent() {
    // Two pieces ("AB"), motion 0..1, stagger 0.5 from the start: the
    // last piece ends at 1.5, inside the 2s clip window — valid with
    // no clamp entry.
    let dir = tmp("fit");
    write_comp(
        &dir,
        &format!(
            "{OPEN}    text value=\"AB\" start=0 duration=2 split=\"chars\"\n    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 stagger=0.5\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert!(v.stagger_clamps.is_empty());
}

// --- overflow clamp ----------------------------------------------------------

#[test]
fn overflowing_stagger_clamps_and_reports_with_numbers() {
    // Four pieces ("ABCD"), motion 0..2, stagger 0.5 from the start:
    // the last piece ends at 3.5, 1.5s past the 2s clip window.
    let dir = tmp("clamp");
    write_comp(
        &dir,
        &format!(
            "{OPEN}    text value=\"ABCD\" start=0 duration=2 split=\"chars\"\n    tween target=0 property=\"x\" kind=\"to\" start=0 duration=2 to=10 stagger=0.5\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(v.stagger_clamps.len(), 1);
    let clamp = &v.stagger_clamps[0];
    assert_eq!(clamp.kind, "tween");
    assert_eq!(clamp.target, 0);
    assert_eq!(clamp.pieces, 4);
    assert_eq!(clamp.stagger, 0.5);
    assert_eq!(clamp.origin, "start");
    assert!((clamp.max_delay - 1.5).abs() < 1e-9);
    assert!((clamp.motion_end - 3.5).abs() < 1e-9);
    assert!((clamp.clip_end - 2.0).abs() < 1e-9);
    assert!((clamp.overflow - 1.5).abs() < 1e-9);
}

#[test]
fn key_overflow_reports_and_repeat_extends_the_window() {
    // Three pieces ("ABC"), key 0..1 repeated twice from the end:
    // max delay 1.0, last pass ends at 2.0, motion ends at 3.0 —
    // 1.0s past the 2s window.
    let dir = tmp("key-clamp");
    write_comp(
        &dir,
        &format!(
            "{OPEN}    text value=\"ABC\" start=0 duration=2 split=\"chars\"\n    key target=0 property=\"x\" stagger=0.5 stagger-from=\"end\" repeat=2 {{\n        frame at=0 value=0\n        frame at=1 value=60\n    }}\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(v.stagger_clamps.len(), 1);
    let clamp = &v.stagger_clamps[0];
    assert_eq!(clamp.kind, "key");
    assert_eq!(clamp.pieces, 3);
    assert_eq!(clamp.origin, "end");
    assert!((clamp.max_delay - 1.0).abs() < 1e-9);
    assert!((clamp.motion_end - 3.0).abs() < 1e-9);
    assert!((clamp.overflow - 1.0).abs() < 1e-9);
}

// --- refusals ------------------------------------------------------------------

#[test]
fn stagger_and_repeat_refusals_name_the_bound_or_form() {
    let cases = [
        ("neg-stagger", "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 stagger=-0.5\n", "E_BAD_MOTION", "0 seconds and up"),
        ("neg-key-stagger", "    key target=0 property=\"x\" stagger=-1 {\n        frame at=0 value=0\n        frame at=1 value=1\n    }\n", "E_BAD_MOTION", "0 seconds and up"),
        ("bad-origin", "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 stagger=0.5 stagger-from=\"middle\"\n", "E_BAD_MOTION", "stagger origin"),
        ("zero-grid", "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 stagger-grid=0\n", "E_BAD_MOTION", "stagger-grid"),
        ("bad-grid", "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 stagger-grid=\"axb\"\n", "E_BAD_MOTION", "stagger-grid"),
        ("zero-repeat", "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 repeat=0\n", "E_BAD_MOTION", "endless repeat"),
        ("neg-repeat", "    key target=0 property=\"x\" repeat=-2 {\n        frame at=0 value=0\n        frame at=1 value=1\n    }\n", "E_BAD_MOTION", "endless repeat"),
        ("over-repeat", "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 repeat=1001\n", "E_BAD_MOTION", "1000-pass bound"),
        ("infinite-repeat", "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 repeat=\"infinite\"\n", "E_BAD_MOTION", "endless repeat"),
        ("whole-repeat", "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 repeat=1.5\n", "E_BAD_MOTION", "whole number"),
        ("bad-yoyo", "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 yoyo=\"yes\"\n", "E_BAD_MOTION", "yoyo"),
        ("neg-gap", "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 repeat-delay=-1\n", "E_BAD_MOTION", "0 and up"),
        ("set-stagger", "    set target=0 property=\"x\" start=0 duration=1 value=5 stagger=0.5\n", "E_UNKNOWN_KEY", "unknown set key"),
        ("set-repeat", "    set target=0 property=\"x\" start=0 duration=1 value=5 repeat=2\n", "E_UNKNOWN_KEY", "unknown set key"),
    ];
    for (tag, body, code, needle) in cases {
        let dir = tmp(tag);
        write_comp(&dir, &comp_with(body));
        let problems = codes_messages(&dir);
        assert_eq!(
            validate_folder(&dir).status,
            Status::Invalid,
            "{tag} must fail"
        );
        assert!(
            problems
                .iter()
                .any(|(c, m, _)| c == code && m.contains(needle)),
            "{tag}: expected {code} naming {needle:?}, got {problems:?}"
        );
    }
}

#[test]
fn every_repeat_problem_reports_together_and_names_the_range() {
    let dir = tmp("together");
    write_comp(
        &dir,
        &comp_with(
            "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 stagger=-0.5 repeat=0 yoyo=\"maybe\" repeat-delay=-1\n",
        ),
    );
    let problems = codes_messages(&dir);
    assert_eq!(validate_folder(&dir).status, Status::Invalid);
    assert_eq!(problems.len(), 4, "every bad key reports, got {problems:?}");
    assert!(problems.iter().all(|(c, _, _)| c == "E_BAD_MOTION"));
    let joined = problems
        .iter()
        .map(|(_, m, _)| m.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        joined.contains("1 (once) to 1000 passes"),
        "repeat names the range, got {joined:?}"
    );
}
