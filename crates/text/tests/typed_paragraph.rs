//! A paragraph typed out (`docs/spec/text.md`, section 7.2): each piece draws its first characters only, the layout is
//! the whole paragraph's, and a glyph drawn moves left by the advances of the glyphs not drawn before it on its line.
//! Every expected place is read from the layout of the whole paragraph, where every glyph is drawn: a glyph's advance
//! is the distance to the next glyph of its line.

mod common;

use common::{fonts, style};
use text::{layout_pieces, Align, Block, Piece};

const BLACK: [u8; 4] = [0, 0, 0, 255];
const RED: [u8; 4] = [0xc0, 0x20, 0x20, 255];

fn words(text: &str, colour: [u8; 4]) -> Piece {
    Piece::Words {
        words: text.to_string(),
        style: style(24.0),
        colour,
    }
}

/// Every glyph of a block as (glyph, x, y), in the order of its runs.
fn placed(block: &Block) -> Vec<(u32, f32, f32)> {
    block
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter().map(|glyph| (glyph.id, glyph.x, glyph.y)))
        .collect()
}

#[test]
fn the_characters_of_each_piece_are_counted_after_the_white_space_is_collapsed() {
    let mut fonts = fonts();
    // "HELLO" (5); the space between the pieces is the second piece's, where it begins: " WORLD" (6); " END" (4).
    let block = layout_pieces(
        &mut fonts,
        &[words("  HELLO   ", BLACK), words("WORLD", RED), words("  END ", BLACK)],
        Align::Left,
        None,
    )
    .unwrap();
    assert_eq!(block.characters, vec![5, 6, 4]);
}

#[test]
fn a_typed_block_draws_the_first_characters_of_each_piece_where_the_whole_block_has_them() {
    let mut fonts = fonts();
    let block = layout_pieces(&mut fonts, &[words("TYPED TEXT", BLACK)], Align::Left, None).unwrap();
    let whole = placed(&block);
    assert_eq!(whole.len(), 10);
    for shown in 0..=10 {
        let typed = block.typed(&[shown]);
        // The characters already shown keep their places as more appear; the size and the lines are the whole block's.
        assert_eq!(placed(&typed), whole[..shown], "{shown} shown");
        assert_eq!((typed.width, typed.height, &typed.lines), (block.width, block.height, &block.lines));
    }
    assert_eq!(block.typed(&[usize::MAX]), block);
}

#[test]
fn a_centred_line_keeps_its_place_as_it_is_typed() {
    let mut fonts = fonts();
    let block = layout_pieces(&mut fonts, &[words("CENTRE", BLACK)], Align::Center, Some(200.0)).unwrap();
    let whole = placed(&block);
    // The line stands in the middle of 200 pixels as the whole word: its first glyph is not at the left edge.
    assert!(whole[0].1 > 20.0, "{whole:?}");
    assert_eq!(placed(&block.typed(&[3])), whole[..3]);
}

#[test]
fn the_words_after_a_piece_not_yet_typed_close_up_by_its_hidden_advances() {
    let mut fonts = fonts();
    let block = layout_pieces(
        &mut fonts,
        &[words("HELLO", BLACK), words(" WORLD", RED), words(" END", BLACK)],
        Align::Left,
        None,
    )
    .unwrap();
    let whole = placed(&block);
    assert_eq!(whole.len(), 15);
    let advance = |index: usize| whole[index + 1].1 - whole[index].1;
    // The red piece shows " W" (2 of its 6); its "ORLD", glyphs 7 to 10, are not drawn, so " END" moves left by their
    // advances.
    let hidden: f32 = (7..11).map(advance).sum();
    let typed = placed(&block.typed(&[5, 2, 4]));
    let mut want: Vec<(u32, f32, f32)> = whole[..7].to_vec();
    want.extend(whole[11..].iter().map(|&(id, x, y)| (id, x - hidden, y)));
    assert_eq!(typed.len(), want.len());
    for (got, want) in typed.iter().zip(&want) {
        assert_eq!(got.0, want.0);
        assert!((got.1 - want.1).abs() < 1e-3 && got.2 == want.2, "{got:?} against {want:?}");
    }
}

#[test]
fn a_piece_typed_to_none_draws_nothing_and_the_others_close_up() {
    let mut fonts = fonts();
    let block = layout_pieces(&mut fonts, &[words("AB", BLACK), words(" CD", RED)], Align::Left, None).unwrap();
    let whole = placed(&block);
    let typed = placed(&block.typed(&[0, 3]));
    // "AB" is not drawn, and " CD" moves left by the advances of A and B: it starts where A started.
    let shift = whole[2].1 - whole[0].1;
    let want: Vec<(u32, f32)> = whole[2..].iter().map(|&(id, x, _)| (id, x - shift)).collect();
    let got: Vec<(u32, f32)> = typed.iter().map(|&(id, x, _)| (id, x)).collect();
    assert_eq!(got, want);
}

#[test]
fn each_line_closes_up_on_its_own() {
    let mut fonts = fonts();
    // Two lines at 24 pixels wrapped at 100: "AAAA" and "BBBB". With the first piece shown to 2, the second line is not
    // moved by the hidden glyphs of the first.
    let block = layout_pieces(&mut fonts, &[words("AAAA ", BLACK), words("BBBB", RED)], Align::Left, Some(100.0)).unwrap();
    assert_eq!(block.lines.len(), 2);
    let whole = placed(&block);
    let typed = placed(&block.typed(&[2, usize::MAX]));
    let second: Vec<_> = whole.iter().filter(|glyph| glyph.2 == whole.last().unwrap().2).copied().collect();
    let typed_second: Vec<_> = typed.iter().filter(|glyph| glyph.2 == second[0].2).copied().collect();
    assert_eq!(typed_second, second);
}
