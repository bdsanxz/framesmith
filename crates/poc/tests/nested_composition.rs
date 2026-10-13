//! A composition that shows compositions kept in other files of the project.

use poc::cpu::Cpu;
use poc::model::{Dim, Kind};
use poc::scene::Evaluator;
use poc::text::Shaper;
use poc::{layout, xml, Backend};
use std::path::{Path, PathBuf};

fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nested")
}

/// The colour of the pixel at (x, y) of a 64 px wide frame.
fn pixel(backend: &mut dyn Backend, evaluator: &Evaluator, frame: u64, x: usize, y: usize) -> [u8; 3] {
    backend.draw(frame, &evaluator.frame(frame).ops).expect("the frame is drawn");
    let pixels = backend.pixels().expect("the frame is read");
    let at = (y * 64 + x) * 4;
    [pixels[at], pixels[at + 1], pixels[at + 2]]
}

#[test]
fn a_file_shown_in_a_composition_is_a_clip_of_its_own_size_and_length() {
    let composition = xml::parse_file(&project().join("host.xml"), &project()).expect("the project is valid");
    let card = &composition.root.children[0].children[0];
    assert_eq!(card.id, "card");
    assert!(matches!(card.kind, Kind::Box));
    // Its size is the file's, its window is the one written, and nothing shows outside its frame.
    assert_eq!(
        (card.geometry.width, card.geometry.height),
        (Some(Dim::Px(16.0)), Some(Dim::Px(16.0)))
    );
    let window = |node: &poc::model::Node| node.window.map(|(start, duration)| (start.to_f64(), duration.to_f64()));
    assert_eq!(window(card), Some((1.0, 2.0)));
    assert!(card.look.clip);
    assert!(card.look.background.is_empty(), "the file's canvas colour is not drawn");
    let ids: Vec<&str> = card.children.iter().map(|child| child.id.as_str()).collect();
    assert_eq!(ids, ["first", "second"]);
    // Shown again with no window written: from 0, for the file's own length.
    let again = &composition.root.children[1];
    assert_eq!(window(again), Some((0.0, 3.0)));
    // Each element has a key of its own, in both copies.
    assert_ne!(card.children[0].key, again.children[0].key);
}

#[test]
fn the_fonts_and_sounds_of_a_shown_file_join_the_composition() {
    let composition = xml::parse_file(&project().join("host.xml"), &project()).expect("the project is valid");
    // The font is listed once although the file is shown twice.
    let fonts: Vec<&Path> = composition.fonts.iter().map(|font| font.source.as_path()).collect();
    assert_eq!(fonts, [Path::new("fonts/card.ttf")]);
    let sounds: Vec<(&str, f64, f64)> = composition.audio.iter().map(|s| (s.id.as_str(), s.start, s.duration)).collect();
    // The first copy starts at 1 + 1 on the timeline and lasts 2 s: its sound starts half a second
    // in and is cut at the clip's end. The second starts at 0 and lasts 3 s.
    assert_eq!(sounds, [("bed", 0.0, 6.0), ("ping", 2.5, 1.5), ("ping", 0.5, 2.5)]);
    assert_eq!(composition.audio[1].volume, 0.5);
}

#[test]
fn a_shown_file_counts_its_time_from_its_clip_and_is_cut_at_its_frame() {
    let mut composition = xml::parse_file(&project().join("host.xml"), &project()).expect("the project is valid");
    layout::lay_out(&mut composition, &mut Shaper::new(), &[]);
    let evaluator = Evaluator::new(&composition);
    let mut cpu = Cpu::new(&composition, &[]);
    // The first copy is at (8, 4) and starts at frame 20: green for a second, then blue.
    assert_eq!(pixel(&mut cpu, &evaluator, 19, 10, 6), [0, 0, 0]);
    assert_eq!(pixel(&mut cpu, &evaluator, 20, 10, 6), [0, 255, 0]);
    assert_eq!(pixel(&mut cpu, &evaluator, 30, 10, 6), [0, 0, 0]);
    assert_eq!(pixel(&mut cpu, &evaluator, 30, 18, 14), [0, 0, 255]);
    // The file's canvas colour is not drawn, and the blue square stops at the file's frame:
    // 16 px from (8, 4), so at x = 24 and y = 20.
    assert_eq!(pixel(&mut cpu, &evaluator, 30, 22, 6), [0, 0, 0]);
    assert_eq!(pixel(&mut cpu, &evaluator, 30, 23, 19), [0, 0, 255]);
    assert_eq!(pixel(&mut cpu, &evaluator, 30, 25, 19), [0, 0, 0]);
    assert_eq!(pixel(&mut cpu, &evaluator, 30, 23, 21), [0, 0, 0]);
    // The clip ends at frame 40.
    assert_eq!(pixel(&mut cpu, &evaluator, 40, 18, 14), [0, 0, 0]);
    // The second copy, at (40, 0), runs from frame 0.
    assert_eq!(pixel(&mut cpu, &evaluator, 0, 42, 2), [0, 255, 0]);
    assert_eq!(pixel(&mut cpu, &evaluator, 10, 50, 10), [0, 0, 255]);
}

#[test]
fn a_file_that_shows_itself_through_another_is_a_problem() {
    let problems = xml::parse_file(&project().join("scenes/loop-a.xml"), &project())
        .err()
        .expect("the file has a problem");
    let found: Vec<String> = problems.iter().map(ToString::to_string).collect();
    assert_eq!(
        found,
        ["scenes/loop-b.xml, line 2: `scenes/loop-a.xml` shows itself, directly or through the files it shows"]
    );
}

#[test]
fn the_problems_of_a_shown_file_name_the_file() {
    let problems = xml::parse_file(&project().join("faults-host.xml"), &project())
        .err()
        .expect("the files have problems");
    let found: Vec<String> = problems.iter().map(ToString::to_string).collect();
    let missing = project().join("scenes/missing.xml");
    assert_eq!(found.len(), 6, "{found:#?}");
    assert_eq!(
        found[0],
        "scenes/faults.xml, line 2: `background=\"green\"` is not a colour or a list of gradients"
    );
    assert_eq!(
        found[1],
        "line 2: `scenes/faults.xml` has 25 frames a second and the composition that shows it has 10; they must be the same"
    );
    assert!(
        found[2].starts_with(&format!("line 3: the composition `{}` could not be read: ", missing.display())),
        "{}",
        found[2]
    );
    assert_eq!(found[3], "line 4: <composition> has no attribute `width`");
    assert_eq!(
        found[4],
        "line 4: <rect> is not known in a <composition> that names a file; only <tween> and <set> are"
    );
    assert_eq!(found[5], "line 5: a <composition> inside a composition needs the attribute `src`");
}
