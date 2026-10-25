//! A paragraph made of pieces, each in its own style and colour, with room between them: the pieces share lines,
//! baselines and breaks, the white space across their edges collapses, and the room moves the words after it without
//! breaking the line (`docs/spec/text.md`, section 6.5).

mod common;

use common::{font_file, fonts, style};
use text::{layout, layout_pieces, Align, Block, LineHeight, Piece, PositionedGlyph, Style};

const BLACK: [u8; 4] = [0, 0, 0, 255];
const RED: [u8; 4] = [0xc0, 0x20, 0x20, 255];

/// Words in the test font at `size` pixels, drawn in `colour`.
fn words(text: &str, size: f32, colour: [u8; 4]) -> Piece {
    Piece::Words {
        words: text.to_string(),
        style: style(size),
        colour,
    }
}

/// Every glyph of a block, in the order of its runs.
fn glyphs(block: &Block) -> Vec<PositionedGlyph> {
    block.runs.iter().flat_map(|run| run.glyphs.iter().copied()).collect()
}

/// The glyphs of the runs drawn in `colour`.
fn glyphs_in(block: &Block, colour: [u8; 4]) -> Vec<PositionedGlyph> {
    block
        .runs
        .iter()
        .filter(|run| run.colour == colour)
        .flat_map(|run| run.glyphs.iter().copied())
        .collect()
}

/// The distinct baselines of a list of glyphs, to a thousandth of a pixel, in order.
fn baselines(glyphs: &[PositionedGlyph]) -> Vec<f32> {
    let mut ys: Vec<f32> = Vec::new();
    for glyph in glyphs {
        if !ys.iter().any(|y| (y - glyph.y).abs() < 1e-3) {
            ys.push(glyph.y);
        }
    }
    ys
}

#[test]
fn a_piece_is_drawn_in_its_own_colour_and_only_its_words() {
    let mut fonts = fonts();
    let block = layout_pieces(
        &mut fonts,
        &[words("HI ", 30.0, BLACK), words("II", 30.0, RED), words(" HH", 30.0, BLACK)],
        Align::Left,
        None,
    )
    .unwrap();
    // The space that lies between the black piece and the red one is drawn in the red piece too, where the red piece
    // begins. It has no ink, so the letters of the red piece are counted by the glyph of "I".
    let plain = layout(&mut fonts, "II", &style(30.0), None).unwrap();
    let letter_i = plain.runs[0].glyphs[0].id;
    let red_letters = glyphs_in(&block, RED).iter().filter(|glyph| glyph.id == letter_i).count();
    assert_eq!(red_letters, 2, "the letters of the red piece: {:?}", block.runs);
    assert!(
        block.runs.iter().any(|run| run.colour == BLACK),
        "the black words: {:?}",
        block.runs
    );
}

#[test]
fn pieces_of_different_sizes_share_one_baseline() {
    let mut fonts = fonts();
    let block = layout_pieces(
        &mut fonts,
        &[words("HI ", 20.0, BLACK), words("II", 44.0, RED), words(" HH", 20.0, BLACK)],
        Align::Left,
        None,
    )
    .unwrap();
    assert_eq!(block.lines.len(), 1, "{:?}", block.lines);
    let ys = baselines(&glyphs(&block));
    assert_eq!(ys.len(), 1, "the glyphs sit on more than one baseline: {ys:?}");
    assert!((ys[0] - block.lines[0].baseline).abs() < 1e-3, "{ys:?} {:?}", block.lines);
}

#[test]
fn a_larger_piece_makes_its_line_as_tall_as_the_larger_size_needs() {
    // A line with a 44 pixel piece is as tall as the 44 pixel text is alone, and taller than the 20 pixel text alone.
    let mut fonts = fonts();
    let mixed = layout_pieces(
        &mut fonts,
        &[words("HI ", 20.0, BLACK), words("II", 44.0, RED), words(" HH", 20.0, BLACK)],
        Align::Left,
        None,
    )
    .unwrap();
    let larger = layout(&mut fonts, "II", &style(44.0), None).unwrap();
    let smaller = layout(&mut fonts, "HI II HH", &style(20.0), None).unwrap();
    assert!((mixed.height - larger.height).abs() < 1e-3, "{} {}", mixed.height, larger.height);
    assert!(mixed.height > smaller.height + 20.0, "{} {}", mixed.height, smaller.height);
}

