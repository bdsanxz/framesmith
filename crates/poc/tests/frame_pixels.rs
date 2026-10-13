//! What the renderer draws at chosen frames, read from the pixel buffer.

use poc::cpu::Cpu;
use poc::model::Composition;
use poc::scene::Evaluator;
use poc::text::Shaper;
use poc::{layout, xml, Backend};
use std::path::Path;

const SCENE: &str = r##"<composition width="64" height="32" fps="10" duration="2" background="#000000">
  <rect id="a" left="0" top="0" width="16" height="16" background="#ff0000" start="0.5" duration="1">
    <tween at="1" duration="0.5" ease="linear" to-x="32"/>
  </rect>
  <rect id="b" left="48" top="16" width="16" height="16" background="#0000ff">
    <tween at="0" duration="1" ease="linear" from-opacity="0"/>
  </rect>
</composition>"##;

fn load(source: &str) -> Composition {
    let mut composition = xml::parse(source, Path::new(".")).expect("the scene is valid");
    layout::lay_out(&mut composition, &mut Shaper::new(), &[]);
    composition
}

/// The colour of the pixel at (x, y) of `frame`.
fn pixel(backend: &mut dyn Backend, evaluator: &Evaluator, frame: u64, x: usize, y: usize) -> [u8; 3] {
    backend.draw(frame, &evaluator.frame(frame).ops).expect("the frame is drawn");
    let pixels = backend.pixels().expect("the frame is read");
    let at = (y * 64 + x) * 4;
    [pixels[at], pixels[at + 1], pixels[at + 2]]
}

#[test]
fn a_clip_shows_only_inside_its_window_and_moves_with_its_tween() {
    let composition = load(SCENE);
    let evaluator = Evaluator::new(&composition);
    let mut cpu = Cpu::new(&composition, &[]);
    // Clip `a` is on screen from frame 5 to frame 14.
    assert_eq!(pixel(&mut cpu, &evaluator, 4, 8, 8), [0, 0, 0]);
    assert_eq!(pixel(&mut cpu, &evaluator, 5, 8, 8), [255, 0, 0]);
    assert_eq!(pixel(&mut cpu, &evaluator, 15, 40, 8), [0, 0, 0]);
    // Its tween runs from frame 10 to frame 15: at frame 14 it has moved 25.6 px to the right.
    assert_eq!(pixel(&mut cpu, &evaluator, 14, 8, 8), [0, 0, 0]);
    assert_eq!(pixel(&mut cpu, &evaluator, 14, 34, 8), [255, 0, 0]);
}

#[test]
fn opacity_follows_its_tween() {
    let composition = load(SCENE);
    let evaluator = Evaluator::new(&composition);
    let mut cpu = Cpu::new(&composition, &[]);
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 56, 24), [0, 0, 0]);
    let half = pixel(&mut cpu, &evaluator, 5, 56, 24);
    assert!(
        (126..=129).contains(&half[2]) && half[0] == 0,
        "half way the blue is half: {half:?}"
    );
    assert_eq!(pixel(&mut cpu, &evaluator, 10, 56, 24), [0, 0, 255]);
}

#[test]
fn a_child_counts_its_time_from_the_start_of_the_clip_around_it() {
    let composition = load(
        r##"<composition width="64" height="32" fps="10" duration="3" background="#000000">
  <box id="clip" start="1" duration="2">
    <rect id="late" left="0" top="0" width="16" height="16" background="#00ff00" start="0.5" duration="1"/>
  </box>
</composition>"##,
    );
    let evaluator = Evaluator::new(&composition);
    let mut cpu = Cpu::new(&composition, &[]);
    // The clip starts at frame 10; its child half a second into it, at frame 15, for one second.
    assert_eq!(pixel(&mut cpu, &evaluator, 14, 8, 8), [0, 0, 0]);
    assert_eq!(pixel(&mut cpu, &evaluator, 15, 8, 8), [0, 255, 0]);
    assert_eq!(pixel(&mut cpu, &evaluator, 24, 8, 8), [0, 255, 0]);
    assert_eq!(pixel(&mut cpu, &evaluator, 25, 8, 8), [0, 0, 0]);
}

