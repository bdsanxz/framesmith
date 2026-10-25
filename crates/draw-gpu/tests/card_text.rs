//! The card draws text as the processor does (`docs/spec/card-renderer.md`, section 9). A frame of glyph runs at
//! fractional origins, sheared, clipped, with a fill drawn over the text, agrees with the processor's frame within
//! the tolerance of section 4. A font whose table directory cannot be read is refused by the card with its reason,
//! and draws no picture. Each test needs a graphics card that frx7 can draw with, so each is reference machine only
//! (`docs/dev/gates.md` section 4).

use std::fs;
use std::path::Path;
use std::sync::Arc;

use draw::{Colour, Font, Frame, Glyph, GlyphRun, Mark, Picture, Renderer, Step, TextMark};
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use layout::Rect;

/// The least similarity of two pictures, and the most values further apart than 4 levels, as one in this many
/// values (`card-renderer.md` section 4; `measure`).
const LEAST_SIMILARITY: f64 = 0.995;
const ONE_IN: u64 = 2000;

fn test_font() -> Arc<Vec<u8>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts/framesmith-test-stroke-400.ttf");
    Arc::new(fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display())))
}

fn glyphs(ids: &[u32], first_x: f32, y: f32, advance: f32) -> Vec<Glyph> {
    ids.iter()
        .enumerate()
        .map(|(index, &id)| Glyph {
            id,
            x: first_x + advance * index as f32,
            y,
        })
        .collect()
}

/// A frame of one run of glyphs of the first face of the machine's Helvetica, the face the fall-back of
/// `docs/spec/text.md`, section 2.4, draws a character the named font lacks with.
fn frame_of_a_machine_face(font: Arc<Vec<u8>>) -> Frame {
    let run = GlyphRun {
        font: Font { data: font, index: 0 },
        size: 36.0,
        skew_degrees: 0.0,
        colour: Colour {
            red: 250,
            green: 250,
            blue: 250,
        },
        glyphs: glyphs(&[36, 37, 38, 100, 140, 180], 8.4, 52.3, 24.0),
    };
    Frame {
        width: 160,
        height: 80,
        background: Colour {
            red: 10,
            green: 20,
            blue: 30,
        },
        marks: vec![Mark::Text(TextMark {
            runs: vec![run],
            pictures: Vec::new(),
            clip: None,
            shadows: Vec::new(),
            outline: None,
        })],
    }
}

/// A frame of a background, a fill under the text, two runs in a clipped paragraph, a run without a clip that
/// lies over the first, and a fill drawn over the text.
fn frame_of_text(font: &Arc<Vec<u8>>) -> Frame {
    let face = Font {
        data: font.clone(),
        index: 0,
    };
    let under = Colour {
        red: 200,
        green: 40,
        blue: 40,
    };
    let clipped = TextMark {
        runs: vec![
            GlyphRun {
                font: face.clone(),
                size: 40.0,
                skew_degrees: 12.0,
                colour: Colour {
                    red: 250,
                    green: 250,
                    blue: 250,
                },
                glyphs: glyphs(&[5, 6, 7, 8], 20.25, 50.5, 19.75),
            },
            GlyphRun {
                font: face.clone(),
                size: 23.7,
                skew_degrees: 0.0,
                colour: Colour {
                    red: 30,
                    green: 220,
                    blue: 90,
                },
                glyphs: glyphs(&[9, 10, 11, 12, 13], 22.3, 70.3, 11.9),
            },
        ],
        pictures: Vec::new(),
        clip: Some(Rect {
            left: 10,
            top: 10,
            width: 120,
            height: 60,
        }),
        shadows: Vec::new(),
        outline: None,
    };
    let whole = TextMark {
        runs: vec![GlyphRun {
            font: face,
            size: 17.5,
            skew_degrees: -8.0,
            colour: Colour {
                red: 20,
                green: 120,
                blue: 255,
            },
            glyphs: glyphs(&[5, 14, 15], 60.6, 38.2, 14.4),
        }],
        pictures: Vec::new(),
        clip: None,
        shadows: Vec::new(),
        outline: None,
    };
    Frame {
        width: 160,
        height: 80,
        background: Colour {
            red: 10,
            green: 20,
            blue: 30,
        },
        marks: vec![
            Mark::Step(Step::Fill {
                rect: Rect {
                    left: 0,
                    top: 0,
                    width: 40,
                    height: 80,
                },
                colour: under,
            }),
            Mark::Text(clipped),
            Mark::Text(whole),
            Mark::Step(Step::Fill {
                rect: Rect {
                    left: 100,
                    top: 30,
                    width: 30,
                    height: 20,
                },
                colour: Colour {
                    red: 0,
                    green: 0,
                    blue: 255,
                },
            }),
        ],
    }
}

