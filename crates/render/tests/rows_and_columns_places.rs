//! The boxes of a row or a column are placed where the spec says: in document order, with the gaps, along
//! and across the direction, with padding, with no size of their own, and an absolutely placed box leaves
//! the row. Each test reads the rectangles of the filled boxes of a frame from its display list
//! (`docs/spec/render-command.md` section 4.3, rules 1 to 11).

mod common;

use common::{composition, project_from_fixture, project_with};
use draw::Step;
use project::Project;
use render::Scene;

const GROUP: &str = "rows-and-columns-places";

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

/// The scene of a fixture, in a project folder of its own.
fn scene_of_fixture(name: &str) -> Scene {
    let folder = project_from_fixture(GROUP, name, name);
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

/// The scene of a composition with `inside` in its root, in a project folder of its own.
fn scene_with(name: &str, inside: &str) -> Scene {
    let folder = project_with(GROUP, name, &composition(inside));
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

#[test]
fn the_boxes_of_a_row_lie_left_to_right_in_document_order_with_the_gap_between_them() {
    let scene = scene_of_fixture("rows-with-gaps");
    // The row at 4 and 88 wide, with a gap of 20: 4 to 16, 36 to 52 and 72 to 84.
    assert_eq!(filled_in_frame(&scene, 0), vec![(4, 4, 12, 20), (36, 4, 16, 20), (72, 4, 12, 20)]);
}

#[test]
fn the_boxes_of_a_column_lie_top_to_bottom_in_document_order_with_the_gap_between_them() {
    let scene = scene_of_fixture("columns-with-gaps");
    // The column is at 10, 2 and 40 wide, with a gap of 6: its boxes are in the rows 2 to 9, 16 to 27 and
    // 34 to 41.
    assert_eq!(filled_in_frame(&scene, 0), vec![(10, 2, 20, 8), (10, 16, 20, 12), (10, 34, 20, 8)]);
}

#[test]
fn a_centred_group_is_in_the_middle_and_an_end_group_is_at_the_end_of_its_row() {
    let scene = scene_of_fixture("row-along-center-and-end");
    // The centred group is 24 wide in 96 pixels, so it starts 36 in; the end group starts 72 in.
    assert_eq!(
        filled_in_frame(&scene, 0),
        vec![(36, 0, 10, 10), (50, 0, 10, 10), (72, 20, 10, 10), (86, 20, 10, 10)]
    );
}

#[test]
fn across_a_row_each_box_is_at_the_start_the_centre_or_the_end_or_is_stretched() {
    let scene = scene_of_fixture("row-align-across");
    // The row is 50 tall at 2. Its boxes are 16 tall at the start (2), the centre (19) and the end (36);
    // the fourth, with no height, is stretched to the full 50.
    assert_eq!(
        filled_in_frame(&scene, 0),
        vec![(4, 2, 16, 16), (28, 19, 16, 16), (52, 36, 16, 16), (76, 2, 16, 50)]
    );
}

#[test]
fn across_a_column_each_box_is_centred_across_the_width_of_the_column() {
    let scene = scene_of_fixture("column-align-across");
    // The column is 60 wide at 2: the boxes of 10, 20 and 30 wide are centred, at 27, 22 and 17.
    assert_eq!(filled_in_frame(&scene, 0), vec![(27, 2, 10, 8), (22, 14, 20, 8), (17, 26, 30, 8)]);
}

#[test]
fn padding_moves_the_boxes_in_from_the_edges_of_the_row() {
    let scene = scene_of_fixture("padding-inside-a-row");
    // The row is at 8, 6 and 80 by 40, with padding 4 on top and bottom and 8 on each side. Its boxes
    // lie in the content box, from 16 and 10: the first at 16, the second 12 + 4 further on, at 32.
    assert_eq!(filled_in_frame(&scene, 0), vec![(8, 6, 80, 40), (16, 10, 12, 10), (32, 10, 12, 10)]);
}

#[test]
fn a_row_with_no_size_of_its_own_is_as_large_as_its_boxes_and_the_gap() {
    let scene = scene_of_fixture("row-with-no-size");
    // Two boxes of 12 with a gap of 6 make 30 by 8, the size of the row.
    assert_eq!(filled_in_frame(&scene, 0), vec![(10, 30, 30, 8), (10, 30, 12, 8), (28, 30, 12, 8)]);
}

#[test]
fn an_absolute_box_leaves_the_row_and_the_others_close_up() {
    let scene = scene_of_fixture("absolute-child-in-a-row");
    // The absolute box, 6 by 6 at the row's own corner, takes no room: the second child is at 24, where it
    // would be at 30 if the box took room.
    assert_eq!(filled_in_frame(&scene, 0), vec![(4, 4, 20, 20), (4, 4, 6, 6), (24, 4, 20, 20)]);
}

#[test]
fn a_gap_of_zero_leaves_the_boxes_touching() {
    let scene = scene_of_fixture("gap-of-zero");
    assert_eq!(filled_in_frame(&scene, 0), vec![(4, 4, 10, 10), (14, 4, 10, 10), (24, 4, 10, 10)]);
}

#[test]
fn a_row_inside_a_box_is_placed_in_that_box_as_a_box_is() {
    // The row has no size of its own, so it is as large as its two boxes and their gap, 24 by 10, and it
    // sits at the parent's edge, 10 in from the left and top of the root.
    let scene = scene_with(
        "row-inside-a-box",
        "<box id=\"parent\" left=\"10\" top=\"10\" width=\"50\" height=\"30\" background=\"#304080\">\n\
         <box flex-direction=\"row\" gap=\"4\">\n\
         <box width=\"10\" height=\"10\" background=\"#ff8060\"/>\n\
         <box width=\"10\" height=\"10\" background=\"#80ff80\"/>\n\
         </box>\n\
         </box>",
    );
    assert_eq!(
        filled_in_frame(&scene, 0),
        vec![(10, 10, 50, 30), (10, 10, 10, 10), (24, 10, 10, 10)]
    );
}

#[test]
fn a_clip_child_keeps_its_room_in_the_row_in_every_frame() {
    // The clip is shown from frame 5 (start 0.5 at 10 frames a second) and not before. It takes the room
    // between its neighbours all the same, so the third box is at 44 in frame 0 as in frame 5.
    let scene = scene_with(
        "clip-keeps-room",
        "<box id=\"row\" left=\"4\" top=\"4\" width=\"88\" height=\"20\" flex-direction=\"row\" gap=\"10\">\n\
         <box width=\"10\" height=\"10\" background=\"#ff8060\"/>\n\
         <box width=\"10\" height=\"10\" start=\"0.5\" duration=\"0.5\" background=\"#80ff80\"/>\n\
         <box width=\"10\" height=\"10\" background=\"#ffd080\"/>\n\
         </box>",
    );
    assert_eq!(filled_in_frame(&scene, 0), vec![(4, 4, 10, 10), (44, 4, 10, 10)]);
    assert_eq!(filled_in_frame(&scene, 5), vec![(4, 4, 10, 10), (24, 4, 10, 10), (44, 4, 10, 10)]);
}

#[test]
fn a_row_with_no_size_inside_a_row_with_no_size_is_as_large_as_its_boxes_and_the_next_box_follows_it() {
    let scene = scene_of_fixture("row-inside-a-row-with-no-size");
    // The inner row is 22 wide, its boxes and their gap, so the box after it lies at 4 + 22 + 4 = 30.
    assert_eq!(
        filled_in_frame(&scene, 0),
        vec![(4, 4, 36, 10), (4, 4, 22, 10), (4, 4, 10, 10), (16, 4, 10, 10), (30, 4, 10, 10)]
    );
}

#[test]
fn a_child_that_stretches_fills_the_size_across_that_its_row_or_column_has() {
    let scene = scene_of_fixture("stretch-across-with-no-size");
    // The row is as high as its 20 pixel child, so its second child stretches to 20; the column is as wide as
    // its 20 pixel child, so its second child stretches to 20.
    assert_eq!(
        filled_in_frame(&scene, 0),
        vec![
            (4, 4, 24, 20),
            (4, 4, 10, 20),
            (18, 4, 10, 20),
            (50, 4, 20, 24),
            (50, 4, 20, 10),
            (50, 18, 20, 10)
        ]
    );
}

#[test]
fn a_row_inside_a_column_inside_a_row_with_no_size_is_as_large_as_its_boxes_to_any_depth() {
    // The outer row is 36 wide, its column is 22 wide and 22 high, the column's row is 22 by 10. The column
    // and its row are placed at their own start, so each is as large as what is in it.
    let scene = scene_with(
        "row-in-a-column-in-a-row",
        "<box id=\"outer\" left=\"4\" top=\"4\" flex-direction=\"row\" gap=\"4\" background=\"#305050\">\n\
         <box id=\"column\" flex-direction=\"column\" gap=\"2\" align-self=\"flex-start\" background=\"#405060\">\n\
         <box id=\"row\" flex-direction=\"row\" gap=\"2\" align-self=\"flex-start\" background=\"#ffffff\">\n\
         <box width=\"10\" height=\"10\" background=\"#ff8060\"/>\n\
         <box width=\"10\" height=\"10\" background=\"#80ff80\"/>\n\
         </box>\n\
         <box width=\"10\" height=\"10\" background=\"#ffd080\"/>\n\
         </box>\n\
         <box width=\"10\" height=\"10\" background=\"#8080ff\"/>\n\
         </box>",
    );
    assert_eq!(
        filled_in_frame(&scene, 0),
        vec![
            (4, 4, 36, 22),
            (4, 4, 22, 22),
            (4, 4, 22, 10),
            (4, 4, 10, 10),
            (16, 4, 10, 10),
            (4, 16, 10, 10),
            (30, 4, 10, 10)
        ]
    );
}

#[test]
fn a_percentage_width_in_a_row_with_no_size_is_0_wide_inside_it() {
    // The row has no size of its own, so a child's share of its width is 0: the row is 14 wide, the gap of 4
    // and the 10 pixel box, and the second child lies at 10 + 0 + 4. The row has no background, so it draws no
    // box of its own: the 0-wide child is listed at the row's start.
    let scene = scene_with(
        "percentage-in-a-row-with-no-size",
        "<box id=\"parent\" left=\"10\" top=\"10\" width=\"50\" height=\"30\" background=\"#304080\">\n\
         <box flex-direction=\"row\" gap=\"4\">\n\
         <box width=\"50%\" height=\"10\" background=\"#ff8060\"/>\n\
         <box width=\"10\" height=\"10\" background=\"#80ff80\"/>\n\
         </box>\n\
         </box>",
    );
    assert_eq!(
        filled_in_frame(&scene, 0),
        vec![(10, 10, 50, 30), (10, 10, 0, 10), (14, 10, 10, 10)]
    );
}

/// A row with no size, padded by 2, holds a row with no size across that stretches: it has the size its box
/// gives it plus its padding, 16 by 16, so the outer row is 20 by 20 at 4,4 and the inner row 16 by 16 at 6,6.
/// The box is 10 by 10 at 9,9. The column with no width beside it is the same at 50,4 (`render-command.md`
/// section 4.3, rules 1 and 7).
#[test]
fn a_stretched_row_and_column_with_no_size_across_are_as_large_as_their_items_give_them() {
    let scene = scene_of_fixture("stretched-nested-with-no-size");
    assert_eq!(
        filled_in_frame(&scene, 0),
        vec![
            (4, 4, 20, 20),
            (6, 6, 16, 16),
            (9, 9, 10, 10),
            (50, 4, 20, 20),
            (52, 6, 16, 16),
            (55, 9, 10, 10),
        ]
    );
}
