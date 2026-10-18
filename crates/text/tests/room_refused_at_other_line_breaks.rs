//! The room of a span where a line may end before the span, after a character other than a space or a hyphen, is refused,
//! since no no-break space keeps the line end and the room together, and a room of 0 is not refused (`docs/spec/text.md`,
//! sections 6.5 and 10). The text is drawn with the machine's font, so the file is compiled for macOS only.

#![cfg(target_os = "macos")]

mod common;

use common::scripts::{mac_fonts, mac_style};
use text::{layout, layout_pieces, Align, Piece, TextError};

const BLACK: [u8; 4] = [0, 0, 0, 255];

/// Words in the machine's font at 30 pixels, drawn in black.
fn words(text: &str) -> Piece {
    Piece::Words {
        words: text.to_string(),
        style: mac_style(30.0),
        colour: BLACK,
    }
}

#[test]
fn room_between_two_ideographs_is_refused_at_the_piece_that_holds_it() {
    // A line may end between two ideographs (UAX 14, rule LB31). A no-break space after the first one keeps no break before
    // it (rule LB12a) and the room would move the line end, so the room cannot keep both its place and the break: it is refused.
    let mut fonts = mac_fonts();
    let width = layout(&mut fonts, "漢", &mac_style(30.0), None).unwrap().width + 1.0;
    let result = layout_pieces(
        &mut fonts,
        &[words("漢"), Piece::Lead { width: 12.0 }, words("字")],
        Align::Left,
        Some(width),
    );
    assert_eq!(
        result,
        Err(TextError::RoomAtALineBreak { piece: 1 }),
        "the room of the span that begins at the second ideograph is refused"
    );
}

#[test]
fn room_after_an_em_dash_at_a_break_is_refused() {
    // A line may end after an em dash (UAX 14, class B2), and a no-break space after it keeps no break before it (rule
    // LB12a), so the room is refused in the same way as the room between two ideographs.
    let mut fonts = mac_fonts();
    let width = layout(&mut fonts, "AB—", &mac_style(30.0), None).unwrap().width + 1.0;
    let result = layout_pieces(
        &mut fonts,
        &[words("AB—"), Piece::Lead { width: 12.0 }, words("CD")],
        Align::Left,
        Some(width),
    );
    assert_eq!(result, Err(TextError::RoomAtALineBreak { piece: 1 }));
}

#[test]
fn a_room_of_0_between_two_ideographs_is_not_refused() {
    // A room of 0 holds no place: it is not carried, and the text is laid out as the text without it.
    let mut fonts = mac_fonts();
    let width = layout(&mut fonts, "漢", &mac_style(30.0), None).unwrap().width + 1.0;
    let plain = layout_pieces(&mut fonts, &[words("漢字")], Align::Left, Some(width)).unwrap();
    let with_room = layout_pieces(
        &mut fonts,
        &[words("漢"), Piece::Lead { width: 0.0 }, words("字")],
        Align::Left,
        Some(width),
    )
    .unwrap();
    assert_eq!(with_room, plain, "a room of 0 changes nothing");
}
