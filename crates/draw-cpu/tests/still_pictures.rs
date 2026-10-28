//! Still pictures drawn by the processor (`docs/spec/still-pictures.md`, section 5), against values worked by hand from
//! the spec: the picture at its own size, enlarged, made smaller, transparent, at an edge between pixels, and cut to its
//! box.

use std::sync::Arc;

use draw::{Colour, Fit, Frame, Map, Mark, Picture, Renderer, Still, StillMark};
use draw_cpu::CpuRenderer;
use layout::Rect;

fn still(width: u32, height: u32, pixels: Vec<u8>) -> Arc<Still> {
    Arc::new(Still::from_straight(width, height, &pixels).unwrap())
}

fn grey(values: &[u8]) -> Vec<u8> {
    values.iter().flat_map(|&value| [value, value, value, 255]).collect()
}

fn placed(left: i32, top: i32, width: u32, height: u32, still: Arc<Still>, fit: Fit, position: (f64, f64)) -> Mark {
    StillMark {
        rect: Rect { left, top, width, height },
        still,
        fit,
        position,
    }
    .mark()
}

fn drawn(width: u32, height: u32, background: Colour, marks: Vec<Mark>) -> Picture {
    CpuRenderer
        .draw_frame(&Frame {
            width,
            height,
            background,
            marks,
        })
        .unwrap()
}

const BLACK: Colour = Colour { red: 0, green: 0, blue: 0 };
const BLUE: Colour = Colour {
    red: 0,
    green: 0,
    blue: 255,
};

fn red_of(picture: &Picture, x: u32, y: u32) -> u8 {
    picture.pixel(x, y).unwrap()[0]
}

#[test]
fn a_picture_at_its_own_size_on_whole_pixels_gives_its_own_pixels() {
    let pixels: Vec<u8> = (0..6_u8).flat_map(|index| [index * 40, 255 - index * 30, index * 7, 255]).collect();
    let picture = drawn(
        10,
        8,
        BLUE,
        vec![placed(4, 5, 3, 2, still(3, 2, pixels.clone()), Fit::Fill, (0.5, 0.5))],
    );
    for row in 0..2 {
        for column in 0..3 {
            let at = (row * 3 + column) as usize * 4;
            assert_eq!(
                picture.pixel(4 + column, 5 + row).unwrap(),
                [pixels[at], pixels[at + 1], pixels[at + 2], 255]
            );
        }
    }
    // Around it the picture below is untouched.
    for (x, y) in [(3, 5), (7, 5), (4, 4), (4, 7)] {
        assert_eq!(picture.pixel(x, y).unwrap(), [0, 0, 255, 255], "({x}, {y})");
    }
}

#[test]
fn a_picture_drawn_larger_is_sampled_between_its_two_nearest_pixels() {
    // Black and white, 2 by 1, drawn 4 by 2: u = (x + 0.5) / 2 is 0.25, 0.75, 1.25, 1.75, and the weights of the two pixels
    // are t(0.5 - u) and t(1.5 - u): (0.75, 0), (0.75, 0.25), (0.25, 0.75), (0, 0.75), so the values are 0, 255 * 0.25 / 1
    // = 63.75, 255 * 0.75 / 1 = 191.25 and 255: 0, 64, 191, 255.
    let picture = drawn(
        6,
        4,
        BLUE,
        vec![placed(1, 1, 4, 2, still(2, 1, grey(&[0, 255])), Fit::Fill, (0.5, 0.5))],
    );
    for row in [1, 2] {
        let values: Vec<u8> = (1..5).map(|x| red_of(&picture, x, row)).collect();
        assert_eq!(values, vec![0, 64, 191, 255], "row {row}");
    }
}

#[test]
fn a_picture_drawn_smaller_is_averaged_over_the_pixels_its_pixel_spans() {
    // 0, 0, 255, 255 drawn in 2 by 1: the reach across is 4 / 2 = 2; at x = 0, u = 1.0, and the weights of the four pixels
    // are t(-0.25), t(0.25), t(0.75), t(1.25) = 0.75, 0.75, 0.25, 0: (0.25 * 255) / 1.75 = 36.43, so 36. At x = 1, u = 3.0:
    // 0, 0.25, 0.75, 0.75, so (1.5 * 255) / 1.75 = 218.57, 219. Sampled between the two nearest pixels alone it would be
    // 0 and 255.
    let picture = drawn(
        2,
        1,
        BLUE,
        vec![placed(0, 0, 2, 1, still(4, 1, grey(&[0, 0, 255, 255])), Fit::Fill, (0.5, 0.5))],
    );
    assert_eq!([red_of(&picture, 0, 0), red_of(&picture, 1, 0)], [36, 219]);
}

