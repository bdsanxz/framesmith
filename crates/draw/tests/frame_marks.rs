//! The frame mark of a picture is the 64-bit FNV-1a hash of its width, its height and its bytes
//! (`docs/spec/render-command.md`, section 7.1): equal for equal pictures, and another for a picture that differs in one value
//! or in its size.

use draw::Picture;

#[test]
fn the_mark_of_a_picture_is_the_fnv_1a_hash_of_its_size_and_its_bytes() {
    // Worked by hand from the published offset basis and prime, which give af63dc4c8601ec8c for the one byte `a`: the bytes
    // 01 00 00 00, 01 00 00 00, then 01 02 03 ff.
    let picture = Picture::new(1, 1, vec![1, 2, 3, 255]).unwrap();
    assert_eq!(picture.mark(), 0xef74_213a_80c9_4fac);
}

#[test]
fn equal_pictures_have_equal_marks() {
    let bytes: Vec<u8> = (0..=255).cycle().take(64 * 48 * 4).collect();
    let first = Picture::new(64, 48, bytes.clone()).unwrap();
    let second = Picture::new(64, 48, bytes).unwrap();
    assert_eq!(first.mark(), second.mark());
}

#[test]
fn a_picture_that_differs_in_one_value_has_another_mark() {
    let bytes: Vec<u8> = vec![255; 64 * 48 * 4];
    let mut changed = bytes.clone();
    changed[64 * 4 * 20 + 33] = 254;
    let first = Picture::new(64, 48, bytes).unwrap();
    let second = Picture::new(64, 48, changed).unwrap();
    assert_ne!(first.mark(), second.mark());
}

#[test]
fn pictures_of_the_same_bytes_and_another_size_have_other_marks() {
    let bytes = vec![7; 2 * 3 * 4];
    let wide = Picture::new(3, 2, bytes.clone()).unwrap();
    let tall = Picture::new(2, 3, bytes).unwrap();
    assert_ne!(wide.mark(), tall.mark());
}
