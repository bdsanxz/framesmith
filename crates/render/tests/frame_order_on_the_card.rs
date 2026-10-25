//! A frame depends on the composition and its number alone, on the card's renderer (`docs/spec/render-command.md`,
//! section 8, "The order proof"): the frame marks of the fixture `frame-order` drawn in order on the card equal those drawn
//! in reverse and in a mixed order, with one card renderer kept throughout, and the control made to depend on drawing order
//! fails. Each needs a graphics card that frx7 can draw with: reference machine only.

mod common;
mod frame_order;

use draw_gpu::CardRenderer;
use frame_order::{in_order, marks, reversed, same_as_in_order, scene, CarriesAValue, MIXED};

const GROUP: &str = "frame-order-on-the-card";

fn card() -> CardRenderer {
    CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"))
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_s_marks_drawn_in_order_equal_those_drawn_in_reverse_and_in_a_mixed_order() {
    let scene = scene(GROUP);
    let mut renderer = card();
    let in_order: Vec<u64> = marks(&scene, &mut renderer, &in_order(&scene))
        .into_iter()
        .map(|(_, mark)| mark)
        .collect();
    same_as_in_order(&in_order, &marks(&scene, &mut renderer, &reversed(&scene))).unwrap();
    same_as_in_order(&in_order, &marks(&scene, &mut renderer, &MIXED)).unwrap();
    // A second card renderer, new, gives the same marks.
    same_as_in_order(&in_order, &marks(&scene, &mut card(), &MIXED)).unwrap();
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_card_renderer_made_to_depend_on_drawing_order_fails_the_proof_naming_the_frame() {
    let scene = scene(GROUP);
    let mut control = CarriesAValue::new(card());
    let in_order: Vec<u64> = marks(&scene, &mut control, &in_order(&scene))
        .into_iter()
        .map(|(_, mark)| mark)
        .collect();
    let mut control = CarriesAValue::new(card());
    let failure = same_as_in_order(&in_order, &marks(&scene, &mut control, &reversed(&scene))).unwrap_err();
    assert!(failure.starts_with("frame 0059: mark "), "{failure}");
}
