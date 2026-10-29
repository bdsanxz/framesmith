//! A `media-start` on a `<video>` (`docs/spec/notation.md` N4; `docs/spec/check-command.md`): a negative one and one at
//! or past the end of the source are reported with the element's line, the path and the source's length, by `check`
//! and by `render`, which writes no film; garbage is the form's. A source that ends inside its clip passes `check`
//! with a note naming the source's and the clip's lengths. The clips are made at test time by frx7's writer.

mod common;

use std::fs;
use std::path::Path;

use common::{err, framesmith, out, project};
use measure::Frame;

fn flat(width: u32, height: u32, rgb: [u8; 3]) -> Frame {
    Frame {
        width,
        height,
        rgb: rgb.iter().copied().cycle().take((width * height * 3) as usize).collect(),
    }
}

fn composition(seconds: u32, inside: &str) -> String {
    format!(
        "<composition version=\"1\" width=\"96\" height=\"54\" fps=\"30\" duration=\"{seconds}\" background=\"#102040\">\n{inside}\n</composition>\n"
    )
}

fn has_a_film(project: &Path) -> bool {
    fs::read_dir(project.join("renders"))
        .map(|entries| {
            entries
                .flatten()
                .any(|entry| entry.path().extension().is_some_and(|ext| ext == "mp4"))
        })
        .unwrap_or(false)
}

fn coded(printed: &str) -> Vec<String> {
    printed.lines().filter(|line| line.contains("FS-")).map(str::to_string).collect()
}

/// Makes `source.mp4` of `pictures` numbered flat colours at 30 pictures a second in `folder`.
fn numbered_clip(folder: &Path, pictures: u32) {
    let frames: Vec<Frame> = (0..pictures)
        .map(|k| flat(32, 18, [((k % 8) * 32 + 4) as u8, 128, (((k / 8) % 8) * 32 + 4) as u8]))
        .collect();
    measure::write_video(&folder.join("source.mp4"), (30, 1), &frames).unwrap();
}

#[test]
fn a_negative_media_start_is_reported_with_the_element_line_the_path_and_the_source_length() {
    // A 12.5 s source (375 pictures at 30 a second) with `media-start="-1"`: `check` reports FS-0089 on the
    // element's line with the path and the source's length and exits 1; `render` reports the same and writes no film.
    let inside = "<video src=\"source.mp4\" width=\"32\" height=\"18\" left=\"8\" top=\"4\" media-start=\"-1\"/>";
    let (folder, cwd) = project("media-start", "negative", &composition(1, inside));
    numbered_clip(&folder, 375);
    let checked = framesmith(&cwd, ["check", folder.to_str().unwrap()], &[]);
    assert_eq!(checked.status.code(), Some(1), "{}{}", out(&checked), err(&checked));
    let lines = coded(&out(&checked));
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].contains("line 2: FS-0089")
            && lines[0].contains("`media-start=\"-1\"` is before the start of `source.mp4`, which is 12.5 s long"),
        "{lines:?}"
    );
    let rendered = framesmith(&cwd, ["render", folder.to_str().unwrap(), "--backend", "cpu"], &[]);
    assert_eq!(rendered.status.code(), Some(1), "{}{}", out(&rendered), err(&rendered));
    assert!(!has_a_film(&folder), "no film is written");
    assert_eq!(coded(&out(&rendered)), lines, "render reports what check reports");
}

