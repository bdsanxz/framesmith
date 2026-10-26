//! A `<composition src>` that moves, turns and fades (`docs/spec/render-command.md`, section 4.5, rule 11), measured in the
//! frame on the processor and on the card. Every expected place is worked by hand from the spec and written beside it.
//!
//! The frame is 160 by 100, black, 30 frames a second. The shown file `shown.xml` is 40 by 30: its left half (x 0 to 20) red,
//! its right half (20 to 40) blue, and a green box at (30, 10), 30 by 10, of which only x 30 to 40 lies inside the file, so
//! the cut to the file's rectangle can be seen. The `<composition src>` stands at (10, 20): its rectangle is x 10 to 50 and
//! y 20 to 50 of the frame, its origin by default its centre, (30, 35). Unmoved, the frame shows red at x 10 to 30, blue at
//! 30 to 50, and green at x 40 to 50, y 30 to 40.

mod common;

use std::fs;

use common::project_with;
use draw::Picture;
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

const RED: [u8; 3] = [255, 0, 0];
const BLUE: [u8; 3] = [0, 0, 255];
const GREEN: [u8; 3] = [0, 255, 0];
const BLACK: [u8; 3] = [0, 0, 0];
const YELLOW: [u8; 3] = [255, 255, 0];

const SHOWN: &str = "<composition version=\"1\" width=\"40\" height=\"30\" fps=\"30\" duration=\"3\" background=\"#000000\">\n\
     <box left=\"0\" top=\"0\" width=\"20\" height=\"30\" background=\"#ff0000\"/>\n\
     <box left=\"20\" top=\"0\" width=\"20\" height=\"30\" background=\"#0000ff\"/>\n\
     <box left=\"30\" top=\"10\" width=\"30\" height=\"10\" background=\"#00ff00\"/>\n\
     </composition>\n";

/// A file whose one yellow box of 4 by 4 at its corner shows from 0.5 s to 1 s of the file's time.
const WINDOWED: &str = "<composition version=\"1\" width=\"40\" height=\"30\" fps=\"30\" duration=\"3\" background=\"#000000\">\n\
     <box left=\"0\" top=\"0\" width=\"4\" height=\"4\" background=\"#ffff00\" start=\"0.5\" duration=\"0.5\"/>\n\
     </composition>\n";

#[derive(Clone, Copy)]
enum On {
    Processor,
    Card,
}

/// The scene of a composition holding `inside`, with the two shown files beside it.
fn scene(name: &str, inside: &str) -> Scene {
    let text = format!(
        "<composition version=\"1\" width=\"160\" height=\"100\" fps=\"30\" duration=\"3\" background=\"#000000\">\n{inside}\n</composition>\n"
    );
    let folder = project_with("shown-composition-movement", name, &text);
    fs::write(folder.join("shown.xml"), SHOWN).unwrap();
    fs::write(folder.join("windowed.xml"), WINDOWED).unwrap();
    Scene::from_project(&Project::open(&folder).unwrap()).unwrap_or_else(|reports| panic!("{name}: {reports:?}"))
}

fn picture(scene: &Scene, number: u64, on: On) -> Picture {
    match on {
        On::Processor => scene.picture(number, &mut CpuRenderer).unwrap(),
        On::Card => {
            let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
            scene.picture(number, &mut card).unwrap()
        }
    }
}

/// The `<composition src>` of `shown.xml` at (10, 20) with `attributes` and `inside` it.
fn shown(attributes: &str, inside: &str) -> String {
    format!("<composition src=\"shown.xml\" left=\"10\" top=\"20\" {attributes}>{inside}</composition>")
}

/// Whether the pixel is the colour, each channel within 8 levels (a pixel wholly inside an edge).
fn is(picture: &Picture, x: u32, y: u32, colour: [u8; 3]) -> bool {
    let got = picture.pixel(x, y).unwrap();
    (0..3).all(|channel| got[channel].abs_diff(colour[channel]) <= 8)
}

