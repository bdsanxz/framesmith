//! The pictures of the placement fixtures on the processor's renderer are the boxes the compositions
//! place: each picture is worked out by hand here, from the places the composition gives, and drawn
//! pixel by pixel (`docs/spec/render-command.md` sections 4 and 4.1).

mod common;

use common::{compare, project_from_fixture};
use draw::Picture;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const CANVAS: [u8; 4] = [0x10, 0x20, 0x40, 255];
const OCHRE: [u8; 4] = [0xe0, 0xa0, 0x30, 255];
const TEAL: [u8; 4] = [0x30, 0xc0, 0xa0, 255];
const VIOLET: [u8; 4] = [0xa0, 0x50, 0xe0, 255];
const SLATE: [u8; 4] = [0x30, 0x40, 0x80, 255];
const GOLD: [u8; 4] = [0xff, 0xd0, 0x80, 255];
const MINT: [u8; 4] = [0x80, 0xff, 0x80, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

/// A box as the picture shows it: the columns `left` to `right` less one and the rows `top` to `bottom`
/// less one, filled with `colour`.
#[derive(Clone, Copy)]
struct Shown {
    left: u32,
    right: u32,
    top: u32,
    bottom: u32,
    colour: [u8; 4],
}

fn shown(left: u32, right: u32, top: u32, bottom: u32, colour: [u8; 4]) -> Shown {
    Shown {
        left,
        right,
        top,
        bottom,
        colour,
    }
}

/// The picture of a 96 by 54 frame: the canvas, with each box painted over what is there, in order.
fn worked_out(boxes: &[Shown]) -> Picture {
    let (width, height) = (96u32, 54u32);
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let mut colour = CANVAS;
            for shown in boxes {
                if (shown.left..shown.right).contains(&x) && (shown.top..shown.bottom).contains(&y) {
                    colour = shown.colour;
                }
            }
            pixels.extend_from_slice(&colour);
        }
    }
    Picture::new(width, height, pixels).unwrap()
}

/// Draws every frame of a fixture and holds each to the picture worked out for it.
fn drawn_as_worked_out(fixture: &str, boxes: &[Shown]) {
    let folder = project_from_fixture("placed-boxes-pictures", fixture, fixture);
    let project = Project::open(&folder).unwrap();
    let scene = Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"));
    let want = worked_out(boxes);
    assert_eq!((scene.width(), scene.height(), scene.frames()), (96, 54, 10));
    for number in [0, 9] {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
        if let Err(difference) = compare(number, &drawn, &want) {
            panic!("{fixture}: {difference}");
        }
    }
}

#[test]
fn a_box_placed_in_pixels_covers_the_pixels_between_its_four_edges() {
    drawn_as_worked_out("placement-in-pixels", &[shown(12, 52, 8, 38, OCHRE)]);
}

#[test]
fn boxes_placed_in_percentages_cover_the_pixels_the_shares_of_the_frame_round_to() {
    // 10% of 96 is 9.6 and 50% is 48: columns 10 to 57. 20% of 54 is 10.8 and 50% is 27: rows 11 to 37.
    // 50% of 96 is 48 and 25% is 24: columns 48 to 71. 75% of 54 is 40.5, which rounds to 41; 20% is 10.8
    // and the far edge at 51.3 rounds to 51: rows 41 to 50.
    drawn_as_worked_out(
        "placement-in-percentages",
        &[shown(10, 58, 11, 38, OCHRE), shown(48, 72, 41, 51, TEAL)],
    );
}

#[test]
fn a_box_with_two_edges_and_no_size_covers_the_space_between_them_and_a_later_box_is_drawn_over() {
    // The across box: 8 from each side, 6 from the top and the bottom: columns 8 to 87, rows 6 to 47.
    // The strip is 20% in from each side (19.2 and 76.8, which round to 19 and 77) and rows 40 to 45.
    drawn_as_worked_out(
        "placement-between-edges",
        &[shown(8, 88, 6, 48, OCHRE), shown(19, 77, 40, 46, TEAL)],
    );
}

#[test]
fn a_box_inside_a_box_is_placed_from_its_parents_edges() {
    // The outer box: columns 20 to 79, rows 10 to 39. Inside it, the inner box starts 10 and 5 from its
    // top left corner, and the far box 5 from its right and bottom edges.
    drawn_as_worked_out(
        "placement-inside-a-box",
        &[
            shown(20, 80, 10, 40, SLATE),
            shown(30, 50, 15, 25, GOLD),
            shown(65, 75, 30, 35, MINT),
        ],
    );
}

#[test]
fn a_box_placed_from_its_far_edges_is_placed_from_the_far_edge_it_writes() {
    // The corner box is 6 from the right and 4 from the bottom: columns 60 to 89 and rows 30 to 49. The
    // box with a top, a bottom and a height keeps its top: rows 2 to 11. The box with a bottom and no top
    // lies 2 from the bottom: rows 42 to 51.
    drawn_as_worked_out(
        "placement-from-the-bottom-and-right",
        &[
            shown(60, 90, 30, 50, OCHRE),
            shown(2, 22, 2, 12, TEAL),
            shown(2, 22, 42, 52, VIOLET),
        ],
    );
}

#[test]
fn a_margin_moves_the_absolute_box_in_from_its_edges_and_no_other_box() {
    // The absolute box, at left 50 and top 10 with a margin of 30, lies at 80, 40 and the frame cuts it at its right
    // edge and foot; the plain and the static box keep their places (`render-command.md` section 4.1, rule 8).
    drawn_as_worked_out(
        "placement-margin-and-position",
        &[
            shown(10, 30, 10, 30, OCHRE),
            shown(80, 96, 40, 54, TEAL),
            shown(10, 30, 40, 50, VIOLET),
        ],
    );
}

#[test]
fn a_size_of_zero_draws_nothing_and_the_child_of_an_empty_box_is_drawn() {
    // The first four boxes have a size of 0, or a size that two edges leave at 0: none is drawn. The blue
    // box has a width of 0; the white box inside it is 10 by 10 from its left and top edges.
    drawn_as_worked_out("placement-zero-sizes", &[shown(45, 55, 15, 25, WHITE)]);
}
