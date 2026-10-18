//! The room of a span at the edge of its words: the room is carried where the line breaker allows a line to end before
//! the span, so that it moves to the line the span begins (a space or a hyphen before it, with a no-break space), and
//! where no line may end, so that the lines of the text stay as they are (a word joiner). The word joiner carries the
//! room before the first character of a text and is never drawn, two rooms at one edge add up, and a text with no word
//! refuses its room (`docs/spec/text.md`, sections 6.5 and 10).

mod common;

use common::{fonts, style};
use text::{layout, layout_pieces, Align, Block, Piece, PositionedGlyph, TextError};

const BLACK: [u8; 4] = [0, 0, 0, 255];

/// Words in the test font at `size` pixels, drawn in black.
fn words(text: &str, size: f32) -> Piece {
    Piece::Words {
        words: text.to_string(),
        style: style(size),
        colour: BLACK,
    }
}

/// Every glyph of a block, in the order of its runs.
fn glyphs(block: &Block) -> Vec<PositionedGlyph> {
    block.runs.iter().flat_map(|run| run.glyphs.iter().copied()).collect()
}

#[test]
fn room_before_the_first_character_of_a_text_moves_the_words_and_draws_no_glyph() {
    let mut fonts = fonts();
    let plain = layout_pieces(&mut fonts, &[words("CD", 30.0)], Align::Left, None).unwrap();
    let with_room = layout_pieces(&mut fonts, &[Piece::Lead { width: 12.0 }, words("CD", 30.0)], Align::Left, None).unwrap();
    let plain_glyphs = glyphs(&plain);
    let room_glyphs = glyphs(&with_room);
    // The carrier of the room is never drawn: the block has the same two glyphs, and only they move.
    assert_eq!(room_glyphs.len(), plain_glyphs.len(), "{room_glyphs:?} {plain_glyphs:?}");
    assert!(
        (room_glyphs[0].x - plain_glyphs[0].x - 12.0).abs() < 1e-3,
        "{room_glyphs:?} {plain_glyphs:?}"
    );
    assert!(
        (with_room.width - plain.width - 12.0).abs() < 1e-3,
        "{} {}",
        with_room.width,
        plain.width
    );
    assert!(
        (with_room.height - plain.height).abs() < 1e-3,
        "{} {}",
        with_room.height,
        plain.height
    );
}

#[test]
fn room_after_a_space_at_a_line_break_goes_to_the_line_the_span_begins() {
    // The space after "AB" is a place where the line may end, and the words do not fit one line, so the line breaks at
    // that space. The room of the span that begins "CD" is kept on the second line, where the span begins.
    let mut fonts = fonts();
    let width = layout(&mut fonts, "AB", &style(30.0), None).unwrap().width + 1.0;
    let with_room = layout_pieces(
        &mut fonts,
        &[words("AB ", 30.0), Piece::Lead { width: 12.0 }, words("CD", 30.0)],
        Align::Left,
        Some(width),
    )
    .unwrap();
    let without = layout_pieces(&mut fonts, &[words("AB ", 30.0), words("CD", 30.0)], Align::Left, Some(width)).unwrap();
    assert_eq!(with_room.lines.len(), 2, "{:?}", with_room.lines);
    assert_eq!(without.lines.len(), 2, "{:?}", without.lines);
    assert!(
        (with_room.lines[0].width - without.lines[0].width).abs() < 1e-3,
        "the first line holds the room: {:?} {:?}",
        with_room.lines,
        without.lines
    );
    assert!(
        with_room.lines[1].left.abs() < 1e-3,
        "the second line begins at the left edge: {:?}",
        with_room.lines
    );
    let room_glyphs = glyphs(&with_room);
    let plain_glyphs = glyphs(&without);
    // The glyphs are the letters A, B, the space, C and D: the carrier adds none.
    assert_eq!(room_glyphs.len(), 5, "{room_glyphs:?}");
    assert_eq!(plain_glyphs.len(), 5, "{plain_glyphs:?}");
    let (room_c, plain_c) = (room_glyphs[3], plain_glyphs[3]);
    assert!(
        (room_c.x - plain_c.x - 12.0).abs() < 1e-3,
        "the first letter of the span is moved by the room: {room_c:?} {plain_c:?}"
    );
    assert!(
        (with_room.height - without.height).abs() < 1e-3,
        "the room leaves the line heights: {} {}",
        with_room.height,
        without.height
    );
}

