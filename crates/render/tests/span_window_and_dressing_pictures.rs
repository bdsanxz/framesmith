//! A `<span>` with a window and a span dressed, drawn on the processor (`docs/spec/render-command.md` section 4.2,
//! rule 5; `docs/spec/text.md` sections 6.5 and 6.6). Each chosen frame is held to the picture of a composition that
//! writes, in plain words, what the frame must show. A span not shown on a frame is written in the background's colour,
//! so that it keeps its room and shows nothing. A dressed span is held to a text that has the same dressing on those
//! words alone, at the same place, drawn over a plain text that writes the span's words in the background's colour and the
//! words after them. The fixtures' checked-in pictures are those references' pictures, and the frames drawn equal them
//! to the byte.

mod common;

use common::{compare, expected, numbers_with_a_picture, project_copied_from_fixture, project_with};
use draw::{Mark, Picture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;
use std::fs;
use std::path::Path;

const FONT: &str = "<font src=\"fonts/framesmith-test-stroke-400.ttf\" family=\"Framesmith Test Stroke\"/>";
const LOOK: &str = "font-family=\"Framesmith Test Stroke\" font-size=\"20\" color=\"#202020\"";
const RED: &str = "#c03020";
const BLUE: &str = "#2050c0";
const GREEN: &str = "#208040";
/// The background of every composition here: words written in it take their room and show nothing.
const WHITE: &str = "#ffffff";

fn scene_of(folder: &Path) -> Scene {
    Scene::from_project(&Project::open(folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

/// A folder holding a composition of `width` by `height` at 10 frames a second for one second, on a white ground, with
/// the test font and `inside`.
fn folder_of(name: &str, width: u32, height: u32, inside: &str) -> std::path::PathBuf {
    let text = format!(
        "<composition version=\"1\" width=\"{width}\" height=\"{height}\" fps=\"10\" duration=\"1\" background=\"#ffffff\">\n{FONT}\n{inside}\n</composition>\n"
    );
    let folder = project_with("span-window-and-dressing-pictures", name, &text);
    fs::create_dir_all(folder.join("fonts")).unwrap();
    let font = "framesmith-test-stroke-400.ttf";
    fs::copy(common::fixtures().join("fonts").join(font), folder.join("fonts").join(font)).unwrap();
    folder
}

/// Frame 0 of [`folder_of`].
fn picture(name: &str, width: u32, height: u32, inside: &str) -> Picture {
    scene_of(&folder_of(name, width, height, inside))
        .picture(0, &mut CpuRenderer)
        .unwrap()
}

/// `words` in `colour` when `shown`, and in the background's colour when not.
fn span(shown: bool, colour: &str, words: &str) -> String {
    format!("<span color=\"{}\">{words}</span>", if shown { colour } else { WHITE })
}

/// A text at `top`, at the left of every text here.
fn text_at(top: u32, words: &str) -> String {
    format!("<text left=\"10\" top=\"{top}\" {LOOK}>{words}</text>\n")
}

/// What frame `number` of the window fixture must show. At 30 frames a second a time `t` falls on frame `30 t`.
fn window_reference(number: u64) -> Picture {
    let n = number;
    let mut inside = String::new();
    // First text: CD from 1 s for 2 s, frames 30 to 89; EF has no window, so it shows whenever its text does.
    inside += &text_at(
        4,
        &format!("AB {} {} GH", span((30..90).contains(&n), RED, "CD"), span(true, BLUE, "EF")),
    );
    // Second text, frames 30 to 119: CD from 1 + 0.5 = 1.5 s for 1 s, frames 45 to 74.
    if (30..120).contains(&n) {
        inside += &text_at(30, &format!("AB {} EF", span((45..75).contains(&n), RED, "CD")));
    }
    // Third text, frames 30 to 59: CD from 1 s for 3 s, cut to its text's frames; EF from 1 + 0.5 = 1.5 s, frame 45,
    // cut to CD's, and so to its text's.
    if (30..60).contains(&n) {
        inside += &text_at(56, &format!("AB {}{} GH", span(true, RED, "CD "), span(n >= 45, BLUE, "EF")));
    }
    // Fourth text: 0.33333 s and 0.33334 s are frame 10, 0.34 s is frame 10.2, so frame 11. Each shows for one second;
    // its last frame is not among the chosen ones after frame 30, where all three are shown, and before frame 59, where none
    // is.
    let (ab, ef) = ((10..40).contains(&n), (11..40).contains(&n));
    inside += &text_at(
        82,
        &format!("{} {} {} GH", span(ab, RED, "AB"), span(ab, BLUE, "CD"), span(ef, GREEN, "EF")),
    );
    picture(&format!("window-{number}"), 160, 110, &inside)
}

/// What frame 0 of the dressing fixture must show.
fn dressing_reference() -> Picture {
    // The dressed span stands at the start of its line: a text with only its words, dressed, is drawn where it is. The
    // words after it are written first by a plain text at the same place, the span's words in the background's colour,
    // which the dressed text then covers. Its shadows and outline end before the words after it, (540 + 540 + 300) x 0.02
    // = 27.6 pixels from the left: 21.6 + 5 for the farther shadow, 21.6 + 2 for the outline.
    let dressed = |top: u32, dressing: &str| {
        format!(
            "{}<text left=\"10\" top=\"{top}\" {LOOK} {dressing}>HI</text>\n",
            text_at(top, &format!("<span color=\"{WHITE}\">HI</span> HI"))
        )
    };
    let mut inside = String::new();
    inside += &dressed(4, "text-shadow=\"3px 2px #c03020\"");
    inside += &dressed(30, "text-shadow=\"3px 2px #c03020, 5px 4px #2050c0\"");
    inside += &dressed(56, "text-stroke=\"2px #c03020\"");
    // Figures change only the figures, so the whole text written with them is the same picture.
    inside += &format!("<text left=\"10\" top=\"82\" {LOOK} font-variant-numeric=\"tabular-nums\">A111 B</text>\n");
    inside += &format!("<text left=\"10\" top=\"108\" {LOOK} font-variant-numeric=\"oldstyle-nums\">A395 B</text>\n");
    inside += &text_at(134, "low LOW low");
    // Line height "normal" at size 20 is (850 + 250) / 1000 x 20 = 22 pixels, so the second line stands 22 pixels down.
    inside += &text_at(160, "AB");
    inside += &text_at(182, "CD EF");
    picture("dressing", 160, 220, &inside)
}

/// Holds the chosen frames of `fixture` to what they must show, and to the fixture's checked-in pictures, to the byte.
fn frames_show_what_is_worked_out(fixture: &str, chosen: &[u64], reference: impl Fn(u64) -> Picture) {
    let scene = scene_of(&project_copied_from_fixture("span-window-and-dressing-pictures", fixture, fixture));
    assert_eq!(
        numbers_with_a_picture(fixture),
        chosen,
        "{fixture}: the chosen frames have a picture"
    );
    for &number in chosen {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
        let want = reference(number);
        compare(number, &drawn, &want).unwrap_or_else(|difference| panic!("{fixture}, drawn: {difference}"));
        compare(number, &expected(fixture, number), &want).unwrap_or_else(|difference| panic!("{fixture}, checked in: {difference}"));
    }
}

#[test]
fn a_span_shows_in_its_window_counted_from_its_clip_cut_to_its_text_and_keeps_its_room_outside_it() {
    frames_show_what_is_worked_out(
        "text-span-window",
        &[0, 9, 10, 11, 29, 30, 44, 45, 59, 60, 74, 75, 89, 90],
        window_reference,
    );
}

#[test]
fn a_span_s_shadows_outline_figures_upper_case_and_no_wrapping_reach_its_own_words_only() {
    frames_show_what_is_worked_out("text-span-dressing", &[0], |_| dressing_reference());
}

/// The x of each glyph frame 0 of `folder` draws, in drawing order.
fn glyph_xs(folder: &Path) -> Vec<f32> {
    marks_of(folder)
        .iter()
        .flat_map(|mark| match mark {
            Mark::Text(text) => text.runs.iter().flat_map(|run| run.glyphs.iter().map(|glyph| glyph.x)).collect(),
            _ => Vec::new(),
        })
        .collect()
}

fn marks_of(folder: &Path) -> Vec<Mark> {
    scene_of(folder).frame(0).unwrap().marks
}

#[test]
fn tabular_figures_in_a_span_are_equally_wide_and_move_the_words_after_it() {
    // "A" is 740 units, a space 300; at size 20 a unit is 0.02 pixel. Tabular figures are each 560 units; lining "1" is 440
    // and lining "8" is 560. B, the last glyph, is at 10 + 0.02 (740 + 3 w + 300), w the figure's width.
    let b_of = |name: &str, form: &str, figures: &str| {
        let folder = folder_of(
            name,
            160,
            40,
            &format!("<text left=\"10\" top=\"4\" {LOOK}>A<span font-variant-numeric=\"{form}\">{figures}</span> B</text>"),
        );
        *glyph_xs(&folder).last().unwrap()
    };
    let near = |got: f32, want: f32| assert!((got - want).abs() < 1e-3, "{got} against {want}");
    near(
        b_of("tabular-111", "tabular-nums", "111"),
        10.0 + 0.02 * (740.0 + 3.0 * 560.0 + 300.0),
    );
    near(
        b_of("tabular-888", "tabular-nums", "888"),
        10.0 + 0.02 * (740.0 + 3.0 * 560.0 + 300.0),
    );
    near(
        b_of("proportional-111", "proportional-nums", "111"),
        10.0 + 0.02 * (740.0 + 3.0 * 440.0 + 300.0),
    );
    near(
        b_of("proportional-888", "proportional-nums", "888"),
        10.0 + 0.02 * (740.0 + 3.0 * 560.0 + 300.0),
    );
}

#[test]
fn a_span_inside_a_dressed_span_takes_its_dressing_and_one_that_writes_its_own_changes_it_for_itself() {
    // The outer span is outlined 2 pixels wide in red, CD takes it, and EF writes its own, 1 pixel wide in blue.
    let folder = folder_of(
        "nested",
        200,
        40,
        &format!(
            "<text left=\"10\" top=\"4\" {LOOK}>XY <span text-stroke=\"2px #c03020\" text-shadow=\"1px 1px #208040\">AB <span color=\"{BLUE}\">CD</span> <span text-stroke=\"1px #2050c0\">EF</span></span> GH</text>"
        ),
    );
    // The glyph of a space, the second glyph of "A B": spaces draw nothing, and are left out of the count below.
    let space = marks_of(&folder_of("space", 100, 40, &text_at(4, "A B")))
        .iter()
        .find_map(|mark| match mark {
            Mark::Text(text) => Some(text.runs[0].glyphs[1].id),
            _ => None,
        })
        .unwrap();
    // Each group of words with one dressing is one mark: its glyphs, with its outline width and its shadows.
    let groups: Vec<(usize, Option<f32>, usize)> = marks_of(&folder)
        .iter()
        .filter_map(|mark| match mark {
            Mark::Text(text) => Some((
                text.runs
                    .iter()
                    .map(|run| run.glyphs.iter().filter(|glyph| glyph.id != space).count())
                    .sum::<usize>(),
                text.outline.as_ref().map(|outline| outline.width),
                text.shadows.len(),
            )),
            _ => None,
        })
        .collect();
    // The groups are kept in the order each dressing first appears: the plain words, XY and GH; AB and CD, outlined 2 wide
    // with the shadow; EF, 1 wide with the shadow.
    let glyphs: Vec<usize> = groups.iter().map(|group| group.0).collect();
    let dressing: Vec<(Option<f32>, usize)> = groups.iter().map(|group| (group.1, group.2)).collect();
    assert_eq!(dressing, vec![(None, 0), (Some(2.0), 1), (Some(1.0), 1)], "{groups:?}");
    assert_eq!(glyphs, vec![4, 4, 2], "{groups:?}");
}

#[test]
fn a_no_wrap_span_wider_than_its_line_starts_the_next_line_and_runs_over() {
    // At 40 pixels, "AB" (27.3) fits and "CD EF GH" (0.02 x (598 + 680 + 300 + 590 + 590 + 300 + 660 + 540) = 85.16)
    // does not; no line ends inside the span, so it is the second line, whole.
    let drawn = picture(
        "nowrap-wide",
        160,
        60,
        &format!("<text left=\"10\" top=\"4\" width=\"40\" {LOOK}>AB <span white-space=\"nowrap\">CD EF GH</span></text>"),
    );
    let want = picture(
        "nowrap-wide-reference",
        160,
        60,
        &format!("{}{}", text_at(4, "AB"), text_at(26, "CD EF GH")),
    );
    compare(0, &drawn, &want).unwrap();
}

#[test]
fn upper_case_on_a_span_leaves_the_words_the_composition_holds_as_written() {
    let folder = folder_of(
        "upper-case-source",
        160,
        40,
        &text_at(4, "low <span text-transform=\"uppercase\">low</span> low"),
    );
    let project = Project::open(&folder).unwrap();
    scene_of(&folder);
    let span = project
        .tree()
        .nodes()
        .iter()
        .find(|node| node.attribute("text-transform").is_some())
        .unwrap();
    assert_eq!(format!("{:?}", span.parts()), "[Words(\"low\")]");
}

#[test]
fn a_span_inside_a_span_is_cut_to_the_window_of_the_span_around_it() {
    // At 10 frames a second for 3 seconds, CD shows from 1 s for 1 s, frames 10 to 19; EF inside it starts with it and
    // would run 2 seconds, to frame 29, but is cut to CD's frames. The text itself has no window.
    let text = format!(
        "<composition version=\"1\" width=\"160\" height=\"40\" fps=\"10\" duration=\"3\" background=\"#ffffff\">\n{FONT}\n{}</composition>\n",
        text_at(
            4,
            &format!("AB <span start=\"1\" duration=\"1\" color=\"{RED}\">CD <span start=\"0\" duration=\"2\" color=\"{BLUE}\">EF</span></span> GH")
        )
    );
    let folder = project_with("span-window-and-dressing-pictures", "nested-window", &text);
    fs::create_dir_all(folder.join("fonts")).unwrap();
    let font = "framesmith-test-stroke-400.ttf";
    fs::copy(common::fixtures().join("fonts").join(font), folder.join("fonts").join(font)).unwrap();
    let scene = scene_of(&folder);
    for (number, shown) in [(9, false), (10, true), (19, true), (20, false), (25, false)] {
        let want = picture(
            &format!("nested-window-{number}"),
            160,
            40,
            &text_at(4, &format!("AB {}{} GH", span(shown, RED, "CD "), span(shown, BLUE, "EF"))),
        );
        compare(number, &scene.picture(number, &mut CpuRenderer).unwrap(), &want).unwrap();
    }
}
