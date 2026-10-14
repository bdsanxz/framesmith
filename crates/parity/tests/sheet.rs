//! The sheet of low frames: the numbers drawn on it, its layout and the file written.

use measure::Frame;
use parity::sheet::{self, SheetEntry};

fn flat(colour: [u8; 3]) -> Frame {
    Frame {
        width: 160,
        height: 90,
        rgb: colour.iter().copied().cycle().take(160 * 90 * 3).collect(),
    }
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

#[test]
fn every_word_the_sheet_prints_leaves_marks() {
    let blank = sheet::Canvas::new(400, 30, [0, 0, 0]).rgb;
    for word in [
        "OURS",
        "REFERENCE",
        "DIFFERENT PIXELS",
        "FRAME 12",
        "SCORE 87.500 %",
        "FAIL",
        "EXCUSED",
    ] {
        let mut canvas = sheet::Canvas::new(400, 30, [0, 0, 0]);
        let width = sheet::draw_text(&mut canvas, 2, 2, word, 2, [255, 255, 255]);
        assert_eq!(width, 12 * word.chars().count() as u32, "{word}");
        // Every letter of the word has a shape of its own, so that nothing is left blank.
        for (at, letter) in word.chars().enumerate().filter(|(_, c)| *c != ' ') {
            let mut single = sheet::Canvas::new(400, 30, [0, 0, 0]);
            sheet::draw_text(&mut single, 2, 2, &letter.to_string(), 2, [255, 255, 255]);
            assert_ne!(single.rgb, blank, "'{letter}' at {at} of {word}");
        }
    }
}

#[test]
fn the_sheet_has_one_row_for_each_frame_and_a_third_picture_marks_the_pixels_that_differ() {
    let (ours, theirs) = (flat([20, 30, 50]), flat([20, 30, 50]));
    let mut mask = vec![false; 160 * 90];
    for y in 40..60 {
        for x in 60..100 {
            mask[y * 160 + x] = true;
        }
    }
    let entries = [SheetEntry {
        frame: 12,
        score: "97.000".into(),
        state: "fail",
        ours: &ours,
        theirs: &theirs,
        mask: &mask,
    }];
    let canvas = sheet::draw(&entries);
    let thumb = (90 * sheet::THUMB_WIDTH).div_ceil(160);
    let layout = sheet::layout(1, thumb);
    assert_eq!((canvas.width, canvas.height), layout.size);
    let (left, top) = layout.spots[0];
    let third = left + 2 * (sheet::THUMB_WIDTH + 6);
    let row = top + 7 * 2 + 8;
    // The middle of the marked block is in the colour of a differing pixel, a corner of the picture is not.
    assert_eq!(canvas.pixel(third + sheet::THUMB_WIDTH / 2, row + thumb / 2), sheet::MARK);
    assert_ne!(canvas.pixel(third + 2, row + 2), sheet::MARK);
}

#[test]
fn the_sheet_is_written_as_one_png_file_in_a_folder_made_for_it() {
    let dir = std::path::absolute(std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("parity").join("sheet_file")).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let (ours, theirs) = (flat([20, 30, 50]), flat([200, 30, 50]));
    let mask = vec![true; 160 * 90];
    let entries: Vec<SheetEntry> = (0..3)
        .map(|n| SheetEntry {
            frame: n,
            score: "0.000".into(),
            state: "excused",
            ours: &ours,
            theirs: &theirs,
            mask: &mask,
        })
        .collect();
    let path = dir.join("nested").join("sheet.png");
    sheet::write(&entries, &path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let thumb = (90 * sheet::THUMB_WIDTH).div_ceil(160);
    assert_eq!(measure::png_size(&bytes), Some(sheet::layout(3, thumb).size));
    assert_eq!(
        std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
        1,
        "exactly one file is written"
    );
    assert!(sheet::write(&[], &dir.join("none.png")).is_err());
}