#[test]
fn a_transparent_part_shows_what_is_under_it() {
    // Straight (255, 0, 0, 128) is premultiplied to (128, 0, 0, 128) and laid over blue: red 128 + 0, blue 0 +
    // round(255 * 127 / 255) = 127. A pixel of alpha 0 changes nothing.
    let pixels = vec![255, 0, 0, 128, 255, 255, 255, 0];
    let picture = drawn(2, 1, BLUE, vec![placed(0, 0, 2, 1, still(2, 1, pixels), Fit::Fill, (0.5, 0.5))]);
    assert_eq!(picture.pixel(0, 0).unwrap(), [128, 0, 127, 255]);
    assert_eq!(picture.pixel(1, 0).unwrap(), [0, 0, 255, 255]);
}

#[test]
fn an_edge_between_pixels_covers_its_share_of_the_pixel() {
    // A white portrait of 40 by 80 contained in (10, 20) 100 by 50 is drawn from x = 47.5 to 72.5: columns 47 and 72 are
    // half covered, round(255 * 0.5) = 128 halves up, and laid over black they are 128.
    let picture = drawn(
        120,
        80,
        BLACK,
        vec![placed(
            10,
            20,
            100,
            50,
            still(40, 80, vec![255; 40 * 80 * 4]),
            Fit::Contain,
            (0.5, 0.5),
        )],
    );
    let row: Vec<u8> = (45..75).map(|x| red_of(&picture, x, 40)).collect();
    let mut wanted = vec![0, 0, 128];
    wanted.extend([255; 24]);
    wanted.extend([128, 0, 0]);
    assert_eq!(row, wanted);
    // The rest of the box shows what is under it.
    assert_eq!(red_of(&picture, 20, 40), 0);
    assert_eq!(red_of(&picture, 100, 40), 0);
}

#[test]
fn cover_draws_nothing_outside_the_box() {
    // A white square of 20 covering (5, 5) 20 by 10 at 50% 50% is placed at (5, 0) 20 by 20: rows 0 to 4 and 15 to 19 are
    // beyond the box and keep the black below.
    let picture = drawn(
        30,
        20,
        BLACK,
        vec![placed(5, 5, 20, 10, still(20, 20, vec![255; 20 * 20 * 4]), Fit::Cover, (0.5, 0.5))],
    );
    for y in 0..20 {
        let wanted = if (5..15).contains(&y) { 255 } else { 0 };
        assert_eq!(red_of(&picture, 10, y), wanted, "row {y}");
    }
    for x in [4, 25] {
        assert_eq!(red_of(&picture, x, 10), 0, "column {x}");
    }
}

#[test]
fn a_corner_named_by_the_position_holds_the_picture_there() {
    // A picture of 4 by 2 whose left column is 10 and right column 250, covering a box of 2 by 2 (scale 1): at 0% it shows
    // its two left columns, 10 and 90; at 100% its two right columns, 170 and 250.
    let pixels = grey(&[10, 90, 170, 250, 10, 90, 170, 250]);
    for (position, wanted) in [((0.0, 0.0), [10, 90]), ((1.0, 1.0), [170, 250])] {
        let picture = drawn(
            2,
            2,
            BLACK,
            vec![placed(0, 0, 2, 2, still(4, 2, pixels.clone()), Fit::Cover, position)],
        );
        assert_eq!([red_of(&picture, 0, 0), red_of(&picture, 1, 0)], wanted, "{position:?}");
    }
    // Contained in 4 by 4 (scale 1), at 100% 100% it stands at the bottom, rows 2 and 3; rows 0 and 1 show the black.
    let picture = drawn(4, 4, BLACK, vec![placed(0, 0, 4, 4, still(4, 2, pixels), Fit::Contain, (1.0, 1.0))]);
    let column: Vec<u8> = (0..4).map(|y| red_of(&picture, 0, y)).collect();
    assert_eq!(column, vec![0, 0, 10, 10]);
}

#[test]
fn a_picture_moved_by_whole_pixels_is_the_picture_drawn_there() {
    let pixels: Vec<u8> = (0..12_u8).flat_map(|index| [index * 20, 100, 255 - index * 20, 255]).collect();
    let moved = drawn(
        12,
        10,
        BLUE,
        vec![Mark::Group(draw::Group {
            map: Map::moving(3.0, 2.0),
            opacity: 1.0,
            clip: None,
            marks: vec![placed(1, 1, 8, 6, still(4, 3, pixels.clone()), Fit::Fill, (0.5, 0.5))],
            turn: None,
            filter: draw::Filter::NONE,
            shadow: None,
        })],
    );
    let there = drawn(12, 10, BLUE, vec![placed(4, 3, 8, 6, still(4, 3, pixels), Fit::Fill, (0.5, 0.5))]);
    assert_eq!(moved, there);
}
