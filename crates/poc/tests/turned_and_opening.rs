//! An element turned in space, and an element that opens over what lies under it: where their
//! pixels are, read from the pixel buffer of the CPU renderer.

use poc::cpu::Cpu;
use poc::scene::{turn, Evaluator};
use poc::text::Shaper;
use poc::{layout, xml, Backend};
use std::path::Path;

const WIDTH: usize = 200;

/// Frame `frame` of a scene of 200 by 100 pixels, as its pixels.
fn drawn(source: &str, frame: u64) -> Vec<u8> {
    let mut composition = xml::parse(source, Path::new(".")).expect("the scene is valid");
    layout::lay_out(&mut composition, &mut Shaper::new(), &[]);
    let mut cpu = Cpu::new(&composition, &[]);
    cpu.draw(frame, &Evaluator::new(&composition).frame(frame).ops)
        .expect("the frame is drawn");
    cpu.pixels().expect("the frame is read").to_vec()
}

fn at(pixels: &[u8], x: usize, y: usize) -> [u8; 3] {
    let place = (y * WIDTH + x) * 4;
    [pixels[place], pixels[place + 1], pixels[place + 2]]
}

const WHITE: [u8; 3] = [255, 255, 255];
const BLACK: [u8; 3] = [0, 0, 0];
const RED: [u8; 3] = [255, 0, 0];
const BLUE: [u8; 3] = [0, 0, 255];

/// A white card of 100 by 60 in the middle of a black frame, with these attributes.
fn card(attributes: &str) -> String {
    format!(
        r##"<composition width="200" height="100" fps="10" duration="1" background="#000000">
  <box left="50" top="20" width="100" height="60" background="#ffffff" {attributes}/>
</composition>"##
    )
}

#[test]
fn a_point_is_seen_where_the_turn_and_the_distance_of_the_eye_put_it() {
    // Turned by 60 degrees about the upright line through (100, 50), seen from 200 away: a
    // point 50 to the right comes to 25 to the right and 43.3 further away.
    let matrix = turn((100.0, 50.0), 0.0, 60.0, 200.0);
    let seen = |x: f64, y: f64| {
        let row = |row: usize| matrix[row][0] * x + matrix[row][1] * y + matrix[row][2];
        (row(0) / row(2), row(1) / row(2))
    };
    let far = 200.0 / (200.0 + 50.0 * 60.0_f64.to_radians().sin());
    let near = 200.0 / (200.0 - 50.0 * 60.0_f64.to_radians().sin());
    let close = |found: (f64, f64), expected: (f64, f64)| (found.0 - expected.0).abs() < 1e-9 && (found.1 - expected.1).abs() < 1e-9;
    assert!(close(seen(100.0, 50.0), (100.0, 50.0)), "the origin stays");
    assert!(
        close(seen(150.0, 20.0), (100.0 + 25.0 * far, 50.0 - 30.0 * far)),
        "{:?}",
        seen(150.0, 20.0)
    );
    assert!(
        close(seen(50.0, 80.0), (100.0 - 25.0 * near, 50.0 + 30.0 * near)),
        "{:?}",
        seen(50.0, 80.0)
    );
    // About the level line: the top goes away, so it is seen lower and smaller.
    let matrix = turn((100.0, 50.0), 60.0, 0.0, 200.0);
    let top = (matrix[1][0] * 100.0 + matrix[1][1] * 20.0 + matrix[1][2]) / (matrix[2][0] * 100.0 + matrix[2][1] * 20.0 + matrix[2][2]);
    let expected = 50.0 - 15.0 * 200.0 / (200.0 + 30.0 * 60.0_f64.to_radians().sin());
    assert!((top - expected).abs() < 1e-9, "{top} and not {expected}");
}

