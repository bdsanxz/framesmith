//! Sprint 8 T005: vector `path` shapes and `svg` pictures through
//! the public [`validate_folder`] API — the same verdict `check`
//! reports, with the resolved shapes asserted on the model.
//!
//! Covers every acceptance criterion of
//! `T005-draw-vector-paths-and-svg-images.md`: filled and stroked `d`
//! sequences in each fill rule, unparsable `d` naming the failing
//! command index, below-64px SVG rasters at their declared size, and
//! each refusal variant naming its file, rule, or command.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Diagnostic, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n08pathsvg-{}-{}-{}",
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

const LOGO_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"48\" height=\"32\"><rect width=\"48\" height=\"32\" fill=\"#ff0000\"/></svg>\n";

fn write_logo(dir: &Path) {
    std::fs::write(dir.join("logo.svg"), LOGO_SVG).unwrap();
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

fn first_path(body: &str) -> notation::model::PathClip {
    let dir = tmp("valid");
    write_comp(&dir, body);
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match v.composition.unwrap().clips.into_iter().next().unwrap() {
        notation::model::Clip::Path(p) => p,
        other => panic!("expected a path, got {other:?}"),
    }
}

fn first_svg(body: &str) -> notation::model::SvgClip {
    let dir = tmp("valid");
    write_logo(&dir);
    write_comp(&dir, body);
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match v.composition.unwrap().clips.into_iter().next().unwrap() {
        notation::model::Clip::Svg(s) => s,
        other => panic!("expected an svg, got {other:?}"),
    }
}

fn pathd(d: &str, extra: &str) -> String {
    format!("{OPEN}path d=\"{d}\" start=0 duration=2 {extra}\n}}\n")
}

// --- valid shapes ------------------------------------------------------------

#[test]
fn filled_and_stroked_paths_validate_in_each_fill_rule() {
    use notation::model::{FillRule, StrokePosition};
    for (spell, want) in [("nonzero", FillRule::NonZero), ("evenodd", FillRule::EvenOdd)] {
        let p = first_path(&pathd(
            "M10,10 L110,10 L110,90 L10,90 Z",
            &format!("fill=\"#00ff00\" fill-rule=\"{spell}\" stroke-width=4 stroke-color=\"#ff0000\" stroke-position=\"center\""),
        ));
        assert_eq!(p.fill_rule, want);
        assert_eq!(
            p.fill,
            notation::model::Rgb { r: 0, g: 255, b: 0 }
        );
        assert_eq!(p.border_width, 4.0);
        assert_eq!(
            p.border_color,
            notation::model::Rgb { r: 255, g: 0, b: 0 }
        );
        assert_eq!(p.stroke_position, StrokePosition::Center);
        assert_eq!(p.d, "M10,10 L110,10 L110,90 L10,90 Z");
    }
    // The default rule is nonzero with an inside stroke.
    let p = first_path(&pathd("M0,0 L10,0", ""));
    assert_eq!(p.fill_rule, FillRule::NonZero);
    assert_eq!(p.stroke_position, StrokePosition::Inside);
}

#[test]
fn path_aliases_validate_like_hyphens() {
    let p = first_path(&pathd(
        "M0,0 L8,8",
        "fill_rule=\"evenodd\" stroke_width=2 stroke_color=\"#123456\" stroke_position=\"outside\"",
    ));
    assert_eq!(p.fill_rule, notation::model::FillRule::EvenOdd);
    assert_eq!(p.border_width, 2.0);
    assert_eq!(p.stroke_position, notation::model::StrokePosition::Outside);
}

#[test]
fn path_paint_stack_validates() {
    use notation::model::{BlendMode, Overflow};
    let dir = tmp("paint");
    write_logo(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}box x=0 y=0 width=60 height=60 id=\"lens\" start=0 duration=2\npath d=\"M0,0 L20,0 L20,20 Z\" start=0 duration=2 fill=\"#ffffff\" box-shadow=\"4 4 6 0 #000000\" mix-blend-mode=\"multiply\" backdrop-blur=2 backdrop-brightness=1 overflow=\"hidden\" mask=\"lens\" clip-path=\"lens\" rotation-x=15 perspective=500 filter=\"blur(2)\"\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match v.composition.unwrap().clips.into_iter().nth(1).unwrap() {
        notation::model::Clip::Path(p) => {
            assert!(p.box_shadow.is_some());
            assert_eq!(p.blend_mode, BlendMode::Multiply);
            assert_eq!(p.backdrop_blur, 2.0);
            assert_eq!(p.overflow, Overflow::Hidden);
            assert_eq!(p.mask.as_ref().unwrap().target, "lens");
            assert_eq!((p.turn.rotation_x, p.turn.perspective), (15.0, Some(500.0)));
            assert_eq!(p.filter_blur, 2.0);
        }
        other => panic!("expected a path, got {other:?}"),
    }
}