/// Asserts the colour of each pixel of row `y` from `from` up to `to`.
fn row_is(picture: &Picture, y: u32, from: u32, to: u32, colour: [u8; 3], what: &str) {
    for x in from..to {
        assert!(
            is(picture, x, y, colour),
            "{what}: ({x}, {y}) is {:?}, not {colour:?}",
            picture.pixel(x, y).unwrap()
        );
    }
}

/// Asserts the colour of each pixel of column `x` from `from` up to `to`.
fn column_is(picture: &Picture, x: u32, from: u32, to: u32, colour: [u8; 3], what: &str) {
    for y in from..to {
        assert!(
            is(picture, x, y, colour),
            "{what}: ({x}, {y}) is {:?}, not {colour:?}",
            picture.pixel(x, y).unwrap()
        );
    }
}

/// Criterion 9, the move: a tween of `x` from its own 0 to 60 over 1 s from `at="1"` (the proportions of 0 to 300 at 30
/// frames a second, five times smaller). Frame 30 is at 1 s: not moved. Frame 45, half way: 30 to the right. Frames 60
/// and 75: 60 to the right. Row 45 (y 25 of the file) is red from 10 + dx to 30 + dx and blue from 30 + dx to 50 + dx.
fn holds_the_move(on: On) {
    let scene = scene("move-x", &shown("", "<tween at=\"1\" duration=\"1\" to-x=\"60\"/>"));
    for (number, dx) in [(30, 0), (45, 30), (60, 60), (75, 60)] {
        let picture = picture(&scene, number, on);
        let what = format!("frame {number}");
        row_is(&picture, 45, 0, 10 + dx, BLACK, &what);
        row_is(&picture, 45, 10 + dx, 30 + dx, RED, &what);
        row_is(&picture, 45, 30 + dx, 50 + dx, BLUE, &what);
        row_is(&picture, 45, 50 + dx, 160, BLACK, &what);
    }
}

/// Criterion 9, `y`: a tween to `y="40"` over 1 s; at frame 15, half way, 20 down: column 15 is red from 40 to 70.
fn holds_the_move_down(on: On) {
    let picture = picture(&scene("move-y", &shown("", "<tween duration=\"1\" to-y=\"40\"/>")), 15, on);
    column_is(&picture, 15, 0, 40, BLACK, "above");
    column_is(&picture, 15, 40, 70, RED, "moved down");
    column_is(&picture, 15, 70, 100, BLACK, "below");
}

/// Criterion 9, `scale`, and the cut before the move: a tween to `scale="3"` over 1 s is at 2 on frame 15, about the centre
/// (30, 35). The file's x 0 to 40 goes to 30 + 2 (x + 10 - 30), from -10 to 70, its y 0 to 30 to 5 to 65. Row 55: red from
/// 0 (the frame's edge) to 30, blue from 30 to 70. The green box's part inside the file, x 30 to 40 and y 10 to 20, goes to
/// x 50 to 70, y 25 to 45; its part outside the file is cut before the scale, so row 30 is black from 70 on. Cut after the
/// scale, at the unmoved rectangle, the red would start at 10 and the blue end at 50.
fn holds_the_scale(on: On) {
    let picture = picture(&scene("scale", &shown("", "<tween duration=\"1\" to-scale=\"3\"/>")), 15, on);
    row_is(&picture, 55, 0, 30, RED, "row 55");
    row_is(&picture, 55, 30, 70, BLUE, "row 55");
    row_is(&picture, 55, 70, 160, BLACK, "row 55");
    row_is(&picture, 30, 0, 30, RED, "row 30");
    row_is(&picture, 30, 30, 50, BLUE, "row 30");
    row_is(&picture, 30, 50, 70, GREEN, "row 30");
    row_is(&picture, 30, 70, 160, BLACK, "row 30, the green outside the file cut");
    column_is(&picture, 20, 0, 5, BLACK, "column 20");
    column_is(&picture, 20, 6, 65, RED, "column 20");
    column_is(&picture, 20, 66, 100, BLACK, "column 20");
}

