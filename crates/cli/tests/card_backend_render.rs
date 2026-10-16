//! `framesmith render --backend gpu` on the first fixture: the summary names the card's renderer and
//! the option that chose it, and the film has the checked-in colours in every frame
//! (`docs/spec/render-command.md` sections 4 and 7). The card needs a graphics card that frx7 can
//! draw with, so the test is reference machine only (`docs/dev/gates.md` section 4).

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

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_backend_names_the_card_and_its_film_shows_the_checked_in_colours() {
    let (project, cwd) = make(GROUP, "card", &first_fixture());
    let project_text = project.display().to_string();
    let output = framesmith(&cwd, ["render", project_text.as_str(), "--backend", "gpu"], &[]);
    assert_eq!(output.status.code(), Some(0), "{}{}", out(&output), err(&output));
    assert_eq!(err(&output), "");

    let printed = out(&output);
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines.len(), 14, "{printed}");
    assert_eq!(lines[5], "renderer: card", "{printed}");
    assert_eq!(lines[7], "backend: gpu (option)", "{printed}");

    let film = project.join("renders").join("demo.mp4");
    let facts = video_facts(&film).unwrap();
    assert_eq!((facts.width, facts.height, facts.frames), (96, 54, 10));
    let mut pictures = open_pictures(&film).unwrap();
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
