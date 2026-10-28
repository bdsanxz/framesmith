//! A box inside a box is placed from its parent's edges, and moves with the parent when the parent's
//! `left` changes (`docs/spec/render-command.md` section 4.1, rule 4).

mod common;

use common::{composition, extent_of, project_with};
use draw::Picture;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const PARENT: [u8; 4] = [0x30, 0x40, 0x80, 255];
const CHILD: [u8; 4] = [255, 255, 255, 255];

/// The picture of frame 0 of a parent at `parent_left`, 30 by 20 pixels, with a child 5 pixels in from
/// its left edge and 4 from its top, 8 by 6 pixels.
fn picture_with_parent_at(name: &str, parent_left: u32) -> Picture {
    let inside = format!(
        "<box id=\"parent\" left=\"{parent_left}\" top=\"10\" width=\"30\" height=\"20\" background=\"#304080\">\n\
         <box id=\"child\" left=\"5\" top=\"4\" width=\"8\" height=\"6\" background=\"#ffffff\"/>\n\
         </box>"
    );
    let folder = project_with("child-follows-parent", name, &composition(&inside));
    let project = Project::open(&folder).unwrap();
    let scene = Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    scene.picture(0, &mut CpuRenderer).unwrap()
}

#[test]
fn a_box_inside_a_box_is_placed_from_its_parents_edges_and_moves_with_the_parent() {
    let before = picture_with_parent_at("before", 20);
    let after = picture_with_parent_at("after", 30);

    // The parent at 20 covers the columns 20 to 49 and the rows 10 to 29. The child, 5 from its left
    // edge and 4 from its top, covers the columns 25 to 32 and the rows 14 to 19.
    assert_eq!(extent_of(&before, PARENT), Some((20, 10, 50, 30)));
    assert_eq!(extent_of(&before, CHILD), Some((25, 14, 33, 20)));

    // The parent moved 10 to the right, so the child moved 10 to the right with it.
    assert_eq!(extent_of(&after, PARENT), Some((30, 10, 60, 30)));
    assert_eq!(extent_of(&after, CHILD), Some((35, 14, 43, 20)));
}
