//! Sprint 8 T004: stroke, shadow, blend, blur, mask, and clip through
//! the public [`validate_folder`] API — the same verdict `check`
//! reports, with the resolved paint stack asserted on the model.
//!
//! Covers every acceptance criterion of
//! `T004-stroke-shade-blend-blur-mask-and-clip.md`: stroke positions,
//! shadows, blend modes, backdrop sampling, overflow clipping, masks,
//! and each refusal variant naming its element and property.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Diagnostic, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n08paint-{}-{}-{}",
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

const OPEN: &str =
    "composition name=\"main\" width=640 height=480 duration=2 background=\"#000\" version=1 {\n";

fn boxed(attrs: &str) -> String {
    format!("{OPEN}box x=10 y=20 width=200 height=100 start=0 duration=2 {attrs}\n}}\n")
}

fn diags(body: &str) -> Vec<Diagnostic> {
    let dir = tmp("case");
    write_comp(&dir, body);
    validate_folder(&dir).diagnostics
}

fn codes(ds: &[Diagnostic]) -> Vec<String> {
    ds.iter().map(|d| d.code.to_string()).collect()
}

fn assert_refusal(ds: &[Diagnostic], code: &str, needles: &[&str]) {
    assert!(!ds.is_empty(), "expected a refusal, got none");
    assert!(
        ds.iter().any(|d| d.code == code),
        "expected code {code} in {:?}",
        codes(ds)
    );
    let messages: Vec<String> = ds.iter().map(|d| d.message.clone()).collect();
    for needle in needles {
        assert!(
            messages.iter().any(|m| m.contains(needle)),
            "expected {needle:?} in {messages:?}"
        );
    }
}

fn first_box(body: &str) -> notation::model::BoxClip {
    let dir = tmp("valid");
    write_comp(&dir, body);
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match v.composition.unwrap().clips.into_iter().next().unwrap() {
        notation::model::Clip::Box(b) => b,
        other => panic!("expected a box, got {other:?}"),
    }
}

// --- valid paint features ---------------------------------------------------

#[test]
fn stroke_positions_validate_with_defaults() {
    use notation::model::StrokePosition;
    let b = first_box(&boxed(""));
    assert_eq!(b.stroke_position, StrokePosition::Inside);
    assert_eq!(b.border_width, 0.0);
    assert!(b.box_shadow.is_none());
    for (spell, want) in [
        ("center", StrokePosition::Center),
        ("inside", StrokePosition::Inside),
        ("outside", StrokePosition::Outside),
    ] {
        let b = first_box(&boxed(&format!(
            "stroke-width=6 stroke-color=\"#ff0000\" stroke-position=\"{spell}\"\n"
        )));
        assert_eq!(b.stroke_position, want);
        assert_eq!(b.border_width, 6.0);
        assert_eq!(
            b.border_color,
            notation::model::Rgb { r: 255, g: 0, b: 0 }
        );
    }
}

#[test]
fn stroke_aliases_validate_like_hyphens() {
    let b = first_box(&boxed(
        "stroke_width=4 stroke_color=\"#00ff00\" stroke_position=\"outside\"\n",
    ));
    assert_eq!(b.border_width, 4.0);
    assert_eq!(
        b.border_color,
        notation::model::Rgb { r: 0, g: 255, b: 0 }
    );
    assert_eq!(
        b.stroke_position,
        notation::model::StrokePosition::Outside
    );
}

#[test]
fn shadow_blend_backdrop_overflow_validate() {
    use notation::model::{BlendMode, Overflow};
    let b = first_box(&boxed(
        "box-shadow=\"8 8 12 0 #112233\" mix-blend-mode=\"multiply\" backdrop-blur=2 backdrop-brightness=0.5 overflow=\"hidden\"\n",
    ));
    let sh = b.box_shadow.unwrap();
    assert_eq!((sh.offset_x, sh.offset_y), (8.0, 8.0));
    assert_eq!((sh.blur, sh.spread), (12.0, 0.0));
    assert_eq!(
        sh.color,
        notation::model::Rgb { r: 0x11, g: 0x22, b: 0x33 }
    );
    assert_eq!(b.blend_mode, BlendMode::Multiply);
    assert_eq!((b.backdrop_blur, b.backdrop_brightness), (2.0, 0.5));
    assert_eq!(b.overflow, Overflow::Hidden);
    for (mode, want) in [
        ("normal", BlendMode::Normal),
        ("screen", BlendMode::Screen),
        ("overlay", BlendMode::Overlay),
    ] {
        let b = first_box(&boxed(&format!("mix-blend-mode=\"{mode}\"\n")));
        assert_eq!(b.blend_mode, want);
    }
    for (spell, want) in [("visible", Overflow::Visible), ("clip", Overflow::Clip)] {
        let b = first_box(&boxed(&format!("overflow=\"{spell}\"\n")));
        assert_eq!(b.overflow, want);
    }
    let b = first_box(&boxed("box-shadow=\"none\"\n"));
    assert!(b.box_shadow.is_none());
}

#[test]
fn mask_forward_reference_resolves() {
    let dir = tmp("mask-fwd");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box id=\"top\" x=0 y=0 width=50 height=50 start=0 duration=2 mask=\"shape\"\n\
             box id=\"shape\" x=0 y=0 width=50 height=50 start=0 duration=2\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.unwrap().clips[0] {
        notation::model::Clip::Box(b) => {
            assert_eq!(b.mask.as_ref().unwrap().target, "shape");
        }
        other => panic!("expected a box, got {other:?}"),
    }
}

