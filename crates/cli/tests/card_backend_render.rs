//! `framesmith render` on the first fixture with the card: the summary names the card's renderer and
//! how it was chosen, and the film has the checked-in colours in every frame
//! (`docs/spec/render-command.md` sections 1.2, 4 and 7). The card needs a graphics card that frx7 can
//! draw with, so the tests are reference machine only (`docs/dev/gates.md` section 4).

mod common;

use std::fs;
use std::path::Path;

use common::err;
use common::framesmith;
use common::out;
use common::project as make;
use measure::{open_pictures, video_facts};

const GROUP: &str = "card-backend-render";

fn first_fixture() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/first-render/composition.xml");
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

const SAMPLES: [((u32, u32), [u8; 3]); 4] = [
    ((2, 2), [0x10, 0x20, 0x40]),
    ((60, 45), [0x10, 0x20, 0x40]),
    ((30, 20), [0xe0, 0xa0, 0x30]),
    ((45, 30), [0xe0, 0xa0, 0x30]),
];

fn near(pixel: [u8; 3], want: [u8; 3]) -> bool {
    pixel.iter().zip(want).all(|(a, b)| a.abs_diff(b) <= 12)
}

/// Standard error of a render of the first fixture (`docs/spec/render-command.md`, section 5.5):
/// every line names one of the four stages and carries no colour; footage has nothing to do and
/// says so once; frames, encoding and sound count to the ten frames of the fixture (a film always
/// holds a silent sound track, so sound counts even with no sound in the composition).
fn assert_first_fixture_progress(stderr: &str) {
    assert!(!stderr.is_empty(), "no progress on standard error");
    let mut footage_idle = 0;
    let mut reached = [false; 3];
    for line in stderr.lines() {
        assert!(!line.contains('\u{1b}'), "{line}");
        if line == "footage: nothing to do" {
            footage_idle += 1;
            continue;
        }
        let (stage, rest) = line.split_once(": ").unwrap_or_else(|| panic!("not a stage line: {line}"));
        let slot = match stage {
            "frames" => 0,
            "encoding" => 1,
            "sound" => 2,
            _ => panic!("not a spec stage: {line}"),
        };
        let counts = rest.strip_suffix(" frames").unwrap_or_else(|| panic!("not a count line: {line}"));
        let (done, total) = counts.split_once('/').unwrap_or_else(|| panic!("not a count line: {line}"));
        let done: u64 = done.parse().unwrap_or_else(|_| panic!("not a count: {line}"));
        assert_eq!(total, "10", "{line}");
        assert!((0..=10).contains(&done), "{line}");
        reached[slot] = reached[slot] || done == 10;
    }
    assert_eq!(footage_idle, 1, "{stderr}");
    assert!(reached == [true, true, true], "a counting stage never reached ten: {stderr}");
}

/// The film at `film` has ten frames of 96 by 54 pixels, and the checked-in colours at the sample
/// points of every frame.
fn assert_checked_in_colours(film: &Path) {
    let facts = video_facts(film).unwrap();
    assert_eq!((facts.width, facts.height, facts.frames), (96, 54, 10));
    let mut pictures = open_pictures(film).unwrap();
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

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_backend_names_the_card_and_its_film_shows_the_checked_in_colours() {
    let (project, cwd) = make(GROUP, "card", &first_fixture());
    let project_text = project.display().to_string();
    let output = framesmith(&cwd, ["render", project_text.as_str(), "--backend", "gpu"], &[]);
    assert_eq!(output.status.code(), Some(0), "{}{}", out(&output), err(&output));
    assert_first_fixture_progress(&err(&output));

    let printed = out(&output);
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines.len(), 14, "{printed}");
    assert_eq!(lines[5], "renderer: card", "{printed}");
    assert_eq!(lines[7], "backend: gpu (option)", "{printed}");

    assert_checked_in_colours(&project.join("renders").join("demo.mp4"));
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_auto_takes_the_card_when_no_backend_is_set_and_says_so() {
    let (project, cwd) = make(GROUP, "auto", &first_fixture());
    let project_text = project.display().to_string();
    let output = framesmith(&cwd, ["render", project_text.as_str()], &[]);
    assert_eq!(output.status.code(), Some(0), "{}{}", out(&output), err(&output));
    assert_first_fixture_progress(&err(&output));

    let printed = out(&output);
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines.len(), 14, "{printed}");
    assert_eq!(lines[5], "renderer: card (chosen by auto)", "{printed}");
    assert_eq!(lines[7], "backend: auto (default)", "{printed}");

    assert_checked_in_colours(&project.join("renders").join("demo.mp4"));
}
