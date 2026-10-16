//! What the tests of the render crate share: the fixtures, the checked-in pictures and their
//! comparison, scratch folders, and projects made from a fixture.

#![allow(dead_code)]

pub mod agreement;
pub mod png;

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
    static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let call = CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = scratch(group, &format!("{name}-{call}"));
    fs::copy(fixtures().join(fixture).join("composition.xml"), dir.join("composition.xml")).unwrap();
    dir
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