#[test]
fn clip_path_and_underscore_spellings_validate() {
    let dir = tmp("clip-path");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box id=\"a\" x=0 y=0 width=10 height=10 start=0 duration=2 clip-path=\"s\"\n\
             box id=\"s\" x=0 y=0 width=10 height=10 start=0 duration=2\n\
             box x=0 y=0 width=10 height=10 start=0 duration=2 mix_blend_mode=\"screen\" backdrop_blur=1 backdrop_brightness=2 box_shadow=\"1 2 3 0 #ffffff\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
}

#[test]
fn flex_child_stroke_position_validates() {
    let dir = tmp("flex-stroke");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             flex display=\"flex\" flex-direction=\"row\" width=200 height=100 start=0 duration=2 {{\n\
             box width=50 height=50 stroke-position=\"center\"\n\
             }}\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
}

// --- refusals -----------------------------------------------------------------

#[test]
fn negative_stroke_width_fails_naming_element_and_property() {
    let ds = diags(&boxed("id=\"card\" stroke-width=-2\n"));
    assert_refusal(&ds, "E_BAD_PAINT", &["\"card\"", "stroke-width"]);
}

#[test]
fn unknown_stroke_position_fails_naming_the_set() {
    let ds = diags(&boxed("stroke-position=\"middle\"\n"));
    assert_refusal(
        &ds,
        "E_BAD_PAINT",
        &["stroke-position", "`center`", "`inside`", "`outside`"],
    );
}

#[test]
fn unknown_blend_mode_fails_naming_the_set() {
    let ds = diags(&boxed("mix-blend-mode=\"darken\"\n"));
    assert_refusal(
        &ds,
        "E_BAD_PAINT",
        &[
            "mix-blend-mode",
            "`normal`",
            "`multiply`",
            "`screen`",
            "`overlay`",
        ],
    );
}

#[test]
fn unknown_overflow_fails_naming_the_set() {
    let ds = diags(&boxed("overflow=\"scroll\"\n"));
    assert_refusal(
        &ds,
        "E_BAD_PAINT",
        &["overflow", "`visible`", "`hidden`", "`clip`"],
    );
}

#[test]
fn malformed_shadow_fails_naming_property() {
    for bad in [
        "box-shadow=\"8 8\"\n",
        "box-shadow=\"8 8 -1 0 #000000\"\n",
        "box-shadow=\"8 8 1 0 red\"\n",
        "box-shadow=\"8 8 1 0 #000000 extra\"\n",
        "box-shadow=5\n",
    ] {
        let ds = diags(&boxed(bad));
        assert_refusal(&ds, "E_BAD_PAINT", &["box-shadow"]);
    }
}

#[test]
fn negative_backdrop_fails_naming_property() {
    let ds = diags(&boxed("backdrop-blur=-1\n"));
    assert_refusal(&ds, "E_BAD_PAINT", &["backdrop-blur"]);
    let ds = diags(&boxed("backdrop-brightness=-0.5\n"));
    assert_refusal(&ds, "E_BAD_PAINT", &["backdrop-brightness"]);
}

#[test]
fn dangling_mask_fails_naming_target() {
    let ds = diags(&boxed("id=\"card\" mask=\"ghost\"\n"));
    assert_refusal(&ds, "E_BAD_PAINT", &["\"card\"", "mask", "\"ghost\""]);
}

#[test]
fn non_box_mask_target_fails_naming_kind() {
    let dir = tmp("mask-text");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box id=\"card\" x=0 y=0 width=10 height=10 start=0 duration=2 mask=\"words\"\n\
             text id=\"words\" value=\"Hi\" x=0 y=0 size=12 start=0 duration=2\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_refusal(&v.diagnostics, "E_BAD_PAINT", &["mask", "\"words\"", "text"]);
}

#[test]
fn mask_cycle_fails_naming_every_reference() {
    let dir = tmp("mask-cycle");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box id=\"a\" x=0 y=0 width=10 height=10 start=0 duration=2 mask=\"b\"\n\
             box id=\"b\" x=0 y=0 width=10 height=10 start=0 duration=2 clip-path=\"a\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_ne!(v.status, Status::Valid);
    assert_refusal(&v.diagnostics, "E_BAD_PAINT", &["\"a\"", "\"b\"", "cycle"]);
}

#[test]
fn long_mask_chain_names_every_reference() {
    let dir = tmp("mask-chain");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box id=\"a\" x=0 y=0 width=10 height=10 start=0 duration=2 mask=\"b\"\n\
             box id=\"b\" x=0 y=0 width=10 height=10 start=0 duration=2 mask=\"c\"\n\
             box id=\"c\" x=0 y=0 width=10 height=10 start=0 duration=2 clip-path=\"a\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_ne!(v.status, Status::Valid);
    assert_refusal(
        &v.diagnostics,
        "E_BAD_PAINT",
        &["\"a\"", "\"b\"", "\"c\"", "cycle"],
    );
}

#[test]
fn mask_self_cycle_fails() {
    let dir = tmp("mask-self");
    write_comp(
        &dir,
        &format!(
            "{OPEN}box id=\"solo\" x=0 y=0 width=10 height=10 start=0 duration=2 mask=\"solo\"\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_refusal(&v.diagnostics, "E_BAD_PAINT", &["\"solo\"", "cycle"]);
}

#[test]
fn flex_child_paint_keys_fail_on_boxes_only() {
    let dir = tmp("flex-paint");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             flex display=\"flex\" flex-direction=\"row\" width=200 height=100 start=0 duration=2 {{\n\
             box width=50 height=50 box-shadow=\"1 1 2 0 #000000\"\n\
             }}\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_refusal(&v.diagnostics, "E_BAD_PAINT", &["box-shadow", "box"]);
}
