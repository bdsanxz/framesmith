//! The kinds of font file the spec lists are read, and the kinds it does not list are refused (`docs/spec/text.md`, section
//! 2.1). The files are the test font of the repository, whose first four bytes are changed where a kind needs another tag.

use std::fs;
use std::path::PathBuf;

use text::{FontError, Fonts};

const FAMILY: &str = "Framesmith Test Stroke";

fn test_font_bytes() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts/framesmith-test-stroke-400.ttf");
    fs::read(path).unwrap()
}

/// The test font's bytes with its first four bytes replaced by `tag`.
fn with_tag(tag: &[u8; 4]) -> Vec<u8> {
    let mut bytes = test_font_bytes();
    bytes[..4].copy_from_slice(tag);
    bytes
}

#[test]
fn a_truetype_file_whose_tag_is_0x00010000_is_read_as_a_face_of_its_family() {
    let mut fonts = Fonts::new();
    fonts.register(test_font_bytes(), Some(FAMILY)).unwrap();
    assert!(fonts.has_family(FAMILY));
}

#[test]
fn a_truetype_file_whose_tag_is_true_is_read_as_a_face_of_its_family() {
    let mut fonts = Fonts::new();
    fonts.register(with_tag(b"true"), Some(FAMILY)).unwrap();
    assert!(fonts.has_family(FAMILY));
}

#[test]
fn an_opentype_file_with_postscript_outlines_is_refused_as_a_kind_this_version_does_not_draw() {
    let mut fonts = Fonts::new();
    let error = fonts.register(with_tag(b"OTTO"), Some(FAMILY)).unwrap_err();
    assert_eq!(error, FontError::Unsupported("OpenType with PostScript outlines"));
    assert!(!fonts.has_family(FAMILY));
}

#[test]
fn a_woff2_file_is_refused_as_a_kind_this_version_does_not_draw() {
    let mut fonts = Fonts::new();
    let error = fonts.register(with_tag(b"wOF2"), Some(FAMILY)).unwrap_err();
    assert_eq!(error, FontError::Unsupported("WOFF2"));
    assert!(!fonts.has_family(FAMILY));
}

#[test]
fn a_file_whose_first_four_bytes_name_no_font_kind_is_not_a_font() {
    let mut fonts = Fonts::new();
    let error = fonts.register(b"These bytes are not a font.\n".to_vec(), Some(FAMILY)).unwrap_err();
    assert_eq!(error, FontError::NotAFont);
    assert!(fonts.is_empty());
}
