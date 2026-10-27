//! The colour of a text's stroke is laid on its coverage by the rule of `docs/spec/box-edges.md`: each channel is the colour
//! times the alpha of the coverage, divided by 255 and rounded to the nearest whole level, halves up, and the layer is laid
//! over the picture by src-over (`docs/spec/text.md`, section 6.6, rule 10).
//!
//! Worked from the spec at pixel (35, 22) of frame 0 of `motion-text` (`fixtures/motion-text/composition.xml`): a 1-pixel
//! stroke of #d03020 over white, with no fill and no shadow there. One sub-path of the outline meets the pixel: a horizontal
//! edge at y = 22.7 whose strip below it, 0.3 high and 1 wide, lies inside the stroke, so the exact share is 0.3. The rasteriser
//! gives the coverage byte 76, the floor of 76.5, which lies within one level of the exact 77 that halves up would give
//! (`docs/spec/text.md`, section 6.6, rule 10). The colour is then laid on that byte by the rule: the red channel is
//! 208 * 76 / 255 = 62.0, so 62; the green 48 * 76 / 255 = 14.3, so 14; the blue 32 * 76 / 255 = 9.5, so 10. Over white,
//! each channel is its layer value plus 255 - 76 = 179: (241, 193, 189). With the exact 77 the same rule gives (241, 192, 188).

mod common;

use common::project_copied_from_fixture;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

/// The pixel of the frame, as its red, green and blue channels.
fn pixel_at(x: usize, y: usize) -> (u8, u8, u8) {
    let folder = project_copied_from_fixture("text-stroke-colour-laid-by-rule", "motion-text", "motion-text");
    let project = Project::open(&folder).unwrap();
    let scene = Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"));
    let picture = scene.picture(0, &mut CpuRenderer).unwrap();
    let at = (y * picture.width() as usize + x) * 4;
    let pixels = picture.pixels();
    (pixels[at], pixels[at + 1], pixels[at + 2])
}

#[test]
fn the_stroke_colour_is_laid_on_the_coverage_by_the_rule_at_a_horizontal_edge() {
    assert_eq!(pixel_at(35, 22), (241, 193, 189));
}