#[test]
fn two_rooms_at_one_edge_add_up_on_the_line_the_span_begins() {
    let mut fonts = fonts();
    let plain = layout_pieces(&mut fonts, &[words("AB ", 30.0), words("CD", 30.0)], Align::Left, None).unwrap();
    let with_rooms = layout_pieces(
        &mut fonts,
        &[
            words("AB ", 30.0),
            Piece::Lead { width: 5.0 },
            Piece::Lead { width: 10.0 },
            words("CD", 30.0),
        ],
        Align::Left,
        None,
    )
    .unwrap();
    let plain_c = glyphs(&plain)[3];
    let room_c = glyphs(&with_rooms)[3];
    assert!(
        (room_c.x - plain_c.x - 15.0).abs() < 1e-3,
        "the rooms are 5 and 10 pixels: {room_c:?} {plain_c:?}"
    );
    assert!(
        (with_rooms.width - plain.width - 15.0).abs() < 1e-3,
        "{} {}",
        with_rooms.width,
        plain.width
    );
}

#[test]
fn room_in_a_text_with_no_word_is_refused_and_room_with_a_word_is_not() {
    let mut fonts = fonts();
    let refused: [Vec<Piece>; 3] = [
        vec![Piece::Lead { width: 12.0 }, Piece::Room { width: 12.0 }],
        vec![words("", 30.0), Piece::Lead { width: 12.0 }],
        vec![words("   ", 30.0), Piece::Lead { width: 12.0 }],
    ];
    for pieces in refused {
        let result = layout_pieces(&mut fonts, &pieces, Align::Left, None);
        assert!(matches!(result, Err(TextError::RoomWithoutWords)), "{pieces:?} gave {result:?}");
    }
    let with_word = layout_pieces(&mut fonts, &[words("A", 30.0), Piece::Lead { width: 12.0 }], Align::Left, None);
    assert!(with_word.is_ok(), "a text with a word draws its room: {with_word:?}");
}

#[test]
fn room_after_a_hyphen_at_a_break_is_drawn_at_the_start_of_the_line_the_span_begins() {
    // The line may break after the hyphen (UAX 14, rule LB21 and LB12a allow a break before a no-break space after HY), so
    // the no-break space that carries the room keeps the break where the text without the room has it, and the room
    // moves the letter C to 12 pixels on the second line (`docs/spec/text.md`, section 10).
    let mut fonts = fonts();
    let width = layout(&mut fonts, "AB-", &style(30.0), None).unwrap().width + 1.0;
    let plain = layout_pieces(&mut fonts, &[words("AB-CD", 30.0)], Align::Left, Some(width)).unwrap();
    let block = layout_pieces(
        &mut fonts,
        &[words("AB-", 30.0), Piece::Lead { width: 12.0 }, words("CD", 30.0)],
        Align::Left,
        Some(width),
    )
    .unwrap();
    assert_eq!(block.lines.len(), 2, "{:?}", block.lines);
    assert_eq!(
        block.lines.len(),
        plain.lines.len(),
        "the room keeps the lines of the text without it: {:?}",
        block.lines
    );
    assert!(
        (block.lines[0].width - plain.lines[0].width).abs() < 1e-3,
        "the first line keeps its width without the room: {:?} {:?}",
        block.lines,
        plain.lines
    );
    assert!(
        (block.lines[1].width - plain.lines[1].width - 12.0).abs() < 1e-3,
        "the second line takes the room: {:?} {:?}",
        block.lines,
        plain.lines
    );
    let c = glyphs(&block)[3];
    assert!((c.x - 12.0).abs() < 1e-3, "the letter C begins the second line 12 pixels in: {c:?}");
}

