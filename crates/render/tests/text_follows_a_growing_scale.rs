//! A glyph's outline is never hinted (`docs/spec/render-command.md`, section 4.5, rule 7; `docs/spec/card-renderer.md`,
//! section 9): a text that grows under a uniform scale about its middle, and moves by parts of a pixel, follows its map in
//! every frame. Its ink's centre is where the map takes the centre at rest, within 0.3 pixels, and its ink is the ink at
//! rest times the map's area, within 3 per cent, on the processor and on the card. A hinted outline is snapped to the
//! pixel rows at each size, so its ink jumps in steps: up to 0.9 pixels and 13 per cent from the map in this text.

mod common;

use std::fs;

use common::project_copied_from_fixture;
use common::worked::{ink, near, palindrome_centre_across_from_middle, tween, Worked};
use draw::Renderer;
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

/// The text of the fixture `frame-order` alone, 16 pixels at (40, 60), growing to 1.3 about its middle and moving 20
/// pixels left, in-expo from 0.2 s for 1.6 s, on white, 160 by 90, 30 frames a second for 2 s.
fn growing_text(group: &str) -> Scene {
    let folder = project_copied_from_fixture(group, "growing-text", "frame-order");
    fs::write(
        folder.join("composition.xml"),
        r##"<composition version="1" id="growing-text" width="160" height="90" fps="30" duration="2" background="#ffffff">
  <font src="fonts/framesmith-test-stroke-400.ttf" family="Framesmith Test Stroke"/>
  <text left="40" top="60" font-family="Framesmith Test Stroke" font-size="16" color="#202020">HIIH<tween at="0.2" duration="1.6" to-x="-20" to-scale="1.3" ease="in-expo"/></text>
</composition>
"##,
    )
    .unwrap();
    Scene::from_project(&Project::open(&folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

/// Holds every frame's text to its map, drawn by one renderer.
fn every_frame_follows_the_map(scene: &Scene, renderer: &mut dyn Renderer) -> Result<(), String> {
    let (size, top) = (16.0, 60.0);
    let blank = Worked::new(160, 90, [255, 255, 255]);
    let whole = (0.0, 0.0, 160.0, 90.0);
    let at_rest = ink(&scene.picture(0, renderer).unwrap(), &blank, [0x20; 3], whole);
    // From the middle of the paragraph, about which the text grows: across as the font's spec gives it, down as measured.
    let from_middle = (palindrome_centre_across_from_middle(size), at_rest.centre.1 - (top + 0.55 * size));
    for number in 0..scene.frames() {
        let t = number as f64 / 30.0;
        let (across, scale) = (tween(t, 0.2, 1.6, 0.0, -20.0, "in-expo"), tween(t, 0.2, 1.6, 1.0, 1.3, "in-expo"));
        let inked = ink(&scene.picture(number, renderer).unwrap(), &blank, [0x20; 3], whole);
        let centre = (
            at_rest.centre.0 + across + (scale - 1.0) * from_middle.0,
            at_rest.centre.1 + (scale - 1.0) * from_middle.1,
        );
        near(&format!("frame {number}: the ink's centre across"), inked.centre.0, centre.0, 0.3)?;
        near(&format!("frame {number}: the ink's centre down"), inked.centre.1, centre.1, 0.3)?;
        near(
            &format!("frame {number}: the ink over the ink at rest"),
            inked.mass / at_rest.mass,
            scale * scale,
            0.03 * scale * scale,
        )?;
    }
    Ok(())
}

#[test]
fn a_growing_text_follows_its_map_in_every_frame_on_the_processor() {
    every_frame_follows_the_map(&growing_text("growing-text-on-the-processor"), &mut CpuRenderer).unwrap();
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_growing_text_follows_its_map_in_every_frame_on_the_card() {
    let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
    every_frame_follows_the_map(&growing_text("growing-text-on-the-card"), &mut card).unwrap();
}
