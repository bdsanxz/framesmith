//! A coloured box blended over another (`docs/spec/blends.md`), each of the sixteen blends in a case of its own, drawn
//! through the scene on both renderers and held to colours worked by hand from the spec's formulas (the same two pairs as
//! `crates/draw/tests/blend_functions.rs`).
//!
//! The two boxes stand in a parent `<box>` with `mix-blend-mode="lighten"`. A blend other than `normal` draws the element
//! with its children apart, over a clear backdrop (section 2, as CSS's stacking context does), so outside the overlap each
//! box is laid over nothing and keeps its own colour by the formula (section 3, with `ab` = 0). The parent's picture is then
//! laid by `lighten` over the black frame, which gives the greater of 0 and each channel: the picture itself, exactly.
//! One case blends straight over the frame's background with no parent, where the blended box mixes with the black too.
//!
//! The frame is 30 by 10: the box under it at (0, 0) 20 by 10, the blended box at (10, 0) 20 by 10, the overlap from x = 10
//! to 19.

mod common;

use common::project_with;
use draw::Picture;
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

/// Pair A: the element and what is under it; pair B the same.
const A: ([u8; 3], [u8; 3]) = ([204, 51, 153], [102, 153, 51]);
const B: ([u8; 3], [u8; 3]) = ([51, 255, 102], [204, 0, 255]);

fn hex(colour: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", colour[0], colour[1], colour[2])
}

fn composition(boxes: &str) -> String {
    format!(
        "<composition version=\"1\" width=\"30\" height=\"10\" fps=\"10\" duration=\"0.1\" background=\"#000000\">\n{boxes}\n</composition>\n"
    )
}

/// The two boxes of a pair, the blended one with `blend` and the attributes `extra`, in a parent drawn apart.
fn in_a_parent(blend: &str, (source, under): ([u8; 3], [u8; 3]), extra: &str) -> String {
    composition(&format!(
        "<box left=\"0\" top=\"0\" width=\"30\" height=\"10\" mix-blend-mode=\"lighten\">\n\
         <box left=\"0\" top=\"0\" width=\"20\" height=\"10\" background=\"{}\"/>\n\
         <box left=\"10\" top=\"0\" width=\"20\" height=\"10\" background=\"{}\" mix-blend-mode=\"{blend}\"{extra}/>\n\
         </box>",
        hex(under),
        hex(source)
    ))
}

enum On {
    Processor,
    Card,
}

fn drawn(name: &str, composition: &str, on: &On) -> Picture {
    let folder = project_with("blends-worked-by-hand", name, composition);
    let scene = Scene::from_project(&Project::open(&folder).unwrap(), &render::system_codecs())
        .unwrap_or_else(|reports| panic!("{name}: {reports:?}"));
    match on {
        On::Processor => scene.picture(0, &mut CpuRenderer).unwrap(),
        On::Card => {
            let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
            scene.picture(0, &mut card).unwrap()
        }
    }
}

fn within_one(got: [u8; 4], wanted: [u8; 3]) -> bool {
    got[3] == 255 && (0..3).all(|channel| got[channel].abs_diff(wanted[channel]) <= 1)
}

/// Draws both pairs blended by `blend` and asserts, along the middle row, the box under it alone, the overlap at the worked
/// colour within one level, and the blended box alone at its own colour; all exactly but the overlap.
fn holds(blend: &str, worked_a: [u8; 3], worked_b: [u8; 3], on: &On) {
    for (pair, colours, worked) in [("A", A, worked_a), ("B", B, worked_b)] {
        let picture = drawn(&format!("{blend}-{pair}"), &in_a_parent(blend, colours, ""), on);
        let at = |x: u32| picture.pixel(x, 5).unwrap();
        let (source, under) = colours;
        for x in [0, 9] {
            assert_eq!(at(x)[..3], under, "{blend}: pair {pair}: the box under it alone at x = {x}");
        }
        for x in [10, 15, 19] {
            assert!(
                within_one(at(x), worked),
                "{blend}: pair {pair}: overlap at x = {x}: {:?} against the worked {worked:?}",
                at(x)
            );
        }
        for x in [20, 29] {
            assert_eq!(at(x)[..3], source, "{blend}: pair {pair}: the blended box alone at x = {x}");
        }
    }
}

/// The worked colours of each blend for pairs A and B (`crates/draw/tests/blend_functions.rs` works each one).
const WORKED: [(&str, [u8; 3], [u8; 3]); 16] = [
    ("normal", [204, 51, 153], [51, 255, 102]),
    ("multiply", [82, 31, 31], [41, 0, 102]),
    ("screen", [224, 173, 173], [214, 255, 255]),
    ("overlay", [163, 92, 61], [173, 0, 255]),
    ("darken", [102, 51, 51], [51, 0, 102]),
    ("lighten", [204, 153, 153], [204, 255, 255]),
    ("color-dodge", [255, 191, 128], [255, 0, 255]),
    ("color-burn", [64, 0, 0], [0, 0, 255]),
    ("hard-light", [194, 61, 92], [82, 255, 204]),
    ("soft-light", [138, 116, 64], [180, 0, 255]),
    ("difference", [102, 102, 102], [153, 255, 153]),
    ("exclusion", [143, 143, 143], [173, 255, 153]),
    ("hue", [190, 88, 156], [0, 145, 36]),
    ("saturation", [90, 166, 13], [181, 18, 222]),
    ("color", [222, 69, 171], [0, 145, 36]),
    ("luminosity", [84, 135, 33], [231, 135, 255]),
];