#[test]
fn a_piece_that_does_not_fit_breaks_with_the_paragraph() {
    let mut fonts = fonts();
    let first = layout(&mut fonts, "HI II", &style(20.0), None).unwrap().width;
    let block = layout_pieces(
        &mut fonts,
        &[words("HI ", 20.0, BLACK), words("II HH II", 20.0, RED), words(" HI", 20.0, BLACK)],
        Align::Left,
        Some(first + 1.0),
    )
    .unwrap();
    assert!(block.lines.len() >= 2, "{:?}", block.lines);
    let red_lines = baselines(&glyphs_in(&block, RED));
    assert!(red_lines.len() >= 2, "the red words sit on one line only: {red_lines:?}");
}

#[test]
fn white_space_across_the_edge_of_a_piece_collapses_to_one_space() {
    let mut fonts = fonts();
    let joined = layout(&mut fonts, "AB CD", &style(30.0), None).unwrap();
    let split = layout_pieces(
        &mut fonts,
        &[words("AB  ", 30.0, BLACK), words("  CD", 30.0, BLACK)],
        Align::Left,
        None,
    )
    .unwrap();
    let xs = |block: &Block| glyphs(block).iter().map(|glyph| glyph.x).collect::<Vec<f32>>();
    assert_eq!(xs(&split), xs(&joined));
    assert!((split.width - joined.width).abs() < 1e-3, "{} {}", split.width, joined.width);
}

#[test]
fn room_moves_the_words_after_it_and_leaves_the_line_height() {
    let mut fonts = fonts();
    let plain = layout_pieces(&mut fonts, &[words("AB", 30.0, BLACK), words("CD", 30.0, BLACK)], Align::Left, None).unwrap();
    let with_room = layout_pieces(
        &mut fonts,
        &[words("AB", 30.0, BLACK), Piece::Room { width: 20.0 }, words("CD", 30.0, BLACK)],
        Align::Left,
        None,
    )
    .unwrap();
    // Room after a letter adds no glyph, so the first and the last glyph are the letters A and D.
    let plain_glyphs = glyphs(&plain);
    let room_glyphs = glyphs(&with_room);
    let (first, last) = (plain_glyphs[0], plain_glyphs[plain_glyphs.len() - 1]);
    let (room_first, room_last) = (room_glyphs[0], room_glyphs[room_glyphs.len() - 1]);
    assert!((room_first.x - first.x).abs() < 1e-3, "the words before the room moved");
    assert!((room_last.x - last.x - 20.0).abs() < 1e-3, "{room_glyphs:?} {plain_glyphs:?}");
    assert!(
        (with_room.height - plain.height).abs() < 1e-3,
        "{} {}",
        with_room.height,
        plain.height
    );
}

#[test]
fn an_edge_of_a_piece_is_not_a_place_where_a_line_breaks() {
    // The room between "AB" and "CD" would overflow the width if it were a place to break, so the line must stay one
    // line: the words "ABCD" have no break between them (`docs/spec/text.md`, section 6.5).
    let mut fonts = fonts();
    let width = layout(&mut fonts, "AB", &style(30.0), None).unwrap().width + 10.0;
    let block = layout_pieces(
        &mut fonts,
        &[words("AB", 30.0, BLACK), Piece::Room { width: 30.0 }, words("CD", 30.0, BLACK)],
        Align::Left,
        Some(width),
    )
    .unwrap();
    assert_eq!(block.lines.len(), 1, "{:?}", block.lines);
}

#[test]
fn an_absent_family_in_one_piece_is_passed_over_for_the_next_of_its_list() {
    let mut fonts = fonts();
    let mut named = style(30.0);
    named.families = vec!["Nowhere".to_string(), common::FAMILY.to_string()];
    let pieces = [
        words("AB", 30.0, BLACK),
        Piece::Words {
            words: "CD".to_string(),
            style: named,
            colour: BLACK,
        },
    ];
    let block = layout_pieces(&mut fonts, &pieces, Align::Left, None).unwrap();
    let plain = layout_pieces(&mut fonts, &[words("ABCD", 30.0, BLACK)], Align::Left, None).unwrap();
    assert_eq!(block.width, plain.width);
    assert_eq!(block.missing, None);
}

