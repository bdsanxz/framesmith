//! Sprint 7 motion (T001 + T007): keyframe chains validate and the full
//! curve set parses.
//!
//! Validates through the public [`validate_folder`] API only: ordered
//! stops with per-segment easings land in the composition; unordered,
//! duplicate, unknown-property, and wrong-shape stops fail naming the
//! stop; malformed and unknown easings fail naming the accepted forms.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n07-{tag}-{}-{}",
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

// --- valid keyframes -------------------------------------------------------

#[test]
fn ordered_stops_with_per_segment_easings_validate() {
    let dir = tmp("valid");
    write_comp(
        &dir,
        &comp_with(
            "    key target=0 property=\"x\" ease=\"out-quad\" {\n        frame at=0 value=10\n        frame at=1 value=60 ease=\"in-quad\"\n        frame at=2 value=110\n    }\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!(comp.keys.len(), 1);
    let key = &comp.keys[0];
    assert_eq!(key.target, 0);
    assert_eq!(key.property, "x");
    assert!(matches!(key.ease, Some(notation::model::EaseDecl::OutQuad)));
    assert_eq!(key.stops.len(), 3);
    assert_eq!((key.stops[0].at, key.stops[0].value), (0.0, 10.0));
    assert!(key.stops[0].ease.is_none(), "stop without ease stays none");
    assert!(matches!(
        key.stops[1].ease,
        Some(notation::model::EaseDecl::InQuad)
    ));
    assert_eq!((key.stops[2].at, key.stops[2].value), (2.0, 110.0));
}

#[test]
fn every_new_curve_validates_on_tweens_frames_and_keys() {
    let dir = tmp("curves");
    write_comp(
        &dir,
        &comp_with(
            "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 ease=\"cubic-bezier(0.25,0.1,0.25,1)\"\n    tween target=0 property=\"y\" kind=\"to\" start=0 duration=1 to=10 ease=\"steps(4)\"\n    tween target=0 property=\"x\" kind=\"to\" start=1 duration=1 to=20 ease=\"spring\"\n    tween target=0 property=\"y\" kind=\"to\" start=1 duration=1 to=20 ease=\"spring(100,5)\"\n    tween target=0 property=\"x\" kind=\"to\" start=2 duration=1 to=30 ease=\"bounce\"\n    tween target=0 property=\"y\" kind=\"to\" start=2 duration=1 to=30 ease=\"elastic\"\n    tween target=0 property=\"x\" kind=\"to\" start=3 duration=0.5 to=40 ease=\"wiggle\"\n    tween target=0 property=\"y\" kind=\"to\" start=3 duration=0.5 to=40 ease=\"wiggle(2)\"\n    key target=0 property=\"opacity\" {\n        frame at=0 value=0 ease=\"hold\"\n        frame at=1 value=1\n    }\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!(comp.tweens.len(), 8);
    assert_eq!(comp.keys.len(), 1);
    assert!(matches!(
        comp.tweens[0].ease,
        notation::model::EaseDecl::CubicBezier(_, _, _, _)
    ));
    assert!(matches!(
        comp.tweens[1].ease,
        notation::model::EaseDecl::Steps(4)
    ));
    assert!(matches!(
        comp.tweens[2].ease,
        notation::model::EaseDecl::Spring(s, d) if s == 100.0 && d == 5.0
    ));
    assert!(matches!(
        comp.tweens[6].ease,
        notation::model::EaseDecl::Wiggle(3)
    ));
    assert!(matches!(
        comp.keys[0].stops[0].ease,
        Some(notation::model::EaseDecl::Hold)
    ));
}

#[test]
fn underscore_ease_aliases_validate_identically() {
    let dir = tmp("alias");
    write_comp(
        &dir,
        &comp_with(
            "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 ease=\"in_quad\"\n    tween target=0 property=\"y\" kind=\"to\" start=1 duration=1 to=10 ease=\"out_back(1.7)\"\n    tween target=0 property=\"x\" kind=\"to\" start=2 duration=1 to=10 ease=\"cubic_bezier(0.25,0.1,0.25,1)\"\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert!(matches!(
        comp.tweens[0].ease,
        notation::model::EaseDecl::InQuad
    ));
    assert!(matches!(
        comp.tweens[1].ease,
        notation::model::EaseDecl::OutBack(_)
    ));
    assert!(matches!(
        comp.tweens[2].ease,
        notation::model::EaseDecl::CubicBezier(_, _, _, _)
    ));
}

// --- keyframe refusals -----------------------------------------------------

#[test]
fn unordered_stops_fail_naming_the_stop() {
    let dir = tmp("unordered");
    write_comp(
        &dir,
        &comp_with(
            "    key target=0 property=\"x\" {\n        frame at=0 value=10\n        frame at=2 value=60\n        frame at=1 value=110\n    }\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert!(v.composition.is_none());
    let found: Vec<_> = codes_messages(&dir);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "E_BAD_MOTION");
    assert!(found[0].1.contains("stop 3"), "{:?}", found[0].1);
    assert!(found[0].1.contains("time order"), "{:?}", found[0].1);
    assert!(found[0].2.contains("reorder"), "{:?}", found[0].2);
}

#[test]
fn duplicate_stops_fail_naming_both_and_hinting_hold() {
    let dir = tmp("duplicate");
    write_comp(
        &dir,
        &comp_with(
            "    key target=0 property=\"x\" {\n        frame at=0 value=10\n        frame at=1 value=60\n        frame at=1 value=110\n    }\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    let found = codes_messages(&dir);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "E_BAD_MOTION");
    assert!(found[0].1.contains("duplicate"), "{:?}", found[0].1);
    assert!(found[0].1.contains("stops 2 and 3"), "{:?}", found[0].1);
    assert!(found[0].2.contains("hold"), "{:?}", found[0].2);
}

#[test]
fn unknown_properties_fail_with_fixes() {
    let dir = tmp("badprop");
    write_comp(
        &dir,
        &comp_with(
            "    key target=0 property=\"zebra\" {\n        frame at=0 value=10\n        frame at=1 value=60\n    }\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    let found = codes_messages(&dir);
    assert!(
        found
            .iter()
            .any(|(c, m, _)| c == "E_BAD_MOTION" && m.contains("\"zebra\"")),
        "{found:?}"
    );
}

#[test]
fn later_properties_wait_for_their_sprint() {
    // Sprint 7 motion (T006) animates `blur` through keys, so the
    // later-property refusal now exercises `scale-x` (still Sprint 7).
    let dir = tmp("laterprop");
    write_comp(
        &dir,
        &comp_with(
            "    key target=0 property=\"scale-x\" {\n        frame at=0 value=0\n        frame at=1 value=3\n    }\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Unsupported, "{:?}", v.diagnostics);
    let found = codes_messages(&dir);
    assert!(
        found
            .iter()
            .any(|(c, m, _)| c == "E_UNSUPPORTED" && m.contains("\"scale-x\"")),
        "{found:?}"
    );
}

#[test]
fn wrong_shape_values_fail() {
    for (tag, frames) in [
        (
            "unreadable",
            "frame at=0 value=\"abc\"\n        frame at=1 value=60\n",
        ),
        (
            "opacity-range",
            "frame at=0 value=0\n        frame at=1 value=1.5\n",
        ),
        (
            "missing-at",
            "frame value=10\n        frame at=1 value=60\n",
        ),
        ("missing-value", "frame at=0\n        frame at=1 value=60\n"),
        (
            "negative-at",
            "frame at=-1 value=10\n        frame at=1 value=60\n",
        ),
    ] {
        let dir = tmp(&format!("shape-{tag}"));
        write_comp(
            &dir,
            &comp_with(&format!(
                "    key target=0 property=\"{}\" {{\n        {}\n    }}\n",
                if tag == "opacity-range" {
                    "opacity"
                } else {
                    "x"
                },
                frames
            )),
        );
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Invalid, "{tag}: {:?}", v.diagnostics);
        let found = codes_messages(&dir);
        assert!(
            found.iter().all(|(c, _, _)| c == "E_BAD_MOTION"),
            "{tag}: {found:?}"
        );
        assert!(
            found.iter().any(|(_, m, _)| m.contains("stop")),
            "{tag}: every refusal names its stop: {found:?}"
        );
    }
}

#[test]
fn too_few_stops_fail() {
    for (tag, inner) in [("none", ""), ("one", "frame at=0 value=10\n")] {
        let dir = tmp(&format!("few-{tag}"));
        write_comp(
            &dir,
            &comp_with(&format!(
                "    key target=0 property=\"x\" {{\n        {inner}    }}\n"
            )),
        );
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Invalid, "{tag}: {:?}", v.diagnostics);
        let found = codes_messages(&dir);
        assert!(
            found
                .iter()
                .any(|(c, m, _)| c == "E_BAD_MOTION" && m.contains("at least two")),
            "{tag}: {found:?}"
        );
    }
}

#[test]
fn non_frame_children_and_nesting_fail() {
    let dir = tmp("child");
    write_comp(
        &dir,
        &comp_with(
            "    key target=0 property=\"x\" {\n        frame at=0 value=10\n        tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=5 ease=\"linear\"\n    }\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    let found = codes_messages(&dir);
    assert!(
        found
            .iter()
            .any(|(c, m, _)| c == "E_BAD_MOTION" && m.contains("only `frame` stops")),
        "{found:?}"
    );

    let dir = tmp("grandchild");
    write_comp(
        &dir,
        &comp_with(
            "    key target=0 property=\"x\" {\n        frame at=0 value=10 {\n            frame at=0 value=1\n        }\n        frame at=1 value=60\n    }\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    let found = codes_messages(&dir);
    assert!(
        found
            .iter()
            .any(|(c, m, _)| c == "E_BAD_MOTION" && m.contains("no children")),
        "{found:?}"
    );
}

#[test]
fn unknown_key_and_frame_attributes_fail() {
    let dir = tmp("attrs");
    write_comp(
        &dir,
        &comp_with(
            "    key target=0 property=\"x\" speed=2 {\n        frame at=0 value=10 wobble=1\n        frame at=1 value=60\n    }\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    let found = codes_messages(&dir);
    assert!(
        found
            .iter()
            .any(|(c, m, _)| c == "E_UNKNOWN_KEY" && m.contains("`key`")),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|(c, m, _)| c == "E_UNKNOWN_KEY" && m.contains("`frame`")),
        "{found:?}"
    );
}

#[test]
fn every_stop_problem_reports_together() {
    let dir = tmp("together");
    write_comp(
        &dir,
        &comp_with(
            "    key target=0 property=\"x\" {\n        frame at=0 value=\"abc\"\n        frame at=2 value=60 ease=\"fast\"\n        frame at=1 value=110\n    }\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    let found = codes_messages(&dir);
    assert!(found.len() >= 3, "all stops report together: {found:?}");
    assert!(
        found.iter().any(|(c, _, _)| c == "E_BAD_EASING"),
        "{found:?}"
    );
    assert!(
        found.iter().filter(|(c, _, _)| c == "E_BAD_MOTION").count() >= 2,
        "{found:?}"
    );
}

// --- easing refusals ---------------------------------------------------------

#[test]
fn malformed_parameters_fail_naming_the_accepted_forms() {
    for (tag, ease) in [
        ("steps-zero", "steps(0)"),
        ("steps-frac", "steps(2.5)"),
        ("steps-bare", "steps"),
        ("spring-neg", "spring(-1,5)"),
        ("spring-one", "spring(100)"),
        ("spring-three", "spring(100,5,2)"),
        ("bez-three", "cubic-bezier(0.25,0.1,0.25)"),
        ("bez-x-range", "cubic-bezier(2,0,0,1)"),
        ("bez-nonnum", "cubic-bezier(a,0,0,1)"),
        ("bez-bare", "cubic-bezier"),
        ("bounce-param", "bounce(2)"),
        ("hold-param", "hold(1)"),
        ("linear-param", "linear(1)"),
        ("outback-bare", "out-back"),
        ("unbalanced", "spring(100,5"),
    ] {
        let dir = tmp(&format!("ease-{tag}"));
        write_comp(
            &dir,
            &comp_with(&format!(
                "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 ease=\"{ease}\"\n"
            )),
        );
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Invalid, "{tag}: {:?}", v.diagnostics);
        let found = codes_messages(&dir);
        assert_eq!(found.len(), 1, "{tag}: {found:?}");
        assert_eq!(found[0].0, "E_BAD_EASING", "{tag}: {found:?}");
        assert!(
            found[0].1.contains("accepted easings are:"),
            "{tag}: names the accepted forms: {:?}",
            found[0].1
        );
    }
}

#[test]
fn unknown_ease_names_fail() {
    for (tag, ease) in [("fast", "fast"), ("ease-in", "ease-in"), ("snap", "snap")] {
        let dir = tmp(&format!("unknown-{tag}"));
        write_comp(
            &dir,
            &comp_with(&format!(
                "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 ease=\"{ease}\"\n"
            )),
        );
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Invalid, "{tag}: {:?}", v.diagnostics);
        let found = codes_messages(&dir);
        assert_eq!(found.len(), 1, "{tag}: {found:?}");
        assert_eq!(found[0].0, "E_BAD_EASING", "{tag}: {found:?}");
        assert!(
            found[0].1.contains("unknown easing"),
            "{tag}: {:?}",
            found[0].1
        );
        assert!(
            found[0].1.contains("accepted easings are:"),
            "{tag}: {:?}",
            found[0].1
        );
    }
}

#[test]
fn existing_easings_still_validate() {
    let dir = tmp("existing");
    let mut body = String::new();
    for (i, ease) in [
        "linear",
        "in-quad",
        "out-quad",
        "in-out-quad",
        "in-cubic",
        "out-cubic",
        "in-out-cubic",
        "in-quart",
        "out-quart",
        "in-out-quart",
        "in-quint",
        "out-quint",
        "in-out-quint",
        "in-sine",
        "out-sine",
        "in-out-sine",
        "in-expo",
        "out-expo",
        "in-out-expo",
        "out-back(1.7)",
    ]
    .into_iter()
    .enumerate()
    {
        body.push_str(&format!(
            "    tween target=0 property=\"x\" kind=\"to\" start={} duration=0.1 to=10 ease=\"{ease}\"\n",
            i as f64 * 0.1
        ));
    }
    write_comp(&dir, &comp_with(&body));
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(v.composition.expect("valid composition").tweens.len(), 20);
}
