//! Sprint 8 T003: gradient and image fills through the public
//! [`validate_folder`] API — the same verdict `check` reports, with
//! the resolved fill paint asserted on the model.
//!
//! Covers every acceptance criterion of
//! `T003-fill-with-gradients-and-images.md`: linear and radial
//! stop lists on the model, image fills in each `object-fit` mode,
//! and each refusal variant naming its stop or fill with the rule.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Diagnostic, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n08fill-{}-{}-{}",
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

/// A still-image content sniff passes on the bare PNG signature
/// (mirrors the `image` clip tests): decoding happens at render.
fn write_png(dir: &Path, name: &str) {
    std::fs::write(dir.join(name), b"\x89PNG\r\n\x1a\n").unwrap();
}

const OPEN: &str =
    "composition name=\"main\" width=640 height=480 duration=2 background=\"#000\" version=1 {\n";

fn boxed(body: &str) -> String {
    format!("{OPEN}box x=10 y=20 width=200 height=100 start=0 duration=2 {{\n{body}}}\n}}\n")
}

fn linear_three() -> String {
    boxed(
        "linear-gradient {\n\
         stop offset=0 color=\"#ff0000\"\n\
         stop offset=0.5 color=\"#00ff00\"\n\
         stop offset=1 color=\"#0000ff\"\n\
         }\n",
    )
}

fn diags(body: &str) -> Vec<Diagnostic> {
    let dir = tmp("case");
    write_comp(&dir, body);
    validate_folder(&dir).diagnostics
}

fn codes(ds: &[Diagnostic]) -> Vec<String> {
    ds.iter().map(|d| d.code.to_string()).collect()
}

fn assert_refusal(ds: &[Diagnostic], code: &str, needle: &str) {
    assert!(!ds.is_empty(), "expected a refusal, got none");
    assert!(
        ds.iter().any(|d| d.code == code),
        "expected code {code} in {:?}",
        codes(ds)
    );
    assert!(
        ds.iter().any(|d| d.message.contains(needle)),
        "expected {needle:?} in {:?}",
        ds.iter().map(|d| d.message.clone()).collect::<Vec<_>>()
    );
}

// --- valid fills ------------------------------------------------------------

#[test]
fn linear_three_stop_gradient_is_valid() {
    let dir = tmp("linear");
    write_comp(&dir, &linear_three());
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.unwrap().clips[0] {
        notation::model::Clip::Box(b) => match &b.fill_paint {
            notation::model::FillPaint::Linear { angle, stops } => {
                assert_eq!(*angle, 90.0);
                assert_eq!(stops.len(), 3);
                assert_eq!(stops[0].offset, 0.0);
                assert_eq!(stops[1].offset, 0.5);
                assert_eq!(stops[2].offset, 1.0);
                assert_eq!(stops[1].color, notation::model::Rgb { r: 0, g: 255, b: 0 });
            }
            other => panic!("expected a linear fill, got {other:?}"),
        },
        other => panic!("expected a box, got {other:?}"),
    }
}

