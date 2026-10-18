//! A text is drawn in its colour on the processor, and a heavier face gives wider strokes than a lighter one at the
//! same size (`docs/spec/text.md`, sections 3.2 and 6.3).

use std::path::PathBuf;
use std::sync::Arc;

use draw::{Colour, Font, Frame, Glyph, GlyphRun, Mark, Picture, Renderer, TextMark};
use draw_cpu::CpuRenderer;

/// The bytes of a test font file of the repository's `fixtures/fonts` folder.
fn font_file(name: &str) -> Arc<Vec<u8>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts").join(name);
    Arc::new(std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display())))
}

/// Glyph 42 is the capital I: a glyph is the character's code less 0x1F (`docs/spec/test-font.md`, section 2).
const CAPITAL_I: u32 = 0x49 - 0x1F;

/// A white frame of 120 by 80 pixels with one capital I at 100 pixels, its baseline 70 pixels from the top.
fn capital_i(file: &str, colour: Colour) -> Picture {
    let frame = Frame {
        width: 120,
        height: 80,
        background: Colour {
            red: 255,
            green: 255,
            blue: 255,
        },
        marks: vec![Mark::Text(TextMark {
            runs: vec![GlyphRun {
                font: Font {
                    data: font_file(file),
                    index: 0,
                },
                size: 100.0,
                skew_degrees: 0.0,
                colour,
                glyphs: vec![Glyph {
                    id: CAPITAL_I,
                    x: 20.0,
                    y: 70.0,
                }],
            }],
            pictures: Vec::new(),
            clip: None,
        })],
    };
    CpuRenderer.draw_frame(&frame).unwrap()
}

/// The sum of the coverage of every pixel of a black glyph: 255 for a pixel the glyph covers wholly.
fn ink_of_black(picture: &Picture) -> u64 {
    let mut ink = 0u64;
    for y in 0..picture.height() {
        for x in 0..picture.width() {
            let [red, _, _, _] = picture.pixel(x, y).unwrap();
            ink += u64::from(255 - red);
        }
    }
    ink
}

#[test]
fn a_text_is_drawn_in_its_colour_and_every_pixel_lies_between_the_colour_and_white() {
    let colour = Colour {
        red: 0x20,
        green: 0x60,
        blue: 0xc0,
    };
    let picture = capital_i("framesmith-test-stroke-400.ttf", colour);
    let mut fully_covered = 0;
    for y in 0..picture.height() {
        for x in 0..picture.width() {
            let pixel = picture.pixel(x, y).unwrap();
            if pixel == [0x20, 0x60, 0xc0, 255] {
                fully_covered += 1;
            }
            // Coverage a from the red channel: each channel is white moved a of the way to the colour.
            let coverage = f64::from(255 - pixel[0]) / f64::from(255 - colour.red);
            for (channel, target) in [(1, colour.green), (2, colour.blue)] {
                let want = 255.0 - coverage * f64::from(255 - target);
                assert!(
                    (f64::from(pixel[channel]) - want).abs() <= 2.0,
                    "pixel ({x}, {y}) is {pixel:?}, not a blend of white and the colour"
                );
            }
        }
    }
    assert!(fully_covered > 0, "some pixels of the glyph are the colour itself");
}

#[test]
fn a_heavier_face_gives_more_ink_than_a_lighter_face_at_the_same_size() {
    let black = Colour { red: 0, green: 0, blue: 0 };
    let regular = ink_of_black(&capital_i("framesmith-test-stroke-400.ttf", black));
    let bold = ink_of_black(&capital_i("framesmith-test-stroke-700.ttf", black));
    assert!(regular > 0, "the regular face draws ink");
    assert!(bold > regular, "bold ink {bold} is not more than regular ink {regular}");
}
