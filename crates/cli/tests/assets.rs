//! T013: local still images and a project font are in place before the
//! first frame, and again when a clip first shows them.

mod common;

use common::*;
use measure::{Frame, Rgb};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// A picture of 4000 by 3000 pixels in four flat quarters, with noise in
/// the blue channel so the file is large and slow to decode. The quarters
/// are named by their red and green levels, left to right, top to bottom.
fn big_picture(file: &Path, quarters: [&str; 4]) {
    let sources: String = quarters
        .iter()
        .zip(["a", "b", "c", "d"])
        .map(|(colour, name)| format!("color=c=0x{colour}:s=2000x1500,format=gbrp[{name}];"))
        .collect();
    let graph = format!("{sources}[a][b]hstack[t];[c][d]hstack[u];[t][u]vstack,noise=c1s=60:c1f=u");
    let made = measure::run(
        &ffmpeg(),
        &["-v", "error", "-y", "-f", "lavfi", "-i", &graph, "-frames:v", "1", text(file)],
        file.parent().unwrap(),
        &[],
    );
    assert_eq!(made.code, 0, "{}", made.all());
    assert!(std::fs::metadata(file).unwrap().len() > 5_000_000);
}

/// The two pictures, made once for all the tests in this file.
fn pictures() -> &'static PathBuf {
    static MADE: OnceLock<PathBuf> = OnceLock::new();
    MADE.get_or_init(|| {
        let dir = scratch("t013_pictures");
        big_picture(&dir.join("first.png"), ["ffff00", "00ff00", "ff0000", "000000"]);
        big_picture(&dir.join("second.png"), ["000000", "ff0000", "00ff00", "ffff00"]);
        dir
    })
}

const STYLE: &str = "@font-face { font-family: \"Blocks\"; src: url(\"fonts/blocks.ttf\"); }\n\
[data-fs-composition] { background: #808080; }\n\
img { position: absolute; top: 20px; width: 200px; height: 150px; }\n\
#first { left: 20px; }\n\
#second { left: 250px; }\n\
#words { position: absolute; left: 20px; top: 200px; margin: 0; font: 40px/40px \"Blocks\", sans-serif; color: #ffffff; }";

const BODY: &str = "<img id=\"first\" src=\"pictures/first.png\" alt=\"\">\n\
<div data-fs-clip data-fs-start=\"3\" data-fs-duration=\"1\"><img id=\"second\" src=\"pictures/second.png\" alt=\"\"></div>\n\
<p id=\"words\">XX</p>";

/// 480 by 270, two frames a second for four seconds.
fn card(test: &str, style: &str) -> PathBuf {
    let dir = project(
        test,
        "card",
        &page("data-fs-size=\"480x270\" data-fs-fps=\"2\" data-fs-duration=\"4\"", style, BODY, ""),
    );
    for name in ["first.png", "second.png"] {
        std::fs::create_dir_all(dir.join("pictures")).unwrap();
        std::fs::copy(pictures().join(name), dir.join("pictures").join(name)).unwrap();
    }
    write(&dir, "fonts/blocks.ttf", measure::block_font("Blocks"));
    dir
}

/// Checks that the whole picture is painted at `left`, 20, 200 by 150:
/// every pixel of each quarter, a pixel in from its edges, has the
/// quarter's red and green.
fn assert_whole(frame: &Frame, left: u32, quarters: [[u8; 2]; 4], what: &str) {
    for (index, wanted) in quarters.iter().enumerate() {
        let (column, row) = (index as u32 % 2, index as u32 / 2);
        for y in 20 + row * 75 + 1..20 + (row + 1) * 75 - 1 {
            for x in left + column * 100 + 1..left + (column + 1) * 100 - 1 {
                let [r, g, _] = frame.pixel(x, y);
                assert!(
                    r.abs_diff(wanted[0]) <= 2 && g.abs_diff(wanted[1]) <= 2,
                    "{what}: pixel {x},{y} is {:?}, not red {} green {}",
                    frame.pixel(x, y),
                    wanted[0],
                    wanted[1]
                );
            }
        }
    }
}

const GREY: Rgb = [0x80, 0x80, 0x80];
const FIRST: [[u8; 2]; 4] = [[255, 255], [0, 255], [255, 0], [0, 0]];
const SECOND: [[u8; 2]; 4] = [[0, 0], [255, 0], [0, 255], [255, 255]];

#[test]
fn t013_c1_c2_c4_large_pictures_are_whole_on_frame_0_and_when_their_clip_first_shows() {
    let dir = card("t013_c1", STYLE);
    let (ran, frames) = render_frames(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(frames.len(), 8);
    // c1: frame 0 shows the whole picture. c4: at the place and the size the CSS gives.
    assert_whole(&frames[0], 20, FIRST, "the first picture on frame 0");
    for (x, y) in [(19, 95), (221, 95), (120, 19), (120, 171)] {
        assert_eq!(frames[0].pixel(x, y), GREY, "just outside the picture at {x},{y}");
    }
    // The second picture is in a clip that starts at 3 s: frame 6.
    for frame in &frames[..6] {
        assert!(frame.crop(250, 20, 200, 150).is_all(GREY, 0));
    }
    // c2: the first frame of that clip shows the whole picture.
    assert_whole(&frames[6], 250, SECOND, "the second picture on its first frame");
    assert_whole(&frames[7], 250, SECOND, "the second picture on its second frame");
    assert_whole(&frames[7], 20, FIRST, "the first picture on the last frame");
}

#[test]
fn t013_c3_text_in_a_project_font_is_in_that_font_on_frame_0() {
    let white: Rgb = [255, 255, 255];
    let dir = card("t013_c3", STYLE);
    let (ran, frames) = render_frames(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    // In the project font each X is a solid square one em wide: two make a block of 80 by 40.
    for frame in [&frames[0], &frames[7]] {
        let block = frame.bounds(white, 0).unwrap();
        assert_eq!((block.left, block.top, block.width(), block.height()), (20, 200, 80, 40));
        assert_eq!(frame.count(white, 0), 80 * 40);
    }
    // The same page without the font rule falls back to an installed font, and the measure shows it.
    let fallback = card("t013_c3_fallback", &STYLE.replace("@font-face", "@media print"));
    let (ran, frames) = render_frames(&fallback, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(
        frames[0].count(white, 0) < 80 * 40 / 2,
        "an installed font paints letters, not a block"
    );
}

#[test]
fn t013_c5_two_renders_paint_the_picture_and_the_text_the_same() {
    let hashes = |test: &str| {
        let (ran, frames) = render_frames(&card(test, STYLE), &[], &[]);
        assert_eq!(ran.code, 0, "{}", ran.all());
        frames.iter().map(Frame::hash).collect::<Vec<u64>>()
    };
    let (first, second) = (hashes("t013_c5_first"), hashes("t013_c5_second"));
    assert_eq!(first.len(), 8);
    assert_eq!(measure::first_difference(&first, &second), None);
}