#[test]
fn multiply_darkens_what_is_under_it_and_a_gradient_runs_across_its_box() {
    let composition = load(
        r##"<composition width="64" height="32" fps="10" duration="1" background="#808080">
  <rect left="0" top="0" width="32" height="32" background="#ff8000" mix-blend-mode="multiply"/>
  <rect left="32" top="0" width="32" height="32" background="linear-gradient(90deg, #000000 0%, #ffffff 100%)"/>
</composition>"##,
    );
    let evaluator = Evaluator::new(&composition);
    let mut cpu = Cpu::new(&composition, &[]);
    let multiplied = pixel(&mut cpu, &evaluator, 0, 8, 8);
    assert!(
        multiplied[0].abs_diff(128) <= 1 && multiplied[1].abs_diff(64) <= 1 && multiplied[2] == 0,
        "grey times orange: {multiplied:?}"
    );
    let (dark, light) = (pixel(&mut cpu, &evaluator, 0, 33, 8), pixel(&mut cpu, &evaluator, 0, 62, 8));
    assert!(dark[0] < 16 && light[0] > 239, "from black to white: {dark:?} {light:?}");
}

#[test]
fn a_box_that_hides_overflow_cuts_its_children_and_a_column_centres_them() {
    let composition = load(
        r##"<composition width="64" height="32" fps="10" duration="1" background="#000000">
  <box left="0" top="0" width="16" height="16" overflow="hidden">
    <rect left="8" top="8" width="32" height="32" background="#ffffff"/>
  </box>
  <box left="32" top="0" width="32" height="32" flex-direction="column" justify-content="center" align-items="center">
    <rect width="8" height="8" background="#ff0000"/>
  </box>
</composition>"##,
    );
    let evaluator = Evaluator::new(&composition);
    let mut cpu = Cpu::new(&composition, &[]);
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 12, 12), [255, 255, 255]);
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 20, 12), [0, 0, 0]);
    // The 8 by 8 square sits in the middle of the 32 by 32 column: from 44 to 52, from 12 to 20.
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 48, 16), [255, 0, 0]);
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 42, 16), [0, 0, 0]);
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 48, 10), [0, 0, 0]);
}

#[test]
fn a_slant_the_font_does_not_have_leans_to_the_right() {
    let mut composition = load(
        r##"<composition width="64" height="64" fps="10" duration="1" background="#000000">
  <text left="24" top="0" font-family="Arial" font-size="48" color="#ffffff">l</text>
</composition>"##,
    );
    // Ask for the slant a font without an italic gets, whatever faces this machine's font has.
    for run in &mut composition.root.children[0].runs {
        run.skew = Some(0.25);
    }
    let evaluator = Evaluator::new(&composition);
    let mut cpu = Cpu::new(&composition, &[]);
    cpu.draw(0, &evaluator.frame(0).ops).expect("the frame is drawn");
    let pixels = cpu.pixels().expect("the frame is read");
    // Where the ink of a row lies, weighted by how bright it is.
    let centre = |y: usize| -> Option<f64> {
        let row = &pixels[y * 64 * 4..(y + 1) * 64 * 4];
        let ink: f64 = row.chunks(4).map(|pixel| f64::from(pixel[0])).sum();
        let weighted: f64 = row.chunks(4).enumerate().map(|(x, pixel)| x as f64 * f64::from(pixel[0])).sum();
        (ink > 255.0).then(|| weighted / ink)
    };
    let inked: Vec<(usize, f64)> = (0..64).filter_map(|y| centre(y).map(|x| (y, x))).collect();
    let (top, bottom) = (
        inked.first().expect("the letter is drawn"),
        inked.last().expect("the letter is drawn"),
    );
    assert!(bottom.0 - top.0 > 20, "the letter is tall: {top:?} {bottom:?}");
    // A quarter of its height to the right at the top.
    let lean = (top.1 - bottom.1) / (bottom.0 - top.0) as f64;
    assert!(
        (0.2..=0.3).contains(&lean),
        "the top leans right by a quarter of the height: {lean}"
    );
}

#[test]
fn a_sized_layer_is_laid_side_by_side_and_a_layer_drawn_once_stays_in_its_place() {
    let composition = load(
        r##"<composition width="64" height="32" fps="10" duration="1" background="#000000">
  <box left="0" top="0" width="32" height="32" background="linear-gradient(90deg, #ff0000 0, #ff0000 4, #00000000 4, #00000000 8)"
       background-size="8 8" background-position="2 0"/>
  <box left="32" top="0" width="32" height="32" background="linear-gradient(#00ff00 0%, #00ff00 100%), #0000ff"
       background-size="4 100%, auto" background-position="50% 0, 0 0" background-repeat="no-repeat, repeat"/>
</composition>"##,
    );
    let evaluator = Evaluator::new(&composition);
    let mut cpu = Cpu::new(&composition, &[]);
    // Tiles of 8 px that start 2 px in: red from 2 to 6, nothing from 6 to 10, red again from 10.
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 4, 4), [255, 0, 0]);
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 8, 20), [0, 0, 0]);
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 12, 28), [255, 0, 0]);
    // The tile before the first one shows too: its empty half covers 0 to 2.
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 0, 4), [0, 0, 0]);
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 28, 4), [255, 0, 0]);
    // A bar 4 px wide, centred in a box of 32 px that starts at 32: from 46 to 50, drawn once.
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 48, 16), [0, 255, 0]);
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 44, 16), [0, 0, 255]);
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 52, 16), [0, 0, 255]);
}