#[test]
fn room_after_a_hyphen_before_a_digit_keeps_the_lines_of_the_text_without_it() {
    // A hyphen and a digit after it are kept together (UAX 14, rule LB25), so no line may end at the place of the room:
    // the word joiner carries the room, which moves the digit 12 pixels on the one line, as the text without it has it.
    let mut fonts = fonts();
    let width = layout(&mut fonts, "AB-", &style(30.0), None).unwrap().width + 1.0;
    let plain = layout_pieces(&mut fonts, &[words("AB-1", 30.0)], Align::Left, Some(width)).unwrap();
    let block = layout_pieces(
        &mut fonts,
        &[words("AB-", 30.0), Piece::Lead { width: 12.0 }, words("1", 30.0)],
        Align::Left,
        Some(width),
    )
    .unwrap();
    assert_eq!(plain.lines.len(), 1, "{:?}", plain.lines);
    assert_eq!(block.lines.len(), plain.lines.len(), "{:?}", block.lines);
    let digit = glyphs(&block)[3];
    let plain_digit = glyphs(&plain)[3];
    assert!(
        (digit.x - plain_digit.x - 12.0).abs() < 1e-3,
        "the digit moves 12 pixels: {digit:?} {plain_digit:?}"
    );
}

#[test]
fn an_empty_span_between_words_keeps_its_room_as_css_keeps_an_empty_inline_box() {
    // The room of a span with no words is its left and right space together, and the words after it move on by both.
    let mut fonts = fonts();
    let plain = layout_pieces(&mut fonts, &[words("AB ", 30.0), words("CD", 30.0)], Align::Left, None).unwrap();
    let with_room = layout_pieces(
        &mut fonts,
        &[
            words("AB ", 30.0),
            Piece::Lead { width: 12.0 },
            Piece::Room { width: 12.0 },
            words("CD", 30.0),
        ],
        Align::Left,
        None,
    )
    .unwrap();
    let plain_c = glyphs(&plain)[3];
    let room_c = glyphs(&with_room)[3];
    assert!(
        (room_c.x - plain_c.x - 24.0).abs() < 1e-3,
        "the empty span takes 24 pixels: {room_c:?} {plain_c:?}"
    );
    assert_eq!(with_room.lines.len(), 1, "{:?}", with_room.lines);
}

#[test]
fn a_room_before_a_full_stop_after_a_space_keeps_the_lines_of_the_text_without_it() {
    // A full stop is kept with the space before it (UAX 14, rule LB13), so no line may end before it, and the word joiner
    // carries the room: the text stays on one line, and the full stop moves 12 pixels as it does with a room of 12.
    let mut fonts = fonts();
    let width = layout(&mut fonts, "AB", &style(30.0), None).unwrap().width + 1.0;
    let plain = layout_pieces(&mut fonts, &[words("AB ", 30.0), words(".CD", 30.0)], Align::Left, Some(width)).unwrap();
    let with_room = layout_pieces(
        &mut fonts,
        &[words("AB ", 30.0), Piece::Lead { width: 12.0 }, words(".CD", 30.0)],
        Align::Left,
        Some(width),
    )
    .unwrap();
    assert_eq!(
        plain.lines.len(),
        1,
        "without the room the full stop stays with the space: {:?}",
        plain.lines
    );
    assert_eq!(
        with_room.lines.len(),
        plain.lines.len(),
        "with the room the lines are those of the text without it: {:?}",
        with_room.lines
    );
    let stop = glyphs(&with_room)[3];
    let plain_stop = glyphs(&plain)[3];
    assert!(
        (stop.x - plain_stop.x - 12.0).abs() < 1e-3,
        "the full stop moves 12 pixels: {stop:?} {plain_stop:?}"
    );
}
