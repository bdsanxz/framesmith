//! A picture holds the bytes of its size and no others, and gives back the pixel at a place.

use draw::Picture;

#[test]
fn a_picture_needs_exactly_the_bytes_of_its_size() {
    assert!(Picture::new(2, 2, vec![0; 16]).is_some());
    assert!(Picture::new(2, 2, vec![0; 15]).is_none());
    assert!(Picture::new(2, 2, vec![0; 17]).is_none());
    assert!(Picture::new(0, 5, Vec::new()).is_some());
}

#[test]
fn a_pixel_is_found_by_its_column_and_row_and_outside_the_picture_there_is_none() {
    let bytes: Vec<u8> = (0..24).collect();
    let picture = Picture::new(3, 2, bytes).unwrap();
    assert_eq!((picture.width(), picture.height()), (3, 2));
    assert_eq!(picture.pixel(0, 0), Some([0, 1, 2, 3]));
    assert_eq!(picture.pixel(2, 0), Some([8, 9, 10, 11]));
    assert_eq!(picture.pixel(0, 1), Some([12, 13, 14, 15]));
    assert_eq!(picture.pixel(2, 1), Some([20, 21, 22, 23]));
    assert_eq!(picture.pixel(3, 0), None);
    assert_eq!(picture.pixel(0, 2), None);
    assert_eq!(picture.pixels().len(), 24);
}
