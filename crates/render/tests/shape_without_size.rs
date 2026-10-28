//! A `<shape>` with no `width` and no `height` has no size and shows nothing (`docs/spec/shape.md`,
//! section 10; ticket T062 criterion 10): it is not sized as a box is, as large as its parent.

mod common;

use common::project_with;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const GROUND: [u8; 4] = [16, 32, 64, 255];

#[test]
fn a_shape_with_no_width_and_no_height_shows_nothing() {
    let folder = project_with(
        "shape-without-size",
        "root",
        "<composition version=\"1\" width=\"64\" height=\"64\" fps=\"10\" duration=\"1\" background=\"#102040\">\n<shape d=\"M 0 0 H 24 V 24 H 0 Z\" color=\"#E0A030\"/>\n</composition>\n",
    );
    let project = Project::open(&folder).unwrap();
    let scene = Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"));
    let picture = scene.picture(0, &mut CpuRenderer).unwrap();
    for (x, y) in [(2, 2), (61, 2), (2, 61), (61, 61), (32, 32)] {
        assert_eq!(picture.pixel(x, y).unwrap(), GROUND, "({x}, {y})");
    }
}
