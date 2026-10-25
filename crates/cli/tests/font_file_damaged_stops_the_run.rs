//! A `<font>` whose file is damaged, a TrueType file the font reader refuses, stops `framesmith check` and `framesmith render`
//! before the first frame: the report gives the element's line, the path as written and the reader's reason, both commands
//! exit with 1, and no film is written (`docs/spec/text.md`, section 2.1).

mod common;
mod font_file;

use font_file::{check_and_render, composition, refused_the_same_way, test_font};

#[test]
fn a_truetype_file_cut_short_stops_check_and_render_with_its_line_its_path_and_the_reader_s_reason() {
    let damaged = test_font()[..300].to_vec();
    let (check, render) = check_and_render(
        "font-file-damaged",
        "cut-short",
        &composition("fonts/damaged.ttf"),
        &[("fonts/damaged.ttf", damaged)],
    );
    let line = refused_the_same_way(&check, &render, 1);
    assert!(line.starts_with("composition.xml, line 2: FS-0057: invalid:"), "{line}");
    assert!(
        line.contains("`src=\"fonts/damaged.ttf\"` is damaged: the font reader refuses it: the table "),
        "{line}"
    );
    assert!(line.contains("lies beyond the end of the file of 300 bytes"), "{line}");
}

#[test]
fn a_truetype_file_with_a_glyph_beyond_its_table_stops_check_and_render_with_the_reader_s_reason() {
    let mut damaged = test_font();
    // The record of `loca` in the table directory: its offset, then the end of glyph 1, put past every table.
    let tables = u16::from_be_bytes([damaged[4], damaged[5]]) as usize;
    let at = (0..tables)
        .map(|number| 12 + 16 * number)
        .find(|&at| &damaged[at..at + 4] == b"loca")
        .unwrap();
    let loca = u32::from_be_bytes(damaged[at + 8..at + 12].try_into().unwrap()) as usize;
    damaged[loca + 2..loca + 4].copy_from_slice(&u16::MAX.to_be_bytes());
    damaged[loca + 4..loca + 6].copy_from_slice(&u16::MAX.to_be_bytes());
    let (check, render) = check_and_render(
        "font-file-damaged",
        "glyph",
        &composition("fonts/damaged.ttf"),
        &[("fonts/damaged.ttf", damaged)],
    );
    let line = refused_the_same_way(&check, &render, 1);
    assert!(line.starts_with("composition.xml, line 2: FS-0057: invalid:"), "{line}");
    assert!(
        line.contains("`src=\"fonts/damaged.ttf\"` is damaged: the font reader refuses it: glyph 0: An offset was out of bounds"),
        "{line}"
    );
}
