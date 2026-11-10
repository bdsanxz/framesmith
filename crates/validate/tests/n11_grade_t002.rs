//! Sprint 11 grade detail stages (T002) validation: `wheel`, `curve`,
//! `hsl`, `vignette`, and `grain` children of `grade` on clips, lane
//! groups, and the composition, with inward-out piece composition.
//!
//! Validates through the public [`validate_folder`] API only: every
//! stage lands where written and merges piece by piece; malformed
//! curve lists, out-of-range detail values, bad grain seeds and sizes,
//! duplicated stages, and unknown stage keys or children fail naming
//! the element and the fix.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n11-detail-{tag}-{}-{}",
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

fn touch_media(dir: &Path) {
    std::fs::write(dir.join("photo.png"), b"\x89PNG\r\n\x1a\n".to_vec()).unwrap();
    std::fs::write(dir.join("clip.mp4"), []).unwrap();
    std::fs::write(dir.join("s.wav"), []).unwrap();
}

const OPEN: &str =
    "composition name=\"main\" root=true width=160 height=120 duration=4 background=\"#000\" version=1 {\n";

fn codes_messages(dir: &Path) -> Vec<(String, String, String)> {
    validate_folder(dir)
        .diagnostics
        .iter()
        .map(|d| (d.code.to_string(), d.message.clone(), d.fix.clone()))
        .collect()
}

fn codes(dir: &Path) -> Vec<String> {
    codes_messages(dir)
        .iter()
        .map(|(c, _, _)| c.clone())
        .collect()
}

// --- T002: stages validate on every level ------------------------------------

#[test]
fn detail_stages_validate_on_clip_lane_and_composition() {
    // Every stage on a clip, one stage per lane, two stages on the
    // composition: all validate and land on the authored structs.
    let dir = tmp("levels-ok");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             grade preset=\"filmic\" {{\n wheel shadows=0.2 highlights=-0.1\n curve master=\"0,0 1,1\"\n}}\n\
             lane track=\"video\" {{\n grade {{\n curve red=\"0,0 0.5,0.6 1,1\"\n}}\n}}\n\
             lane track=\"graphics\" {{\n grade {{\n grain amount=0.3 size=2 seed=7\n}}\n}}\n\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade {{\n wheel midtones=0.1\n hsl hue-min=300 hue-max=60 saturation=0.2 lightness=0.1\n vignette amount=0.5 softness=0.4\n}}\n}}\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!(comp.grade.wheel.shadows, Some(0.2));
    assert_eq!(comp.grade.wheel.highlights, Some(-0.1));
    assert_eq!(comp.grade.wheel.midtones, None);
    assert_eq!(comp.grade.curve.master, Some(vec![(0.0, 0.0), (1.0, 1.0)]));
    assert_eq!(comp.lane_grades.len(), 2);
    assert_eq!(
        comp.lane_grades[0].grade.curve.red,
        Some(vec![(0.0, 0.0), (0.5, 0.6), (1.0, 1.0)])
    );
    assert_eq!(comp.lane_grades[1].grade.grain.amount, Some(0.3));
    assert_eq!(comp.lane_grades[1].grade.grain.size, Some(2));
    assert_eq!(comp.lane_grades[1].grade.grain.seed, Some(7));
    use notation::model::Clip;
    match &comp.clips[0] {
        Clip::Box(c) => {
            assert_eq!(c.grade.wheel.midtones, Some(0.1));
            assert_eq!(c.grade.hsl.hue_min, Some(300.0));
            assert_eq!(c.grade.hsl.hue_max, Some(60.0));
            assert_eq!(c.grade.hsl.saturation, Some(0.2));
            assert_eq!(c.grade.hsl.lightness, Some(0.1));
            assert_eq!(c.grade.vignette.amount, Some(0.5));
            assert_eq!(c.grade.vignette.softness, Some(0.4));
        }
        other => panic!("expected box, got {other:?}"),
    }
}

