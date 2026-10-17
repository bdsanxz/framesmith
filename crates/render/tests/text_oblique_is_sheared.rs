//! The oblique of a text that has no italic face is the upright face sheared about its baseline by 14 degrees
//! (`docs/spec/text.md`, section 3.3). The test measures the ink of the picture that the processor draws: the centre of
//! the ink of the top third of the glyph moves right, against the centre of the ink of its bottom third, by the tangent
//! of 14 degrees for each pixel of height between the two centres.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::fixtures;
use draw::Picture;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const FAMILY: &str = "font-family=\"Framesmith Test Stroke\"";

/// A project of a 160 by 120 pixel white canvas whose root holds one `<text>` of the letter H at 80 pixels, with the
/// attributes given after its family.
fn text_project(name: &str, attributes: &str) -> PathBuf {
    let folder = common::scratch("text-oblique-is-sheared", name);
    fs::create_dir_all(folder.join("fonts")).unwrap();
    let file = "framesmith-test-stroke-400.ttf";
    fs::copy(fixtures().join("fonts").join(file), folder.join("fonts").join(file)).unwrap();
    let composition = format!(
        "<composition version=\"1\" width=\"160\" height=\"120\" fps=\"10\" duration=\"1\" background=\"#ffffff\">\n\
         <font src=\"fonts/{file}\" family=\"Framesmith Test Stroke\"/>\n\
         <text left=\"20\" top=\"10\" {FAMILY} font-size=\"80\" {attributes}>H</text>\n\
         </composition>\n"
    );
    fs::write(folder.join("composition.xml"), composition).unwrap();
    folder
}

/// The picture of frame 0 that the processor draws for a project.
fn picture_of(folder: &Path) -> Picture {
    let scene = Scene::from_project(&Project::open(folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"));
    scene.picture(0, &mut CpuRenderer).unwrap()
}

/// How dark a pixel is, from 0 for white to 255 for black, read from its red byte: the text is black on white.
fn darkness(picture: &Picture, x: u32, y: u32) -> f64 {
    f64::from(255 - picture.pixel(x, y).unwrap()[0])
}

/// The first and the last row that holds ink.
fn ink_rows(picture: &Picture) -> (u32, u32) {
    let rows: Vec<u32> = (0..picture.height())
        .filter(|&y| (0..picture.width()).any(|x| darkness(picture, x, y) > 0.0))
        .collect();
    (*rows.first().expect("the picture holds ink"), *rows.last().unwrap())
}

/// The centre of the ink, as (x, y), of the rows `first` to `last`, each row counted with its darkness.
fn centre_of_ink(picture: &Picture, first: u32, last: u32) -> (f64, f64) {
    let (mut weight, mut sum_x, mut sum_y) = (0.0, 0.0, 0.0);
    for y in first..=last {
        for x in 0..picture.width() {
            let dark = darkness(picture, x, y);
            weight += dark;
            sum_x += dark * f64::from(x);
            sum_y += dark * f64::from(y);
        }
    }
    (sum_x / weight, sum_y / weight)
}

#[test]
fn an_oblique_text_shears_its_glyph_about_the_baseline_by_fourteen_degrees_on_the_ink() {
    let upright = picture_of(&text_project("upright", ""));
    let italic = picture_of(&text_project("oblique", "font-style=\"italic\""));

    // The rows of the bands are those of the upright glyph, so that both pictures are measured in the same rows.
    let (top, bottom) = ink_rows(&upright);
    assert!(
        bottom - top > 30,
        "the glyph is {} rows high, which is too few to measure",
        bottom - top + 1
    );
    let third = (bottom - top + 1) / 3;
    let (top_band, bottom_band) = ((top, top + third - 1), (bottom + 1 - third, bottom));

    let (upright_top_x, upright_top_y) = centre_of_ink(&upright, top_band.0, top_band.1);
    let (upright_bottom_x, upright_bottom_y) = centre_of_ink(&upright, bottom_band.0, bottom_band.1);
    let (italic_top_x, _) = centre_of_ink(&italic, top_band.0, top_band.1);
    let (italic_bottom_x, _) = centre_of_ink(&italic, bottom_band.0, bottom_band.1);

    // A point at height h above the baseline moves right by h tan 14 degrees: the top band moves more than the bottom
    // band by that tangent times the distance between the band centres.
    let moved_top = italic_top_x - upright_top_x;
    let moved_bottom = italic_bottom_x - upright_bottom_x;
    let slope = (moved_top - moved_bottom) / (upright_bottom_y - upright_top_y);
    let expected = 14.0f64.to_radians().tan();
    assert!(
        (slope - expected).abs() < 0.03,
        "the top moves {slope:.4} pixels per pixel of height against the bottom; 14 degrees is {expected:.4}"
    );
}