/// Criterion 9, `rotation`: a tween to `rotation="180"` over 1 s is at 90 degrees, clockwise, on frame 15. About (30, 35) a
/// point goes to (30 - (y - 35), 35 + (x - 30)): the file covers x 15 to 45, y 15 to 55, its red half the top (y 15 to 35),
/// its blue half the bottom. The green inside the file (frame x 40 to 50, y 30 to 40) goes to x 25 to 35, y 45 to 55; the
/// part cut away would lie at y 55 to 75. Column 20: red from 15 to 35, blue from 35 to 55, black after.
fn holds_the_rotation(on: On) {
    let picture = picture(
        &scene("rotation", &shown("", "<tween duration=\"1\" to-rotation=\"180\"/>")),
        15,
        on,
    );
    column_is(&picture, 20, 0, 15, BLACK, "column 20");
    column_is(&picture, 20, 15, 35, RED, "column 20");
    column_is(&picture, 20, 35, 55, BLUE, "column 20");
    column_is(&picture, 20, 55, 100, BLACK, "column 20");
    column_is(&picture, 30, 45, 55, GREEN, "column 30");
    column_is(&picture, 30, 55, 100, BLACK, "column 30, the green outside the file cut");
    row_is(&picture, 25, 0, 15, BLACK, "row 25");
    row_is(&picture, 25, 15, 45, RED, "row 25");
    row_is(&picture, 25, 45, 160, BLACK, "row 25");
}

/// Criterion 9, `skew-y`: a tween to `skew-y="90"` over 1 s is at 45 degrees on frame 15, so a point d pixels right of the
/// origin's x, 30, moves down by d. Column 12, its centre at 12.5, moves by -17.5: red from y 2.5 to 32.5, whole from 3 to
/// 32. Column 35, at 35.5, moves by 5.5: blue from 25.5 to 55.5, whole from 26 to 55.
fn holds_the_slant(on: On) {
    let picture = picture(&scene("skew", &shown("", "<tween duration=\"1\" to-skew-y=\"90\"/>")), 15, on);
    column_is(&picture, 12, 0, 2, BLACK, "column 12");
    column_is(&picture, 12, 3, 32, RED, "column 12");
    column_is(&picture, 12, 33, 100, BLACK, "column 12");
    column_is(&picture, 35, 0, 25, BLACK, "column 35");
    column_is(&picture, 35, 26, 55, BLUE, "column 35");
    column_is(&picture, 35, 56, 100, BLACK, "column 35");
}

/// Criterion 9, the order scale, slant, turn, move: `scale-x="2"` and `rotation="90"` written on the element. Scaled first,
/// the file is 80 across, then turned upright: a point (u, v) from the origin goes to (30 - v, 35 + 2u), so the file covers
/// x 15 to 45 and y -5 to 75, red above 35 and blue below. Turned first and then scaled across, it would be 60 wide and 40
/// tall. Column 20: red from 0 to 35, blue from 35 to 75; row 50: blue from 15 to 45 only.
fn holds_the_order(on: On) {
    let picture = picture(&scene("order", &shown("scale-x=\"2\" rotation=\"90\"", "")), 0, on);
    column_is(&picture, 20, 0, 35, RED, "column 20");
    column_is(&picture, 20, 35, 75, BLUE, "column 20");
    column_is(&picture, 20, 75, 100, BLACK, "column 20");
    row_is(&picture, 50, 0, 15, BLACK, "row 50");
    row_is(&picture, 50, 15, 45, BLUE, "row 50");
    row_is(&picture, 50, 45, 160, BLACK, "row 50");
}

