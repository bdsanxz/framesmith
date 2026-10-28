//! `margin` moves an absolutely placed `<box>`, `<text>` or `<composition src>` in from each edge it is placed by, and
//! every child of a plain box is absolutely placed, whatever its `position` (`docs/spec/render-command.md` section 4.1,
//! rule 8; `notation.md` N10, B41). On an axis with no edge the item lies at its static position, the parent's near
//! edge, moved in by its near margin. Each picture is worked out by hand here from the composition, pixel by pixel, and the
//! frames drawn on the processor's renderer, and the fixture's checked-in pictures, are held to it to the byte.

mod common;

use std::fs;

use common::{compare, expected, fixtures, project_from_fixture, project_with};
use draw::{Mark, Picture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const FIXTURE: &str = "placement-margin-and-position";
const CANVAS: [u8; 4] = [0x10, 0x20, 0x40, 255];
const OCHRE: [u8; 4] = [0xe0, 0xa0, 0x30, 255];
const TEAL: [u8; 4] = [0x30, 0xc0, 0xa0, 255];
const VIOLET: [u8; 4] = [0xa0, 0x50, 0xe0, 255];

/// A box as a picture of 96 by 54 shows it: the columns `left` to `right` less one and the rows `top` to `bottom` less
/// one, in `colour`.
type Shown = (u32, u32, u32, u32, [u8; 4]);

/// The picture of a 96 by 54 frame: the canvas, with each box painted over what is there, in order.
fn worked_out(boxes: &[Shown]) -> Picture {
    let mut pixels = Vec::with_capacity(96 * 54 * 4);
    for y in 0..54 {
        for x in 0..96 {
            let mut colour = CANVAS;
            for &(left, right, top, bottom, painted) in boxes {
                if (left..right).contains(&x) && (top..bottom).contains(&y) {
                    colour = painted;
                }
            }
            pixels.extend_from_slice(&colour);
        }
    }
    Picture::new(96, 54, pixels).unwrap()
}

/// The fixture's three boxes as the rule places them. `plain` (left 10, top 10, 20 by 20, margin 8 4, no `position`)
/// lies 4 right and 8 down: columns 14 to 33, rows 18 to 37. `absolute` (left 50, top 10, 20 by 20, margin 30) lies 30
/// further in from its left and top edges, at 80, 40, and the frame cuts it: columns 80 to 95, rows 40 to 53. `static`
/// (left 10, top 40, 20 by 10, margin 1 2 3 4, `position="static"`) lies 4 right and 1 down: columns 14 to 33, rows 41
/// to 50.
const FIXTURE_BOXES: [Shown; 3] = [(14, 34, 18, 38, OCHRE), (80, 96, 40, 54, TEAL), (14, 34, 41, 51, VIOLET)];

fn scene_of(folder: &std::path::Path) -> Scene {
    let project = Project::open(folder).unwrap();
    Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

/// Draws frames 0 and 9 of a composition of 96 by 54 and holds each to the picture worked out for it.
fn drawn_as_worked_out(name: &str, inside: &str, boxes: &[Shown]) {
    let text = format!(
        "<composition version=\"1\" width=\"96\" height=\"54\" fps=\"10\" duration=\"1\" background=\"#102040\">\n{inside}\n</composition>\n"
    );
    let scene = scene_of(&project_with("absolute-margin-pictures", name, &text));
    let want = worked_out(boxes);
    for number in [0, 9] {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
        if let Err(difference) = compare(number, &drawn, &want) {
            panic!("{name}: {difference}");
        }
    }
}

#[test]
fn the_fixture_s_frames_and_checked_in_pictures_show_each_box_moved_in_by_its_margin() {
    let scene = scene_of(&project_from_fixture("absolute-margin-pictures", "fixture", FIXTURE));
    assert_eq!((scene.width(), scene.height(), scene.frames()), (96, 54, 10));
    let want = worked_out(&FIXTURE_BOXES);
    for number in [0, 9] {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
        compare(number, &drawn, &want).unwrap_or_else(|difference| panic!("drawn: {difference}"));
        compare(number, &expected(FIXTURE, number), &want).unwrap_or_else(|difference| panic!("checked in: {difference}"));
    }
}

#[test]
fn a_box_of_a_plain_box_is_moved_by_its_margin_as_an_absolute_one_is() {
    // Each box written with no `position`, with `position="static"` and with `position="absolute"`, at left 6, top 3,
    // 10 by 8, margin 2 3 4 5: each lies 5 right and 2 down, at 11, 5, the columns 11 to 20 and the rows 5 to 12. The
    // three are side by side, 30 apart.
    drawn_as_worked_out(
        "every-position",
        "<box left=\"6\" top=\"3\" width=\"10\" height=\"8\" margin=\"2 3 4 5\" background=\"#e0a030\"/>\n\
         <box left=\"36\" top=\"3\" width=\"10\" height=\"8\" margin=\"2 3 4 5\" position=\"static\" background=\"#a050e0\"/>\n\
         <box left=\"66\" top=\"3\" width=\"10\" height=\"8\" margin=\"2 3 4 5\" position=\"absolute\" background=\"#30c0a0\"/>",
        &[(11, 21, 5, 13, OCHRE), (41, 51, 5, 13, VIOLET), (71, 81, 5, 13, TEAL)],
    );
}

#[test]
fn a_box_with_no_edge_lies_at_its_parent_s_near_edges_moved_in_by_its_near_margins() {
    // No `left`, `top`, `right` or `bottom`: the static position is the parent's left and top edges, and the left and
    // top margins move the box in from them; the right and bottom margins move nothing. In the frame, 20 by 10 with
    // margin 18 7 5 14: columns 14 to 33, rows 18 to 27. Inside a plain box at 40, 30 (50 by 20), 6 by 4 with margin
    // 3 9 9 2: columns 42 to 47, rows 33 to 36.
    drawn_as_worked_out(
        "no-edge",
        "<box width=\"20\" height=\"10\" margin=\"18 7 5 14\" background=\"#e0a030\"/>\n\
         <box left=\"40\" top=\"30\" width=\"50\" height=\"20\"><box width=\"6\" height=\"4\" margin=\"3 9 9 2\" background=\"#30c0a0\"/></box>",
        &[(14, 34, 18, 28, OCHRE), (42, 48, 33, 37, TEAL)],
    );
}

#[test]
fn a_row_and_a_set_of_columns_in_a_plain_box_are_moved_in_by_their_margins() {
    // A row at left 4, top 2, 20 by 10, margin 3 0 0 5, holding one item 6 by 4: the row lies at 9, 5, and its item at
    // its start, 9, 5 (columns 9 to 14, rows 5 to 8). A set of one 12-pixel column at right 4, bottom 2, margin 0 6 3
    // 0, holding one cell 12 by 5: its right edge 10 in, 96 - 10 = 86, columns 74 to 85; its foot 5 up, 54 - 5 = 49,
    // rows 44 to 48.
    drawn_as_worked_out(
        "row-and-set",
        "<box left=\"4\" top=\"2\" width=\"20\" height=\"10\" margin=\"3 0 0 5\" flex-direction=\"row\" align-items=\"flex-start\"><box width=\"6\" height=\"4\" background=\"#e0a030\"/></box>\n\
         <box right=\"4\" bottom=\"2\" margin=\"0 6 3 0\" columns=\"12\"><box height=\"5\" background=\"#30c0a0\"/></box>",
        &[(9, 15, 5, 9, OCHRE), (74, 86, 44, 49, TEAL)],
    );
}

#[test]
fn a_box_placed_by_right_and_bottom_is_moved_in_by_its_right_and_bottom_margins() {
    // right 6 + 4 = 10 from the right edge of 96: columns 74 to 85. bottom 3 + 5 = 8 from the foot of 54: rows 36 to 45.
    drawn_as_worked_out(
        "right-and-bottom",
        "<box right=\"6\" bottom=\"3\" width=\"12\" height=\"10\" margin=\"1 4 5 2\" position=\"absolute\" background=\"#30c0a0\"/>",
        &[(74, 86, 36, 46, TEAL)],
    );
}

#[test]
fn a_box_placed_by_both_edges_with_no_size_is_what_lies_between_them_less_its_margins() {
    // Across 96 - 10 - 20 - 3 - 7 = 56 from 17: columns 17 to 72. Down 54 - 4 - 6 - 2 - 8 = 34 from 6: rows 6 to 39.
    drawn_as_worked_out(
        "both-edges",
        "<box left=\"10\" right=\"20\" top=\"4\" bottom=\"6\" margin=\"2 3 8 7\" position=\"absolute\" background=\"#e0a030\"/>",
        &[(17, 73, 6, 40, OCHRE)],
    );
}

#[test]
fn each_of_four_different_margins_moves_the_box_from_its_own_side() {
    // Two boxes with the margin 3 5 7 9: the first placed by left 10 and top 10 moves 9 right and 3 down, to columns
    // 19 to 30 and rows 13 to 20; the second placed by right 10 and bottom 10 moves 5 left and 7 up: right edge
    // 96 - 15 = 81, columns 69 to 80; foot 54 - 17 = 37, rows 29 to 36.
    drawn_as_worked_out(
        "four-sides",
        "<box left=\"10\" top=\"10\" width=\"12\" height=\"8\" margin=\"3 5 7 9\" position=\"absolute\" background=\"#e0a030\"/>\n\
         <box right=\"10\" bottom=\"10\" width=\"12\" height=\"8\" margin=\"3 5 7 9\" position=\"absolute\" background=\"#a050e0\"/>",
        &[(19, 31, 13, 21, OCHRE), (69, 81, 29, 37, VIOLET)],
    );
}

#[test]
fn an_absolute_composition_src_is_moved_in_by_its_margin() {
    // The file shown is 10 by 6, a teal box over the whole of it. Placed at right 0, bottom 0 with margin 2 4: its right edge 4 in, 96 - 4
    // = 92, columns 82 to 91; its foot 2 up, 54 - 2 = 52, rows 46 to 51.
    let text = "<composition version=\"1\" width=\"96\" height=\"54\" fps=\"10\" duration=\"1\" background=\"#102040\">\n\
                <composition src=\"scenes/patch.xml\" right=\"0\" bottom=\"0\" margin=\"2 4\" position=\"absolute\"/>\n\
                </composition>\n";
    let folder = project_with("absolute-margin-pictures", "composition-src", text);
    fs::create_dir_all(folder.join("scenes")).unwrap();
    fs::write(
        folder.join("scenes/patch.xml"),
        "<composition version=\"1\" width=\"10\" height=\"6\" fps=\"10\" duration=\"1\">\n\
         <box left=\"0\" top=\"0\" width=\"10\" height=\"6\" background=\"#30c0a0\"/>\n\
         </composition>\n",
    )
    .unwrap();
    let scene = scene_of(&folder);
    let want = worked_out(&[(82, 92, 46, 52, TEAL)]);
    for number in [0, 9] {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
        compare(number, &drawn, &want).unwrap();
    }
}

#[test]
fn a_text_of_a_plain_box_is_moved_in_by_its_margin_whatever_its_position() {
    // Three texts in the test font at 10 pixels. The first glyph of each starts on its text's left edge, and its
    // baseline lies one same distance below its text's top edge, so only the edges are worked out here:
    // - absolute at left 10, top 4, with margin 2 3 5 6: left edge 10 + 6 = 16, top edge 4 + 2 = 6;
    // - absolute at left 10, top 4, with no margin: 10, 4;
    // - with no `position`, at left 10, top 30, with margin 2 3 5 6: moved as the first is, to 16, 32 (B41).
    let text = "<composition version=\"1\" width=\"96\" height=\"54\" fps=\"10\" duration=\"1\" background=\"#ffffff\">\n\
                <font src=\"fonts/framesmith-test-stroke-400.ttf\" family=\"Framesmith Test Stroke\"/>\n\
                <text left=\"10\" top=\"4\" margin=\"2 3 5 6\" position=\"absolute\" font-family=\"Framesmith Test Stroke\" font-size=\"10\">HIIH</text>\n\
                <text left=\"10\" top=\"4\" position=\"absolute\" font-family=\"Framesmith Test Stroke\" font-size=\"10\">HIIH</text>\n\
                <text left=\"10\" top=\"30\" margin=\"2 3 5 6\" font-family=\"Framesmith Test Stroke\" font-size=\"10\">HIIH</text>\n\
                </composition>\n";
    let folder = project_with("absolute-margin-pictures", "text", text);
    fs::create_dir_all(folder.join("fonts")).unwrap();
    let font = "framesmith-test-stroke-400.ttf";
    fs::copy(
        fixtures().join("text-default-colour/fonts").join(font),
        folder.join("fonts").join(font),
    )
    .unwrap();
    let scene = scene_of(&folder);
    let frame = scene.frame(0).unwrap();
    let firsts: Vec<(f32, f32)> = frame
        .marks
        .iter()
        .filter_map(|mark| match mark {
            Mark::Text(text) => text.runs.first().and_then(|run| run.glyphs.first()).map(|glyph| (glyph.x, glyph.y)),
            _ => None,
        })
        .collect();
    assert_eq!(firsts.len(), 3);
    let across: Vec<f32> = firsts.iter().map(|first| first.0).collect();
    assert_eq!(across, vec![16.0, 10.0, 16.0]);
    // The baselines are as far apart as the top edges: 6 - 4 = 2 and 32 - 6 = 26.
    assert_eq!(firsts[0].1 - firsts[1].1, 2.0);
    assert_eq!(firsts[2].1 - firsts[0].1, 26.0);
}
