//! Sprint 11 grade (T001) validation: presets plus adjust attributes on
//! `grade` children of clips, lane groups, and the composition, with
//! inward-out level composition.
//!
//! Validates through the public [`validate_folder`] API only: every
//! preset lands on every clip kind; composition, lane, and clip grades
//! compose with the clip winning conflicts and missing pieces skipped;
//! unknown presets, out-of-range adjusts, duplicated grades, bad lanes,
//! and unknown keys or children fail with the element and the fix.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n11-grade-{tag}-{}-{}",
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

/// Dummy media files: validation checks presence (and image magic
/// bytes), never decodes.
fn touch_media(dir: &Path) {
    std::fs::write(dir.join("photo.png"), b"\x89PNG\r\n\x1a\n".to_vec()).unwrap();
    std::fs::write(
        dir.join("pic.svg"),
        b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"48\" height=\"32\"></svg>".to_vec(),
    )
    .unwrap();
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

// --- T001: presets on every clip kind ----------------------------------------

#[test]
fn presets_land_on_every_clip_kind() {
    // One `grade` preset per clip kind; the preset lands on the clip.
    let dir = tmp("presets-ok");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"Hi\" start=0 duration=4 {{\n grade preset=\"filmic\"\n}}\n\
             video path=\"clip.mp4\" start=0 duration=4 {{\n grade preset=\"noir\"\n}}\n\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade preset=\"teal-orange\"\n}}\n\
             flex x=0 y=0 width=60 height=30 flex-direction=\"row\" start=0 duration=4 {{\n grade preset=\"pastel\"\n box width=20 height=10 fill=\"#00ff00\"\n}}\n\
             image path=\"photo.png\" start=0 duration=4 {{\n grade preset=\"bleach\"\n}}\n\
             path d=\"M10,10 L50,10 L50,50 Z\" start=0 duration=4 {{\n grade preset=\"none\"\n}}\n\
             svg path=\"pic.svg\" width=48 height=32 start=0 duration=4 {{\n grade preset=\"filmic\"\n}}\n\
             audio path=\"s.wav\" start=0 duration=4 {{\n grade preset=\"noir\"\n}}\n\
             nest nest=\"bumper\" start=0 duration=1 {{\n grade preset=\"pastel\"\n}}\n\
             }}\n\
             composition name=\"bumper\" width=64 height=64 duration=1 background=\"#000\" version=1 {{\n\
             text value=\"B\" start=0 duration=1\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!(comp.clips.len(), 9);
    use notation::model::{Clip, GradePreset};
    let presets: Vec<GradePreset> = comp
        .clips
        .iter()
        .map(|c| match c {
            Clip::Text(t) => t.grade.preset,
            Clip::Video(t) => t.grade.preset,
            Clip::Box(t) => t.grade.preset,
            Clip::Flex(t) => t.grade.preset,
            Clip::Image(t) => t.grade.preset,
            Clip::Path(t) => t.grade.preset,
            Clip::Svg(t) => t.grade.preset,
            Clip::Audio(t) => t.grade.preset,
            Clip::Nest(t) => t.grade.preset,
            Clip::Chart(t) => t.grade.preset,
            Clip::Lottie(t) => t.grade.preset,
            Clip::Map(t) => t.grade.preset,
            Clip::Scene3d(t) => t.grade.preset,
            Clip::Caption(t) => t.grade.preset,
        })
        .map(|p| p.expect("preset written"))
        .collect();
    assert_eq!(
        presets,
        vec![
            GradePreset::Filmic,
            GradePreset::Noir,
            GradePreset::TealOrange,
            GradePreset::Pastel,
            GradePreset::Bleach,
            GradePreset::None,
            GradePreset::Filmic,
            GradePreset::Noir,
            GradePreset::Pastel,
        ]
    );
}

// --- T001: levels compose ------------------------------------------------------

#[test]
fn composition_lane_and_clip_grades_compose() {
    // Composition, lane, and clip grades all validate; the clip's
    // pieces win, missing pieces inherit.
    let dir = tmp("levels-ok");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             grade preset=\"filmic\" exposure=1 contrast=0.2\n\
             lane track=\"video\" {{\n grade saturation=0.5\n}}\n\
             lane track=\"graphics\" {{\n grade preset=\"none\" tint=0.25\n}}\n\
             video path=\"clip.mp4\" start=0 duration=4 {{\n grade exposure=-1\n}}\n\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    use notation::model::GradePreset;
    assert_eq!(comp.grade.preset, Some(GradePreset::Filmic));
    assert_eq!(comp.grade.exposure, Some(1.0));
    assert_eq!(comp.lane_grades.len(), 2);
    assert_eq!(comp.lane_grades[0].track, notation::model::TrackKind::Video);
    assert_eq!(comp.lane_grades[0].grade.saturation, Some(0.5));
    assert_eq!(comp.lane_grades[0].grade.preset, None);
    assert_eq!(comp.lane_grades[1].grade.preset, Some(GradePreset::None));
    use notation::model::Clip;
    match &comp.clips[0] {
        Clip::Video(c) => assert_eq!(c.grade.exposure, Some(-1.0)),
        other => panic!("expected video, got {other:?}"),
    }
    match &comp.clips[1] {
        Clip::Box(c) => assert!(c.grade.is_empty()),
        other => panic!("expected box, got {other:?}"),
    }
}

#[test]
fn empty_and_partial_grades_are_skipped_not_zeroed() {
    // An empty `grade` validates and carries nothing; a partial grade
    // leaves the other pieces unset for the outer levels to fill.
    let dir = tmp("partial-ok");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade\n}}\n\
             box x=30 y=0 width=20 height=12 fill=\"#00ff00\" start=0 duration=4 {{\n grade exposure=0.5\n}}\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    use notation::model::Clip;
    match &comp.clips[0] {
        Clip::Box(c) => assert!(c.grade.is_empty()),
        other => panic!("expected box, got {other:?}"),
    }
    match &comp.clips[1] {
        Clip::Box(c) => {
            assert_eq!(c.grade.exposure, Some(0.5));
            assert_eq!(c.grade.preset, None);
            assert_eq!(c.grade.contrast, None);
        }
        other => panic!("expected box, got {other:?}"),
    }
}

