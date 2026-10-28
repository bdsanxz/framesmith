//! A glyph's fill unions the shares of its contours where they overlap in part (`docs/spec/text.md`, section 6.6, rule 10):
//! the H of the test font has a stem and a crossbar that share pixels without covering them twice.
use draw::{Colour, Font, Glyph, GlyphRun};
use std::sync::Arc;
use vello_cpu::kurbo::Affine;

fn h_run() -> GlyphRun {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts/framesmith-test-stroke-400.ttf");
    let data = Arc::new(std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display())));
    GlyphRun {
        font: Font { data, index: 0 },
        size: 24.0,
        skew_degrees: 0.0,
        colour: Colour {
            red: 0x20,
            green: 0x20,
            blue: 0x20,
        },
        glyphs: vec![Glyph { id: 41, x: 20.0, y: 40.0 }],
    }
}

#[test]
fn overlapping_contours_cover_their_union_not_their_sum() {
    // The left stem (x 21.44-23.36) and the crossbar (y 30.64-32.56) of the H share the pixel (23, 30):
    // stem 0.36, crossbar 0.36, shared square 0.1296. Union 0.5904 -> 151; the sum 0.72 -> 184 is wrong.
    let run = h_run();
    let cov = draw_cpu::glyph_coverage(200, 90, &run, &run.glyphs[0], (0.0, 0.0), Affine::IDENTITY).unwrap();
    assert_eq!(cov[30 * 200 + 23], 151, "union of the stem and the crossbar at (23, 30)");
    // The same joint one row down: stem 0.36, crossbar 0.56, shared 0.2016. Union 0.7184 -> 183.
    assert_eq!(cov[32 * 200 + 23], 183, "union of the stem and the crossbar at (23, 32)");
}