/// Criterion 10: `opacity="0.5"` fades the shown file as one picture. Where the green lies over the blue, the file's picture
/// is green, and half of it over black is floor(255 · 0.5 + 0.5) = 128: (0, 128, 0). Faded element by element, the blue
/// would show through the half-clear green: (0, 128, 64). The red alone is (128, 0, 0).
fn holds_the_fade(on: On) {
    let picture = picture(&scene("opacity", &shown("opacity=\"0.5\"", "")), 0, on);
    let near = |x: u32, y: u32, wanted: [u8; 3]| {
        let got = picture.pixel(x, y).unwrap();
        assert!(
            (0..3).all(|channel| got[channel].abs_diff(wanted[channel]) <= 1),
            "({x}, {y}): {got:?} against {wanted:?}"
        );
    };
    near(45, 35, [0, 128, 0]);
    near(15, 45, [128, 0, 0]);
    near(35, 45, [0, 0, 128]);
}

/// Criterion 11: `perspective="400"` and `rotate-y="60"`. A point (u, v) from the origin (30, 35) is drawn at
/// (30, 35) + (u cos b, v) · 400 / (400 + u sin b). The left edge, u = -20: x = 30 - 10 · 400 / 382.68 = 19.55, height
/// 30 · 400 / 382.68 = 31.36; the right edge, u = 20: x = 30 + 10 · 400 / 417.32 = 39.58, height 28.75. So 20.03 wide, the
/// right edge 2.61 shorter. At 120 degrees the file is mirrored: its red half lies on the right.
fn holds_the_turn_in_space(on: On) {
    let picture = picture(&scene("rotate-y-60", &shown("perspective=\"400\" rotate-y=\"60\"", "")), 0, on);
    let lit = |x: u32, y: u32| f64::from(*picture.pixel(x, y).unwrap()[..3].iter().max().unwrap()) / 255.0;
    let width: f64 = (0..160).map(|x| lit(x, 35)).sum();
    assert!((width - 20.03).abs() <= 1.0, "width {width}");
    let column = |x: u32| (0..100).map(|y| lit(x, y)).sum::<f64>();
    let (left, right) = (column(20), column(38));
    // Columns 20 and 38, wholly inside the edges, centres at 20.5 and 38.5, by the straight line between the edges' heights.
    let height_at = |x: f64| 31.36 + (28.75 - 31.36) * (x - 19.55) / (39.58 - 19.55);
    assert!((left - height_at(20.5)).abs() <= 1.0, "column 20: {left}");
    assert!((right - height_at(38.5)).abs() <= 1.0, "column 38: {right}");
    assert!(left > right + 1.5, "the right edge is shorter: {left} and {right}");

    let mirrored = picture_of(&scene("rotate-y-120", &shown("perspective=\"400\" rotate-y=\"120\"", "")), on);
    let mean_x = |colour: [u8; 3]| {
        let xs: Vec<u32> = (0..160).filter(|x| is(&mirrored, *x, 35, colour)).collect();
        assert!(!xs.is_empty(), "no {colour:?} on row 35");
        xs.iter().sum::<u32>() as f64 / xs.len() as f64
    };
    assert!(mean_x(RED) > mean_x(BLUE), "past 90 degrees the red half lies on the right");
}

fn picture_of(scene: &Scene, on: On) -> Picture {
    picture(scene, 0, on)
}

/// Criterion 12: a `<composition src>` with `start="1"` and a tween `at="1"` of `x` to 60 over 1 s. The tween counts in the
/// time of the element's parent, the root: it begins at 1 s, not at 2. Frame 30: not moved; frame 45: 30 to the right;
/// frame 15: before the file's window, nothing. A `<set>` at 0.5 s puts `x` at 20 from frame 15 on, not before.
fn holds_the_time_of_a_tween_inside(on: On) {
    let scene_of_tween = scene("tween-start", &shown("start=\"1\"", "<tween at=\"1\" duration=\"1\" to-x=\"60\"/>"));
    let early = picture(&scene_of_tween, 15, on);
    row_is(&early, 45, 0, 160, BLACK, "frame 15");
    for (number, dx) in [(30, 0), (45, 30)] {
        let frame = picture(&scene_of_tween, number, on);
        row_is(&frame, 45, 10 + dx, 30 + dx, RED, &format!("frame {number}"));
        row_is(&frame, 45, 30 + dx, 50 + dx, BLUE, &format!("frame {number}"));
    }
    let scene_of_set = scene("set", &shown("", "<set at=\"0.5\" x=\"20\"/>"));
    for (number, dx) in [(14, 0), (15, 20), (40, 20)] {
        let frame = picture(&scene_of_set, number, on);
        row_is(&frame, 45, 10 + dx, 30 + dx, RED, &format!("set, frame {number}"));
        row_is(&frame, 45, 30 + dx, 50 + dx, BLUE, &format!("set, frame {number}"));
    }
}

