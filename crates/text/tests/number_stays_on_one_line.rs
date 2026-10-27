//! A number is one word with no place to break, so it stays on one line even at a width narrower than itself
//! (`docs/spec/text.md`, section 7.1). A counting text lays out each number this way.

mod common;

use common::{fonts, style};
use text::{layout_pieces, Align, Piece};

const BLACK: [u8; 4] = [0, 0, 0, 255];

#[test]
fn a_number_is_one_word_and_stays_on_one_line_at_a_narrow_width() {
    let mut fonts = fonts();
    for number in ["12,345.68", "0.00", "6:05.0", "100"] {
        let piece = Piece::Words {
            words: number.to_string(),
            style: style(24.0),
            colour: BLACK,
        };
        let block = layout_pieces(&mut fonts, &[piece], Align::Left, Some(10.0)).unwrap();
        assert_eq!(block.lines.len(), 1, "{number}");
    }
}
