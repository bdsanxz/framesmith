//! Children of a set of columns with a stated first width and a share of the rest lie two to a row: the first column
//! is the stated width, the second fills the rest, and each row is as tall as its items. Each test reads the
//! rectangles of the filled boxes of a frame from its display list (`docs/spec/render-command.md` section 4.4,
//! rules 1, 3 and 4).

mod common;

use common::project_from_fixture;
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
fn four_children_lie_two_to_a_row_in_a_stated_column_and_a_column_that_fills_the_rest() {
    let folder = project_from_fixture(
        "columns-stated-and-shares-places",
        "columns-stated-and-shares",
        "columns-stated-and-shares",
    );
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    let scene = Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    // The box is 120 wide: a column of 40, then the rest, 80. Its four children are 10 tall, in two rows of 10.
    assert_eq!(
        filled_in_frame(&scene, 0),
        vec![(0, 0, 40, 10), (40, 0, 80, 10), (0, 10, 40, 10), (40, 10, 80, 10)]
    );
}

#[test]
fn the_columns_are_the_same_in_every_frame() {
    let folder = project_from_fixture(
        "columns-stated-and-shares-places",
        "columns-every-frame",
        "columns-stated-and-shares",
    );
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    let scene = Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let first = filled_in_frame(&scene, 0);
    for number in 1..scene.frames() {
        assert_eq!(filled_in_frame(&scene, number), first, "frame {number}");
    }
}