/// Criterion 13: the shown file keeps its time. Its yellow box shows from 0.5 s to 1 s of the file, frames 15 to 29, whether
/// the `<composition src>` stands still at (10, 20) or is moved by `x="60"` to (70, 20).
fn holds_the_files_own_time(on: On) {
    for (name, attributes, left) in [("windowed-still", "", 10), ("windowed-moved", "x=\"60\"", 70)] {
        let scene = scene(
            name,
            &format!("<composition src=\"windowed.xml\" left=\"10\" top=\"20\" {attributes}/>"),
        );
        for (number, shown) in [(14, false), (15, true), (29, true), (30, false)] {
            let frame = picture(&scene, number, on);
            let colour = if shown { YELLOW } else { BLACK };
            row_is(&frame, 22, left, left + 4, colour, &format!("{name}, frame {number}"));
        }
    }
}

macro_rules! on_both {
    ($($check:ident: $processor:ident, $card:ident;)*) => {
        $(
            #[test]
            fn $processor() {
                $check(On::Processor);
            }

            #[test]
            #[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
            fn $card() {
                $check(On::Card);
            }
        )*
    };
}

on_both! {
    holds_the_move: a_tween_of_x_moves_the_whole_shown_file_on_the_processor,
        reference_machine_only_a_tween_of_x_moves_the_whole_shown_file_on_the_card;
    holds_the_move_down: a_tween_of_y_moves_it_down_by_half_at_the_middle_on_the_processor,
        reference_machine_only_a_tween_of_y_moves_it_down_by_half_at_the_middle_on_the_card;
    holds_the_scale: a_scale_about_the_centre_cuts_the_file_before_it_scales_on_the_processor,
        reference_machine_only_a_scale_about_the_centre_cuts_the_file_before_it_scales_on_the_card;
    holds_the_rotation: a_rotation_turns_the_cut_file_about_its_centre_on_the_processor,
        reference_machine_only_a_rotation_turns_the_cut_file_about_its_centre_on_the_card;
    holds_the_slant: a_slant_moves_each_column_down_by_its_distance_on_the_processor,
        reference_machine_only_a_slant_moves_each_column_down_by_its_distance_on_the_card;
    holds_the_order: scale_comes_before_the_turn_on_the_processor,
        reference_machine_only_scale_comes_before_the_turn_on_the_card;
    holds_the_fade: opacity_fades_the_shown_file_as_one_picture_on_the_processor,
        reference_machine_only_opacity_fades_the_shown_file_as_one_picture_on_the_card;
    holds_the_turn_in_space: a_turn_in_space_narrows_the_file_and_mirrors_it_past_90_degrees_on_the_processor,
        reference_machine_only_a_turn_in_space_narrows_the_file_and_mirrors_it_past_90_degrees_on_the_card;
    holds_the_time_of_a_tween_inside: a_tween_inside_counts_in_its_parents_time_and_a_set_at_its_instant_on_the_processor,
        reference_machine_only_a_tween_inside_counts_in_its_parents_time_and_a_set_at_its_instant_on_the_card;
    holds_the_files_own_time: the_shown_files_own_windows_keep_their_frames_on_the_processor,
        reference_machine_only_the_shown_files_own_windows_keep_their_frames_on_the_card;
}
