//! A file that begins as a font of a kind this version reads, and that the font reader refuses, is damaged: it is not
//! registered, and the error gives the reader's reason (`docs/spec/text.md`, section 2.1). The files are the test font of
//! the repository, cut short or with one number of its tables changed.

use std::fs;
use std::path::PathBuf;

use text::{FontError, Fonts};

const FAMILY: &str = "Framesmith Test Stroke";

fn test_font_bytes() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts/framesmith-test-stroke-400.ttf");
    fs::read(path).unwrap()
}

fn be32(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap())
}

/// The place in the file of the table directory's record of `tag`: its tag, checksum, offset and length.
fn record(bytes: &[u8], tag: &[u8; 4]) -> usize {
    let tables = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
    (0..tables)
        .map(|number| 12 + 16 * number)
        .find(|&at| &bytes[at..at + 4] == tag)
        .unwrap()
}

/// The reason the reader gives for `bytes`, which must be refused as damaged and leave nothing registered.
fn damage(bytes: Vec<u8>) -> String {
    let mut fonts = Fonts::new();
    let error = fonts.register(bytes, Some(FAMILY)).unwrap_err();
    assert!(fonts.is_empty(), "a damaged file registers no face");
    match error {
        FontError::Damaged(reason) => reason,
        other => panic!("refused as damaged, not as {other:?}"),
    }
}

#[test]
fn the_whole_test_font_is_read_by_the_font_reader() {
    let mut fonts = Fonts::new();
    fonts.register(test_font_bytes(), Some(FAMILY)).unwrap();
    assert!(fonts.has_family(FAMILY));
}

#[test]
fn a_file_cut_inside_its_table_directory_is_damaged_and_says_so() {
    let reason = damage(test_font_bytes()[..64].to_vec());
    assert_eq!(reason, "its table directory of 11 tables runs past the end of the file of 64 bytes");
}

#[test]
fn a_file_cut_inside_its_tables_is_damaged_and_names_the_table_beyond_the_end() {
    let bytes = test_font_bytes()[..300].to_vec();
    let reason = damage(bytes);
    assert!(reason.starts_with("the table "), "{reason}");
    assert!(reason.ends_with("lies beyond the end of the file of 300 bytes"), "{reason}");
}

#[test]
fn a_head_table_shorter_than_its_fields_is_damaged_with_the_reader_s_reason() {
    let mut bytes = test_font_bytes();
    let at = record(&bytes, b"head");
    bytes[at + 12..at + 16].copy_from_slice(&10u32.to_be_bytes());
    assert_eq!(damage(bytes), "the table head: An offset was out of bounds");
}

#[test]
fn a_glyph_whose_outline_lies_beyond_the_glyf_table_is_damaged_and_names_the_glyph() {
    let mut bytes = test_font_bytes();
    let glyf_length = be32(&bytes, record(&bytes, b"glyf") + 12);
    let loca = be32(&bytes, record(&bytes, b"loca") + 8) as usize;
    let head = be32(&bytes, record(&bytes, b"head") + 8) as usize;
    // indexToLocFormat, at byte 50 of `head`: 0 for offsets of 16 bits (halved), 1 for offsets of 32 bits.
    let long = bytes[head + 51] == 1;
    // The end of glyph 1 is put past the end of `glyf`.
    if long {
        bytes[loca + 8..loca + 12].copy_from_slice(&(glyf_length + 400).to_be_bytes());
    } else {
        bytes[loca + 4..loca + 6].copy_from_slice(&(((glyf_length + 400) / 2) as u16).to_be_bytes());
    }
    assert_eq!(damage(bytes), "glyph 1: An offset was out of bounds");
}

#[test]
fn a_collection_cut_inside_its_header_is_damaged_and_says_so() {
    let mut bytes = b"ttcf".to_vec();
    bytes.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 5]);
    assert_eq!(
        damage(bytes),
        "its header lists 5 faces, and the file of 12 bytes ends before their places"
    );
}
