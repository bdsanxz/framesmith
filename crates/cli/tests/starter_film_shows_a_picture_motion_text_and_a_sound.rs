//! The film of the starter, rendered and read back, shows its picture, the motion of its ball between two times, its text,
//! and a sound track that is not silent (`docs/spec/init-command.md`, section 8; the composition of section 3.1). Every
//! expected colour and place is worked from the composition: the frames are decoded from the MP4 file, not taken from the
//! renderer. The same checks run on the processor and on the card, which must agree.

mod common;

use std::path::Path;

use common::{err, framesmith, out, scratch};
use measure::{open_pictures, read_sound, Frame, Rgb};

/// The colours of the starter (`crates/cli/starter/composition.xml`).
const SKY: Rgb = [0x2c, 0x5f, 0x8a];
const SUN: Rgb = [0xf5, 0xc5, 0x42];
const BALL: Rgb = [0xe8, 0x55, 0x3d];
const BACKGROUND: Rgb = [0x1b, 0x2a, 0x3a];

/// The largest difference of one channel that a colour may have after the film's compression.
const TOLERANCE: u8 = 12;

fn near(actual: Rgb, expected: Rgb) -> bool {
    actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= TOLERANCE)
}

/// Every picture of the film, in the order it is shown.
fn pictures(film: &Path) -> Vec<Frame> {
    let mut reader = open_pictures(film).unwrap();
    let mut frames = Vec::new();
    while let Some(frame) = reader.next_frame().unwrap() {
        frames.push(frame);
    }
    frames
}

/// The number of pixels of `frame` in the rectangle from `(left, top)` up to, not including, `(right, bottom)` that are
/// white.
fn white_in(frame: &Frame, left: u32, top: u32, right: u32, bottom: u32) -> usize {
    (top..bottom)
        .flat_map(|y| (left..right).map(move |x| (x, y)))
        .filter(|&(x, y)| frame.pixel(x, y).iter().all(|&channel| channel >= 200))
        .count()
}

/// Initialises the starter, renders it with `backend`, and checks the film that comes out of it.
fn assert_starter_film_shows_its_picture_motion_text_and_sound(name: &str, backend: &str) {
    let cwd = scratch("init", name);
    assert_eq!(framesmith(&cwd, ["init", "demo"], &[]).status.code(), Some(0));
    let render = framesmith(&cwd, ["render", "demo", "--backend", backend], &[]);
    assert_eq!(render.status.code(), Some(0), "{}{}", out(&render), err(&render));

    let film = cwd.join("demo/renders/demo.mp4");
    let frames = pictures(&film);
    // Three seconds at 30 frames a second, 480 by 270 pixels: the film has 90 frames of that size.
    assert_eq!(frames.len(), 90);
    assert_eq!((frames[0].width, frames[0].height), (480, 270));

    // The picture: the sky above line 150, the sun in its box (340 to 430 by 24 to 114), the background below.
    let first = &frames[0];
    assert!(near(first.pixel(10, 10), SKY), "sky: {:?}", first.pixel(10, 10));
    assert!(near(first.pixel(385, 69), SUN), "sun: {:?}", first.pixel(385, 69));
    assert!(near(first.pixel(10, 260), BACKGROUND), "background: {:?}", first.pixel(10, 260));

    // The motion: the ball is 60 pixels square, starts at (20, 170) and moves 340 pixels to the right in three seconds.
    // On frame 0 its centre is at (50, 200); on frame 89 (2.967 seconds) it has moved 336.2 pixels, to about 386.
    let last = &frames[89];
    assert!(near(first.pixel(50, 200), BALL), "ball at the start: {:?}", first.pixel(50, 200));
    assert!(near(first.pixel(386, 200), BACKGROUND), "no ball at the far place on frame 0");
    assert!(
        near(last.pixel(386, 200), BALL),
        "ball at the far place on frame 89: {:?}",
        last.pixel(386, 200)
    );
    assert!(near(last.pixel(50, 200), BACKGROUND), "no ball at the start on frame 89");

    // The text: white letters on the sky, in the box that starts at (30, 36), and none on the sky below them.
    assert!(white_in(first, 30, 36, 330, 84) > 300, "the title is drawn in white");
    assert_eq!(white_in(first, 30, 120, 330, 140), 0, "no white below the title");

    // The sound: the tone is 0.15 of full scale, so it peaks near 0.15 in the steady part and is silent at the very start.
    let sound = read_sound(&film).unwrap().expect("the film has a sound track");
    let level = |from: f64, to: f64| {
        let (first, last) = ((from * f64::from(sound.rate)) as usize, (to * f64::from(sound.rate)) as usize);
        sound.samples[first * usize::from(sound.channels)..last * usize::from(sound.channels)]
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()))
    };
    let steady = level(0.5, 2.5);
    assert!((0.10..=0.20).contains(&steady), "the steady tone peaks near 0.15: {steady}");
    assert!(level(0.0, 0.02) < 0.05, "the tone starts from silence");
}

#[test]
fn the_rendered_starter_shows_its_picture_motion_text_and_a_sound_on_the_processor() {
    assert_starter_film_shows_its_picture_motion_text_and_sound("film-on-the-processor", "cpu");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_rendered_starter_shows_its_picture_motion_text_and_a_sound_on_the_card() {
    assert_starter_film_shows_its_picture_motion_text_and_sound("film-on-the-card", "gpu");
}
