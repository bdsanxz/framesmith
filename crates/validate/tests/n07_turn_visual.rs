//! Sprint 7 motion (T005 + T006) validation: 3D turns with origin
//! and perspective, plus animated visual properties.
//!
//! Validates through the public [`validate_folder`] API only: 3D clip
//! attributes land on every visual clip; non-positive perspectives and
//! unknown origins fail with diagnostics; every listed visual property
//! tweens and keys on the clips carrying it with frame-exact value
//! shapes (colors in straight RGBA); kind mismatches, wrong shapes,
//! out-of-range values, sets on colors, and beyond-blur filters fail
//! naming the clip, the stop, or Sprint 11.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n07-turn-{tag}-{}-{}",
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
    std::fs::write(dir.join("clip.mp4"), []).unwrap();
    std::fs::write(dir.join("s.mp3"), []).unwrap();
}

const OPEN: &str =
    "composition name=\"main\" width=160 height=120 duration=4 background=\"#000\" version=1 {\n";

fn codes_messages(dir: &Path) -> Vec<(String, String, String)> {
    validate_folder(dir)
        .diagnostics
        .iter()
        .map(|d| (d.code.to_string(), d.message.clone(), d.fix.clone()))
        .collect()
}

// --- T005: 3D turns with origin and perspective ------------------------------

#[test]
fn turn_attributes_land_on_every_visual_clip() {
    // rotation-x/rotation-y/perspective/origin validate on text, box,
    // image, and video; the values land on the clips.
    let dir = tmp("turn-ok");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"Hi\" start=0 duration=4 rotation-x=10 rotation-y=-5 perspective=300 origin=\"top-left\"\n\
             box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 rotate-x=15 perspective=250 origin=\"center\"\n\
             image path=\"photo.png\" start=0 duration=4 rotation-y=45 perspective=400 origin=\"bottom-right\"\n\
             video path=\"clip.mp4\" start=0 duration=4 rotation-x=30 origin=\"left top\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!(comp.clips.len(), 4);
    use notation::model::Clip;
    let (rx, ry, persp) = match &comp.clips[0] {
        Clip::Text(tc) => (tc.turn.rotation_x, tc.turn.rotation_y, tc.turn.perspective),
        other => panic!("expected text, got {other:?}"),
    };
    assert_eq!((rx, ry, persp), (10.0, -5.0, Some(300.0)));
    match &comp.clips[1] {
        Clip::Box(bc) => {
            assert_eq!(
                (bc.turn.rotation_x, bc.turn.perspective),
                (15.0, Some(250.0))
            );
            assert_eq!(bc.turn.origin, notation::model::Placement::center());
        }
        other => panic!("expected box, got {other:?}"),
    }
    match &comp.clips[2] {
        Clip::Image(ic) => {
            assert_eq!(
                (ic.turn.rotation_y, ic.turn.perspective),
                (45.0, Some(400.0))
            );
        }
        other => panic!("expected image, got {other:?}"),
    }
    match &comp.clips[3] {
        Clip::Video(vc) => {
            assert_eq!(vc.turn.rotation_x, 30.0);
            assert_eq!(vc.turn.perspective, None, "flat when omitted");
        }
        other => panic!("expected video, got {other:?}"),
    }
}

#[test]
fn non_positive_perspective_fails() {
    // Zero and negative perspectives are not finite positive pixels:
    // both fail with E_BAD_MOTION naming the bound.
    for (tag, value) in [("zero", "0"), ("negative", "-50"), ("word", "\"far\"")] {
        let dir = tmp(&format!("persp-{tag}"));
        write_comp(
            &dir,
            &format!(
                "{OPEN}    box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 perspective={value}\n}}\n"
            ),
        );
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Invalid, "{tag}: {:?}", v.diagnostics);
        let found = codes_messages(&dir);
        assert!(
            found
                .iter()
                .any(|(c, m, _)| c == "E_BAD_MOTION" && m.contains("perspective")),
            "{tag}: {found:?}"
        );
    }
}

