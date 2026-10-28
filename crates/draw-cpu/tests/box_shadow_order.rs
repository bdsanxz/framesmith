//! Stacked box shadows on the processor (`docs/spec/render-command.md`, section 4.7, rule 4): a box's shadows are drawn before
//! its background, the last listed first, so the first listed lies on top. The marks of a frame are drawn in order, and the
//! timeline puts the last listed shadow first. The expected colours are worked by hand: where two shadows overlap, the one
//! drawn last shows.

use draw::{BoxShadow, Colour, Frame, Mark, Renderer, Rounded, Shadow};
use draw_cpu::CpuRenderer;
use layout::Rect;

const WHITE: Colour = Colour {
    red: 255,
    green: 255,
    blue: 255,
};

fn shadow_box(colour: Colour) -> Mark {
    let rect = Rect {
        left: 0,
        top: 0,
        width: 20,
        height: 20,
    };
    Mark::BoxShadow(BoxShadow {
        shape: Rounded::of_box(rect, 0.0).expect("a box with a size has a shape"),
        shadow: Shadow {
            across: 10.0,
            down: 10.0,
            blur: 0.0,
            colour,
        },
    })
}

fn draw(marks: Vec<Mark>) -> [u8; 3] {
    let picture = CpuRenderer
        .draw_frame(&Frame {
            width: 40,
            height: 40,
            background: WHITE,
            marks,
        })
        .unwrap();
    // (25, 15) lies in both moved boxes and beyond the box itself.
    let [red, green, blue, alpha] = picture.pixel(25, 15).unwrap();
    assert_eq!(alpha, 255);
    [red, green, blue]
}

#[test]
fn the_shadow_drawn_last_lies_on_top_where_two_shadows_overlap() {
    let red = Colour {
        red: 255,
        green: 0,
        blue: 0,
    };
    let blue = Colour {
        red: 0,
        green: 0,
        blue: 255,
    };
    // The list `blue, red` is drawn as red first, then blue, so blue lies on top.
    assert_eq!(draw(vec![shadow_box(red), shadow_box(blue)]), [0, 0, 255]);
    // The reverse order draws blue first, so red lies on top.
    assert_eq!(draw(vec![shadow_box(blue), shadow_box(red)]), [255, 0, 0]);
}
