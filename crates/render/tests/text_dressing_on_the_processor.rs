//! The processor draws a text with its upper-case transform in capitals, with its shadows at their offsets and colours under
//! the glyphs (the first listed on top, blurred when a blur is given), with its outline around each glyph over the fill, and
//! with its wrapping turned off on one line whatever its width (`docs/spec/text.md`, section 6.6).

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{fixtures, project_copied_from_fixture};
use draw::{Mark, Picture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const FAMILY: &str = "font-family=\"Framesmith Test Stroke\"";
const WHITE: [u8; 4] = [255, 255, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];
const BLUE: [u8; 4] = [0x20, 0x50, 0xc0, 255];
const RED: [u8; 4] = [0xc0, 0x20, 0x20, 255];

/// A project of a 200 by 90 pixel white canvas whose root holds `body`, with the test font declared and its file copied.
fn text_project(name: &str, body: &str) -> PathBuf {
    let folder = common::scratch("text-dressing-on-the-processor", name);
    fs::create_dir_all(folder.join("fonts")).unwrap();
    let file = "framesmith-test-stroke-400.ttf";
    fs::copy(fixtures().join("fonts").join(file), folder.join("fonts").join(file)).unwrap();
    let composition = format!(
        "<composition version=\"1\" width=\"200\" height=\"90\" fps=\"10\" duration=\"1\" background=\"#ffffff\">\n\
         <font src=\"fonts/{file}\" family=\"Framesmith Test Stroke\"/>\n\
         {body}\n</composition>\n"
    );
    fs::write(folder.join("composition.xml"), composition).unwrap();
    folder
}

/// A text at 10 by 8 pixels, 30 pixels high, with the attributes given after its family.
fn text_body(attributes: &str, words: &str) -> String {
    format!("<text left=\"10\" top=\"8\" {FAMILY} font-size=\"30\" {attributes}>{words}</text>")
}

/// The picture the processor draws for frame 0 of a project.
fn picture_of(folder: &Path) -> Picture {
    let scene = Scene::from_project(&Project::open(folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"));
    scene.picture(0, &mut CpuRenderer).unwrap()
}

/// The number of distinct heights of the glyphs of the texts of frame 0: one for a text on one line.
fn lines_of(folder: &Path) -> usize {
    let scene = Scene::from_project(&Project::open(folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let mut heights: Vec<i64> = scene
        .frame(0)
        .unwrap()
        .marks
        .iter()
        .flat_map(|mark| match mark {
            Mark::Text(text) => text
                .runs
                .iter()
                .flat_map(|run| run.glyphs.iter().map(|glyph| (glyph.y * 1000.0) as i64))
                .collect(),
            Mark::Step(_) | Mark::Group(_) | Mark::Background(_) | Mark::Ring(_) | Mark::Cut(_) => Vec::new(),
        })
        .collect();
    heights.sort_unstable();
    heights.dedup();
    heights.len()
}

/// What a pixel of a picture is when it is black or white; None for any other colour, a pixel of a shade in between.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Ink {
    Black,
    White,
}

/// The ink of the pixel at (x, y): white outside the picture, and None for a pixel that is neither black nor white.
fn ink(picture: &Picture, x: i64, y: i64) -> Option<Ink> {
    if x < 0 || y < 0 || x >= i64::from(picture.width()) || y >= i64::from(picture.height()) {
        return Some(Ink::White);
    }
    match picture.pixel(x as u32, y as u32).unwrap() {
        WHITE => Some(Ink::White),
        BLACK => Some(Ink::Black),
        _ => None,
    }
}

/// The pixel at (x, y) of a picture, for a pixel inside it.
fn at(picture: &Picture, x: i64, y: i64) -> [u8; 4] {
    picture.pixel(x as u32, y as u32).unwrap()
}

#[test]
fn a_text_with_the_uppercase_transform_is_drawn_in_capitals_and_its_source_words_stay_in_lower_case() {
    let folder = project_copied_from_fixture("text-dressing-on-the-processor", "upper-case", "text-dressing-upper-case");
    let composition = fs::read_to_string(folder.join("composition.xml")).unwrap();
    assert!(
        composition.contains(">hello, world</text>"),
        "the source words are unchanged: {composition}"
    );
    assert!(
        !composition.contains("HELLO"),
        "the source words are not written in capitals: {composition}"
    );

    let drawn = picture_of(&folder);
    let capitals = picture_of(&text_project(
        "upper-case-written-as-capitals",
        &format!("<text left=\"10\" top=\"8\" {FAMILY} font-size=\"30\" color=\"#c02020\">HELLO, WORLD</text>"),
    ));
    assert_eq!(
        drawn.pixels(),
        capitals.pixels(),
        "the transformed text is drawn as the same capitals written out"
    );

    let lower = picture_of(&text_project(
        "upper-case-without-the-transform",
        &format!("<text left=\"10\" top=\"8\" {FAMILY} font-size=\"30\" color=\"#c02020\">hello, world</text>"),
    ));
    assert_ne!(drawn.pixels(), lower.pixels(), "the transform changes the drawing");
}

#[test]
fn a_shadow_is_drawn_at_its_written_offset_and_colour_under_the_glyphs() {
    let plain = picture_of(&text_project("shadow-plain", &text_body("", "HIIH")));
    let shadowed = picture_of(&text_project("shadow-one", &text_body("text-shadow=\"6px 4px #2050c0\"", "HIIH")));
    let (across, down) = (6i64, 4i64);
    let (mut shadow_pixels, mut glyph_pixels) = (0, 0);
    for y in 0..i64::from(plain.height()) {
        for x in 0..i64::from(plain.width()) {
            let drawn = at(&shadowed, x, y);
            match (ink(&plain, x, y), ink(&plain, x - across, y - down)) {
                (Some(Ink::Black), _) => {
                    assert_eq!(drawn, BLACK, "the glyph is drawn over the shadow at ({x}, {y})");
                    glyph_pixels += 1;
                }
                (Some(Ink::White), Some(Ink::Black)) => {
                    assert_eq!(drawn, BLUE, "the shadow is drawn in its colour at ({x}, {y})");
                    shadow_pixels += 1;
                }
                (Some(Ink::White), Some(Ink::White)) => {
                    assert_eq!(drawn, WHITE, "nothing is drawn beyond the shadow at ({x}, {y})")
                }
                _ => {}
            }
        }
    }
    assert!(shadow_pixels > 0 && glyph_pixels > 0, "the shadow and the glyphs were both checked");
}

#[test]
fn of_two_shadows_the_first_listed_is_on_top_where_they_overlap() {
    let plain = picture_of(&text_project("two-shadows-plain", &text_body("", "HIIH")));
    let shadowed = picture_of(&text_project(
        "two-shadows",
        &text_body("text-shadow=\"6px 4px #2050c0, 2px 2px #c02020\"", "HIIH"),
    ));
    let first = (6i64, 4i64);
    let second = (2i64, 2i64);
    let (mut overlap, mut only_second) = (0, 0);
    for y in 0..i64::from(plain.height()) {
        for x in 0..i64::from(plain.width()) {
            let drawn = at(&shadowed, x, y);
            match ink(&plain, x, y) {
                Some(Ink::Black) => assert_eq!(drawn, BLACK, "the glyph is on top at ({x}, {y})"),
                Some(Ink::White) => {
                    let (Some(under_first), Some(under_second)) =
                        (ink(&plain, x - first.0, y - first.1), ink(&plain, x - second.0, y - second.1))
                    else {
                        continue;
                    };
                    let expected = match (under_first, under_second) {
                        (Ink::Black, Ink::Black) => {
                            overlap += 1;
                            BLUE
                        }
                        (Ink::Black, Ink::White) => BLUE,
                        (Ink::White, Ink::Black) => {
                            only_second += 1;
                            RED
                        }
                        (Ink::White, Ink::White) => WHITE,
                    };
                    assert_eq!(drawn, expected, "the shadows at ({x}, {y})");
                }
                None => {}
            }
        }
    }
    assert!(overlap > 0 && only_second > 0, "both kinds of place were checked");
}

#[test]
fn a_blurred_shadow_fades_from_its_colour_to_the_white_around_it() {
    let plain = picture_of(&text_project("blurred-plain", &text_body("", "HIIH")));
    let blurred = picture_of(&text_project("blurred", &text_body("text-shadow=\"5px 4px 3px #2050c0\"", "HIIH")));
    let mut soft = 0;
    for y in 0..i64::from(plain.height()) {
        for x in 0..i64::from(plain.width()) {
            if ink(&plain, x, y) != Some(Ink::White) {
                continue;
            }
            let drawn = at(&blurred, x, y);
            if drawn == WHITE || drawn == BLUE {
                continue;
            }
            assert_eq!(drawn[3], 255, "the shadow is opaque at ({x}, {y})");
            for channel in 0..3 {
                assert!(
                    drawn[channel] >= BLUE[channel],
                    "a shade between the shadow and white at ({x}, {y}): {drawn:?}"
                );
            }
            soft += 1;
        }
    }
    assert!(soft > 0, "the blurred shadow has soft edges");
}

/// The distance, in pixels, from (x, y) to the nearest pixel of `picture` that is not white, searched up to `reach`.
fn distance_to_ink(picture: &Picture, x: i64, y: i64, reach: i64) -> Option<i64> {
    (0..=reach).find(|&distance| {
        (-distance..=distance).any(|dy| (-distance..=distance).any(|dx| ink(picture, x + dx, y + dy) != Some(Ink::White)))
    })
}

#[test]
fn an_outline_surrounds_each_glyph_over_its_fill_by_the_written_width() {
    let plain = picture_of(&text_project("outline-plain", &text_body("", "HIIH")));
    let outlined = picture_of(&text_project(
        "outline-two-pixels",
        &text_body("text-stroke=\"2px #2050c0\"", "HIIH"),
    ));
    let (mut over_fill, mut around) = (0, 0);
    for y in 0..i64::from(plain.height()) {
        for x in 0..i64::from(plain.width()) {
            if at(&outlined, x, y) != BLUE {
                continue;
            }
            if ink(&plain, x, y) == Some(Ink::Black) {
                over_fill += 1;
                continue;
            }
            // Outside the glyphs, a 2 pixel line reaches one whole pixel past the edge and no further.
            let distance = distance_to_ink(&plain, x, y, 3).unwrap_or_else(|| panic!("an outline far from its glyph at ({x}, {y})"));
            assert!(distance <= 1, "the outline reaches {distance} pixels from the glyph at ({x}, {y})");
            around += 1;
        }
    }
    assert!(over_fill > 0, "the outline is drawn over the fill of the glyphs");
    assert!(around > 0, "the outline is drawn around the glyphs");
}

/// The farthest that an outline pixel of `outlined` lies from the glyphs of `plain`, over the pixels that are not over a glyph.
fn outline_reach(plain: &Picture, outlined: &Picture) -> i64 {
    let mut reach = 0;
    for y in 0..i64::from(plain.height()) {
        for x in 0..i64::from(plain.width()) {
            if at(outlined, x, y) != BLUE || ink(plain, x, y) == Some(Ink::Black) {
                continue;
            }
            let distance = distance_to_ink(plain, x, y, 4).unwrap_or_else(|| panic!("an outline far from its glyph at ({x}, {y})"));
            reach = reach.max(distance);
        }
    }
    reach
}

#[test]
fn an_outline_reaches_half_its_width_past_each_glyph_edge() {
    let plain = picture_of(&text_project("outline-reach-plain", &text_body("", "HIIH")));
    for (width, reach) in [(2, 1), (4, 2)] {
        let outlined = picture_of(&text_project(
            &format!("outline-reach-{width}-pixels"),
            &text_body(&format!("text-stroke=\"{width}px #2050c0\""), "HIIH"),
        ));
        assert_eq!(
            outline_reach(&plain, &outlined),
            reach,
            "an outline {width} pixels wide reaches {reach} pixels past each glyph edge"
        );
    }
}

#[test]
fn an_outline_of_width_zero_draws_nothing() {
    let plain = picture_of(&text_project("outline-none-plain", &text_body("", "HIIH")));
    let zero = picture_of(&text_project(
        "outline-width-zero",
        &text_body("text-stroke=\"0px #2050c0\"", "HIIH"),
    ));
    assert_eq!(plain.pixels(), zero.pixels(), "a width of 0 is no outline");
}

#[test]
fn a_text_with_wrapping_turned_off_stays_on_one_line_however_narrow_its_width() {
    let mut pictures = Vec::new();
    for width in ["1", "20", "200"] {
        let folder = text_project(
            &format!("no-wrap-width-{width}"),
            &format!("<text left=\"8\" top=\"8\" width=\"{width}\" {FAMILY} font-size=\"16\" white-space=\"nowrap\">AB CD EF GH</text>"),
        );
        assert_eq!(lines_of(&folder), 1, "the text is on one line at a width of {width}");
        pictures.push(picture_of(&folder));
    }
    assert_eq!(
        pictures[0].pixels(),
        pictures[2].pixels(),
        "the narrowest and the widest are drawn alike"
    );
    assert_eq!(
        pictures[1].pixels(),
        pictures[2].pixels(),
        "a width of 20 and of 200 are drawn alike"
    );

    let wrapped = text_project(
        "wrapping-width-20",
        &format!("<text left=\"8\" top=\"8\" width=\"20\" {FAMILY} font-size=\"16\">AB CD EF GH</text>"),
    );
    assert!(lines_of(&wrapped) > 1, "the same words wrap at a width of 20 when wrapping is on");
}

#[test]
fn figure_words_the_font_has_no_lookup_for_leave_its_own_figures_as_they_are() {
    let plain = picture_of(&text_project("figures-plain", &text_body("", "111 888")));
    let worded = picture_of(&text_project(
        "figures-words",
        &text_body("font-variant-numeric=\"lining-nums proportional-nums\"", "111 888"),
    ));
    assert_eq!(
        plain.pixels(),
        worded.pixels(),
        "lining-nums and proportional-nums have no lookup in the test font, so its own figures are drawn"
    );
}