#[test]
fn unknown_origin_fails_listing_accepted_values() {
    // An unknown origin keyword fails with E_BAD_MOTION and lists the
    // accepted placements.
    let dir = tmp("origin-bad");
    write_comp(
        &dir,
        &format!(
            "{OPEN}    box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 origin=\"middle-earth\"\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    let found = codes_messages(&dir);
    assert!(
        found.iter().any(|(c, m, _)| c == "E_BAD_MOTION"
            && m.contains("\"middle-earth\"")
            && m.contains("`center`")
            && m.contains("`top-left`")),
        "{found:?}"
    );
}

#[test]
fn unreadable_rotation_fails() {
    // A non-numeric rotation fails with E_BAD_MOTION naming degrees.
    let dir = tmp("rot-bad");
    write_comp(
        &dir,
        &format!(
            "{OPEN}    box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 rotation-x=\"fast\"\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    let found = codes_messages(&dir);
    assert!(
        found
            .iter()
            .any(|(c, m, _)| c == "E_BAD_MOTION" && m.contains("rotation-x")),
        "{found:?}"
    );
}

#[test]
fn turn_keys_refuse_on_clips_without_pictures() {
    // `rotation-x` on audio is an unknown key (audio has no picture to
    // turn); perspective/origin as motion properties are not animatable.
    let dir = tmp("turn-audio");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!("{OPEN}    audio path=\"s.mp3\" start=0 duration=4 rotation-x=10\n}}\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    let found = codes_messages(&dir);
    assert!(
        found.iter().any(|(c, _, _)| c == "E_UNKNOWN_KEY"),
        "{found:?}"
    );
    for prop in ["perspective", "origin"] {
        let dir = tmp(&format!("turn-prop-{prop}"));
        write_comp(
            &dir,
            &format!(
                "{OPEN}    box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4\n    tween target=0 property=\"{prop}\" kind=\"to\" start=0 duration=1 to=1\n}}\n"
            ),
        );
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Invalid, "{prop}: {:?}", v.diagnostics);
        let found = codes_messages(&dir);
        assert!(
            found
                .iter()
                .any(|(c, m, _)| c == "E_BAD_MOTION" && m.contains("not animatable")),
            "{prop}: {found:?}"
        );
    }
}

// --- T006: animated visual properties ----------------------------------------

#[test]
fn visual_properties_validate_on_their_clips() {
    // Every listed property tweens on the clips carrying it; aliases
    // normalize to the canonical property.
    for (clip, prop, to, canonical) in [
        (
            "box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4",
            "width",
            "60",
            "width",
        ),
        (
            "box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4",
            "height",
            "40",
            "height",
        ),
        (
            "box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4",
            "fill-color",
            "\"#00ff00\"",
            "fill",
        ),
        (
            "box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4",
            "radius",
            "6",
            "radius",
        ),
        (
            "box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4",
            "border-radius",
            "6",
            "radius",
        ),
        (
            "box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4",
            "border-width",
            "3",
            "border-width",
        ),
        (
            "box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4",
            "border-color",
            "\"#0000ff\"",
            "border-color",
        ),
        (
            "box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4",
            "rotate-x",
            "30",
            "rotation-x",
        ),
        (
            "box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4",
            "rotation_y",
            "15",
            "rotation-y",
        ),
        (
            "box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4",
            "blur",
            "2",
            "blur",
        ),
        (
            "text value=\"Hi\" start=0 duration=4",
            "font-size",
            "32",
            "font-size",
        ),
        (
            "text value=\"Hi\" start=0 duration=4",
            "size",
            "32",
            "font-size",
        ),
        (
            "text value=\"Hi\" start=0 duration=4",
            "letter_spacing",
            "3",
            "letter-spacing",
        ),
        (
            "text value=\"Hi\" start=0 duration=4",
            "rotation-x",
            "45",
            "rotation-x",
        ),
        ("text value=\"Hi\" start=0 duration=4", "blur", "1", "blur"),
        (
            "image path=\"photo.png\" start=0 duration=4",
            "width",
            "80",
            "width",
        ),
        (
            "image path=\"photo.png\" start=0 duration=4",
            "height",
            "60",
            "height",
        ),
        (
            "video path=\"clip.mp4\" start=0 duration=4",
            "rotation-y",
            "20",
            "rotation-y",
        ),
        (
            "video path=\"clip.mp4\" start=0 duration=4",
            "blur",
            "2",
            "blur",
        ),
    ] {
        let dir = tmp(&format!("vis-{canonical}-{to}"));
        touch_media(&dir);
        write_comp(
            &dir,
            &format!(
                "{OPEN}    {clip}\n    tween target=0 property=\"{prop}\" kind=\"to\" start=0 duration=1 to={to}\n}}\n"
            ),
        );
        let v = validate_folder(&dir);
        assert_eq!(
            v.status,
            Status::Valid,
            "{prop} on [{clip}]: {:?}",
            v.diagnostics
        );
        let comp = v.composition.expect("valid composition");
        assert_eq!(comp.tweens.len(), 1);
        assert_eq!(comp.tweens[0].property, canonical, "{prop} normalizes");
    }
}

#[test]
fn kind_mismatches_fail_naming_the_clip() {
    // Width on text, fill on text, font-size on box, radius on image,
    // and letter-spacing on box all fail naming the clip kind they need.
    for (clip, prop, to, needs) in [
        (
            "text value=\"Hi\" start=0 duration=4",
            "width",
            "60",
            "box, image, flex, or svg",
        ),
        (
            "text value=\"Hi\" start=0 duration=4",
            "fill",
            "\"#00ff00\"",
            "box",
        ),
        (
            "box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4",
            "font-size",
            "32",
            "text",
        ),
        (
            "image path=\"photo.png\" start=0 duration=4",
            "radius",
            "6",
            "box",
        ),
        (
            "box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4",
            "letter-spacing",
            "3",
            "text",
        ),
    ] {
        let dir = tmp(&format!("mismatch-{prop}"));
        touch_media(&dir);
        write_comp(
            &dir,
            &format!(
                "{OPEN}    {clip}\n    tween target=0 property=\"{prop}\" kind=\"to\" start=0 duration=1 to={to}\n}}\n"
            ),
        );
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Invalid, "{prop}: {:?}", v.diagnostics);
        let found = codes_messages(&dir);
        assert!(
            found
                .iter()
                .any(|(c, m, _)| c == "E_BAD_MOTION" && m.contains(prop) && m.contains(needs)),
            "{prop}: {found:?}"
        );
    }
}

#[test]
fn clip_shape_names_the_numeric_shape_fields() {
    // No `clip-shape` property exists: the clip's shape animates
    // through width/height/radius/border-width, which the refusal names.
    let dir = tmp("clip-shape");
    write_comp(
        &dir,
        &format!(
            "{OPEN}    box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4\n    tween target=0 property=\"clip-shape\" kind=\"to\" start=0 duration=1 to=1\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    let found = codes_messages(&dir);
    assert!(
        found.iter().any(|(c, m, _)| c == "E_BAD_MOTION"
            && m.contains("clip-shape")
            && m.contains("`width`")
            && m.contains("`radius`")),
        "{found:?}"
    );
}

#[test]
fn color_endpoints_validate_and_land() {
    // Fill and border-color tweens and keys carry hex colors into
    // from_color/to_color and value_color; numbers in their place fail.
    let dir = tmp("color-ok");
    write_comp(
        &dir,
        &format!(
            "{OPEN}    box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4\n    tween target=0 property=\"fill\" kind=\"from-to\" start=0 duration=1 from=\"#ff0000\" to=\"#0000ff\"\n    key target=0 property=\"border-color\" {{\n        frame at=0 value=\"#000000\"\n        frame at=1 value=\"#ffffff\"\n    }}\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    let tween = &comp.tweens[0];
    assert_eq!(tween.property, "fill");
    assert_eq!(
        (tween.from_color, tween.to_color),
        (
            Some(notation::model::Rgb { r: 255, g: 0, b: 0 }),
            Some(notation::model::Rgb { r: 0, g: 0, b: 255 })
        )
    );
    let key = &comp.keys[0];
    assert_eq!(
        key.stops[1].value_color,
        Some(notation::model::Rgb {
            r: 255,
            g: 255,
            b: 255
        })
    );
    // A number where a color belongs fails; a bare color name fails too.
    for (tag, to) in [
        ("number", "10"),
        ("named", "\"blue\""),
        ("short", "\"#ff\""),
    ] {
        let dir = tmp(&format!("color-bad-{tag}"));
        write_comp(
            &dir,
            &format!(
                "{OPEN}    box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4\n    tween target=0 property=\"fill\" kind=\"to\" start=0 duration=1 to={to}\n}}\n"
            ),
        );
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Invalid, "{tag}: {:?}", v.diagnostics);
        let found = codes_messages(&dir);
        assert!(
            found
                .iter()
                .any(|(c, m, _)| c == "E_BAD_MOTION" && m.contains("#rrggbb")),
            "{tag}: {found:?}"
        );
    }
}

#[test]
fn set_on_a_color_fails_naming_tween_or_key() {
    // A set holds one number, so colors animate with tween or key.
    let dir = tmp("set-color");
    write_comp(
        &dir,
        &format!(
            "{OPEN}    box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4\n    set target=0 property=\"fill\" start=0 duration=1 value=\"#00ff00\"\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    let found = codes_messages(&dir);
    assert!(
        found.iter().any(|(c, m, _)| c == "E_BAD_MOTION"
            && m.contains("holds a color")
            && m.contains("tween")),
        "{found:?}"
    );
    // Numeric sets still validate on the new properties.
    let dir = tmp("set-width-ok");
    write_comp(
        &dir,
        &format!(
            "{OPEN}    box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4\n    set target=0 property=\"width\" start=0 duration=1 value=60\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
}

#[test]
fn out_of_range_values_fail() {
    // Sizes run positive; radii, spacing, and blur run 0 and up.
    for (prop, to) in [
        ("width", "-5"),
        ("width", "0"),
        ("font-size", "0"),
        ("radius", "-1"),
        ("letter-spacing", "-0.5"),
        ("border-width", "-2"),
        ("blur", "-1"),
    ] {
        let clip = if prop == "font-size" || prop == "letter-spacing" {
            "text value=\"Hi\" start=0 duration=4"
        } else {
            "box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4"
        };
        let dir = tmp(&format!("range-{prop}"));
        write_comp(
            &dir,
            &format!(
                "{OPEN}    {clip}\n    tween target=0 property=\"{prop}\" kind=\"to\" start=0 duration=1 to={to}\n}}\n"
            ),
        );
        let v = validate_folder(&dir);
        assert_eq!(
            v.status,
            Status::Invalid,
            "{prop}={to}: {:?}",
            v.diagnostics
        );
        let found = codes_messages(&dir);
        assert!(
            found
                .iter()
                .any(|(c, m, _)| c == "E_BAD_MOTION" && m.contains(prop)),
            "{prop}: {found:?}"
        );
    }
}

#[test]
fn beyond_blur_filters_name_sprint_11() {
    // Filter properties and filter functions beyond blur (and blur
    // with a bad shape) fail as unsupported naming Sprint 11; `none`
    // and `blur(N[px])` draw.
    for prop in [
        "brightness",
        "filter",
        "contrast",
        "saturate",
        "grayscale",
        "hue-rotate",
        "sepia",
        "invert",
        "drop-shadow",
    ] {
        let dir = tmp(&format!("filter-prop-{prop}"));
        write_comp(
            &dir,
            &format!(
                "{OPEN}    box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4\n    tween target=0 property=\"{prop}\" kind=\"to\" start=0 duration=1 to=1\n}}\n"
            ),
        );
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Unsupported, "{prop}: {:?}", v.diagnostics);
        let found = codes_messages(&dir);
        assert!(
            found.iter().any(|(c, m, _)| c == "E_UNSUPPORTED"
                && m.contains(prop)
                && m.contains("Sprint 11")),
            "{prop}: {found:?}"
        );
    }
    for (tag, filter, status, code) in [
        ("none", "filter=\"none\"", Status::Valid, ""),
        ("blur", "filter=\"blur(2)\"", Status::Valid, ""),
        ("blur-px", "filter=\"blur(2.5px)\"", Status::Valid, ""),
        (
            "contrast",
            "filter=\"contrast(2)\"",
            Status::Unsupported,
            "E_UNSUPPORTED",
        ),
        (
            "multi",
            "filter=\"blur(2) contrast(3)\"",
            Status::Unsupported,
            "E_UNSUPPORTED",
        ),
        (
            "word",
            "filter=\"smooth\"",
            Status::Unsupported,
            "E_UNSUPPORTED",
        ),
        (
            "negative",
            "filter=\"blur(-1)\"",
            Status::Invalid,
            "E_BAD_MOTION",
        ),
        (
            "unreadable",
            "filter=\"blur(much)\"",
            Status::Invalid,
            "E_BAD_MOTION",
        ),
        ("number", "filter=3", Status::Invalid, "E_BAD_MOTION"),
    ] {
        let dir = tmp(&format!("filter-key-{tag}"));
        write_comp(
            &dir,
            &format!(
                "{OPEN}    box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {filter}\n}}\n"
            ),
        );
        let v = validate_folder(&dir);
        assert_eq!(v.status, status, "{tag}: {:?}", v.diagnostics);
        if status == Status::Valid {
            let comp = v.composition.expect("valid composition");
            let blur = match &comp.clips[0] {
                notation::model::Clip::Box(bc) => bc.filter_blur,
                other => panic!("expected box, got {other:?}"),
            };
            let want = match tag {
                "blur" => 2.0,
                "blur-px" => 2.5,
                _ => 0.0,
            };
            assert_eq!(blur, want, "{tag}: static blur lands");
        } else {
            let found = codes_messages(&dir);
            assert!(
                found
                    .iter()
                    .any(|(c, m, _)| c == code && m.contains("filter")),
                "{tag}: {found:?}"
            );
            if code == "E_UNSUPPORTED" {
                assert!(
                    found.iter().any(|(_, m, _)| m.contains("Sprint 11")),
                    "{tag}: names Sprint 11: {found:?}"
                );
            }
        }
    }
}