#[test]
fn a_media_start_past_the_end_of_the_source_is_reported_the_same_way() {
    // The same 12.5 s source with `media-start="40"`: FS-0090, `past the end`, on the element's line, exit 1 both
    // commands, and no film.
    let inside = "<video src=\"source.mp4\" width=\"32\" height=\"18\" left=\"8\" top=\"4\" media-start=\"40\"/>";
    let (folder, cwd) = project("media-start", "past-end", &composition(1, inside));
    numbered_clip(&folder, 375);
    let checked = framesmith(&cwd, ["check", folder.to_str().unwrap()], &[]);
    assert_eq!(checked.status.code(), Some(1), "{}{}", out(&checked), err(&checked));
    let lines = coded(&out(&checked));
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].contains("line 2: FS-0090")
            && lines[0].contains("`media-start=\"40\"` is past the end of `source.mp4`, which is 12.5 s long"),
        "{lines:?}"
    );
    let rendered = framesmith(&cwd, ["render", folder.to_str().unwrap(), "--backend", "cpu"], &[]);
    assert_eq!(rendered.status.code(), Some(1), "{}{}", out(&rendered), err(&rendered));
    assert!(!has_a_film(&folder), "no film is written");
    assert_eq!(coded(&out(&rendered)), lines, "render reports what check reports");
}

#[test]
fn a_media_start_that_is_not_a_time_is_the_form_check() {
    // `media-start="ten"`: FS-0033 on the attribute's line, both commands, no film.
    let inside = "<video src=\"source.mp4\" width=\"32\" height=\"18\" left=\"8\" top=\"4\" media-start=\"ten\"/>";
    let (folder, cwd) = project("media-start", "garbage", &composition(1, inside));
    numbered_clip(&folder, 30);
    let checked = framesmith(&cwd, ["check", folder.to_str().unwrap()], &[]);
    assert_eq!(checked.status.code(), Some(1), "{}{}", out(&checked), err(&checked));
    let lines = coded(&out(&checked));
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].contains("line 2: FS-0033") && lines[0].contains("`media-start=\"ten\"` is not a time in seconds"),
        "{lines:?}"
    );
    let rendered = framesmith(&cwd, ["render", folder.to_str().unwrap(), "--backend", "cpu"], &[]);
    assert_eq!(rendered.status.code(), Some(1), "{}{}", out(&rendered), err(&rendered));
    assert!(!has_a_film(&folder), "no film is written");
    assert_eq!(coded(&out(&rendered)), lines, "render reports what check reports");
}

#[test]
fn a_source_shorter_than_its_clip_passes_check_with_a_note_naming_both_lengths() {
    // A 0.5 s source (15 pictures) in a 1 s clip: `check` passes and prints the N4 note with the element's line,
    // the source's length and the clip's length; `render` draws the clip with the last picture holding.
    let inside = "<video src=\"source.mp4\" width=\"32\" height=\"18\" left=\"8\" top=\"4\"/>";
    let (folder, cwd) = project("media-start", "short-note", &composition(1, inside));
    numbered_clip(&folder, 15);
    let checked = framesmith(&cwd, ["check", folder.to_str().unwrap()], &[]);
    assert_eq!(checked.status.code(), Some(0), "{}{}", out(&checked), err(&checked));
    let printed = out(&checked);
    assert!(
        printed.contains("note: composition.xml, line 2: `source.mp4` is 0.5 s long and its <video> lasts 1 s; its last picture holds"),
        "{printed}"
    );
}

#[test]
fn a_supported_media_start_passes_check_and_renders() {
    // The S45 case turned supported: `media-start="1"` on a real clip passes `check` and renders a film.
    let inside = "<video src=\"source.mp4\" width=\"32\" height=\"18\" left=\"8\" top=\"4\" media-start=\"1\"/>";
    let (folder, cwd) = project("media-start", "supported-form", &composition(1, inside));
    numbered_clip(&folder, 60);
    let checked = framesmith(&cwd, ["check", folder.to_str().unwrap()], &[]);
    assert_eq!(checked.status.code(), Some(0), "{}{}", out(&checked), err(&checked));
    let rendered = framesmith(&cwd, ["render", folder.to_str().unwrap(), "--backend", "cpu"], &[]);
    assert_eq!(rendered.status.code(), Some(0), "{}{}", out(&rendered), err(&rendered));
    assert!(has_a_film(&folder), "a film is written");
}
