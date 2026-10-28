//! A `<composition src>` with `start` alone, or with neither `start` nor `duration`, is drawn in the frames the
//! defaults give it: `start` is 0 of the parent's time and `duration` is the whole length of the file
//! (`docs/spec/notation.md` sections 6.9 and 10; `docs/spec/render-command.md` section 3).

mod common;

use common::scratch;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;
use std::fs;

/// The background of the composition that shows the file, and the colour of the box in the shown file.
const BACK: [u8; 4] = [16, 32, 64, 255];
const RED: [u8; 4] = [255, 0, 0, 255];

/// The shown file: 20 by 10 pixels, 10 frames a second, half a second long, with a red box over all of it.
const INNER: &str = "<composition version=\"1\" width=\"20\" height=\"10\" fps=\"10\" duration=\"0.5\" background=\"#000000\">\n\
<box left=\"0\" top=\"0\" width=\"20\" height=\"10\" background=\"#ff0000\"/>\n\
</composition>\n";

/// The numbers of the frames of a one-second composition in which the shown file covers the point (15, 15),
/// the shown file being placed at 10, 10 by `clip`, the attributes that its element takes after `src`.
/// Every other frame holds the background of the composition at the point (15, 15) and at (5, 5).
fn frames_with_the_shown_file(name: &str, clip: &str) -> Vec<u64> {
    let folder = scratch("shown-composition-clip-defaults", name);
    fs::create_dir_all(folder.join("scenes")).unwrap();
    fs::write(folder.join("scenes/inner.xml"), INNER).unwrap();
    let composition = format!(
        "<composition version=\"1\" width=\"80\" height=\"40\" fps=\"10\" duration=\"1\" background=\"#102040\">\n\
<composition src=\"scenes/inner.xml\" left=\"10\" top=\"10\"{clip}/>\n\
</composition>\n"
    );
    fs::write(folder.join("composition.xml"), composition).unwrap();
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    let scene = Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let mut shown = Vec::new();
    for number in 0..10u64 {
        let picture = scene.picture(number, &mut CpuRenderer).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(picture.pixel(5, 5), Some(BACK), "{name}: frame {number}, the point 5, 5");
        let colour = picture.pixel(15, 15);
        assert!(
            colour == Some(RED) || colour == Some(BACK),
            "{name}: frame {number}, the point 15, 15 is {colour:?}"
        );
        if colour == Some(RED) {
            shown.push(number);
        }
    }
    shown
}

#[test]
fn a_shown_file_with_a_start_and_no_duration_lasts_the_length_of_its_file_from_that_start() {
    // Start 0.2 s, and the file is 0.5 s long: the frames at 0.2 s to 0.6 s, which are the frames 2 to 6.
    assert_eq!(frames_with_the_shown_file("start-alone", " start=\"0.2\""), vec![2, 3, 4, 5, 6]);
}

#[test]
fn a_shown_file_with_neither_start_nor_duration_starts_at_0_and_lasts_the_length_of_its_file() {
    // No start: 0 of the parent's time. No duration: the file's 0.5 s, which are the frames 0 to 4.
    assert_eq!(frames_with_the_shown_file("neither", ""), vec![0, 1, 2, 3, 4]);
}