#[test]
fn a_circle_with_stops_in_pixels_is_a_dot_of_that_size() {
    let composition = load(
        r##"<composition width="64" height="32" fps="10" duration="1" background="#000000">
  <box left="0" top="0" width="64" height="32" background="radial-gradient(circle at 25% 50%, #ffffff 0, #ffffff 5, #00000000 5)"/>
</composition>"##,
    );
    let evaluator = Evaluator::new(&composition);
    let mut cpu = Cpu::new(&composition, &[]);
    // The centre is at (16, 16) and the dot is 5 px in radius.
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 16, 16), [255, 255, 255]);
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 19, 16), [255, 255, 255]);
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 16, 13), [255, 255, 255]);
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 23, 16), [0, 0, 0]);
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 16, 24), [0, 0, 0]);
}

#[test]
fn a_set_written_a_hair_after_a_frame_counts_on_that_frame() {
    // At 30 frames a second 1.6667 s is a hair over frame 50.
    let composition = load(
        r##"<composition width="64" height="32" fps="30" duration="3" background="#000000">
  <rect left="0" top="0" width="16" height="16" background="#ff0000" opacity="0">
    <set at="1.6667" opacity="1"/>
  </rect>
</composition>"##,
    );
    let evaluator = Evaluator::new(&composition);
    let mut cpu = Cpu::new(&composition, &[]);
    assert_eq!(pixel(&mut cpu, &evaluator, 49, 8, 8), [0, 0, 0]);
    assert_eq!(pixel(&mut cpu, &evaluator, 50, 8, 8), [255, 0, 0]);
}

#[test]
fn a_shape_casts_a_shadow_that_follows_its_outline() {
    let composition = load(
        r##"<composition width="64" height="32" fps="10" duration="1" background="#ffffff">
  <shape left="0" top="0" width="32" height="32" d="M 4 4 L 12 4 L 12 12 L 4 12 Z" view-box="0 0 32 32" color="#0000ff"
         drop-shadow="12 8 0 #ff0000"/>
</composition>"##,
    );
    let evaluator = Evaluator::new(&composition);
    let mut cpu = Cpu::new(&composition, &[]);
    // The square is at 4 to 12; its shadow, moved by (12, 8), is at 16 to 24 by 12 to 20.
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 8, 8), [0, 0, 255]);
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 20, 16), [255, 0, 0]);
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 28, 16), [255, 255, 255]);
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 20, 24), [255, 255, 255]);
}

#[test]
fn brightness_multiplies_the_finished_picture_of_an_element_and_its_children() {
    let composition = load(
        r##"<composition width="64" height="32" fps="10" duration="1" background="#000000">
  <box left="0" top="0" width="32" height="32" background="#ffffff40, #202020" brightness="2">
    <rect left="16" top="0" width="16" height="16" background="#303030"/>
  </box>
  <box left="32" top="0" width="32" height="32" background="#808080">
    <tween at="0" duration="1" ease="linear" from-brightness="3" to-brightness="1"/>
  </box>
</composition>"##,
    );
    let evaluator = Evaluator::new(&composition);
    let mut cpu = Cpu::new(&composition, &[]);
    // A quarter of white over #202020 is 88; twice that is 176. Brightening each layer by
    // itself would give 112.
    let own = pixel(&mut cpu, &evaluator, 0, 8, 24);
    assert!((174..=178).contains(&own[0]) && own[0] == own[1] && own[1] == own[2], "{own:?}");
    // The child is brightened with the element that holds it.
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 24, 8), [96, 96, 96]);
    // A colour stops at white, and the tween brings the brightness back to 1.
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 48, 16), [255, 255, 255]);
    let half = pixel(&mut cpu, &evaluator, 5, 48, 16);
    assert!((254..=255).contains(&half[0]), "at half way the brightness is 2: {half:?}");
    let late = pixel(&mut cpu, &evaluator, 9, 48, 16);
    assert!((152..=156).contains(&late[0]), "at nine tenths the brightness is 1.2: {late:?}");
}
