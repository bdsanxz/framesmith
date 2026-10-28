//! The marks a frame draws for a text: the colour and the size it is drawn in, the colour black of a text that names
//! none, the attributes a box gives its texts, and the letter spacing in ems (`docs/spec/text.md`, sections 3, 6.1,
//! 6.3 and 8).

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::fixtures;
use draw::{Colour, GlyphRun, Mark};
use project::{Project, Report};
use render::Scene;

/// A project whose composition is `body` in the root of a 200 by 90 pixel canvas, with the test font declared and its
/// two files copied into the project folder.
fn text_project(name: &str, body: &str) -> PathBuf {
    let folder = common::scratch("text-frame-marks", name);
    fs::create_dir_all(folder.join("fonts")).unwrap();
    for file in ["framesmith-test-stroke-400.ttf", "framesmith-test-stroke-700.ttf"] {
        fs::copy(fixtures().join("fonts").join(file), folder.join("fonts").join(file)).unwrap();
    }
    let composition = format!(
        "<composition version=\"1\" width=\"200\" height=\"90\" fps=\"10\" duration=\"1\" background=\"#ffffff\">\n\
         <font src=\"fonts/framesmith-test-stroke-400.ttf\" family=\"Framesmith Test Stroke\"/>\n\
         <font src=\"fonts/framesmith-test-stroke-700.ttf\" family=\"Framesmith Test Stroke\"/>\n\
         {body}\n</composition>\n"
    );
    fs::write(folder.join("composition.xml"), composition).unwrap();
    folder
}

/// The glyph runs that frame 0 of a project draws for its texts, in drawing order.
fn text_runs(folder: &Path) -> Vec<GlyphRun> {
    let scene =
        Scene::from_project(&Project::open(folder).unwrap(), &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let frame = scene.frame(0).unwrap();
    frame
        .marks
        .into_iter()
        .flat_map(|mark| match mark {
            Mark::Text(text) => text.runs,
            Mark::Step(_) | Mark::Group(_) | Mark::Background(_) | Mark::Ring(_) | Mark::Cut(_) | Mark::BoxShadow(_) => Vec::new(),
        })
        .collect()
}

/// The x of the second glyph of the first run: where the second letter starts.
fn second_letter_x(runs: &[GlyphRun]) -> f32 {
    runs[0].glyphs[1].x
}

/// A report as `file line code message`, the form the refusal tests compare.
fn shown(report: &Report) -> String {
    format!(
        "{} {} {} {}",
        report.file(),
        report.problem().line(),
        report.problem().code(),
        report.problem().message()
    )
}

const FAMILY: &str = "font-family=\"Framesmith Test Stroke\"";

#[test]
fn a_text_that_names_no_colour_is_drawn_in_black() {
    let folder = text_project(
        "no-colour",
        &format!("<text left=\"10\" top=\"10\" {FAMILY} font-size=\"20\">AB</text>"),
    );
    let runs = text_runs(&folder);
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].colour, Colour { red: 0, green: 0, blue: 0 });
}

#[test]
fn a_text_is_drawn_in_its_own_colour_and_size() {
    let folder = text_project(
        "own-colour",
        &format!("<text left=\"10\" top=\"10\" {FAMILY} font-size=\"20\" color=\"#c02020\">AB</text>"),
    );
    let runs = text_runs(&folder);
    assert_eq!(
        runs[0].colour,
        Colour {
            red: 0xc0,
            green: 0x20,
            blue: 0x20
        }
    );
    assert_eq!(runs[0].size, 20.0);
}

#[test]
fn a_text_takes_the_colour_and_size_of_its_box_and_its_own_colour_wins() {
    let folder = text_project(
        "from-box",
        &format!(
            "<box left=\"0\" top=\"0\" width=\"200\" height=\"90\" {FAMILY} font-size=\"24\" color=\"#c02020\">\n\
             <text left=\"4\" top=\"4\">AB</text>\n\
             <text left=\"4\" top=\"40\" color=\"#2050c0\">AB</text>\n\
             </box>"
        ),
    );
    let runs = text_runs(&folder);
    assert_eq!(runs.len(), 2, "{runs:?}");
    assert_eq!(
        runs[0].colour,
        Colour {
            red: 0xc0,
            green: 0x20,
            blue: 0x20
        },
        "the box's colour"
    );
    assert_eq!(
        runs[1].colour,
        Colour {
            red: 0x20,
            green: 0x50,
            blue: 0xc0
        },
        "the text's own colour"
    );
    assert!(runs.iter().all(|run| run.size == 24.0), "the box's size: {runs:?}");
}

