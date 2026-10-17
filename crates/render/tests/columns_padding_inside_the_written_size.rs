//! A written `width` and `height` of a set of columns include its `padding`: the box is as big as it is written,
//! and its columns and rows are placed inside the padding, as a row's are (`docs/spec/render-command.md` section 4.4,
//! rule 2; `DECISIONS.md` B17). The box has a background, so its own rectangle is read from the display list.

mod common;

use common::{composition, project_with};
use draw::Step;
use project::Project;
use render::Scene;

/// The rectangles, as (left, top, width, height), of the filled boxes of one frame, in drawing order.
fn filled_in_frame(scene: &Scene, number: u64) -> Vec<(i32, i32, u32, u32)> {
    scene
        .list(number)
        .unwrap_or_else(|error| panic!("frame {number}: {error:?}"))
        .steps
        .iter()
        .map(|step| match step {
            Step::Fill { rect, .. } => (rect.left, rect.top, rect.width, rect.height),
        })
        .collect()
}

#[test]
fn a_written_width_and_height_of_a_set_of_columns_include_its_padding() {
    // The set is 80 by 24 with a padding of 8 on every side, so its content is 64 by 8: two columns of 32, and two
    // rows of 4 that fill the 8.
    let inside =
        "<box id=\"grid\" left=\"0\" top=\"0\" width=\"80\" height=\"24\" padding=\"8\" columns=\"1fr 1fr\" background=\"#202020\">\n\
                  <box height=\"4\" background=\"#ff8060\"/>\n\
                  <box height=\"4\" background=\"#80ff80\"/>\n\
                  <box height=\"4\" background=\"#ffd080\"/>\n\
                  <box height=\"4\" background=\"#80c0ff\"/>\n\
                  </box>";
    let folder = project_with("columns-padding-inside-the-written-size", "set-with-padding", &composition(inside));
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    let scene = Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"));
    assert_eq!(
        filled_in_frame(&scene, 0),
        vec![(0, 0, 80, 24), (8, 8, 32, 4), (40, 8, 32, 4), (8, 12, 32, 4), (40, 12, 32, 4)]
    );
}