fn measured(picture: &Picture) -> measure::Frame {
    measure::Frame {
        width: picture.width(),
        height: picture.height(),
        rgb: picture
            .pixels()
            .chunks(4)
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .collect(),
    }
}

/// Asserts that the card's picture agrees with the processor's within the tolerance of section 4.
fn agrees(processor: &Picture, card: &Picture) {
    let similarity = measure::similarity(&measured(processor), &measured(card));
    let further_apart: u64 = processor
        .pixels()
        .chunks(4)
        .zip(card.pixels().chunks(4))
        .map(|(a, b)| a[..3].iter().zip(&b[..3]).filter(|&(x, y)| x.abs_diff(*y) > 4).count() as u64)
        .sum();
    let values = u64::from(processor.width()) * u64::from(processor.height()) * 3;
    assert!(
        matches!(similarity, Some(value) if value >= LEAST_SIMILARITY),
        "similarity {similarity:?} is below {LEAST_SIMILARITY}"
    );
    assert!(
        further_apart * ONE_IN <= values,
        "{further_apart} of {values} values are further apart than 4 levels, above 0.05 %"
    );
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_glyphs_of_a_machine_face_agree_with_the_processor() {
    let collection = fs::read("/System/Library/Fonts/Helvetica.ttc").unwrap();
    let font = Arc::new(text::one_face(collection, 0).expect("the first face of Helvetica"));
    let frame = frame_of_a_machine_face(font);
    let processor = CpuRenderer.draw_frame(&frame).unwrap();
    let card = CardRenderer::new().unwrap().draw_frame(&frame).unwrap();
    let background = processor.pixel(0, 79).unwrap();
    let ink = processor.pixels().chunks(4).filter(|pixel| pixel[..3] != background[..3]).count();
    assert!(ink > 300, "the frame draws too little text to test anything: {ink} pixels");
    agrees(&processor, &card);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_frame_of_clipped_sheared_text_agrees_with_the_processor() {
    let frame = frame_of_text(&test_font());
    let processor = CpuRenderer.draw_frame(&frame).unwrap();
    let card = CardRenderer::new().unwrap().draw_frame(&frame).unwrap();

    let background = processor.pixel(0, 79).unwrap();
    let ink = processor.pixels().chunks(4).filter(|pixel| pixel[..3] != background[..3]).count();
    assert!(
        ink > 500,
        "the frame draws too little text to test anything: {ink} pixels differ from the background"
    );

    let similarity = measure::similarity(&measured(&processor), &measured(&card));
    let further_apart: u64 = processor
        .pixels()
        .chunks(4)
        .zip(card.pixels().chunks(4))
        .map(|(a, b)| a[..3].iter().zip(&b[..3]).filter(|&(x, y)| x.abs_diff(*y) > 4).count() as u64)
        .sum();
    let values = u64::from(processor.width()) * u64::from(processor.height()) * 3;
    assert!(
        matches!(similarity, Some(value) if value >= LEAST_SIMILARITY),
        "similarity {similarity:?} is below {LEAST_SIMILARITY}"
    );
    assert!(
        further_apart * ONE_IN <= values,
        "{further_apart} of {values} values are further apart than 4 levels, above 0.05 %"
    );
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_font_whose_table_directory_cannot_be_read_is_refused_by_the_card_with_its_reason() {
    let whole = test_font();
    let cut = Arc::new(whole[..12].to_vec());
    let frame = frame_of_text(&cut);
    let mut card = CardRenderer::new().unwrap();
    let error = card.draw_frame(&frame).expect_err("the card refuses a font it cannot read");
    assert!(error.to_string().contains("head table"), "{error}");
}
