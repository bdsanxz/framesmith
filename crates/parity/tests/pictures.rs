//! Sampling and the frame number drawn on the side-by-side sheet, where no video is needed.

use parity::pictures;
use parity::sheet;

#[test]
fn sampling_is_fixed_and_repeatable_and_includes_the_first_and_last_frames() {
    assert_eq!(pictures::sample_indices(60, 8), vec![0, 8, 17, 25, 34, 42, 51, 59]);
    assert_eq!(pictures::sample_indices(60, 8), pictures::sample_indices(60, 8));
    assert_eq!(pictures::sample_indices(60, 2), vec![0, 59]);
    assert_eq!(pictures::sample_indices(3, 8), vec![0, 1, 2]);
    assert_eq!(pictures::sample_indices(1, 8), vec![0]);
    assert!(pictures::sample_indices(0, 8).is_empty());
}

#[test]
fn the_frame_number_is_drawn_and_differs_from_one_number_to_another() {
    let draw = |text: &str| {
        let mut canvas = sheet::Canvas::new(200, 30, [0, 0, 0]);
        sheet::draw_text(&mut canvas, 2, 2, text, 3, [255, 255, 255]);
        canvas.rgb
    };
    let blank = sheet::Canvas::new(200, 30, [0, 0, 0]).rgb;
    for digits in ["0", "17", "42", "59"] {
        assert_ne!(draw(digits), blank, "{digits} leaves marks");
    }
    assert_ne!(draw("42"), draw("24"));
    assert_ne!(draw("17"), draw("42"));
    assert_eq!(draw("42"), draw("42"));
}
