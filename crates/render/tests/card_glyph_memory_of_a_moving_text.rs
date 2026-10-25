//! A text that moves, grows and turns for a whole minute, drawn on the card frame by frame: the coverage the card keeps
//! for its glyph masks rises to the bound of `docs/spec/card-renderer.md` section 9.6 and holds level there to the end,
//! never above it. The kept bytes of each second are printed, as a table to keep. Reference machine only.

mod common;

use common::project_copied_from_fixture;
use draw_gpu::{CardRenderer, MASK_BOUND};
use project::Project;
use render::Scene;

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_glyph_masks_of_a_text_that_moves_for_a_minute_hold_level_at_the_bound() {
    let fixture = "motion-text-a-minute";
    let folder = project_copied_from_fixture("card-glyph-memory-of-a-moving-text", fixture, fixture);
    let scene = Scene::from_project(&Project::open(&folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"));
    assert_eq!(scene.frames(), 1800);
    let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
    let mut kept = Vec::new();
    println!("second,frame,kept bytes,bound");
    for number in 0..scene.frames() {
        scene.picture(number, &mut card).unwrap();
        kept.push(card.mask_bytes());
        if number % 30 == 29 {
            println!("{},{number},{},{MASK_BOUND}", number / 30 + 1, card.mask_bytes());
        }
    }
    let most = *kept.iter().max().unwrap();
    assert!(most <= MASK_BOUND, "the card kept {most} bytes, above the bound of {MASK_BOUND}");
    let first_full = kept
        .iter()
        .position(|&bytes| bytes * 10 >= MASK_BOUND * 9)
        .expect("the kept coverage reaches the bound");
    assert!(
        kept[first_full..].iter().all(|&bytes| bytes * 10 >= MASK_BOUND * 9),
        "the kept coverage fell below the bound after frame {first_full}"
    );
    println!("reached nine tenths of the bound at frame {first_full}; most kept {most} of {MASK_BOUND}");
}
