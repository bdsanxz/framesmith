//! Sprint 8 T006: shared `style` blocks with element precedence,
//! through the public [`validate_folder`] API — the same verdict
//! `check` reports, with the resolved model asserted on the clips.
//!
//! Covers every acceptance criterion of
//! `T006-share-looks-by-name-with-element-precedence.md`: three
//! elements sharing one look, element-over-style precedence,
//! later-style-wins conflict resolution, pixel-identity with direct
//! values (model field-for-field), styled motion resolving from the
//! styled base, and every refusal variant naming the block or
//! property plus the available names or the valid set.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Diagnostic, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n08styles-{}-{}-{}",
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

fn write_svg(dir: &Path) {
    std::fs::write(dir.join("logo.svg"), LOGO_SVG).unwrap();
}

fn diags(body: &str) -> Vec<Diagnostic> {
    let dir = tmp("case");
    write_comp(&dir, body);
    validate_folder(&dir).diagnostics
}

fn diags_with_media(body: &str) -> Vec<Diagnostic> {
    let dir = tmp("case-media");
    write_comp(&dir, body);
    write_svg(&dir);
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

fn valid_comp(body: &str) -> notation::model::Composition {
    let dir = tmp("valid");
    write_comp(&dir, body);
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    v.composition.unwrap()
}

fn valid_comp_with_media(body: &str) -> notation::model::Composition {
    let dir = tmp("valid-media");
    write_comp(&dir, body);
    write_svg(&dir);
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    v.composition.unwrap()
}

fn boxes_of(comp: &notation::model::Composition) -> Vec<notation::model::BoxClip> {
    comp.clips
        .iter()
        .map(|c| match c {
            notation::model::Clip::Box(b) => b.clone(),
            other => panic!("expected a box, got {other:?}"),
        })
        .collect()
}

// --- shared looks resolve onto every referrer -------------------------------

const CARD: &str =
    "style name=\"card\" fill=\"#ff0000\" x=10 y=20 width=200 height=100 opacity=0.5 radius=4\n";

fn three_sharing(body_prefix: &str) -> Vec<notation::model::BoxClip> {
    let body = format!(
        "{OPEN}{body_prefix}box style=\"card\" start=0 duration=2\nbox style=\"card\" start=0 duration=2\nbox style=\"card\" start=0 duration=2\n}}\n"
    );
    boxes_of(&valid_comp(&body))
}

#[test]
fn three_elements_sharing_one_style_all_render_its_properties() {
    let bs = three_sharing(CARD);
    assert_eq!(bs.len(), 3);
    for b in &bs {
        assert_eq!((b.x, b.y), (10.0, 20.0));
        assert_eq!((b.width, b.height), (200.0, 100.0));
        assert_eq!(b.fill, notation::model::Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(b.opacity, 0.5);
        assert_eq!(b.radius, 4.0);
    }
}

#[test]
fn element_attribute_overrides_the_same_style_property() {
    let body =
        format!("{OPEN}{CARD}box style=\"card\" fill=\"#0000ff\" x=30 start=0 duration=2\n}}\n");
    let bs = boxes_of(&valid_comp(&body));
    assert_eq!(bs.len(), 1);
    // Element-written `fill` and `x` win; the rest rides the style.
    assert_eq!(bs[0].fill, notation::model::Rgb { r: 0, g: 0, b: 255 });
    assert_eq!(bs[0].x, 30.0);
    assert_eq!(bs[0].y, 20.0);
    assert_eq!(bs[0].opacity, 0.5);
}

#[test]
fn later_declared_style_wins_over_earlier_style() {
    let body = format!(
        "{OPEN}style name=\"base\" fill=\"#ff0000\" x=10 y=20 width=200 height=100\nstyle name=\"card\" fill=\"#0000ff\"\nbox style=\"base card\" start=0 duration=2\n}}\n"
    );
    let bs = boxes_of(&valid_comp(&body));
    assert_eq!(bs.len(), 1);
    // `card` is later, so its fill wins; `x` comes from `base` alone.
    assert_eq!(bs[0].fill, notation::model::Rgb { r: 0, g: 0, b: 255 });
    assert_eq!(bs[0].x, 10.0);
}

#[test]
fn style_order_controls_which_shared_property_wins() {
    let body = format!(
        "{OPEN}style name=\"base\" fill=\"#ff0000\" x=10 y=20 width=200 height=100\nstyle name=\"card\" fill=\"#0000ff\"\nbox style=\"card base\" start=0 duration=2\n}}\n"
    );
    let bs = boxes_of(&valid_comp(&body));
    assert_eq!(bs.len(), 1);
    // `base` is later here, so its fill wins back.
    assert_eq!(bs[0].fill, notation::model::Rgb { r: 255, g: 0, b: 0 });
}

#[test]
fn styled_element_matches_direct_values_field_for_field() {
    // The styled clip and the hand-written clip must resolve to the
    // same model: render is a pure function of the model, so equal
    // models paint identical pixels.
    let styled = format!(
        "{OPEN}style name=\"card\" fill=\"#ff0000\" x=10 y=20 width=200 height=100 opacity=0.5 radius=4 stroke-width=2 stroke-color=\"#00ff00\"\nbox style=\"card\" start=0 duration=2\n}}\n"
    );
    let direct = format!(
        "{OPEN}box fill=\"#ff0000\" x=10 y=20 width=200 height=100 opacity=0.5 radius=4 stroke-width=2 stroke-color=\"#00ff00\" start=0 duration=2\n}}\n"
    );
    let mut s = boxes_of(&valid_comp(&styled)).pop().unwrap();
    let mut d = boxes_of(&valid_comp(&direct)).pop().unwrap();
    // Source locations live outside the look (one comes from the
    // style block, both elements sit on different lines).
    s.line = 0;
    s.column = 0;
    d.line = 0;
    d.column = 0;
    assert_eq!(format!("{s:?}"), format!("{d:?}"));
}

#[test]
fn style_can_supply_required_geometry_and_timing() {
    // A style may carry any scalar, including the sizes the box
    // validator requires: the element names only the look.
    let body = format!(
        "{OPEN}style name=\"card\" fill=\"#ff0000\" x=10 y=20 width=200 height=100 start=0 duration=2\nbox style=\"card\"\n}}\n"
    );
    let bs = boxes_of(&valid_comp(&body));
    assert_eq!(bs.len(), 1);
    assert_eq!((bs[0].x, bs[0].y), (10.0, 20.0));
    assert_eq!((bs[0].width, bs[0].height), (200.0, 100.0));
    assert_eq!((bs[0].start, bs[0].duration), (0.0, 2.0));
}

#[test]
fn underscore_spellings_in_styles_route_like_hyphens() {
    let body = format!(
        "{OPEN}style name=\"card\" fill=\"#ff0000\" x=10 y=20 width=200 height=100 border_width=3 stroke_position=\"outside\"\nbox style=\"card\" start=0 duration=2\n}}\n"
    );
    let bs = boxes_of(&valid_comp(&body));
    assert_eq!(bs.len(), 1);
    assert_eq!(bs[0].border_width, 3.0);
    assert_eq!(
        bs[0].stroke_position,
        notation::model::StrokePosition::Outside
    );
}

#[test]
fn forward_style_reference_resolves_whatever_the_order() {
    // The style block may follow its users: styles validate in a
    // pre-pass before any element resolves them.
    let body = format!(
        "{OPEN}box style=\"card\" x=1 y=2 width=50 height=40 start=0 duration=2\n{CARD}}}\n"
    );
    let bs = boxes_of(&valid_comp(&body));
    assert_eq!(bs.len(), 1);
    assert_eq!(bs[0].fill, notation::model::Rgb { r: 255, g: 0, b: 0 });
}

// --- styles compose with every styleable element ----------------------------

#[test]
fn style_applies_to_text_runs() {
    let body = format!(
        "{OPEN}style name=\"headline\" size=48 color=\"#00ff00\"\ntext value=\"Hi\" style=\"headline\" x=5 y=6 start=0 duration=2\n}}\n"
    );
    let comp = valid_comp(&body);
    match &comp.clips[0] {
        notation::model::Clip::Text(t) => {
            assert_eq!(t.size, 48.0);
            assert_eq!(t.color, notation::model::Rgb { r: 0, g: 255, b: 0 });
        }
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn style_applies_to_path_shapes() {
    let body = format!(
        "{OPEN}style name=\"shape\" fill=\"#0000ff\" stroke-width=3\npath d=\"M0,0 L10,0\" style=\"shape\" x=1 y=2 start=0 duration=2\n}}\n"
    );
    let comp = valid_comp(&body);
    match &comp.clips[0] {
        notation::model::Clip::Path(p) => {
            assert_eq!(p.fill, notation::model::Rgb { r: 0, g: 0, b: 255 });
            assert_eq!(p.border_width, 3.0);
        }
        other => panic!("expected a path, got {other:?}"),
    }
}

#[test]
fn style_applies_to_image_and_svg_placement() {
    let body = format!(
        "{OPEN}style name=\"framed\" x=4 y=6 width=48 height=32\nimage path=\"logo.svg\" style=\"framed\" start=0 duration=2\nsvg path=\"logo.svg\" style=\"framed\" start=0 duration=2\n}}\n"
    );
    let comp = valid_comp_with_media(&body);
    assert_eq!(comp.clips.len(), 2);
    match &comp.clips[0] {
        notation::model::Clip::Image(i) => assert_eq!((i.x, i.y), (4.0, 6.0)),
        other => panic!("expected an image, got {other:?}"),
    }
    match &comp.clips[1] {
        notation::model::Clip::Svg(s) => {
            assert_eq!((s.width, s.height), (48.0, 32.0));
        }
        other => panic!("expected an svg, got {other:?}"),
    }
}

#[test]
fn style_applies_to_flex_containers_and_child_boxes() {
    let body = format!(
        "{OPEN}style name=\"row\" flex-direction=\"column\" gap=8\nstyle name=\"cell\" fill=\"#ff0000\" width=60 height=40\nflex display=\"flex\" style=\"row\" x=0 y=0 width=300 height=100 start=0 duration=2 {{\nbox style=\"cell\"\n}}\n}}\n"
    );
    // The KDL subset opens one block per trailing `{`.
    let body = body.replace("{{", "{");
    let comp = valid_comp(&body);
    match &comp.clips[0] {
        notation::model::Clip::Flex(f) => {
            assert_eq!(f.direction, notation::model::FlexDirection::Column);
            assert_eq!(f.gap, 8.0);
            assert_eq!(f.children.len(), 1);
            assert_eq!(
                f.children[0].fill,
                notation::model::Rgb { r: 255, g: 0, b: 0 }
            );
        }
        other => panic!("expected flex, got {other:?}"),
    }
}

// --- styled motion animates the resolved value -------------------------------

#[test]
fn styled_motion_property_animates_from_the_styled_base() {
    // A `to` tween omitting `from` starts at the clip's own value —
    // which the style supplied — and the composition validates clean.
    let body = format!(
        "{OPEN}{CARD}box style=\"card\" id=\"hero\" start=0 duration=2\ntween target=0 property=\"x\" kind=\"to\" start=0 duration=2 to=310\n}}\n"
    );
    let comp = valid_comp(&body);
    match &comp.clips[0] {
        notation::model::Clip::Box(b) => assert_eq!(b.x, 10.0),
        other => panic!("expected a box, got {other:?}"),
    }
    assert_eq!(comp.tweens.len(), 1);
    assert_eq!(comp.tweens[0].property, "x");
    assert_eq!(comp.tweens[0].to, Some(310.0));
    assert_eq!(comp.tweens[0].from, None);
}

#[test]
fn styled_set_and_key_directives_validate_on_resolved_values() {
    let body = format!(
        "{OPEN}{CARD}box style=\"card\" start=0 duration=2\nset target=0 property=\"opacity\" start=0 duration=1 value=0.2\nkey target=0 property=\"y\" {{\nframe at=0 value=20\nframe at=2 value=60\n}}\n}}\n"
    );
    let body = body.replace("{{", "{");
    let comp = valid_comp(&body);
    assert_eq!(comp.sets.len(), 1);
    assert_eq!(comp.sets[0].value, 0.2);
    assert_eq!(comp.keys.len(), 1);
    assert_eq!(comp.keys[0].stops.len(), 2);
}

// --- single code path: styled values fail like direct ones --------------------

#[test]
fn styled_bad_color_refuses_through_the_element_check() {
    let ds = diags(&format!(
        "{OPEN}style name=\"card\" fill=\"red\" x=10 y=20 width=200 height=100\nbox style=\"card\" start=0 duration=2\n}}\n"
    ));
    assert_refusal(&ds, "E_BAD_COLOR", &["`fill`"]);
}

#[test]
fn styled_kind_mismatch_refuses_like_a_direct_key() {
    // `value` is valid vocabulary (text content) but no box key:
    // the box validator refuses the styled value exactly as if
    // written directly on the element.
    let ds = diags(&format!(
        "{OPEN}style name=\"card\" value=\"Hi\" x=10 y=20 width=200 height=100\nbox style=\"card\" start=0 duration=2\n}}\n"
    ));
    assert_refusal(&ds, "E_UNKNOWN_KEY", &["unknown box key", "\"value\""]);
}

// --- refusal variants name the block or property ------------------------------

#[test]
fn unknown_style_names_the_missing_block_and_the_available() {
    let ds = diags(&format!(
        "{OPEN}{CARD}box style=\"ghost\" x=1 y=2 width=50 height=40 start=0 duration=2\n}}\n"
    ));
    assert_refusal(&ds, "E_BAD_STYLE", &["\"ghost\"", "`card`"]);
}

#[test]
fn unknown_style_with_several_names_each_reports() {
    let ds = diags(&format!(
        "{OPEN}{CARD}box style=\"card ghost\" x=1 y=2 width=50 height=40 start=0 duration=2\n}}\n"
    ));
    assert_refusal(&ds, "E_BAD_STYLE", &["\"ghost\"", "`card`"]);
}

#[test]
fn unknown_style_with_no_styles_declared_says_so() {
    let ds = diags(&format!(
        "{OPEN}box style=\"ghost\" x=1 y=2 width=50 height=40 start=0 duration=2\n}}\n"
    ));
    assert_refusal(
        &ds,
        "E_BAD_STYLE",
        &["\"ghost\"", "no styles are declared yet"],
    );
}

#[test]
fn style_unknown_property_names_it_and_the_valid_set() {
    let ds = diags(&format!(
        "{OPEN}style name=\"card\" float=\"left\" fill=\"#ff0000\"\nbox style=\"card\" x=1 y=2 width=50 height=40 start=0 duration=2\n}}\n"
    ));
    assert_refusal(&ds, "E_BAD_STYLE", &["\"float\"", "`fill`"]);
}

#[test]
fn duplicate_style_block_names_the_block() {
    let ds = diags(&format!(
        "{OPEN}{CARD}style name=\"card\" fill=\"#0000ff\"\nbox style=\"card\" x=1 y=2 width=50 height=40 start=0 duration=2\n}}\n"
    ));
    assert_refusal(&ds, "E_BAD_STYLE", &["\"card\"", "more than once"]);
}

#[test]
fn style_key_on_a_style_block_is_circular() {
    let ds = diags(&format!(
        "{OPEN}style name=\"card\" style=\"other\" fill=\"#ff0000\"\n}}\n"
    ));
    assert_refusal(&ds, "E_BAD_STYLE", &["\"card\"", "never reference"]);
}

#[test]
fn empty_style_reference_names_no_block() {
    let ds = diags(&format!(
        "{OPEN}{CARD}box style=\"\" x=1 y=2 width=50 height=40 start=0 duration=2\n}}\n"
    ));
    assert_refusal(&ds, "E_BAD_STYLE", &["names no block", "`card`"]);
}

#[test]
fn non_string_style_reference_fails_where_written() {
    let ds = diags(&format!(
        "{OPEN}{CARD}box style=5 x=1 y=2 width=50 height=40 start=0 duration=2\n}}\n"
    ));
    assert_refusal(&ds, "E_BAD_STYLE", &["quoted style name"]);
}

#[test]
fn style_block_missing_name_fails() {
    let ds = diags(&format!(
        "{OPEN}style fill=\"#ff0000\"\nbox style=\"card\" x=1 y=2 width=50 height=40 start=0 duration=2\n}}\n"
    ));
    assert_refusal(&ds, "E_BAD_STYLE", &["missing required `name`"]);
}