#[test]
fn explicit_none_resets_the_preset_stage() {
    // A clip `preset="none"` is an explicit reset of the preset stage,
    // not a missing piece: the composition filmic does not leak in.
    let dir = tmp("none-resets");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             grade preset=\"filmic\"\n\
             lane track=\"graphics\" {{\n grade preset=\"teal-orange\"\n}}\n\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade preset=\"none\"\n}}\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    use notation::model::{Clip, GradePreset};
    match &comp.clips[0] {
        Clip::Box(c) => assert_eq!(c.grade.preset, Some(GradePreset::None)),
        other => panic!("expected box, got {other:?}"),
    }
}

// --- T001: refusals -------------------------------------------------------------

#[test]
fn grade_adjusts_accept_variable_bindings() {
    // A `number` variable binding in an adjust attribute resolves like
    // a literal (Sprint 9 variables ride every non-structural key).
    let dir = tmp("grade-var");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             variable id=\"ev\" type=\"number\" default=-1\n\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade exposure=\"var(--ev)\"\n}}\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    use notation::model::Clip;
    match &comp.clips[0] {
        Clip::Box(c) => assert_eq!(c.grade.exposure, Some(-1.0)),
        other => panic!("expected box, got {other:?}"),
    }
}

#[test]
fn unknown_preset_fails_naming_the_accepted_set() {
    let dir = tmp("bad-preset");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade preset=\"vintage\"\n}}\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    let all = codes_messages(&dir);
    assert_eq!(all.len(), 1, "{all:?}");
    let (code, message, fix) = &all[0];
    assert_eq!(code, "E_BAD_GRADE");
    for name in ["none", "filmic", "noir", "teal-orange", "pastel", "bleach"] {
        assert!(message.contains(name), "message names {name}: {message}");
    }
    assert!(fix.contains("preset"), "fix names the key: {fix}");
}

#[test]
fn out_of_range_adjusts_fail_naming_attribute_and_range() {
    // Each adjust outside its range fails; the diagnostic names the
    // attribute and the accepted range, and the fix tells the maker
    // how to correct it.
    let cases: &[(&str, &str, &str)] = &[
        ("exposure=9", "exposure", "-4 to 4"),
        ("exposure=-5", "exposure", "-4 to 4"),
        ("contrast=1.5", "contrast", "-1 to 1"),
        ("saturation=3", "saturation", "-1 to 2"),
        ("saturation=-1.5", "saturation", "-1 to 2"),
        ("temperature=2", "temperature", "-1 to 1"),
        ("tint=-2", "tint", "-1 to 1"),
    ];
    for (attr, name, range) in cases {
        let dir = tmp(&format!("bad-adjust-{name}"));
        touch_media(&dir);
        write_comp(
            &dir,
            &format!(
                "{OPEN}\
                 box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade {attr}\n}}\n\
                 }}\n"
            ),
        );
        let all = codes_messages(&dir);
        assert_eq!(all.len(), 1, "{attr}: {all:?}");
        let (code, message, fix) = &all[0];
        assert_eq!(code, "E_BAD_GRADE", "{attr}: {all:?}");
        assert!(message.contains(*name), "{attr}: {message}");
        assert!(message.contains(*range), "{attr}: {message}");
        assert!(fix.contains(*name), "{attr}: {fix}");
    }
    // Boundary values validate: the ranges are inclusive.
    let dir = tmp("adjust-edges-ok");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade exposure=-4 contrast=1 saturation=2 temperature=-1 tint=1\n}}\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
}

