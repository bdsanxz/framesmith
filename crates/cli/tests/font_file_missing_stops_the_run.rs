//! A `<font>` whose file is missing stops `framesmith check` and `framesmith render` before the first frame: the report gives
//! the element's line, the path as written and the reason, both commands exit with 1, and no film is written.

mod common;
mod font_file;

use font_file::{check_and_render, composition, refused_the_same_way};

#[test]
fn a_font_file_that_is_missing_stops_check_and_render_with_its_line_its_path_and_the_reason() {
    let (check, render) = check_and_render("font-file-missing", "gone", &composition("fonts/gone.ttf"), &[]);
    let line = refused_the_same_way(&check, &render, 1);
    assert!(line.starts_with("composition.xml, line 2: FS-0049: invalid:"), "{line}");
    assert!(line.contains("`src=\"fonts/gone.ttf\"` names no file"), "{line}");
}
