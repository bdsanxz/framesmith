//! A frame depends on the composition and its number alone, on the processor's renderer (`docs/spec/render-command.md`,
//! section 8, "The order proof"): the frame marks of the fixture `frame-order` drawn in order equal those drawn in reverse
//! and in a mixed order, with one renderer kept throughout; and the comparison fails, naming the frame, on a control made to
//! depend on drawing order.

mod common;
mod frame_order;

use draw_cpu::CpuRenderer;
use frame_order::{in_order, marks, reversed, same_as_in_order, scene, CarriesAValue, MIXED};

const GROUP: &str = "frame-order-on-the-processor";

fn marks_in_order(scene: &render::Scene) -> Vec<u64> {
    marks(scene, &mut CpuRenderer, &in_order(scene))
        .into_iter()
        .map(|(_, mark)| mark)
        .collect()
}

#[test]
fn the_marks_drawn_in_order_equal_those_drawn_in_reverse_frame_by_frame() {
    let scene = scene(GROUP);
    assert_eq!(scene.frames(), 60);
    let in_order = marks_in_order(&scene);
    let mut renderer = CpuRenderer;
    let reversed = marks(&scene, &mut renderer, &reversed(&scene));
    same_as_in_order(&in_order, &reversed).unwrap();
    // The proof means something only if the frames differ: the composition moves on most frames.
    let distinct = in_order.iter().collect::<std::collections::HashSet<_>>().len();
    assert!(distinct > 50, "{distinct} distinct marks of 60");
}

#[test]
fn frames_asked_for_in_a_mixed_order_with_repeats_and_jumps_across_a_clip_start_have_their_marks_in_order() {
    let scene = scene(GROUP);
    let in_order = marks_in_order(&scene);
    // The clips start on frames 20 and 36: the frames on both sides of each start differ.
    assert_ne!(in_order[19], in_order[20]);
    assert_ne!(in_order[35], in_order[36]);
    let mut renderer = CpuRenderer;
    same_as_in_order(&in_order, &marks(&scene, &mut renderer, &MIXED)).unwrap();
}

#[test]
fn a_renderer_made_to_depend_on_drawing_order_fails_the_proof_naming_the_first_frame_whose_mark_differs() {
    let scene = scene(GROUP);
    let mut control = CarriesAValue::new(CpuRenderer);
    let in_order: Vec<u64> = marks(&scene, &mut control, &in_order(&scene))
        .into_iter()
        .map(|(_, mark)| mark)
        .collect();
    let mut control = CarriesAValue::new(CpuRenderer);
    let failure = same_as_in_order(&in_order, &marks(&scene, &mut control, &reversed(&scene))).unwrap_err();
    assert!(failure.starts_with("frame 0059: mark "), "{failure}");
    let mut control = CarriesAValue::new(CpuRenderer);
    let failure = same_as_in_order(&in_order, &marks(&scene, &mut control, &MIXED)).unwrap_err();
    assert!(failure.starts_with("frame 0059: mark "), "{failure}");
    // The same order with a new control for every frame carries nothing, and passes: the failure is the order's.
    let fresh: Vec<(u64, u64)> = MIXED
        .iter()
        .map(|&number| (number, scene.picture(number, &mut CarriesAValue::new(CpuRenderer)).unwrap().mark()))
        .collect();
    let in_order_fresh = marks_in_order(&scene);
    same_as_in_order(&in_order_fresh, &fresh).unwrap();
}

#[test]
fn a_drawn_frame_and_its_copy_have_one_mark_and_a_copy_with_one_pixel_changed_has_another() {
    let scene = scene(GROUP);
    let drawn = scene.picture(30, &mut CpuRenderer).unwrap();
    let copy = draw::Picture::new(drawn.width(), drawn.height(), drawn.pixels().to_vec()).unwrap();
    assert_eq!(drawn.mark(), copy.mark());
    // The pixel in column 80 of row 45 changed by one in its green, and nothing else.
    let mut pixels = drawn.pixels().to_vec();
    let at = ((45 * drawn.width() + 80) * 4 + 1) as usize;
    pixels[at] ^= 1;
    let changed = draw::Picture::new(drawn.width(), drawn.height(), pixels).unwrap();
    assert_ne!(drawn.mark(), changed.mark());
}