#[test]
fn letter_spacing_in_ems_is_taken_at_the_font_size_of_the_text_that_writes_it() {
    // 0.5 em at 20 pixels is 10 pixels: the second letter starts 10 pixels further on than with no spacing, as it does
    // with 10 pixels written in full.
    let plain = text_runs(&text_project(
        "no-spacing",
        &format!("<text left=\"10\" top=\"10\" {FAMILY} font-size=\"20\">AB</text>"),
    ));
    let ems = text_runs(&text_project(
        "em-spacing",
        &format!("<text left=\"10\" top=\"10\" {FAMILY} font-size=\"20\" letter-spacing=\"0.5em\">AB</text>"),
    ));
    let pixels = text_runs(&text_project(
        "pixel-spacing",
        &format!("<text left=\"10\" top=\"10\" {FAMILY} font-size=\"20\" letter-spacing=\"10\">AB</text>"),
    ));
    assert_close(second_letter_x(&ems) - second_letter_x(&plain), 10.0);
    assert_close(second_letter_x(&pixels) - second_letter_x(&plain), 10.0);
}

/// Asserts that two amounts of pixels are equal to a thousandth of a pixel.
fn assert_close(amount: f32, expected: f32) {
    assert!((amount - expected).abs() < 1e-3, "{amount} is not {expected}");
}

#[test]
fn an_inherited_letter_spacing_in_ems_is_taken_at_the_font_size_of_the_box_that_writes_it() {
    // The box writes 0.5 em at its 20 pixels, which is 10 pixels; the text inside it is drawn at 40 pixels and takes
    // the 10 pixels, not 20.
    let folder = text_project(
        "inherited-em",
        &format!(
            "<box left=\"0\" top=\"0\" width=\"200\" height=\"90\" {FAMILY} font-size=\"20\" letter-spacing=\"0.5em\">\n\
             <text left=\"10\" top=\"10\" font-size=\"40\">AB</text>\n\
             </box>\n\
             <text left=\"10\" top=\"50\" {FAMILY} font-size=\"40\" letter-spacing=\"10\">AB</text>"
        ),
    );
    let runs = text_runs(&folder);
    assert_eq!(runs.len(), 2, "{runs:?}");
    assert_eq!(second_letter_x(&runs[..1]), second_letter_x(&runs[1..]));
}

#[test]
fn a_span_inside_a_text_is_drawn_in_its_own_colour() {
    let folder = text_project(
        "span-colour",
        &format!("<text left=\"10\" top=\"10\" {FAMILY} font-size=\"20\">words <span color=\"#ffffff\">x</span></text>"),
    );
    let runs = text_runs(&folder);
    assert_eq!(runs.len(), 2, "{runs:?}");
    assert_eq!(runs[0].colour, Colour { red: 0, green: 0, blue: 0 }, "the words of the text");
    assert_eq!(
        runs[1].colour,
        Colour {
            red: 0xff,
            green: 0xff,
            blue: 0xff
        },
        "the span"
    );
}

#[test]
fn a_percentage_margin_on_a_span_is_refused_with_its_line() {
    let folder = text_project(
        "span-percent-margin",
        &format!("<text left=\"10\" top=\"10\" {FAMILY} font-size=\"20\">AB <span margin=\"10%\" color=\"#ffffff\">x</span></text>"),
    );
    let reports = match Scene::from_project(&Project::open(&folder).unwrap(), &render::system_codecs()) {
        Ok(_) => panic!("a percentage margin on a span is drawn"),
        Err(reports) => reports,
    };
    assert_eq!(reports.len(), 1, "{reports:#?}");
    assert_eq!(
        shown(&reports[0]),
        "composition.xml 4 FS-0035 `margin=\"10%\"` on <span> is not a form this version supports: a number of pixels"
    );
}

