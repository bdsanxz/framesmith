//! An item of a row with a `flex-grow` above 0 takes a share of the spare room along the row, in proportion to the
//! shares written; an item with no `flex-grow` takes none (`docs/spec/render-command.md` section 4.3, rule 12). Each
//! test reads the rectangles of the filled boxes of a frame from its display list.

mod common;

use common::project_with;
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

fn scene_of(name: &str, composition: &str) -> Scene {
    let folder = project_with("spare-room-shared-by-flex-grow", name, composition);
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

#[test]
fn the_spare_room_is_shared_in_proportion_to_the_shares_written() {
    // The row is 150 wide and its boxes are 20 each: 90 is spare. The first takes 1 part of 3 and the third 2 parts
    // of 3, so 30 and 60 are added: 50 and 80 wide. The second has no share and stays 20 wide.
    let scene = scene_of(
        "shares-one-and-two",
        "<composition version=\"1\" width=\"150\" height=\"20\" fps=\"10\" duration=\"1\">\n\
         <box id=\"row\" left=\"0\" top=\"0\" width=\"150\" height=\"20\" flex-direction=\"row\">\n\
         <box width=\"20\" height=\"20\" flex-grow=\"1\" background=\"#ff8060\"/>\n\
         <box width=\"20\" height=\"20\" background=\"#80ff80\"/>\n\
         <box width=\"20\" height=\"20\" flex-grow=\"2\" background=\"#ffd080\"/>\n\
         </box>\n\
         </composition>\n",
    );
    assert_eq!(filled_in_frame(&scene, 0), vec![(0, 0, 50, 20), (50, 0, 20, 20), (70, 0, 80, 20)]);
}

#[test]
fn shares_that_are_fractions_are_in_proportion_too() {
    // The row is 120 wide and its boxes are 20 each: 80 is spare. The shares 0.5 and 1.5 are 1 part of 4 and 3
    // parts of 4, so 20 and 60 are added: 40 and 80 wide.
    let scene = scene_of(
        "shares-fractions",
        "<composition version=\"1\" width=\"120\" height=\"20\" fps=\"10\" duration=\"1\">\n\
         <box id=\"row\" left=\"0\" top=\"0\" width=\"120\" height=\"20\" flex-direction=\"row\">\n\
         <box width=\"20\" height=\"20\" flex-grow=\"0.5\" background=\"#ff8060\"/>\n\
         <box width=\"20\" height=\"20\" flex-grow=\"1.5\" background=\"#ffd080\"/>\n\
         </box>\n\
         </composition>\n",
    );
    assert_eq!(filled_in_frame(&scene, 0), vec![(0, 0, 40, 20), (40, 0, 80, 20)]);
}