#[test]
fn svg_below_64px_validates_at_declared_size() {
    let s = first_svg(&format!(
        "{OPEN}svg path=\"logo.svg\" x=4 y=4 width=48 height=32 start=0 duration=2\n}}\n"
    ));
    assert_eq!((s.width, s.height), (48.0, 32.0));
    assert_eq!((s.x, s.y), (4.0, 4.0));
    assert_eq!(s.fit, notation::model::Fit::Contain);
}

#[test]
fn image_accepts_svg_sources() {
    let dir = tmp("image-svg");
    write_logo(&dir);
    write_comp(
        &dir,
        &format!("{OPEN}image path=\"logo.svg\" start=0 duration=2\n}}\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
}

#[test]
fn path_and_svg_stills_infer_duration() {
    let dir = tmp("stills");
    write_logo(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}path d=\"M0,0 L10,0\" start=0\nsvg path=\"logo.svg\" width=48 height=32 start=0\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.unwrap();
    assert_eq!(comp.clips.len(), 2);
    assert_eq!(comp.clips[0].duration(), 2.0);
    assert_eq!(comp.clips[1].duration(), 2.0);
}

// --- path refusals -----------------------------------------------------------

#[test]
fn unparsable_d_names_the_failing_command_index() {
    let ds = diags(&pathd("M10,10 X20,20", ""));
    assert_refusal(&ds, "E_BAD_PATH", &["command 2", "`X`"]);
}

#[test]
fn truncated_d_names_the_failing_command_index() {
    let ds = diags(&pathd("M10,10 L", ""));
    assert_refusal(&ds, "E_BAD_PATH", &["command 2", "`L`"]);
}

#[test]
fn bad_number_d_names_the_failing_command_index() {
    let ds = diags(&pathd("M10,10 L20,zz", ""));
    assert_refusal(&ds, "E_BAD_PATH", &["command 2", "`L`"]);
}

#[test]
fn empty_d_is_refused() {
    let ds = diags(&pathd("", ""));
    assert_refusal(&ds, "E_BAD_PATH", &["names no commands"]);
}

#[test]
fn missing_d_is_refused() {
    let ds = diags(&format!(
        "{OPEN}path start=0 duration=2 fill=\"#ff0000\"\n}}\n"
    ));
    assert_refusal(&ds, "E_MISSING_FIELD", &["`d`"]);
}

#[test]
fn unknown_fill_rule_names_the_accepted_set() {
    let ds = diags(&pathd("M0,0 L10,0", "fill-rule=\"winding\""));
    assert_refusal(&ds, "E_BAD_PATH", &["`fill-rule`", "`nonzero`", "`evenodd`"]);
}

#[test]
fn negative_stroke_width_names_the_element() {
    let ds = diags(&pathd("M0,0 L10,0", "stroke-width=-2"));
    assert_refusal(&ds, "E_BAD_PAINT", &["path clip", "`stroke-width`"]);
}

#[test]
fn unknown_path_key_is_refused() {
    let ds = diags(&pathd("M0,0 L10,0", "bogus=1"));
    assert_refusal(&ds, "E_UNKNOWN_KEY", &["bogus"]);
}

#[test]
fn path_children_are_refused() {
    let ds = diags(&format!(
        "{OPEN}path d=\"M0,0 L10,0\" start=0 duration=2 {{\nstop offset=0 color=\"#ff0000\"\n}}\n}}\n"
    ));
    assert_refusal(&ds, "E_BAD_PATH", &["carries no children"]);
}

#[test]
fn dangling_mask_on_path_names_the_target() {
    let ds = diags(&pathd("M0,0 L10,0", "mask=\"ghost\""));
    assert_refusal(&ds, "E_BAD_PAINT", &["path clip", "\"ghost\""]);
}

#[test]
fn non_box_mask_target_on_path_names_the_kind() {
    let ds = diags(&format!(
        "{OPEN}text value=\"Hi\" id=\"words\" start=0 duration=2\npath d=\"M0,0 L10,0\" start=0 duration=2 mask=\"words\"\n}}\n"
    ));
    assert_refusal(&ds, "E_BAD_PAINT", &["path clip", "\"words\"", "text"]);
}

// --- svg refusals ------------------------------------------------------------

#[test]
fn zero_svg_width_names_the_rule_with_the_fix() {
    let dir = tmp("zero-w");
    write_logo(&dir);
    write_comp(
        &dir,
        &format!("{OPEN}svg path=\"logo.svg\" width=0 height=32 start=0 duration=2\n}}\n"),
    );
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(&ds, "E_BAD_IMAGE", &["`width`", "positive number", "canvas pixels"]);
}

#[test]
fn negative_svg_height_names_the_rule_with_the_fix() {
    let dir = tmp("neg-h");
    write_logo(&dir);
    write_comp(
        &dir,
        &format!("{OPEN}svg path=\"logo.svg\" width=48 height=-4 start=0 duration=2\n}}\n"),
    );
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(&ds, "E_BAD_IMAGE", &["`height`", "positive"]);
}

#[test]
fn missing_svg_size_names_the_rule() {
    let dir = tmp("missing-size");
    write_logo(&dir);
    write_comp(
        &dir,
        &format!("{OPEN}svg path=\"logo.svg\" height=32 start=0 duration=2\n}}\n"),
    );
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(&ds, "E_BAD_IMAGE", &["`width`"]);
}

#[test]
fn missing_svg_file_names_the_file() {
    let ds = diags(&format!(
        "{OPEN}svg path=\"gone.svg\" width=48 height=32 start=0 duration=2\n}}\n"
    ));
    assert_refusal(&ds, "E_MISSING_FILE", &["gone.svg"]);
}

#[test]
fn corrupt_svg_names_the_file() {
    let dir = tmp("corrupt");
    std::fs::write(dir.join("bad.svg"), b"definitely not an svg picture {{{").unwrap();
    write_comp(
        &dir,
        &format!("{OPEN}svg path=\"bad.svg\" width=48 height=32 start=0 duration=2\n}}\n"),
    );
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(&ds, "E_BAD_IMAGE", &["bad.svg", "readable SVG"]);
}

#[test]
fn corrupt_image_svg_names_the_file() {
    let dir = tmp("corrupt-image");
    std::fs::write(dir.join("bad.svg"), b"\xff\xfe\x00binary").unwrap();
    write_comp(
        &dir,
        &format!("{OPEN}image path=\"bad.svg\" start=0 duration=2\n}}\n"),
    );
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(&ds, "E_BAD_IMAGE", &["bad.svg"]);
}

#[test]
fn non_svg_extension_on_svg_names_the_rule() {
    let ds = diags(&format!(
        "{OPEN}svg path=\"raster.png\" width=48 height=32 start=0 duration=2\n}}\n"
    ));
    assert_refusal(&ds, "E_BAD_IMAGE", &["`.svg`"]);
}

#[test]
fn svg_on_image_fill_names_the_clip() {
    let dir = tmp("fill-svg");
    write_logo(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}box x=0 y=0 width=64 height=64 start=0 duration=2 {{\nimage-fill src=\"logo.svg\"\n}}\n}}\n"
        ),
    );
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(&ds, "E_BAD_FILL", &["logo.svg", "SVG"]);
}

// --- motion on new clips -----------------------------------------------------

#[test]
fn fill_tween_on_path_and_size_tween_on_svg_validate() {
    let dir = tmp("motion");
    write_logo(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}path d=\"M0,0 L10,0\" start=0 duration=2 id=\"shape\" fill=\"#ff0000\"\nsvg path=\"logo.svg\" width=48 height=32 start=0 duration=2 id=\"mark\"\ntween target=0 property=\"fill\" kind=\"to\" start=0 duration=1 to=\"#00ff00\"\ntween target=1 property=\"width\" kind=\"to\" start=0 duration=1 to=96\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(v.composition.unwrap().tweens.len(), 2);
}

#[test]
fn radius_tween_on_path_names_a_box() {
    let dir = tmp("radius-path");
    write_comp(
        &dir,
        &format!(
            "{OPEN}path d=\"M0,0 L10,0\" start=0 duration=2\ntween target=0 property=\"radius\" kind=\"to\" start=0 duration=1 to=8\n}}\n"
        ),
    );
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(&ds, "E_BAD_MOTION", &["\"radius\"", "a box", "path clip"]);
}
