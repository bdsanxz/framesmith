//! A placed picture is laid on the frame 1:1 at its corner rounded to whole pixels, halves up, by source over with
//! premultiplied alpha, only inside the cut of its group; under a map that is not a whole move it is drawn by the mapped path
//! (`docs/spec/card-renderer.md` section 12; `docs/spec/render-command.md` section 4.7). The values below are worked by hand
//! from those rules.

use std::sync::Arc;

use draw::{Colour, Frame, Group, Map, Mark, PlacedPicture, Renderer};
use draw_cpu::CpuRenderer;
use layout::Rect;

const GROUND: Colour = Colour { red: 100, green: 200, blue: 40 };
const GROUND_PIXEL: [u8; 4] = [100, 200, 40, 255];

/// A picture of `width` by `height` whose pixel (x, y) is opaque and has red 10 + x, green 20 + y and blue 30.
fn counted(width: u32, height: u32, x: f32, y: f32) -> PlacedPicture {
    let mut pixels = Vec::new();
    for row in 0..height {
        for column in 0..width {
            pixels.extend_from_slice(&[10 + column as u8, 20 + row as u8, 30, 255]);
        }
    }
    PlacedPicture { width, height, pixels: Arc::new(pixels), x, y }
}

fn drawn(marks: Vec<Mark>) -> draw::Picture {
    CpuRenderer
        .draw_frame(&Frame {
            width: 16,
            height: 12,
            background: GROUND,
            marks,
        })
        .unwrap()
}

#[test]
fn the_corner_is_rounded_to_whole_pixels_halves_up_and_the_picture_is_laid_one_to_one() {
    // 3.4 rounds to 3 and 1.5 to 2: the picture covers columns 3 to 6 and rows 2 and 3.
    let picture = drawn(vec![Mark::Picture(counted(4, 2, 3.4, 1.5))]);
    for y in 0..12 {
        for x in 0..16 {
            let want = if (3..7).contains(&x) && (2..4).contains(&y) {
                [10 + (x - 3) as u8, 20 + (y - 2) as u8, 30, 255]
            } else {
                GROUND_PIXEL
            };
            assert_eq!(picture.pixel(x, y), Some(want), "({x}, {y})");
        }
    }
}

#[test]
fn a_pixel_that_is_not_opaque_is_laid_by_source_over_with_premultiplied_values() {
    // Source (64, 0, 32, 128), premultiplied, over (100, 200, 40, 255): each value is the source's plus the ground's times
    // 127 / 255, rounded to the nearest: 100 * 127 / 255 = 49.80 -> 50, 200 * 127 / 255 = 99.61 -> 100,
    // 40 * 127 / 255 = 19.92 -> 20, 255 * 127 / 255 = 127. So (114, 100, 52, 255).
    let half = PlacedPicture { width: 1, height: 1, pixels: Arc::new(vec![64, 0, 32, 128]), x: 5.0, y: 5.0 };
    let picture = drawn(vec![Mark::Picture(half)]);
    assert_eq!(picture.pixel(5, 5), Some([114, 100, 52, 255]));
    assert_eq!(picture.pixel(4, 5), Some(GROUND_PIXEL));
}

#[test]
fn the_picture_is_laid_only_inside_the_cut_of_its_group() {
    // An 8 by 8 picture at (2, 2), in a group cut to the rectangle from (4, 3) of 3 by 2: only columns 4 to 6 and rows 3
    // and 4 are laid.
    let group = Mark::Group(Group {
        map: Map::IDENTITY,
        opacity: 1.0,
        clip: Some(Rect { left: 4, top: 3, width: 3, height: 2 }),
        marks: vec![Mark::Picture(counted(8, 8, 2.0, 2.0))],
    });
    let picture = drawn(vec![group]);
    for y in 0..12 {
        for x in 0..16 {
            let want = if (4..7).contains(&x) && (3..5).contains(&y) {
                [10 + (x - 2) as u8, 20 + (y - 2) as u8, 30, 255]
            } else {
                GROUND_PIXEL
            };
            assert_eq!(picture.pixel(x, y), Some(want), "({x}, {y})");
        }
    }
}

#[test]
fn a_whole_move_of_its_group_moves_the_picture_by_those_pixels() {
    let group = Mark::Group(Group {
        map: Map::moving(5.0, -1.0),
        opacity: 1.0,
        clip: None,
        marks: vec![Mark::Picture(counted(2, 2, 1.0, 3.0))],
    });
    let picture = drawn(vec![group]);
    assert_eq!(picture.pixel(6, 2), Some([10, 20, 30, 255]));
    assert_eq!(picture.pixel(7, 3), Some([11, 21, 30, 255]));
    assert_eq!(picture.pixel(5, 2), Some(GROUND_PIXEL));
    assert_eq!(picture.pixel(1, 3), Some(GROUND_PIXEL));
}

#[test]
fn under_a_quarter_turn_the_mapped_path_samples_each_pixel_of_the_picture_exactly() {
    // The map (x, y) -> (10 - y, 2 + x) turns a quarter clockwise. The picture's pixel (i, j) covers [i, i+1] x [j, j+1] and
    // goes to [9 - j, 10 - j] x [2 + i, 3 + i]: the frame pixel (9 - j, 2 + i), whose centre falls on the centre of that
    // picture pixel, so it takes that pixel's values exactly.
    let turn = Map { a: 0.0, b: 1.0, c: -1.0, d: 0.0, e: 10.0, f: 2.0 };
    let group = Mark::Group(Group {
        map: turn,
        opacity: 1.0,
        clip: None,
        marks: vec![Mark::Picture(counted(4, 3, 0.0, 0.0))],
    });
    let picture = drawn(vec![group]);
    for j in 0..3u32 {
        for i in 0..4u32 {
            let (x, y) = (9 - j, 2 + i);
            assert_eq!(picture.pixel(x, y), Some([10 + i as u8, 20 + j as u8, 30, 255]), "picture ({i}, {j}) at ({x}, {y})");
        }
    }
    assert_eq!(picture.pixel(10, 2), Some(GROUND_PIXEL));
    assert_eq!(picture.pixel(6, 2), Some(GROUND_PIXEL));
}
