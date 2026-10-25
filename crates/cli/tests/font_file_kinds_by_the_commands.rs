//! The kinds of font file that `docs/spec/text.md` (section 2.1) lists are read by `framesmith check` and `framesmith render`,
//! and a kind it does not list is reported with the file as written and the kinds taken, by both commands with the same
//! report.

mod common;
mod font_file;

use font_file::{check_and_render, composition, refused_the_same_way, test_font};

const KINDS_TAKEN: &str = "The kinds taken are TrueType (`.ttf`) and font collection (`.ttc`).";

#[test]
fn a_truetype_file_whose_tag_is_true_is_read_and_drawn_by_check_and_by_render() {
    let mut font = test_font();
    font[..4].copy_from_slice(b"true");
    let (check, render) = check_and_render("font-file-kinds", "true-tag", &composition("fonts/f.ttf"), &[("fonts/f.ttf", font)]);
    assert_eq!(check.status, Some(0), "check reads the file: {}", check.printed);
    assert_eq!(render.status, Some(0), "render reads the file: {}", render.printed);
    assert!(render.film, "render writes the film: {}", render.printed);
}

#[test]
fn an_opentype_file_with_postscript_outlines_is_reported_with_its_file_and_the_kinds_taken() {
    let mut font = test_font();
    font[..4].copy_from_slice(b"OTTO");
    let (check, render) = check_and_render("font-file-kinds", "otto", &composition("fonts/f.ttf"), &[("fonts/f.ttf", font)]);
    let line = refused_the_same_way(&check, &render, 2);
    assert!(line.starts_with("composition.xml, line 2: FS-0035: unsupported:"), "{line}");
    assert!(
        line.contains("`src=\"fonts/f.ttf\"` is OpenType with PostScript outlines"),
        "{line}"
    );
    assert!(line.contains(KINDS_TAKEN), "{line}");
}

#[test]
fn a_woff2_file_is_reported_with_its_file_and_the_kinds_taken() {
    let mut font = test_font();
    font[..4].copy_from_slice(b"wOF2");
    let (check, render) = check_and_render("font-file-kinds", "woff2", &composition("fonts/f.ttf"), &[("fonts/f.ttf", font)]);
    let line = refused_the_same_way(&check, &render, 2);
    assert!(line.starts_with("composition.xml, line 2: FS-0035: unsupported:"), "{line}");
    assert!(line.contains("`src=\"fonts/f.ttf\"` is WOFF2"), "{line}");
    assert!(line.contains(KINDS_TAKEN), "{line}");
}

#[test]
fn a_file_of_no_font_kind_is_reported_with_its_file_and_the_kinds_taken() {
    let notes = b"These bytes are not a font.\n".to_vec();
    let (check, render) = check_and_render(
        "font-file-kinds",
        "not-a-font",
        &composition("fonts/notes.txt"),
        &[("fonts/notes.txt", notes)],
    );
    let line = refused_the_same_way(&check, &render, 1);
    assert!(line.starts_with("composition.xml, line 2: FS-0051: invalid:"), "{line}");
    assert!(
        line.contains("`src=\"fonts/notes.txt\"` is not a font of a kind this version takes"),
        "{line}"
    );
    assert!(line.contains(KINDS_TAKEN), "{line}");
}

#[test]
fn a_font_file_fault_is_reported_once_and_the_text_of_its_family_is_not_reported_again() {
    for (name, src, bytes) in [
        ("once-not-a-font", "fonts/notes.txt", b"These bytes are not a font.\n".to_vec()),
        ("once-woff2", "fonts/f.ttf", {
            let mut font = test_font();
            font[..4].copy_from_slice(b"wOF2");
            font
        }),
    ] {
        let (check, render) = check_and_render("font-file-kinds", name, &composition(src), &[(src, bytes)]);
        assert_eq!(check.lines.len(), 1, "one report, on the <font> line: {}", check.printed);
        assert_eq!(render.lines.len(), 1, "one report, on the <font> line: {}", render.printed);
        assert!(check.lines[0].starts_with("composition.xml, line 2: "), "{}", check.lines[0]);
    }
}
