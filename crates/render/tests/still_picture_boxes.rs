//! The box of an `<image>` (`docs/spec/still-pictures.md`, section 4): with no size it is the picture's own size; with one
//! size known the other follows the picture's proportions; with both it is as written. Worked by hand for a picture of 40
//! by 20 in a frame of 200 by 100.

mod common;

use common::project_with;
use draw::Mark;
use layout::Rect;
use project::Project;
use render::Scene;

fn png_of(width: u32, height: u32) -> Vec<u8> {
    common::png::encode(width, height, &vec![200; (width * height * 4) as usize])
}

/// The rectangles of the pictures of frame 0 of a composition of 200 by 100 holding `images`, which name `p.png`, a picture
/// of 40 by 20.
fn boxes(name: &str, images: &str) -> Vec<Rect> {
    boxes_in_a_frame_of(200, name, images)
}

/// As `boxes`, in a frame `width` pixels wide.
fn boxes_in_a_frame_of(width: u32, name: &str, images: &str) -> Vec<Rect> {
    let folder = project_with(
        "still-picture-boxes",
        name,
        &format!(
            "<composition version=\"1\" width=\"{width}\" height=\"100\" fps=\"10\" duration=\"0.2\" background=\"#000000\">\n{images}\n</composition>\n"
        ),
    );
    std::fs::write(folder.join("p.png"), png_of(40, 20)).unwrap();
    let scene =
        Scene::from_project(&Project::open(&folder).unwrap(), &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    scene
        .frame(0)
        .unwrap()
        .marks
        .iter()
        .map(|mark| match mark {
            Mark::Still(still) => still.rect,
            other => panic!("{other:?}"),
        })
        .collect()
}

fn rect(left: i32, top: i32, width: u32, height: u32) -> Rect {
    Rect { left, top, width, height }
}

#[test]
fn an_image_with_no_size_is_its_pictures_own_size() {
    assert_eq!(
        boxes("none", "<image src=\"p.png\" left=\"5\" top=\"7\"/>"),
        vec![rect(5, 7, 40, 20)]
    );
    // Placed from the bottom right, its own size counts from there: 200 - 10 - 40 and 100 - 10 - 20.
    assert_eq!(
        boxes("from-the-end", "<image src=\"p.png\" right=\"10\" bottom=\"10\"/>"),
        vec![rect(150, 70, 40, 20)]
    );
}

#[test]
fn with_one_size_known_the_other_follows_the_pictures_proportions() {
    let found = boxes(
        "one",
        "<image src=\"p.png\" left=\"0\" top=\"0\" width=\"60\"/>\n\
         <image src=\"p.png\" left=\"0\" top=\"0\" height=\"30\"/>\n\
         <image src=\"p.png\" left=\"0\" top=\"0\" width=\"50%\"/>\n\
         <image src=\"p.png\" left=\"10\" right=\"30\" top=\"0\"/>\n\
         <image src=\"p.png\" left=\"0\" top=\"20\" bottom=\"20\"/>",
    );
    assert_eq!(
        found,
        vec![
            // 60 across: 60 * 20 / 40 = 30 down.
            rect(0, 0, 60, 30),
            // 30 down: 30 * 40 / 20 = 60 across.
            rect(0, 0, 60, 30),
            // 50% of 200 is 100 across: 50 down.
            rect(0, 0, 100, 50),
            // 200 - 10 - 30 = 160 across, from its edges: 80 down.
            rect(10, 0, 160, 80),
            // 100 - 20 - 20 = 60 down, from its edges: 120 across.
            rect(0, 20, 120, 60),
        ]
    );
}

#[test]
fn a_proportion_that_lands_on_a_half_rounds_up() {
    // The picture is 40 by 20: a width of 61 gives 61 * 20 / 40 = 30.5, which rounds up to 31 as a written size is
    // rounded (still-pictures.md, section 2; render-command.md, section 4.1, rule 3).
    assert_eq!(
        boxes("half-proportion", "<image src=\"p.png\" left=\"0\" top=\"0\" width=\"61\"/>"),
        vec![rect(0, 0, 61, 31)]
    );
}

#[test]
fn with_both_sizes_the_box_is_as_written() {
    assert_eq!(
        boxes("both", "<image src=\"p.png\" left=\"3\" top=\"4\" width=\"17\" height=\"90\"/>"),
        vec![rect(3, 4, 17, 90)]
    );
}

#[test]
fn an_edge_exactly_half_a_pixel_out_rounds_up_when_it_comes_from_a_percentage() {
    // A frame of 2750 pixels: 1.4% of it is 38.5 exactly, and the spec rounds a half up (render-command.md, section 4.1,
    // rule 3). Its float form falls a hair under 38.5, so the edge is held to the exact value here.
    // The left edge at 38.5 rounds to 39; the box of 10 from there ends at 48.5, which rounds to 49, so its width is 10.
    assert_eq!(
        boxes_in_a_frame_of(
            2750,
            "left-edge-on-a-half",
            "<image src=\"p.png\" left=\"1.4%\" top=\"0\" width=\"10\" height=\"10\"/>"
        ),
        vec![rect(39, 0, 10, 10)]
    );
    // The right edge at 38.5 rounds to 39 too, so the box from 0 is 39 wide.
    assert_eq!(
        boxes_in_a_frame_of(
            2750,
            "right-edge-on-a-half",
            "<image src=\"p.png\" left=\"0\" top=\"0\" width=\"1.4%\" height=\"10\"/>"
        ),
        vec![rect(0, 0, 39, 10)]
    );
}