#[test]
fn duplicated_grades_fail() {
    // Two `grade` children on one clip fail.
    let dir = tmp("dup-clip");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade preset=\"filmic\"\n grade exposure=1\n}}\n\
             }}\n"
        ),
    );
    let all = codes_messages(&dir);
    assert_eq!(
        all.iter().map(|(c, _, _)| c).collect::<Vec<_>>(),
        vec!["E_BAD_GRADE"],
        "{all:?}"
    );
    assert!(
        all[0].1.contains("two `grade`"),
        "names the duplicate: {}",
        all[0].1
    );

    // Two `grade` children on the composition fail.
    let dir = tmp("dup-comp");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             grade preset=\"filmic\"\n\
             grade exposure=1\n\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4\n\
             }}\n"
        ),
    );
    let all = codes_messages(&dir);
    assert_eq!(all.len(), 1, "{all:?}");
    assert_eq!(all[0].0, "E_BAD_GRADE");

    // Two `lane` elements for one track fail.
    let dir = tmp("dup-lane");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             lane track=\"video\" {{\n grade preset=\"filmic\"\n}}\n\
             lane track=\"video\" {{\n grade exposure=1\n}}\n\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4\n\
             }}\n"
        ),
    );
    let all = codes_messages(&dir);
    assert_eq!(all.len(), 1, "{all:?}");
    assert_eq!(all[0].0, "E_BAD_GRADE");
    assert!(all[0].1.contains("video"), "names the track: {}", all[0].1);
}

#[test]
fn bad_lanes_fail_naming_the_track_and_fix() {
    // Missing `track`.
    let dir = tmp("lane-no-track");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!("{OPEN} lane {{\n grade preset=\"filmic\"\n}}\n box x=0 y=0 width=8 height=8 fill=\"#ff0000\" start=0 duration=4\n}}\n"),
    );
    let all = codes_messages(&dir);
    assert_eq!(all.len(), 1, "{all:?}");
    assert_eq!(all[0].0, "E_BAD_GRADE");
    assert!(all[0].1.contains("track"), "names the key: {}", all[0].1);
    assert!(
        all[0].2.contains("track"),
        "fix tells the key: {}",
        all[0].2
    );

    // Unknown `track` names the accepted lanes.
    let dir = tmp("lane-bad-track");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!("{OPEN} lane track=\"titles\" {{\n grade preset=\"filmic\"\n}}\n box x=0 y=0 width=8 height=8 fill=\"#ff0000\" start=0 duration=4\n}}\n"),
    );
    let all = codes_messages(&dir);
    assert_eq!(all.len(), 1, "{all:?}");
    assert_eq!(all[0].0, "E_BAD_GRADE");
    for name in ["video", "audio", "caption", "graphics"] {
        assert!(all[0].1.contains(name), "names {name}: {}", all[0].1);
    }

    // A non-`grade` lane child fails.
    let dir = tmp("lane-bad-child");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!("{OPEN} lane track=\"video\" {{\n text value=\"Hi\" start=0 duration=1\n}}\n box x=0 y=0 width=8 height=8 fill=\"#ff0000\" start=0 duration=4\n}}\n"),
    );
    let all = codes_messages(&dir);
    assert_eq!(codes(&dir), vec!["E_BAD_GRADE"], "{all:?}");
}

#[test]
fn grade_keys_and_children_fail() {
    // An unknown `grade` key fails as an unknown key.
    let dir = tmp("grade-bad-key");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade warmth=1\n}}\n\
             }}\n"
        ),
    );
    let all = codes_messages(&dir);
    assert_eq!(codes(&dir), vec!["E_UNKNOWN_KEY"], "{all:?}");

    // A `grade` child of the wrong shape fails naming the element.
    let dir = tmp("grade-bad-child");
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
    assert!(!all[0].2.is_empty(), "fix is concrete: {}", all[0].2);
}
