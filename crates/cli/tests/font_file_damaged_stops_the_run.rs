//! A `<font>` whose file is damaged, with a TrueType tag and a table directory cut short, stops `framesmith check` and
//! `framesmith render` before the first frame: the report gives the element's line and the path as written, both commands
//! exit with 1, and no film is written. The reason the font reader gives is not printed yet, so the reason is not asserted
//! here.

mod common;
mod font_file;

use font_file::{check_and_render, composition, refused_the_same_way, test_font};

#[test]
fn a_truetype_file_cut_short_stops_check_and_render_with_its_line_and_its_path() {
    let damaged = test_font()[..64].to_vec();
    let (check, render) = check_and_render("font-file-damaged", "cut-short", &composition("fonts/damaged.ttf"), &[("fonts/damaged.ttf", damaged)]);
    let line = refused_the_same_way(&check, &render, 1);
    assert!(line.starts_with("composition.xml, line 2: FS-0051: invalid:"), "{line}");
    assert!(line.contains("`src=\"fonts/damaged.ttf\"` is not a font of a kind this version takes"), "{line}");
}
