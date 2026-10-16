//! A size of zero draws nothing of its box, and its children are still placed and drawn. A width or a
//! height written below 0 is reported with its line and its box is not drawn. A size that two edges
//! leave below 0 is 0 and is reported as nothing (`docs/spec/render-command.md` sections 3, 4.1 and 5;
//! `notation.md` N6).

mod common;

use common::{composition, count_of, extent_of, project_with};
use draw::Picture;
use draw_cpu::CpuRenderer;
use project::{Project, Report};
use render::Scene;

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];

fn shown(report: &Report) -> String {
    format!(
        "{} {} {} {}",
        report.file(),
        report.problem().line(),
        report.problem().code(),
        report.problem().message()
    )
}

/// The scene of a composition with `inside` in its root, or the reports as `file line code message`.
fn scene_of(name: &str, inside: &str) -> Result<Scene, Vec<String>> {
    let folder = project_with("negative-and-zero-sizes", name, &composition(inside));
    let project = Project::open(&folder).unwrap();
    Scene::from_project(&project).map_err(|reports| reports.iter().map(shown).collect())
}

fn picture_of(name: &str, inside: &str) -> Picture {
    scene_of(name, inside)
        .unwrap_or_else(|reports| panic!("{reports:#?}"))
        .picture(0, &mut CpuRenderer)
        .unwrap()
}

#[test]
fn a_width_below_zero_is_reported_on_its_line_and_the_box_is_not_drawn() {
    assert_eq!(
        scene_of(
            "width-below-zero",
            "<box/>\n<box left=\"10\" top=\"10\" width=\"-5\" height=\"10\" background=\"#ff0000\"/>"
        )
        .map(|_| ()),
        Err(vec![
            "composition.xml 3 FS-0034 `width=\"-5\"` is below 0; a size is 0 or more".to_string()
        ])
    );
}

#[test]
fn a_height_and_a_percentage_below_zero_are_reported_with_their_attributes() {
    assert_eq!(
        scene_of("height-below-zero", "<box height=\"-1\"/>").map(|_| ()),
        Err(vec![
            "composition.xml 2 FS-0034 `height=\"-1\"` is below 0; a size is 0 or more".to_string()
        ])
    );
    assert_eq!(
        scene_of("width-percent-below-zero", "<box width=\"-10%\"/>").map(|_| ()),
        Err(vec![
            "composition.xml 2 FS-0034 `width=\"-10%\"` is below 0; a size is 0 or more".to_string()
        ])
    );
}

#[test]
fn a_size_that_two_edges_leave_below_zero_is_zero_and_is_reported_as_nothing() {
    // The two edges of 60 pixels leave 96 - 120 pixels: a width of 0, and no report.
    let picture = picture_of(
        "two-edges-in-pixels",
        "<box left=\"60\" right=\"60\" top=\"0\" height=\"5\" background=\"#ff0000\"/>",
    );
    assert_eq!(count_of(&picture, RED), 0);
    assert_eq!(count_of(&picture, BLACK), 96 * 54);

    // Two edges of 70% leave a negative width too: 96 - 2 times 67.2 pixels.
    let picture = picture_of(
        "two-edges-in-percent",
        "<box left=\"70%\" right=\"70%\" top=\"0\" height=\"5\" background=\"#ff0000\"/>",
    );
    assert_eq!(count_of(&picture, RED), 0);
    assert_eq!(count_of(&picture, BLACK), 96 * 54);
}

#[test]
fn a_size_of_zero_draws_nothing_of_its_box_and_its_child_is_still_drawn() {
    let inside = "<box id=\"empty\" left=\"10\" top=\"10\" width=\"0\" height=\"10\" background=\"#ff0000\">\n\
                  <box id=\"child\" left=\"5\" top=\"5\" width=\"4\" height=\"4\" background=\"#ffffff\"/>\n\
                  </box>\n\
                  <box top=\"20\" height=\"0%\" background=\"#00ff00\"/>";
    let picture = picture_of("zero-with-a-child", inside);
    assert_eq!(count_of(&picture, RED), 0, "a box of width 0 draws nothing");
    assert_eq!(count_of(&picture, GREEN), 0, "a box of height 0% draws nothing");
    // The child lies 5 from the parent's left edge at 10 and 5 from its top at 10: the columns 15 to
    // 18 and the rows 15 to 18.
    assert_eq!(extent_of(&picture, WHITE), Some((15, 15, 19, 19)));
}
