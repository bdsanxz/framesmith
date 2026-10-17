//! What the script tests share: the machine's font the scripts are drawn with, a style in it, and the glyphs of a
//! paragraph read back as characters (`docs/spec/text.md`, section 12). The font is a file of the machine, so only the
//! tests of macOS call `mac_fonts`, and the test files that do are compiled for macOS alone.

#![allow(dead_code)]

use std::collections::HashMap;

use text::{layout, Align, Block, Fonts, LineHeight, PositionedGlyph, Style};

/// The family name the machine's font is declared under in the fixtures (`docs/spec/text.md`, section 12.1).
pub const MAC_FAMILY: &str = "Arial Unicode";

/// The path of the machine's font file (`docs/spec/text.md`, section 12.1). It is read as it is, not through the project.
pub const MAC_FILE: &str = "/System/Library/Fonts/Supplemental/Arial Unicode.ttf";

/// A collection with the machine's font registered under its family name, the way a fixture's `<font>` element
/// registers it.
pub fn mac_fonts() -> Fonts {
    let bytes = std::fs::read(MAC_FILE).unwrap_or_else(|error| panic!("{MAC_FILE}: {error}"));
    let mut fonts = Fonts::new();
    fonts
        .register(bytes, Some(MAC_FAMILY))
        .expect("the machine's font registers as a TrueType font");
    fonts
}

/// A paragraph style in the machine's font at `size` pixels: weight 400, upright, normal line height, no letter
/// spacing, aligned to the left.
pub fn mac_style(size: f32) -> Style {
    Style {
        families: vec![MAC_FAMILY.to_string()],
        size,
        weight: 400.0,
        italic: false,
        line_height: LineHeight::Normal,
        letter_spacing: 0.0,
        align: Align::Left,
    }
}

/// The glyph of a single character laid out alone, which is its isolated form, and its glyph count.
fn alone(fonts: &mut Fonts, character: char) -> Vec<u32> {
    let block = layout(fonts, &character.to_string(), &mac_style(20.0), None)
        .unwrap_or_else(|error| panic!("{character:?} alone: {error}"));
    block
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.id))
        .collect()
}

/// The glyph each character of `chars` has when it is laid out alone, and the glyph the space has, as the names of the
/// glyphs of a paragraph. Two characters that share a glyph would make the reading back ambiguous, so that is refused.
pub fn glyph_names(fonts: &mut Fonts, chars: &str) -> HashMap<u32, char> {
    let mut names = HashMap::new();
    let space = space_glyph(fonts);
    names.insert(space, ' ');
    for character in chars.chars().filter(|character| !character.is_whitespace()) {
        let glyphs = alone(fonts, character);
        assert_eq!(glyphs.len(), 1, "{character:?} alone is one glyph: {glyphs:?}");
        let previous = names.insert(glyphs[0], character);
        assert!(
            previous.is_none() || previous == Some(character),
            "{character:?} and {previous:?} share the glyph {}",
            glyphs[0]
        );
    }
    names
}

/// The glyph of the space: the middle glyph of "a a".
pub fn space_glyph(fonts: &mut Fonts) -> u32 {
    let block = layout(fonts, "a a", &mac_style(20.0), None).expect("a space lays out");
    let glyphs: Vec<u32> = block.runs.iter().flat_map(|run| run.glyphs.iter().map(|glyph| glyph.id)).collect();
    assert_eq!(glyphs.len(), 3, "a space is one glyph between two letters: {glyphs:?}");
    glyphs[1]
}

/// The glyphs of each line of a block, in the order they are drawn from the left, grouped by the line's baseline.
pub fn line_glyphs(block: &Block) -> Vec<Vec<PositionedGlyph>> {
    block
        .lines
        .iter()
        .map(|line| {
            let mut glyphs: Vec<PositionedGlyph> = block
                .runs
                .iter()
                .flat_map(|run| run.glyphs.iter().copied())
                .filter(|glyph| (glyph.y - line.baseline).abs() < 0.01)
                .collect();
            glyphs.sort_by(|first, second| first.x.partial_cmp(&second.x).unwrap());
            glyphs
        })
        .collect()
}

/// The characters of each line, read back through the names of its glyphs, in the order they are drawn from the left.
pub fn line_text(block: &Block, names: &HashMap<u32, char>) -> Vec<String> {
    line_glyphs(block)
        .iter()
        .map(|glyphs| {
            glyphs
                .iter()
                .map(|glyph| {
                    *names
                        .get(&glyph.id)
                        .unwrap_or_else(|| panic!("glyph {} is none of the characters named", glyph.id))
                })
                .collect()
        })
        .collect()
}

/// The glyph ids of the whole block, in the order they are drawn from the left, line by line.
pub fn glyph_ids(block: &Block) -> Vec<Vec<u32>> {
    line_glyphs(block)
        .iter()
        .map(|glyphs| glyphs.iter().map(|glyph| glyph.id).collect())
        .collect()
}
