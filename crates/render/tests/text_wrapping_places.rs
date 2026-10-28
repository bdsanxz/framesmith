//! A text with `left` and `right` and no `width` wraps at the width that its container leaves between them, as it
//! wraps at the width written (`docs/spec/notation.md`, section 7.3; `docs/spec/text.md`, section 5).

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::fixtures;
use draw::{Glyph, Mark};
use project::Project;
use render::Scene;

const FAMILY: &str = "font-family=\"Framesmith Test Stroke\"";

/// A project of a 160 by 60 pixel canvas whose root holds `body`, with the test font declared and copied into it.
fn text_project(name: &str, body: &str) -> PathBuf {
    let folder = common::scratch("text-wrapping-places", name);
    fs::create_dir_all(folder.join("fonts")).unwrap();
    let file = "framesmith-test-stroke-400.ttf";
    fs::copy(fixtures().join("fonts").join(file), folder.join("fonts").join(file)).unwrap();
    let composition = format!(
        "<composition version=\"1\" width=\"160\" height=\"60\" fps=\"10\" duration=\"1\" background=\"#ffffff\">\n\
         <font src=\"fonts/{file}\" family=\"Framesmith Test Stroke\"/>\n\
         {body}\n</composition>\n"
    );
    fs::write(folder.join("composition.xml"), composition).unwrap();
    folder
}

/// The glyphs that frame 0 of a project draws for its texts, in drawing order.
fn glyphs_of_frame_zero(folder: &Path) -> Vec<Glyph> {
    let scene =
        Scene::from_project(&Project::open(folder).unwrap(), &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let frame = scene.frame(0).unwrap();
    frame
        .marks
        .into_iter()
        .flat_map(|mark| match mark {
            Mark::Text(text) => text.runs.into_iter().flat_map(|run| run.glyphs).collect(),
            Mark::Step(_) | Mark::Group(_) | Mark::Background(_) | Mark::Ring(_) | Mark::Cut(_) | Mark::BoxShadow(_) => Vec::new(),
        })
        .collect()
}

/// How many lines the glyphs lie on: the number of different baselines.
fn lines_of(glyphs: &[Glyph]) -> usize {
    let mut baselines: Vec<f32> = glyphs.iter().map(|glyph| glyph.y).collect();
    baselines.sort_by(|a, b| a.partial_cmp(b).unwrap());
    baselines.dedup();
    baselines.len()
}

#[test]
fn a_text_with_left_and_right_and_no_width_wraps_as_at_the_width_they_leave() {
    // 160 - 8 - 100 = 52 pixels: the text lays out as a text of 52 pixels written at the same left does.
    let by_edges = glyphs_of_frame_zero(&text_project(
        "left-and-right",
        &format!("<text left=\"8\" right=\"100\" top=\"8\" {FAMILY} font-size=\"16\">AB CD EF GH IJ KL</text>"),
    ));
    let by_width = glyphs_of_frame_zero(&text_project(
        "width-written",
        &format!("<text left=\"8\" width=\"52\" top=\"8\" {FAMILY} font-size=\"16\">AB CD EF GH IJ KL</text>"),
    ));
    assert!(lines_of(&by_edges) > 1, "the text does not wrap: {by_edges:?}");
    assert_eq!(by_edges.len(), by_width.len(), "{by_edges:?} {by_width:?}");
    for (drawn, expected) in by_edges.iter().zip(&by_width) {
        assert_eq!((drawn.id, drawn.x, drawn.y), (expected.id, expected.x, expected.y));
    }
}

#[test]
fn a_text_with_only_a_left_and_no_width_is_one_line() {
    let glyphs = glyphs_of_frame_zero(&text_project(
        "left-only",
        &format!("<text left=\"8\" top=\"8\" {FAMILY} font-size=\"16\">AB CD EF GH IJ KL</text>"),
    ));
    assert_eq!(lines_of(&glyphs), 1, "{glyphs:?}");
}
