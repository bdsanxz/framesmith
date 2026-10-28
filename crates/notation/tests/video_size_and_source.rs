//! A `<video>` names its file and its size: one without `src` is reported on its line, one without `width` or `height` on
//! its line, and a size that is not a whole number of 2 or more on the line of that attribute (`docs/spec/render-command.md`
//! section 4.8; `docs/spec/check-command.md`).

mod common;

use common::{problems_in, worded};

#[test]
fn a_video_without_a_height_is_reported_on_its_line() {
    let problems = problems_in("<box width=\"8\" height=\"8\"/>\n<video src=\"a.mp4\" width=\"32\"/>");
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(
        worded(&problems[0]),
        "line 3: FS-0085: <video> needs the attribute `height`, a whole number of 2 or more: footage is stretched to its box"
    );
}

#[test]
fn a_video_with_neither_size_is_reported_for_each() {
    let problems = problems_in("<video src=\"a.mp4\"/>");
    let worded: Vec<String> = problems.iter().map(worded).collect();
    assert_eq!(
        worded,
        [
            "line 2: FS-0085: <video> needs the attribute `width`, a whole number of 2 or more: footage is stretched to its box",
            "line 2: FS-0085: <video> needs the attribute `height`, a whole number of 2 or more: footage is stretched to its box",
        ]
    );
}

#[test]
fn a_size_below_two_is_reported_on_the_line_of_its_attribute() {
    let problems = problems_in("<video src=\"a.mp4\" width=\"32\"\n height=\"1\"/>");
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(
        worded(&problems[0]),
        "line 3: FS-0085: `height` of a <video> must be a whole number of 2 or more, not 1"
    );
}

#[test]
fn a_video_without_a_source_is_reported_on_its_line() {
    let problems = problems_in("\n<video width=\"32\" height=\"18\"/>");
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(
        worded(&problems[0]),
        "line 3: FS-0086: <video> needs the attribute `src`, the path of its file from the project folder"
    );
}

#[test]
fn a_video_with_its_source_and_size_is_not_reported() {
    let problems = problems_in("<video src=\"a.mp4\" width=\"32\" height=\"18\"/>");
    assert!(problems.is_empty(), "{problems:#?}");
}
