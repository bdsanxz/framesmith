//! `framesmith render --backend cpu` on the first fixture: the summary names the processor's renderer
//! and the option that chose it, and every frame of the film shows the checked-in colours
//! (`docs/spec/render-command.md` sections 4 and 7). The render crate's tests hold the checked-in
//! pictures to the byte; the film is encoded, so here its colours are checked within a tolerance at
//! points away from the edges of the box.

mod common;

use std::fs;
use std::path::Path;

use common::err;
use common::framesmith;
use common::out;
use common::project as make;
use measure::{open_pictures, video_facts};

const GROUP: &str = "cpu-backend-render";

/// The composition of the first fixture, read from the fixtures folder.
fn first_fixture() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/first-render/composition.xml");
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Points of the frame and the colour the checked-in pictures hold there: the canvas `#102040` and
/// the box `#e0a030`.
const SAMPLES: [((u32, u32), [u8; 3]); 4] = [
    ((2, 2), [0x10, 0x20, 0x40]),
    ((60, 45), [0x10, 0x20, 0x40]),
    ((30, 20), [0xe0, 0xa0, 0x30]),
    ((45, 30), [0xe0, 0xa0, 0x30]),
];

fn near(pixel: [u8; 3], want: [u8; 3]) -> bool {
    pixel.iter().zip(want).all(|(a, b)| a.abs_diff(b) <= 12)
}

#[test]
fn the_summary_names_the_processor_and_the_option_that_chose_it() {
    let (project, cwd) = make(GROUP, "summary", &first_fixture());
    let project_text = project.display().to_string();
    let output = framesmith(&cwd, ["render", project_text.as_str(), "--backend", "cpu"], &[]);
    assert_eq!(output.status.code(), Some(0), "{}{}", out(&output), err(&output));
    // The run tells its progress on standard error (section 5.5): the fixture shows no footage, so
    // footage says so once, and the other stages end at the frame count.
    let stderr = err(&output);
    assert_eq!(
        stderr.lines().filter(|line| *line == "footage: nothing to do").count(),
        1,
        "{stderr}"
    );
    for line in stderr.lines().filter(|line| !line.ends_with("nothing to do")) {
        let (stage, rest) = line.split_once(": ").expect(line);
        assert!(["frames", "encoding", "sound"].contains(&stage), "{line}");
        let (done, total) = rest.strip_suffix(" frames").expect(line).split_once('/').expect(line);
        assert_eq!(total, "10", "{line}");
        assert!(done.parse::<u64>().expect(line) <= 10, "{line}");
    }
    assert!(stderr.lines().any(|line| line == "frames: 10/10 frames"), "{stderr}");

    let printed = out(&output);
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines.len(), 14, "{printed}");
    assert_eq!(lines[5], "renderer: processor", "{printed}");
    assert_eq!(lines[7], "backend: cpu (option)", "{printed}");
    assert!(!printed.contains("renderer: card"), "{printed}");
}

#[test]
fn every_frame_of_the_film_shows_the_checked_in_colours_at_the_sampled_points() {
    let (project, cwd) = make(GROUP, "colours", &first_fixture());
    let project_text = project.display().to_string();
    let output = framesmith(&cwd, ["render", project_text.as_str(), "--backend", "cpu"], &[]);
    assert_eq!(output.status.code(), Some(0), "{}{}", out(&output), err(&output));

    let film = project.join("renders").join("demo.mp4");
    let facts = video_facts(&film).unwrap();
    assert_eq!((facts.width, facts.height, facts.frames), (96, 54, 10));

    let mut pictures = open_pictures(&film).unwrap();
    assert_eq!(pictures.count(), 10);
    let mut seen = 0;
    while let Some(frame) = pictures.next_frame().unwrap() {
        for ((x, y), want) in SAMPLES {
            let got = frame.pixel(x, y);
            assert!(near(got, want), "frame {seen} at ({x}, {y}): {got:?} against {want:?}");
        }
        seen += 1;
    }
    assert_eq!(seen, 10);
}
