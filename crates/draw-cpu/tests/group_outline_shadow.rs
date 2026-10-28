//! A shadow that follows a group (`docs/spec/render-command.md`, section 4.7, rule 5): `drop-shadow` takes the alpha of the
//! finished picture of the element, so a group of two children casts the shadow of both children, and not of the box that
//! would hold them. The gap between the children casts none. The expected colours are worked by hand.

use draw::{Colour, Filter, Frame, Group, Map, Mark, Renderer, Shadow, Step};
use draw_cpu::CpuRenderer;
use layout::Rect;

const WHITE: Colour = Colour {
    red: 255,
    green: 255,
    blue: 255,
};
const RED: Colour = Colour {
    red: 200,
    green: 0,
    blue: 0,
};
const BLACK: [u8; 3] = [0, 0, 0];

fn child(left: i32) -> Mark {
    Mark::Step(Step::Fill {
        rect: Rect {
            left,
            top: 0,
            width: 10,
            height: 10,
        },
        colour: RED,
    })
}

#[test]
fn a_shadow_of_a_group_follows_each_child_and_not_the_box_around_them() {
    // Two children, 0 to 9 and 30 to 39 across, both 0 to 9 down. The shadow moves them 10 across and 10 down: 10 to 19 and
    // 40 to 49 across, 10 to 19 down. A shadow of the box that holds both would cover 10 to 49 across, the gap included.
    let group = Group {
        map: Map::IDENTITY,
        opacity: 1.0,
        clip: None,
        marks: vec![child(0), child(30)],
        turn: None,
        filter: Filter::NONE,
        shadow: Some(Shadow {
            across: 10.0,
            down: 10.0,
            blur: 0.0,
            colour: Colour { red: 0, green: 0, blue: 0 },
        }),
    };
    let picture = CpuRenderer
        .draw_frame(&Frame {
            width: 60,
            height: 40,
            background: WHITE,
            marks: vec![Mark::Group(group)],
        })
        .unwrap();
    let rgb = |x: u32, y: u32| {
        let [red, green, blue, alpha] = picture.pixel(x, y).unwrap();
        assert_eq!(alpha, 255, "the picture is opaque at ({x}, {y})");
        [red, green, blue]
    };
    // Under the first child's shadow, beyond the first child: black.
    assert_eq!(rgb(15, 15), BLACK, "the shadow of the first child");
    // Under the second child's shadow, beyond the second child: black.
    assert_eq!(rgb(45, 15), BLACK, "the shadow of the second child");
    // In the gap between the children, moved to 25 across: no child there, so no shadow, and the background shows.
    assert_eq!(rgb(25, 15), [255, 255, 255], "the gap casts no shadow");
    // The children themselves are drawn over their shadows.
    assert_eq!(rgb(5, 5), [200, 0, 0], "the first child is drawn");
}
