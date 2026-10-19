//! A `<font>` written anywhere but directly in the root of the composition is reported with its line, by `framesmith check`
//! and by `framesmith render`, which refuse the project with exit 1 and write no film (`docs/spec/text.md`, section 2.1;
//! `docs/spec/notation.md`, section 6.12).

mod common;
mod font_file;

use font_file::{check_and_render, refused_the_same_way, FAMILY};

#[test]
fn a_font_inside_a_box_is_reported_with_its_line() {
    let body = format!(
        "<composition version=\"1\" width=\"200\" height=\"60\" fps=\"10\" duration=\"1\" background=\"#ffffff\">\n\
         <box left=\"0\" top=\"0\" width=\"200\" height=\"60\">\n\
         <font src=\"fonts/f.ttf\" family=\"{FAMILY}\"/>\n\
         </box>\n\
         </composition>\n"
    );
    let (check, render) = check_and_render("font-outside-root", "in-box", &body, &[]);
    let line = refused_the_same_way(&check, &render, 1);
    assert!(line.starts_with("composition.xml, line 3: FS-0031: invalid:"), "{line}");
    assert!(line.contains("<font>"), "{line}");
}

#[test]
fn a_font_inside_a_text_is_reported_with_its_line() {
    let body = format!(
        "<composition version=\"1\" width=\"200\" height=\"60\" fps=\"10\" duration=\"1\" background=\"#ffffff\">\n\
         <text left=\"10\" top=\"12\" font-family=\"{FAMILY}\" font-size=\"30\" color=\"#202020\">HI<font src=\"fonts/f.ttf\" family=\"{FAMILY}\"/>H</text>\n\
         </composition>\n"
    );
    let (check, render) = check_and_render("font-outside-root", "in-text", &body, &[]);
    let line = refused_the_same_way(&check, &render, 1);
    assert!(line.starts_with("composition.xml, line 2: FS-0031: invalid:"), "{line}");
    assert!(line.contains("<font>"), "{line}");
}