#[test]
fn a_card_turned_about_its_upright_line_is_narrower_and_its_far_edge_shorter() {
    let pixels = drawn(&card(r#"perspective="200" rotate-y="60""#), 0);
    // Flat it reached from 50 to 150; turned, from 68.1 to 120.6.
    assert_eq!(at(&pixels, 100, 50), WHITE);
    assert_eq!(at(&pixels, 60, 50), BLACK);
    assert_eq!(at(&pixels, 72, 50), WHITE);
    assert_eq!(at(&pixels, 117, 50), WHITE);
    assert_eq!(at(&pixels, 124, 50), BLACK);
    // The near edge, on the left, is taller than the card was (from 11.7 to 88.3 at its end);
    // the far edge is shorter (from 25.3 to 74.7).
    assert_eq!(at(&pixels, 72, 17), WHITE);
    assert_eq!(at(&pixels, 72, 82), WHITE);
    assert_eq!(at(&pixels, 72, 8), BLACK);
    assert_eq!(at(&pixels, 117, 22), BLACK);
    assert_eq!(at(&pixels, 117, 30), WHITE);
    assert_eq!(at(&pixels, 117, 77), BLACK);
}

#[test]
fn a_turn_takes_the_children_and_follows_its_tween() {
    let scene = r##"<composition width="200" height="100" fps="10" duration="1" background="#000000">
  <box left="50" top="20" width="100" height="60" background="#ffffff" perspective="200">
    <tween at="0" duration="1" ease="linear" from-rotate-y="0" to-rotate-y="120"/>
    <box position="absolute" left="80" top="0" width="20" height="60" background="#ff0000"/>
  </box>
</composition>"##;
    // Not turned at frame 0: the red strip is at the card's right end.
    let flat = drawn(scene, 0);
    assert_eq!(at(&flat, 140, 50), RED);
    assert_eq!(at(&flat, 55, 50), WHITE);
    // Turned by 60 degrees at frame 5: the strip, flat from 130 to 150, is seen from 113.3 to 120.6.
    let turned = drawn(scene, 5);
    assert_eq!(at(&turned, 117, 50), RED);
    assert_eq!(at(&turned, 110, 50), WHITE);
    assert_eq!(at(&turned, 140, 50), BLACK);
}

#[test]
fn an_element_without_perspective_or_edge_on_is_not_turned_or_not_drawn() {
    // No distance for the eye: the angles have no effect.
    assert_eq!(drawn(&card(r#"rotate-y="60" rotate-x="20""#), 0), drawn(&card(""), 0));
    // Edge-on, nothing of it is seen.
    let pixels = drawn(&card(r#"perspective="200" rotate-y="90""#), 0);
    assert!(pixels.chunks(4).all(|pixel| pixel[..3] == BLACK), "an edge-on card shows nothing");
}

#[test]
fn a_distance_of_the_eye_that_is_not_above_0_and_a_transition_that_is_not_known_are_refused() {
    let refused = |attributes: &str| {
        let problems = xml::parse(&card(attributes), Path::new(".")).err().expect("the scene is refused");
        assert_eq!(problems.len(), 1, "{attributes}");
        assert_eq!(problems[0].line, 2);
        problems[0].message.clone()
    };
    assert_eq!(refused(r#"perspective="0" rotate-y="30""#), "`perspective` must be above 0, not 0");
    assert_eq!(refused(r#"perspective="-200""#), "`perspective` must be above 0, not -200");
    assert!(refused(r#"transition="fade""#).contains("`transition=\"fade\"` is not one of: none, wipe, cut, iris"));
}

/// A blue sheet over a red frame, with these attributes on the sheet.
fn sheet(attributes: &str) -> String {
    format!(
        r##"<composition width="200" height="100" fps="10" duration="1" background="#ff0000">
  <box left="0" top="0" width="200" height="100" background="#0000ff" {attributes}/>
</composition>"##
    )
}

#[test]
fn a_wipe_opens_from_the_left_with_a_soft_edge() {
    let pixels = drawn(&sheet(r#"transition="wipe" reveal="0.5""#), 0);
    // Half open: the edge lies from 45 to 55 percent of the width.
    assert_eq!(at(&pixels, 40, 50), BLUE);
    assert_eq!(at(&pixels, 88, 10), BLUE);
    assert_eq!(at(&pixels, 112, 90), RED);
    assert_eq!(at(&pixels, 160, 50), RED);
    let [red, _, blue] = at(&pixels, 100, 50);
    assert!(
        (100..=155).contains(&red) && (100..=155).contains(&blue),
        "the edge mixes the two: {red}, {blue}"
    );
}

#[test]
fn a_cut_opens_from_the_left_with_a_hard_edge() {
    let pixels = drawn(&sheet(r#"transition="cut" reveal="0.5""#), 0);
    // Half open: all of the sheet up to the middle, nothing of it beyond.
    assert_eq!(at(&pixels, 0, 0), BLUE);
    assert_eq!(at(&pixels, 99, 50), BLUE);
    assert_eq!(at(&pixels, 100, 50), RED);
    assert_eq!(at(&pixels, 199, 99), RED);
    assert!(drawn(&sheet(r#"transition="cut" reveal="0""#), 0)
        .chunks(4)
        .all(|pixel| pixel[..3] == RED));
    assert!(drawn(&sheet(r#"transition="cut" reveal="1""#), 0)
        .chunks(4)
        .all(|pixel| pixel[..3] == BLUE));
}

#[test]
fn a_cut_takes_the_children_moves_with_the_element_and_follows_its_tween() {
    // A sheet of 100 by 60 moved 50 to the right, with a white child across all of it.
    let scene = r##"<composition width="200" height="100" fps="10" duration="1" background="#ff0000">
  <box left="0" top="20" width="100" height="60" x="50" transition="cut">
    <box position="absolute" left="0" top="0" width="100" height="60" background="#ffffff"/>
    <tween at="0" duration="1" ease="linear" from-reveal="0" to-reveal="1"/>
  </box>
</composition>"##;
    // Three tenths open at frame 3: from 50 to 80.
    let pixels = drawn(scene, 3);
    assert_eq!(at(&pixels, 49, 50), RED);
    assert_eq!(at(&pixels, 51, 50), WHITE);
    assert_eq!(at(&pixels, 78, 50), WHITE);
    assert_eq!(at(&pixels, 81, 50), RED);
    assert_eq!(at(&pixels, 60, 18), RED);
}

#[test]
fn a_cut_joined_with_another_shape_of_the_element_is_refused() {
    let refused = |attributes: &str| {
        let problems = xml::parse(&sheet(attributes), Path::new(".")).err().expect("the file has problems");
        problems
            .iter()
            .map(|p| format!("{}: {}", p.line, p.message))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(
        refused(r#"transition="cut" clip-path="polygon(0% 0%, 100% 0%, 50% 100%)""#),
        "2: `transition=\"cut\"` cannot be joined with `clip-path`"
    );
    assert_eq!(
        refused(r#"transition="cut" overflow="hidden" border-radius="8""#),
        "2: `transition=\"cut\"` cannot be joined with `overflow=\"hidden\"` and rounded corners"
    );
    // A box that hides what overflows it and has square corners opens as any other.
    assert!(xml::parse(&sheet(r#"transition="cut" overflow="hidden""#), Path::new(".")).is_ok());
}

#[test]
fn an_iris_opens_from_the_middle() {
    let pixels = drawn(&sheet(r#"transition="iris" reveal="0.5""#), 0);
    // Half open: the circle reaches about half the way from the middle to a corner (55.9 of 111.8).
    assert_eq!(at(&pixels, 100, 50), BLUE);
    assert_eq!(at(&pixels, 140, 50), BLUE);
    assert_eq!(at(&pixels, 100, 10), BLUE);
    assert_eq!(at(&pixels, 170, 50), RED);
    assert_eq!(at(&pixels, 5, 5), RED);
    assert_eq!(at(&pixels, 195, 95), RED);
}

#[test]
fn an_opening_shows_nothing_at_0_all_at_1_and_follows_its_tween() {
    assert!(drawn(&sheet(r#"transition="wipe" reveal="0""#), 0)
        .chunks(4)
        .all(|pixel| pixel[..3] == RED));
    assert!(drawn(&sheet(r#"transition="iris" reveal="1""#), 0)
        .chunks(4)
        .all(|pixel| pixel[..3] == BLUE));
    // Without a transition `reveal` has no effect.
    assert!(drawn(&sheet(r#"reveal="0.3""#), 0).chunks(4).all(|pixel| pixel[..3] == BLUE));
    let scene = r##"<composition width="200" height="100" fps="10" duration="1" background="#ff0000">
  <box left="0" top="0" width="200" height="100" background="#0000ff" transition="wipe">
    <tween at="0" duration="1" ease="linear" from-reveal="0" to-reveal="1"/>
  </box>
</composition>"##;
    // A fifth open at frame 2, four fifths at frame 8.
    assert_eq!(at(&drawn(scene, 2), 20, 50), BLUE);
    assert_eq!(at(&drawn(scene, 2), 60, 50), RED);
    assert_eq!(at(&drawn(scene, 8), 150, 50), BLUE);
    assert_eq!(at(&drawn(scene, 8), 190, 50), RED);
}

/// A new, empty folder for one test, under the build's own temporary folder.
fn folder(name: &str) -> std::path::PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("turned-and-opening").join(name);
    if folder.exists() {
        std::fs::remove_dir_all(&folder).expect("the old test folder is removed");
    }
    std::fs::create_dir_all(&folder).expect("the test folder is made");
    folder
}

#[test]
fn an_opening_written_as_a_shader_is_read_from_its_file_once() {
    let folder = folder("read");
    std::fs::write(folder.join("mine.wgsl"), "// an opening\n").expect("the shader is written");
    let scene = r##"<composition width="200" height="100" fps="10" duration="1" background="#ff0000">
  <box left="0" top="0" width="100" height="100" background="#0000ff" transition="mine.wgsl" reveal="0.5"/>
  <box left="100" top="0" width="100" height="100" background="#00ff00" transition="mine.wgsl" reveal="0.5"/>
</composition>"##;
    let composition = xml::parse(scene, &folder).expect("the scene is valid");
    assert_eq!(composition.shaders.len(), 1);
    assert_eq!(composition.shaders[0].name, "mine.wgsl");
    assert_eq!(composition.shaders[0].source, "// an opening\n");
    // A file that is not there is the composition's fault, with the element's line.
    let problems = xml::parse(&scene.replace("mine.wgsl", "other.wgsl"), &folder)
        .err()
        .expect("the scene is refused");
    assert_eq!(problems[0].line, 2);
    assert!(
        problems[0].message.contains("the transition")
            && problems[0].message.contains("other.wgsl")
            && problems[0].message.contains("could not be read"),
        "{}",
        problems[0].message
    );
}

#[test]
fn the_cpu_renderer_refuses_an_opening_written_as_a_shader_before_any_frame() {
    let folder = folder("refused");
    std::fs::write(folder.join("mine.wgsl"), "// an opening\n").expect("the shader is written");
    let scene = r##"<composition width="200" height="100" fps="10" duration="1" background="#ff0000">
  <box left="0" top="0" width="200" height="100" background="#0000ff" transition="mine.wgsl">
    <tween at="0.5" duration="0.5" ease="linear" from-reveal="0" to-reveal="1"/>
  </box>
</composition>"##;
    std::fs::write(folder.join("composition.xml"), scene).expect("the composition is written");
    // The opening begins at frame 5; the render is refused although frame 0 could be drawn.
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_poc"))
        .arg("render")
        .arg(folder.join("composition.xml"))
        .args(["--backend", "cpu", "--no-encode", "--dump"])
        .arg(folder.join("dump"))
        .output()
        .expect("poc runs");
    assert!(!run.status.success(), "the render fails");
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(
        said.contains("the transition `mine.wgsl` is a shader, which only the GPU renderer draws"),
        "{said}"
    );
    let kept = std::fs::read_dir(folder.join("dump")).map_or(0, Iterator::count);
    assert_eq!(kept, 0, "no frame was drawn");
    // The library's painter says so too, should it be given such a frame.
    let mut composition = xml::parse(scene, &folder).expect("the scene is valid");
    layout::lay_out(&mut composition, &mut Shaper::new(), &[]);
    let mut cpu = Cpu::new(&composition, &[]);
    let reason = cpu
        .draw(7, &Evaluator::new(&composition).frame(7).ops)
        .expect_err("the frame is refused");
    assert!(reason.contains("a transition written as a shader"), "{reason}");
}