#[test]
fn underscore_spellings_validate_like_hyphens() {
    let dir = tmp("underscore");
    write_png(&dir, "bg.png");
    write_comp(
        &dir,
        &boxed(
            "linear_gradient angle=180 {\n\
             stop offset=0 color=\"#ffffff\"\n\
             stop offset=1 color=\"#000000\"\n\
             }\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);

    let dir = tmp("underscore-img");
    write_png(&dir, "bg.png");
    write_comp(
        &dir,
        &boxed("image_fill src=\"bg.png\" object_fit=\"cover\" object_position=\"top-left\"\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.unwrap().clips[0] {
        notation::model::Clip::Box(b) => match &b.fill_paint {
            notation::model::FillPaint::Image { fit, placement, .. } => {
                assert_eq!(*fit, notation::model::Fit::Cover);
                assert_eq!(
                    *placement,
                    notation::model::Placement::parse("top-left").unwrap()
                );
            }
            other => panic!("expected an image fill, got {other:?}"),
        },
        other => panic!("expected a box, got {other:?}"),
    }
}

#[test]
fn radial_gradient_with_center_and_radius_is_valid() {
    let dir = tmp("radial");
    write_comp(
        &dir,
        &boxed(
            "radial-gradient cx=40 cy=30 radius=90 {\n\
             stop offset=0 color=\"#ffffff\"\n\
             stop offset=1 color=\"#000000\"\n\
             }\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.unwrap().clips[0] {
        notation::model::Clip::Box(b) => match &b.fill_paint {
            notation::model::FillPaint::Radial {
                cx,
                cy,
                radius,
                stops,
            } => {
                assert_eq!((*cx, *cy, *radius), (Some(40.0), Some(30.0), Some(90.0)));
                assert_eq!(stops.len(), 2);
            }
            other => panic!("expected a radial fill, got {other:?}"),
        },
        other => panic!("expected a box, got {other:?}"),
    }
}

#[test]
fn radial_gradient_defaults_to_box_center_and_corners() {
    let dir = tmp("radial-default");
    write_comp(
        &dir,
        &boxed(
            "radial-gradient {\n\
             stop offset=0 color=\"#ffffff\"\n\
             stop offset=1 color=\"#000000\"\n\
             }\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.unwrap().clips[0] {
        notation::model::Clip::Box(b) => match &b.fill_paint {
            notation::model::FillPaint::Radial { cx, cy, radius, .. } => {
                assert_eq!((*cx, *cy, *radius), (None, None, None))
            }
            other => panic!("expected a radial fill, got {other:?}"),
        },
        other => panic!("expected a box, got {other:?}"),
    }
}

#[test]
fn each_object_fit_mode_is_valid() {
    for fit in ["cover", "contain", "fill"] {
        let dir = tmp("fit");
        write_png(&dir, "bg.png");
        write_comp(
            &dir,
            &boxed(&format!("image-fill src=\"bg.png\" object-fit=\"{fit}\"\n")),
        );
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Valid, "fit {fit}: {:?}", v.diagnostics);
    }
}

#[test]
fn single_stop_gradient_paints_flat() {
    let dir = tmp("single");
    write_comp(
        &dir,
        &boxed(
            "linear-gradient {\n\
             stop offset=0.5 color=\"#123456\"\n\
             }\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.unwrap().clips[0] {
        notation::model::Clip::Box(b) => match &b.fill_paint {
            notation::model::FillPaint::Linear { stops, .. } => assert_eq!(stops.len(), 1),
            other => panic!("expected a linear fill, got {other:?}"),
        },
        other => panic!("expected a box, got {other:?}"),
    }
}

#[test]
fn solid_fill_without_children_still_valid() {
    let dir = tmp("solid");
    write_comp(
        &dir,
        &format!("{OPEN}box x=0 y=0 width=10 height=10 fill=\"#ff0000\" start=0 duration=1\n}}\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.unwrap().clips[0] {
        notation::model::Clip::Box(b) => {
            assert!(b.fill_paint.is_solid());
        }
        other => panic!("expected a box, got {other:?}"),
    }
}

// --- stop refusals ----------------------------------------------------------

#[test]
fn first_stop_past_zero_fails_naming_stop_and_rule() {
    let ds = diags(&boxed(
        "linear-gradient {\n\
         stop offset=0.2 color=\"#ff0000\"\n\
         stop offset=1 color=\"#0000ff\"\n\
         }\n",
    ));
    assert_refusal(&ds, "E_BAD_FILL", "stop 1");
    assert_refusal(&ds, "E_BAD_FILL", "offset 0");
}

#[test]
fn last_stop_short_of_one_fails_naming_stop_and_rule() {
    let ds = diags(&boxed(
        "linear-gradient {\n\
         stop offset=0 color=\"#ff0000\"\n\
         stop offset=0.7 color=\"#0000ff\"\n\
         }\n",
    ));
    assert_refusal(&ds, "E_BAD_FILL", "stop 2");
    assert_refusal(&ds, "E_BAD_FILL", "offset 1");
}

#[test]
fn unordered_offsets_fail_naming_stop_and_rule() {
    let ds = diags(&boxed(
        "linear-gradient {\n\
         stop offset=0 color=\"#ff0000\"\n\
         stop offset=0.7 color=\"#00ff00\"\n\
         stop offset=0.3 color=\"#0000ff\"\n\
         stop offset=1 color=\"#ffffff\"\n\
         }\n",
    ));
    assert_refusal(&ds, "E_BAD_FILL", "stop 3");
    assert_refusal(&ds, "E_BAD_FILL", "rising");
}

#[test]
fn out_of_range_offset_fails_naming_stop_and_rule() {
    let ds = diags(&boxed(
        "linear-gradient {\n\
         stop offset=0 color=\"#ff0000\"\n\
         stop offset=1.5 color=\"#0000ff\"\n\
         }\n",
    ));
    assert_refusal(&ds, "E_BAD_FILL", "stop 2");
    assert_refusal(&ds, "E_BAD_FILL", "0 to 1");
}

#[test]
fn duplicate_offsets_fail_naming_stop_and_rule() {
    let ds = diags(&boxed(
        "linear-gradient {\n\
         stop offset=0 color=\"#ff0000\"\n\
         stop offset=0.5 color=\"#00ff00\"\n\
         stop offset=0.5 color=\"#0000ff\"\n\
         stop offset=1 color=\"#ffffff\"\n\
         }\n",
    ));
    assert_refusal(&ds, "E_BAD_FILL", "stop 3");
    assert_refusal(&ds, "E_BAD_FILL", "duplicates");
}

#[test]
fn empty_stop_list_fails() {
    let ds = diags(&boxed("linear-gradient angle=90\n"));
    assert_refusal(&ds, "E_BAD_FILL", "no `stop`");
}

#[test]
fn stop_missing_offset_or_color_fails() {
    let ds = diags(&boxed(
        "linear-gradient {\n\
         stop color=\"#ff0000\"\n\
         stop offset=1 color=\"#0000ff\"\n\
         }\n",
    ));
    assert_refusal(&ds, "E_BAD_FILL", "stop 1");
    assert_refusal(&ds, "E_BAD_FILL", "offset");
    let ds = diags(&boxed(
        "linear-gradient {\n\
         stop offset=0\n\
         stop offset=1 color=\"#0000ff\"\n\
         }\n",
    ));
    assert_refusal(&ds, "E_BAD_FILL", "stop 1");
    assert_refusal(&ds, "E_BAD_FILL", "color");
}

#[test]
fn bad_stop_color_fails_naming_stop() {
    let ds = diags(&boxed(
        "linear-gradient {\n\
         stop offset=0 color=\"red\"\n\
         stop offset=1 color=\"#0000ff\"\n\
         }\n",
    ));
    assert_refusal(&ds, "E_BAD_FILL", "stop 1");
    assert_refusal(&ds, "E_BAD_FILL", "#rrggbb");
}

#[test]
fn non_stop_child_in_gradient_fails() {
    let ds = diags(&boxed(
        "linear-gradient {\n\
         stop offset=0 color=\"#ff0000\"\n\
         box width=10 height=10\n\
         stop offset=1 color=\"#0000ff\"\n\
         }\n",
    ));
    assert_refusal(&ds, "E_BAD_FILL", "stop");
}

#[test]
fn stop_outside_a_gradient_fails() {
    let ds = diags(&boxed("stop offset=0 color=\"#ff0000\"\n"));
    assert_refusal(&ds, "E_BAD_FILL", "`stop`");
}

#[test]
fn unknown_box_child_fails() {
    let ds = diags(&boxed("text value=\"Hi\"\n"));
    assert_refusal(&ds, "E_BAD_FILL", "`text`");
}

#[test]
fn two_fills_on_one_box_fail() {
    let ds = diags(&boxed(
        "linear-gradient {\n\
         stop offset=0 color=\"#ff0000\"\n\
         stop offset=1 color=\"#0000ff\"\n\
         }\n\
         radial-gradient {\n\
         stop offset=0 color=\"#ffffff\"\n\
         stop offset=1 color=\"#000000\"\n\
         }\n",
    ));
    assert_refusal(&ds, "E_BAD_FILL", "exactly one fill");
}

#[test]
fn legacy_gradient_attribute_redirects_to_elements() {
    let dir = tmp("legacy");
    write_comp(
        &dir,
        &format!("{OPEN}box x=0 y=0 width=10 height=10 gradient=\"red\" start=0 duration=1\n}}\n"),
    );
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(&ds, "E_BAD_FILL", "child elements");
}

#[test]
fn bad_linear_angle_and_radial_geometry_fail() {
    let ds = diags(&boxed(
        "linear-gradient angle=\"steep\" {\n\
         stop offset=0 color=\"#ff0000\"\n\
         stop offset=1 color=\"#0000ff\"\n\
         }\n",
    ));
    assert_refusal(&ds, "E_BAD_FILL", "`angle`");
    let ds = diags(&boxed(
        "radial-gradient radius=0 {\n\
         stop offset=0 color=\"#ff0000\"\n\
         stop offset=1 color=\"#0000ff\"\n\
         }\n",
    ));
    assert_refusal(&ds, "E_BAD_FILL", "`radius`");
    let ds = diags(&boxed(
        "radial-gradient cx=\"middle\" {\n\
         stop offset=0 color=\"#ff0000\"\n\
         stop offset=1 color=\"#0000ff\"\n\
         }\n",
    ));
    assert_refusal(&ds, "E_BAD_FILL", "`cx`");
}

// --- image-fill refusals ----------------------------------------------------

#[test]
fn missing_src_fails_naming_the_fill() {
    let ds = diags(&boxed("image-fill object-fit=\"cover\"\n"));
    assert_refusal(&ds, "E_MISSING_FIELD", "image-fill");
    assert_refusal(&ds, "E_MISSING_FIELD", "`src`");
}

#[test]
fn missing_image_file_fails_naming_the_fill() {
    let ds = diags(&boxed("image-fill src=\"gone.png\"\n"));
    assert_refusal(&ds, "E_MISSING_FILE", "gone.png");
}

#[test]
fn corrupt_image_file_fails_naming_the_fill() {
    let dir = tmp("corrupt");
    std::fs::write(dir.join("bad.png"), b"not a picture at all").unwrap();
    write_comp(&dir, &boxed("image-fill src=\"bad.png\"\n"));
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(&ds, "E_BAD_IMAGE", "bad.png");
}

#[test]
fn bad_object_fit_fails_naming_the_accepted_set() {
    let dir = tmp("fit");
    write_png(&dir, "bg.png");
    write_comp(
        &dir,
        &boxed("image-fill src=\"bg.png\" object-fit=\"zoom\"\n"),
    );
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(&ds, "E_BAD_FILL", "object-fit");
    for accepted in ["cover", "contain", "fill"] {
        assert!(
            ds.iter().any(|d| d.message.contains(accepted)),
            "accepted set must name {accepted}: {ds:?}"
        );
    }
}

#[test]
fn bad_object_position_fails() {
    let dir = tmp("pos");
    write_png(&dir, "bg.png");
    write_comp(
        &dir,
        &boxed("image-fill src=\"bg.png\" object-position=\"sideways\"\n"),
    );
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(&ds, "E_BAD_FILL", "object-position");
}

#[test]
fn image_fill_with_children_fails() {
    let dir = tmp("imgkids");
    write_png(&dir, "bg.png");
    write_comp(
        &dir,
        &boxed("image-fill src=\"bg.png\" {\nstop offset=0 color=\"#ff0000\"\n}\n"),
    );
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(&ds, "E_BAD_FILL", "no children");
}

#[test]
fn fill_children_on_flex_child_boxes_fail() {
    let dir = tmp("flexfill");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             flex display=\"flex\" flex-direction=\"row\" x=0 y=0 width=200 height=100 start=0 duration=2 {{\n\
             box width=50 height=50 fill=\"#ff0000\" {{\n\
             linear-gradient {{\n\
             stop offset=0 color=\"#ff0000\"\n\
             stop offset=1 color=\"#0000ff\"\n\
             }}\n\
             }}\n\
             }}\n\
             }}\n"
        ),
    );
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(&ds, "E_BAD_FILL", "flex child");
}
