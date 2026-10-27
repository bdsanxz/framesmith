//! What the tests of the render crate share: the fixtures, the checked-in pictures and their
//! comparison, scratch folders, and projects made from a fixture.

#![allow(dead_code)]

pub mod agreement;
pub mod gaussian;
pub mod held;
pub mod png;
pub mod worked;

use draw::Picture;
use std::fs;
use std::path::{Path, PathBuf};

/// The folder of the checked-in fixture compositions (`docs/spec/render-command.md` section 9), or the
/// folder `FRAMESMITH_FIXTURES` names, for a check that a picture which is not the checked-in one fails.
pub fn fixtures() -> PathBuf {
    std::env::var_os("FRAMESMITH_FIXTURES")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures"))
}

/// A new, empty folder `name` for one test, under the tests' scratch folder.
pub fn scratch(group: &str, name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(group).join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A project folder in a scratch folder, holding a copy of the composition of the fixture.
/// Each call gets a folder of its own, so that tests that run at the same time do not share one.
pub fn project_from_fixture(group: &str, name: &str, fixture: &str) -> PathBuf {
    let dir = numbered_scratch(group, name);
    fs::copy(fixtures().join(fixture).join("composition.xml"), dir.join("composition.xml")).unwrap();
    dir
}

/// A project folder in a scratch folder, holding a copy of every file of the fixture except its `expected`
/// pictures: the composition and the files it shows (`docs/spec/notation.md` section 10).
pub fn project_copied_from_fixture(group: &str, name: &str, fixture: &str) -> PathBuf {
    let dir = numbered_scratch(group, name);
    copy_files(&fixtures().join(fixture), &dir);
    dir
}

/// The text colours the emoji of the emoji fixture is drawn in, each with the name its project folder is given: black,
/// the fixture's own blue, and a yellow close to the emoji's own yellow.
pub const EMOJI_TEXT_COLOURS: [(&str, &str); 3] = [("black", "#000000"), ("blue", "#1e6fd9"), ("yellow", "#f5c000")];

/// A project folder of the test's own, copied from `fixture`, whose composition writes its text colour
/// `from` as `to`. The fixture must write `from` exactly once.
pub fn project_with_text_colour(group: &str, name: &str, fixture: &str, from: &str, to: &str) -> PathBuf {
    let dir = numbered_scratch(group, name);
    copy_files(&fixtures().join(fixture), &dir);
    let path = dir.join("composition.xml");
    let text = fs::read_to_string(&path).unwrap();
    let written = format!("color=\"{from}\"");
    assert_eq!(text.matches(&written).count(), 1, "{fixture} writes its text colour {from} once");
    fs::write(&path, text.replacen(&written, &format!("color=\"{to}\""), 1)).unwrap();
    dir
}

/// The place of the first picture of a text in frame `number` of a scene, as left, top, width and
/// height: the whole pixel nearest to its top-left corner, and its size in pixels.
pub fn first_picture_place(scene: &render::Scene, number: u64) -> (u32, u32, u32, u32) {
    let frame = scene.frame(number).unwrap();
    let picture = frame
        .marks
        .iter()
        .find_map(|mark| match mark {
            draw::Mark::Text(text) => text.pictures.first(),
            _ => None,
        })
        .expect("the emoji is drawn as a picture");
    (picture.x.round() as u32, picture.y.round() as u32, picture.width, picture.height)
}

/// The pixels of `picture` in the rectangle of `width` by `height` from (`left`, `top`), as a picture of their own.
pub fn crop(picture: &Picture, left: u32, top: u32, width: u32, height: u32) -> Picture {
    let pixels = (top..top + height)
        .flat_map(|y| (left..left + width).map(move |x| (x, y)))
        .flat_map(|(x, y)| picture.pixel(x, y).expect("the rectangle lies on the picture"))
        .collect();
    Picture::new(width, height, pixels).unwrap()
}

/// A new folder for one call of a test, named after `name` and the number of the call.
fn numbered_scratch(group: &str, name: &str) -> PathBuf {
    static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let call = CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    scratch(group, &format!("{name}-{call}"))
}

/// Copies the files and folders of `from` into `to`, leaving out the folder of checked-in pictures.
fn copy_files(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            if entry.file_name() != "expected" {
                copy_files(&entry.path(), &target);
            }
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A project folder in a scratch folder, holding a composition with the text given.
pub fn project_with(group: &str, name: &str, composition: &str) -> PathBuf {
    let dir = scratch(group, name);
    fs::write(dir.join("composition.xml"), composition).unwrap();
    dir
}

/// A valid composition of 96 by 54 pixels, ten frames, with `inside` in the root.
pub fn composition(inside: &str) -> String {
    format!("<composition version=\"1\" width=\"96\" height=\"54\" fps=\"10\" duration=\"1\">\n{inside}\n</composition>\n")
}

/// The name of the file of a checked-in picture of frame `number`.
pub fn picture_name(number: u64) -> String {
    format!("frame-{number:04}.png")
}

/// The checked-in picture of frame `number` of a fixture.
pub fn expected(fixture: &str, number: u64) -> Picture {
    let path = fixtures().join(fixture).join("expected").join(picture_name(number));
    let bytes = fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let (width, height, pixels) = png::decode(&bytes).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    Picture::new(width, height, pixels).unwrap()
}

/// The numbers of the frames a fixture has a checked-in picture for, in order.
pub fn numbers_with_a_picture(fixture: &str) -> Vec<u64> {
    let folder = fixtures().join(fixture).join("expected");
    let mut numbers: Vec<u64> = fs::read_dir(&folder)
        .unwrap_or_else(|error| panic!("{}: {error}", folder.display()))
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .map(|name| {
            name.strip_prefix("frame-")
                .and_then(|rest| rest.strip_suffix(".png"))
                .and_then(|digits| digits.parse().ok())
                .unwrap_or_else(|| panic!("`{name}` is not the name of a picture: frame-NNNN.png"))
        })
        .collect();
    numbers.sort_unstable();
    numbers
}

/// Holds the picture drawn for frame `number` to its checked-in picture, to the byte. A difference
/// is told with the frame's number, the first pixel that differs and how many do.
pub fn compare(number: u64, drawn: &Picture, expected: &Picture) -> Result<(), String> {
    if (drawn.width(), drawn.height()) != (expected.width(), expected.height()) {
        return Err(format!(
            "frame {number}: the picture drawn is {}x{} and the checked-in picture is {}x{}",
            drawn.width(),
            drawn.height(),
            expected.width(),
            expected.height()
        ));
    }
    let differing: Vec<usize> = (0..drawn.pixels().len() / 4)
        .filter(|&at| drawn.pixels()[at * 4..at * 4 + 4] != expected.pixels()[at * 4..at * 4 + 4])
        .collect();
    let Some(&first) = differing.first() else {
        return Ok(());
    };
    let (x, y) = (first as u32 % drawn.width(), first as u32 / drawn.width());
    Err(format!(
        "frame {number}: pixel ({x}, {y}) is {:?} and the checked-in picture has {:?}; {} pixels differ",
        drawn.pixel(x, y).unwrap(),
        expected.pixel(x, y).unwrap(),
        differing.len()
    ))
}

/// A copy of a picture with the pixel at `(x, y)` changed to `value`.
pub fn with_pixel(picture: &Picture, x: u32, y: u32, value: [u8; 4]) -> Picture {
    let mut pixels = picture.pixels().to_vec();
    let at = (y as usize * picture.width() as usize + x as usize) * 4;
    pixels[at..at + 4].copy_from_slice(&value);
    Picture::new(picture.width(), picture.height(), pixels).unwrap()
}

/// The smallest rectangle, as (left, top, right, bottom) with right and bottom not included, that
/// holds every pixel of `colour`; None when no pixel has that colour.
pub fn extent_of(picture: &Picture, colour: [u8; 4]) -> Option<(u32, u32, u32, u32)> {
    let mut found: Option<(u32, u32, u32, u32)> = None;
    for y in 0..picture.height() {
        for x in 0..picture.width() {
            if picture.pixel(x, y) != Some(colour) {
                continue;
            }
            found = Some(match found {
                None => (x, y, x + 1, y + 1),
                Some((left, top, right, bottom)) => (left.min(x), top.min(y), right.max(x + 1), bottom.max(y + 1)),
            });
        }
    }
    found
}

/// The number of pixels of `colour` in a picture.
pub fn count_of(picture: &Picture, colour: [u8; 4]) -> usize {
    (0..picture.height())
        .flat_map(|y| (0..picture.width()).map(move |x| (x, y)))
        .filter(|&(x, y)| picture.pixel(x, y) == Some(colour))
        .count()
}