fn worked(blend: &str) -> ([u8; 3], [u8; 3]) {
    let (_, a, b) = WORKED.iter().find(|(name, _, _)| *name == blend).unwrap();
    (*a, *b)
}

/// One test for each blend on each renderer, named after the blend, so that a wrong blend is named when it fails.
macro_rules! blend_cases {
    ($($test:ident, $card:ident: $name:literal;)*) => {
        $(
            #[test]
            fn $test() {
                let (a, b) = worked($name);
                holds($name, a, b, &On::Processor);
            }

            #[test]
            #[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
            fn $card() {
                let (a, b) = worked($name);
                holds($name, a, b, &On::Card);
            }
        )*
    };
}

blend_cases! {
    normal_on_the_processor, reference_machine_only_normal_on_the_card: "normal";
    multiply_on_the_processor, reference_machine_only_multiply_on_the_card: "multiply";
    screen_on_the_processor, reference_machine_only_screen_on_the_card: "screen";
    overlay_on_the_processor, reference_machine_only_overlay_on_the_card: "overlay";
    darken_on_the_processor, reference_machine_only_darken_on_the_card: "darken";
    lighten_on_the_processor, reference_machine_only_lighten_on_the_card: "lighten";
    color_dodge_on_the_processor, reference_machine_only_color_dodge_on_the_card: "color-dodge";
    color_burn_on_the_processor, reference_machine_only_color_burn_on_the_card: "color-burn";
    hard_light_on_the_processor, reference_machine_only_hard_light_on_the_card: "hard-light";
    soft_light_on_the_processor, reference_machine_only_soft_light_on_the_card: "soft-light";
    difference_on_the_processor, reference_machine_only_difference_on_the_card: "difference";
    exclusion_on_the_processor, reference_machine_only_exclusion_on_the_card: "exclusion";
    hue_on_the_processor, reference_machine_only_hue_on_the_card: "hue";
    saturation_on_the_processor, reference_machine_only_saturation_on_the_card: "saturation";
    color_on_the_processor, reference_machine_only_color_on_the_card: "color";
    luminosity_on_the_processor, reference_machine_only_luminosity_on_the_card: "luminosity";
}

/// The blended box of pair A by multiply at an opacity of 0.6 (section 5): its premultiplied values faded to
/// floor(v · 0.6 + 0.5) = (122, 31, 92) with alpha 153, so Cs = (0.797386, 0.202614, 0.601307) and B = Cb · Cs =
/// (0.318954, 0.121569, 0.120261). Over the opaque box: co = 0.4 · Cb + 0.6 · B = (0.351373, 0.312941, 0.152157) → (89.6,
/// 79.8, 38.8). Alone, over nothing: (122, 31, 92) at alpha 0.6, which the parent lays over black as it is.
fn holds_at_an_opacity(on: &On) {
    let picture = drawn("multiply-at-0.6", &in_a_parent("multiply", A, " opacity=\"0.6\""), on);
    let at = |x: u32| picture.pixel(x, 5).unwrap();
    assert_eq!(at(5)[..3], A.1);
    assert!(within_one(at(15), [90, 80, 39]), "the overlap: {:?}", at(15));
    assert!(within_one(at(25), [122, 31, 92]), "the faded box alone: {:?}", at(25));
}

#[test]
fn a_blend_at_an_opacity_gives_the_formula_and_the_opacity_together_on_the_processor() {
    holds_at_an_opacity(&On::Processor);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_blend_at_an_opacity_gives_the_formula_and_the_opacity_together_on_the_card() {
    holds_at_an_opacity(&On::Card);
}

/// Pair A by multiply with no parent: the blended box mixes with the black frame where it is alone, Cb = 0, so it shows
/// black; the overlap is as in a parent, (82, 31, 31); the box under it keeps its colour.
fn holds_over_the_frames_background(on: &On) {
    let boxes = format!(
        "<box left=\"0\" top=\"0\" width=\"20\" height=\"10\" background=\"{}\"/>\n\
         <box left=\"10\" top=\"0\" width=\"20\" height=\"10\" background=\"{}\" mix-blend-mode=\"multiply\"/>",
        hex(A.1),
        hex(A.0)
    );
    let picture = drawn("multiply-with-no-parent", &composition(&boxes), on);
    let at = |x: u32| picture.pixel(x, 5).unwrap();
    assert_eq!(at(5)[..3], A.1);
    assert!(within_one(at(15), [82, 31, 31]), "the overlap: {:?}", at(15));
    assert_eq!(at(25)[..3], [0, 0, 0]);
}

#[test]
fn a_blend_with_no_parent_mixes_with_the_frames_background_on_the_processor() {
    holds_over_the_frames_background(&On::Processor);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_blend_with_no_parent_mixes_with_the_frames_background_on_the_card() {
    holds_over_the_frames_background(&On::Card);
}
