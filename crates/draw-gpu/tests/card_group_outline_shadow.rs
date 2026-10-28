//! A shadow that follows a group, drawn by the card (`docs/spec/render-command.md`, section 4.7, rules 5 and 6): the card lays
//! the processor's picture of the group with its shadow, so the two agree, and the gap between two children casts no shadow.
//! The card needs a graphics card that frx7 can draw with, so this test is reference machine only.

use draw::{Colour, Filter, Frame, Group, Map, Mark, Renderer, Shadow, Step};
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
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

fn frame() -> Frame {
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
    Frame {
        width: 60,
        height: 40,
        background: WHITE,
        marks: vec![Mark::Group(group)],
    }
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_draws_a_group_shadow_as_the_processor_does() {
    let frame = frame();
    let processor = CpuRenderer.draw_frame(&frame).unwrap();
    let card = CardRenderer::new().unwrap().draw_frame(&frame).unwrap();
    for (x, y) in [(15, 15), (45, 15), (25, 15), (5, 5)] {
        let want = processor.pixel(x, y).unwrap();
        let got = card.pixel(x, y).unwrap();
        for channel in 0..3 {
            assert!(
                want[channel].abs_diff(got[channel]) <= 4,
                "at ({x}, {y}) channel {channel}: processor {want:?}, card {got:?}"
            );
        }
    }
    assert_eq!(
        card.pixel(25, 15).unwrap()[..3],
        [255, 255, 255],
        "the gap casts no shadow on the card"
    );
}
