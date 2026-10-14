//! The renderer on the processor draws the canvas and filled rectangles exactly, as worked values
//! (`docs/spec/render-command.md` section 4).

use draw::{Colour, DisplayList, Picture, Renderer, Step};
use draw_cpu::CpuRenderer;
use layout::Rect;

const BACKGROUND: Colour = Colour {
    red: 16,
    green: 32,
    blue: 64,
};
const RED: Colour = Colour {
    red: 255,
    green: 0,
    blue: 0,
};
const GREEN: Colour = Colour {
    red: 0,
    green: 255,
    blue: 0,
};

fn list(width: u32, height: u32, steps: Vec<Step>) -> DisplayList {
    DisplayList {
        width,
        height,
        background: BACKGROUND,
        steps,
    }
}

fn fill(left: i32, top: i32, width: u32, height: u32, colour: Colour) -> Step {
    Step::Fill {
        rect: Rect { left, top, width, height },
        colour,
    }
}

fn draw(list: &DisplayList) -> Picture {
    CpuRenderer.draw(list).unwrap()
}

fn pixel(picture: &Picture, x: u32, y: u32) -> [u8; 4] {
    picture.pixel(x, y).unwrap()
}

fn rgba(colour: Colour) -> [u8; 4] {
    [colour.red, colour.green, colour.blue, 255]
}

#[test]
fn the_renderer_is_the_one_on_the_processor() {
    assert_eq!(CpuRenderer.name(), "processor");
}

#[test]
fn a_list_with_no_step_is_the_canvas_with_full_alpha() {
    let picture = draw(&list(4, 3, Vec::new()));
    assert_eq!((picture.width(), picture.height()), (4, 3));
    assert_eq!(picture.pixels().len(), 4 * 3 * 4);
    for y in 0..3 {
        for x in 0..4 {
            assert_eq!(pixel(&picture, x, y), [16, 32, 64, 255]);
        }
    }
}

#[test]
fn a_box_fills_exactly_the_pixels_from_its_edges_and_no_others() {
    let picture = draw(&list(10, 8, vec![fill(2, 1, 4, 3, RED)]));
    for y in 0..8 {
        for x in 0..10 {
            let inside = (2..6).contains(&x) && (1..4).contains(&y);
            let expected = if inside { rgba(RED) } else { rgba(BACKGROUND) };
            assert_eq!(pixel(&picture, x, y), expected, "({x}, {y})");
        }
    }
}

#[test]
fn a_box_written_later_covers_one_written_earlier() {
    let picture = draw(&list(10, 8, vec![fill(0, 0, 6, 6, RED), fill(3, 3, 6, 4, GREEN)]));
    assert_eq!(pixel(&picture, 2, 2), rgba(RED));
    assert_eq!(pixel(&picture, 5, 2), rgba(RED));
    assert_eq!(pixel(&picture, 3, 3), rgba(GREEN));
    assert_eq!(pixel(&picture, 5, 5), rgba(GREEN));
    assert_eq!(pixel(&picture, 2, 5), rgba(RED));
    assert_eq!(pixel(&picture, 8, 6), rgba(GREEN));
    assert_eq!(pixel(&picture, 9, 7), rgba(BACKGROUND));
    // The same boxes in the other order give the other cover.
    let reversed = draw(&list(10, 8, vec![fill(3, 3, 6, 4, GREEN), fill(0, 0, 6, 6, RED)]));
    assert_eq!(pixel(&reversed, 3, 3), rgba(RED));
    assert_eq!(pixel(&reversed, 5, 5), rgba(RED));
    assert_eq!(pixel(&reversed, 8, 6), rgba(GREEN));
}

#[test]
fn the_part_of_a_box_outside_the_frame_is_not_drawn_and_the_part_inside_is() {
    let picture = draw(&list(6, 4, vec![fill(-2, -1, 4, 3, RED), fill(4, 3, 10, 10, GREEN)]));
    for y in 0..4 {
        for x in 0..6 {
            let red = x < 2 && y < 2;
            let green = x >= 4 && y >= 3;
            let expected = if red {
                rgba(RED)
            } else if green {
                rgba(GREEN)
            } else {
                rgba(BACKGROUND)
            };
            assert_eq!(pixel(&picture, x, y), expected, "({x}, {y})");
        }
    }
}

#[test]
fn a_box_of_size_zero_or_wholly_outside_the_frame_changes_nothing() {
    let steps = vec![
        fill(1, 1, 0, 5, RED),
        fill(1, 1, 5, 0, RED),
        fill(6, 0, 3, 3, RED),
        fill(-9, 0, 3, 3, RED),
        fill(0, 4, 3, 3, RED),
    ];
    assert_eq!(draw(&list(6, 4, steps)), draw(&list(6, 4, Vec::new())));
}

#[test]
fn a_box_as_large_as_the_frame_covers_every_pixel() {
    let picture = draw(&list(6, 4, vec![fill(0, 0, 6, 4, GREEN)]));
    assert!(picture.pixels().as_chunks::<4>().0.iter().all(|pixel| *pixel == rgba(GREEN)));
}

#[test]
fn the_same_list_gives_the_same_picture_whatever_was_drawn_before() {
    let one = list(8, 6, vec![fill(1, 1, 3, 3, RED)]);
    let other = list(5, 5, vec![fill(0, 0, 5, 5, GREEN)]);
    let mut renderer = CpuRenderer;
    let first = renderer.draw(&one).unwrap();
    renderer.draw(&other).unwrap();
    assert_eq!(renderer.draw(&one).unwrap(), first);
}

#[test]
fn a_frame_with_no_pixel_or_too_many_is_refused() {
    assert!(CpuRenderer.draw(&list(0, 4, Vec::new())).is_err());
    assert!(CpuRenderer.draw(&list(4, 0, Vec::new())).is_err());
    assert!(CpuRenderer.draw(&list(8193, 8193, Vec::new())).is_err());
}