#[test]
fn a_character_no_font_maps_in_one_piece_is_named_by_the_block() {
    // U+0378 is not assigned in Unicode, so neither the test font nor any font of the machine maps it: it is drawn as the
    // missing-character mark, and the block names it (`docs/spec/text.md`, section 2.4, point 3).
    let mut fonts = fonts();
    let pieces = [words("AB", 30.0, BLACK), words("\u{0378}", 30.0, RED)];
    let block = layout_pieces(&mut fonts, &pieces, Align::Left, None).unwrap();
    assert_eq!(block.missing, Some('\u{0378}'));
}

#[test]
fn pieces_in_pixel_line_heights_give_a_block_of_their_line_heights() {
    // Every piece of a paragraph is laid out in the line height of its own style; a block of two pieces with pixel
    // line heights is 50 pixels for each of its two lines.
    let mut fonts = fonts();
    let mut first = style(40.0);
    first.line_height = LineHeight::Pixels(50.0);
    let mut second = style(40.0);
    second.line_height = LineHeight::Pixels(50.0);
    let block = layout_pieces(
        &mut fonts,
        &[
            Piece::Words {
                words: "AB ".to_string(),
                style: first,
                colour: BLACK,
            },
            Piece::Words {
                words: "CD".to_string(),
                style: second,
                colour: RED,
            },
        ],
        Align::Left,
        Some(1.0),
    )
    .unwrap();
    assert!((block.height - 100.0).abs() < 0.01, "height {}", block.height);
}

#[test]
fn room_before_the_first_character_moves_the_words_after_it() {
    let mut fonts = fonts();
    let plain = layout_pieces(&mut fonts, &[words("CD", 30.0, BLACK)], Align::Left, None).unwrap();
    let with_room = layout_pieces(
        &mut fonts,
        &[Piece::Room { width: 20.0 }, words("CD", 30.0, BLACK)],
        Align::Left,
        None,
    )
    .unwrap();
    let plain_last = glyphs(&plain).last().copied().unwrap();
    let room_last = glyphs(&with_room).last().copied().unwrap();
    assert!((room_last.x - plain_last.x - 20.0).abs() < 1e-3, "{room_last:?} {plain_last:?}");
    assert!(
        (with_room.width - plain.width - 20.0).abs() < 1e-3,
        "{} {}",
        with_room.width,
        plain.width
    );
}

#[test]
fn room_after_a_space_keeps_the_line_break_at_that_space() {
    // The space before the room is a place where a line may end, as the rules of the line breaking give it: the words
    // "AB" and "CD" do not fit one line, so they break at the space, and the room goes with the space.
    let mut fonts = fonts();
    let width = layout(&mut fonts, "AB", &style(30.0), None).unwrap().width + 1.0;
    let block = layout_pieces(
        &mut fonts,
        &[words("AB ", 30.0, BLACK), Piece::Room { width: 20.0 }, words("CD", 30.0, BLACK)],
        Align::Left,
        Some(width),
    )
    .unwrap();
    assert_eq!(block.lines.len(), 2, "{:?}", block.lines);
}

#[test]
fn a_piece_that_writes_a_bold_weight_is_drawn_in_the_bold_face_and_its_neighbours_in_the_regular_face() {
    // A span that writes font-weight 700 is drawn in the Bold face of the test font, and the words beside it in the
    // Regular face. The faces are told apart by their bytes, which differ.
    let mut fonts = fonts();
    let bold = Piece::Words {
        words: "II".to_string(),
        style: Style {
            weight: 700.0,
            ..style(30.0)
        },
        colour: RED,
    };
    let block = layout_pieces(
        &mut fonts,
        &[words("HI ", 30.0, BLACK), bold, words(" HH", 30.0, BLACK)],
        Align::Left,
        None,
    )
    .unwrap();
    let regular = font_file("framesmith-test-stroke-400.ttf");
    let heavy = font_file("framesmith-test-stroke-700.ttf");
    assert!(regular != heavy, "the two faces of the test font are the same file");
    let red: Vec<_> = block.runs.iter().filter(|run| run.colour == RED).collect();
    assert!(!red.is_empty(), "{:?}", block.runs);
    assert!(
        red.iter().all(|run| run.font.as_slice() == heavy.as_slice()),
        "the bold words are not drawn in the Bold face"
    );
    let black: Vec<_> = block.runs.iter().filter(|run| run.colour == BLACK).collect();
    assert!(!black.is_empty(), "{:?}", block.runs);
    assert!(
        black.iter().all(|run| run.font.as_slice() == regular.as_slice()),
        "the regular words are not drawn in the Regular face"
    );
}