#[test]
fn a_tween_inside_a_span_is_refused_with_its_line() {
    let folder = text_project(
        "tween-in-span",
        &format!(
            "<text left=\"10\" top=\"10\" {FAMILY} font-size=\"20\">AB <span color=\"#ffffff\">x<tween at=\"0\" duration=\"1\" from-opacity=\"0\" to-opacity=\"1\"/></span></text>"
        ),
    );
    let reports = match Scene::from_project(&Project::open(&folder).unwrap(), &render::system_codecs()) {
        Ok(_) => panic!("a tween inside a span is drawn"),
        Err(reports) => reports,
    };
    assert_eq!(reports.len(), 1, "{reports:#?}");
    assert_eq!(
        shown(&reports[0]),
        "composition.xml 4 FS-0035 <tween> inside <text> is not supported by this version, which draws the words of a <text> and of its <span>s and no other element inside it"
    );
}

/// The x of every glyph of a frame's texts, in drawing order.
fn glyph_xs(runs: &[GlyphRun]) -> Vec<f32> {
    runs.iter().flat_map(|run| run.glyphs.iter().map(|glyph| glyph.x)).collect()
}

#[test]
fn a_span_takes_the_size_and_colour_of_its_text_unless_it_writes_its_own() {
    let folder = text_project(
        "span-inherits",
        &format!("<text left=\"10\" top=\"10\" {FAMILY} font-size=\"24\" color=\"#2050c0\">AB <span color=\"#c02020\">CD</span> EF</text>"),
    );
    let runs = text_runs(&folder);
    let blue = Colour {
        red: 0x20,
        green: 0x50,
        blue: 0xc0,
    };
    let red = Colour {
        red: 0xc0,
        green: 0x20,
        blue: 0x20,
    };
    assert!(!runs.is_empty());
    assert!(
        runs.iter().all(|run| run.size == 24.0),
        "the span takes the size of its text: {runs:?}"
    );
    assert!(runs.iter().any(|run| run.colour == blue), "the words of the text: {runs:?}");
    assert!(runs.iter().any(|run| run.colour == red), "the words of the span: {runs:?}");
}

#[test]
fn a_span_that_writes_its_own_size_is_drawn_at_that_size() {
    let folder = text_project(
        "span-own-size",
        &format!("<text left=\"10\" top=\"10\" {FAMILY} font-size=\"24\">AB <span font-size=\"12\">CD</span></text>"),
    );
    let runs = text_runs(&folder);
    assert!(runs.iter().any(|run| run.size == 24.0), "the words of the text: {runs:?}");
    assert!(runs.iter().any(|run| run.size == 12.0), "the words of the span: {runs:?}");
}

#[test]
fn a_span_margin_and_padding_move_the_words_after_them_by_their_sides() {
    // The margin of 12 and the padding of 6 are on each side: 18 pixels before the span's words and 18 after them.
    // Without the span the six letters sit 18 and 36 pixels nearer the left edge of the text.
    let plain = text_runs(&text_project(
        "span-plain-letters",
        &format!("<text left=\"10\" top=\"10\" {FAMILY} font-size=\"30\">HIIIHH</text>"),
    ));
    let spanned = text_runs(&text_project(
        "span-margin-and-padding",
        &format!("<text left=\"10\" top=\"10\" {FAMILY} font-size=\"30\">HI<span margin=\"0 12\" padding=\"0 6\">II</span>HH</text>"),
    ));
    let (plain, spanned) = (glyph_xs(&plain), glyph_xs(&spanned));
    assert_eq!(plain.len(), 6, "{plain:?}");
    assert_eq!(spanned.len(), 6, "{spanned:?}");
    let shifts: Vec<f32> = plain.iter().zip(&spanned).map(|(plain, spanned)| spanned - plain).collect();
    let expected = [0.0, 0.0, 18.0, 18.0, 36.0, 36.0];
    for (shift, expected) in shifts.iter().zip(expected) {
        assert_close(*shift, expected);
    }
}