#[test]
fn empty_and_partial_stages_are_skipped_not_zeroed() {
    // Bare stage children validate as identity; partial stages leave
    // the other knobs unset for the outer levels to fill.
    let dir = tmp("partial-ok");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade {{\n wheel\n curve red=\"0,0 1,1\"\n hsl lightness=0.2\n vignette amount=0.6\n grain amount=0.4\n}}\n}}\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    use notation::model::Clip;
    match &comp.clips[0] {
        Clip::Box(c) => {
            assert!(c.grade.wheel.is_empty());
            assert_eq!(c.grade.curve.red, Some(vec![(0.0, 0.0), (1.0, 1.0)]));
            assert_eq!(c.grade.curve.green, None);
            assert_eq!(c.grade.hsl.lightness, Some(0.2));
            assert_eq!(c.grade.hsl.hue_min, None);
            assert_eq!(c.grade.vignette.amount, Some(0.6));
            assert_eq!(c.grade.vignette.softness, None);
            assert_eq!(c.grade.grain.amount, Some(0.4));
            assert_eq!(c.grade.grain.size, None);
            assert_eq!(c.grade.grain.seed, None);
        }
        other => panic!("expected box, got {other:?}"),
    }
}

#[test]
fn detail_values_accept_variable_bindings() {
    // A `number` variable binding in a detail attribute resolves like
    // a literal (Sprint 9 variables ride every non-structural key).
    let dir = tmp("detail-var");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             variable id=\"lift\" type=\"number\" default=0.25\n\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade {{\n wheel shadows=\"var(--lift)\"\n}}\n}}\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    use notation::model::Clip;
    match &comp.clips[0] {
        Clip::Box(c) => assert_eq!(c.grade.wheel.shadows, Some(0.25)),
        other => panic!("expected box, got {other:?}"),
    }
}

#[test]
fn audio_clip_detail_grades_validate() {
    // Detail stages on an audio clip validate (the grade rests inert:
    // audio draws no picture).
    let dir = tmp("audio-ok");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             audio path=\"s.wav\" start=0 duration=4 {{\n grade {{\n wheel shadows=0.1\n grain amount=0.2 seed=3\n}}\n}}\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
}

// --- T002: refusals -----------------------------------------------------------

#[test]
fn malformed_curve_lists_fail_naming_element_and_shape() {
    // Every malformed list fails with the element (`curve`), the
    // channel, and the expected point-list shape, plus a concrete fix.
    let cases: &[(&str, &str)] = &[
        ("red=\"0,0\"", "a lone point"),
        ("red=\"0,0 0.5\"", "a broken pair"),
        ("red=\"a,b 1,1\"", "non-numbers"),
        ("red=\"0,0 2,1\"", "an out-of-range number"),
        ("red=\"0,0 0.5,0.5 0.5,0.9 1,1\"", "a flat `x` step"),
        ("red=\"0,0 0.7,0.5 0.4,0.9 1,1\"", "a falling `x`"),
        ("red=\"\"", "an empty list"),
        ("red=1", "a bare number"),
        ("green=\"0,0 0.5,0.6 1,1 1,0.5\"", "a duplicated end `x`"),
    ];
    for (attr, label) in cases {
        let dir = tmp("bad-curve");
        touch_media(&dir);
        write_comp(
            &dir,
            &format!(
                "{OPEN}\
                 box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade {{\n curve {attr}\n}}\n}}\n\
                 }}\n"
            ),
        );
        let all = codes_messages(&dir);
        assert_eq!(all.len(), 1, "{label}: {all:?}");
        let (code, message, fix) = &all[0];
        assert_eq!(code, "E_BAD_GRADE", "{label}: {all:?}");
        assert!(message.contains("curve"), "{label}: {message}");
        assert!(
            message.contains("0,0 0.5,0.5 1,1"),
            "{label} names the shape: {message}"
        );
        assert!(!fix.is_empty(), "{label} carries a fix: {fix}");
        assert!(
            fix.contains("curve"),
            "{label} fix names the element: {fix}"
        );
    }
}

#[test]
fn out_of_range_detail_values_fail_naming_attribute_and_range() {
    // Each detail value outside its range fails naming the stage, the
    // attribute, and the accepted range.
    let cases: &[(&str, &str, &str, &str)] = &[
        ("wheel shadows=2", "wheel", "shadows", "-1 to 1"),
        ("wheel highlights=-1.5", "wheel", "highlights", "-1 to 1"),
        ("hsl hue-min=400", "hsl", "hue-min", "0 to 360"),
        ("hsl hue-max=-10", "hsl", "hue-max", "0 to 360"),
        ("hsl saturation=1.5", "hsl", "saturation", "0 to 1"),
        ("hsl lightness=2", "hsl", "lightness", "-1 to 1"),
        ("vignette amount=1.5", "vignette", "amount", "0 to 1"),
        ("vignette softness=-0.1", "vignette", "softness", "0 to 1"),
        ("grain amount=2", "grain", "amount", "0 to 1"),
        ("grain size=0", "grain", "size", "1 to 64"),
        ("grain size=65", "grain", "size", "1 to 64"),
        ("grain size=1.5", "grain", "size", "1 to 64"),
        ("grain seed=1.5", "grain", "seed", "whole number"),
        ("grain seed=\"\"", "grain", "seed", "whole number"),
    ];
    for (stage_attr, stage, name, range) in cases {
        let (stage_name, props) = stage_attr.split_once(' ').unwrap();
        let dir = tmp("bad-detail");
        touch_media(&dir);
        write_comp(
            &dir,
            &format!(
                "{OPEN}\
                 box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade {{\n {stage_name} {props}\n}}\n}}\n\
                 }}\n"
            ),
        );
        let all = codes_messages(&dir);
        assert_eq!(all.len(), 1, "{stage_attr}: {all:?}");
        let (code, message, fix) = &all[0];
        assert_eq!(code, "E_BAD_GRADE", "{stage_attr}: {all:?}");
        assert!(message.contains(*stage), "{stage_attr}: {message}");
        assert!(message.contains(*name), "{stage_attr}: {message}");
        assert!(message.contains(*range), "{stage_attr}: {message}");
        assert!(fix.contains(*name), "{stage_attr}: {fix}");
    }
    // Boundary values validate: every range is inclusive, whole-pixel
    // sizes admit their edges, and any whole seed (even negative) works.
    let dir = tmp("detail-edges-ok");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade {{\n wheel shadows=-1 midtones=1 highlights=0\n curve red=\"0,0 1,1\" blue=\"0,1 1,0\"\n hsl hue-min=0 hue-max=360 saturation=1 lightness=-1\n vignette amount=1 softness=0\n grain amount=1 size=64 seed=-12\n}}\n}}\n\
             box x=30 y=0 width=20 height=12 fill=\"#00ff00\" start=0 duration=4 {{\n grade {{\n grain size=1\n}}\n}}\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
}

#[test]
fn duplicated_stages_fail() {
    // Two `wheel` children on one grade fail naming the duplicate.
    for stage in ["wheel", "curve", "hsl", "vignette", "grain"] {
        let dir = tmp("dup-stage");
        touch_media(&dir);
        write_comp(
            &dir,
            &format!(
                "{OPEN}\
                 box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade {{\n {stage}\n {stage}\n}}\n}}\n\
                 }}\n"
            ),
        );
        let all = codes_messages(&dir);
        assert_eq!(all.len(), 1, "{stage}: {all:?}");
        assert_eq!(all[0].0, "E_BAD_GRADE", "{stage}: {all:?}");
        assert!(
            all[0].1.contains(&format!("two `{stage}`")),
            "{stage} names the duplicate: {}",
            all[0].1
        );
        assert!(!all[0].2.is_empty(), "{stage} carries a fix");
    }
}

#[test]
fn stage_keys_and_children_fail() {
    // An unknown `wheel` key fails as an unknown key.
    let dir = tmp("stage-bad-key");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade {{\n wheel warmth=1\n}}\n}}\n\
             }}\n"
        ),
    );
    assert_eq!(
        codes(&dir),
        vec!["E_UNKNOWN_KEY"],
        "{:?}",
        codes_messages(&dir)
    );

    // An unknown `grade` child (not a stage) fails naming the element.
    let dir = tmp("grade-bad-stage");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade {{\n span value=\"Hi\"\n}}\n}}\n\
             }}\n"
        ),
    );
    let all = codes_messages(&dir);
    assert_eq!(codes(&dir), vec!["E_BAD_GRADE"], "{all:?}");
    assert!(
        all[0].1.contains("grade"),
        "names the element: {}",
        all[0].1
    );
    assert!(all[0].1.contains("span"), "names the child: {}", all[0].1);
    assert!(!all[0].2.is_empty(), "fix is concrete: {}", all[0].2);

    // A child inside a stage fails naming the stage.
    let dir = tmp("stage-bad-child");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade {{\n vignette amount=0.5 {{\n box width=4 height=4 fill=\"#fff\"\n}}\n}}\n}}\n\
             }}\n"
        ),
    );
    let all = codes_messages(&dir);
    assert_eq!(codes(&dir), vec!["E_BAD_GRADE"], "{all:?}");
    assert!(
        all[0].1.contains("vignette"),
        "names the stage: {}",
        all[0].1
    );
    assert!(!all[0].2.is_empty(), "fix is concrete: {}", all[0].2);
}
