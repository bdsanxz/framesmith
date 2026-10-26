//! `framesmith render` writes its picture in the codec the author chose: `hevc` by `--codec` or `render.codec`, `h264`
//! otherwise, an option over the project file; the frame marks do not depend on the codec, and the summary names the codec
//! used (`docs/spec/render-command.md` sections 1.1, 5.3, 7 and 7.1). Run as a program, the way its user runs it. Which
//! codec a file holds is read from the four letters of its first picture sample entry (ISO/IEC 14496-12 and 14496-15:
//! `avc1` for H.264, `hvc1` for HEVC), by this test's own reading of the bytes.

mod common;

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use common::project as make;
use common::*;

const GROUP: &str = "picture-codec-choice";

/// A box that moves, one second at 10 frames a second.
const FILM: &str = "\
<composition version=\"1\" id=\"film\" width=\"96\" height=\"64\" fps=\"10\" duration=\"1\" background=\"#204060\">
<box left=\"16\" top=\"16\" width=\"32\" height=\"32\" background=\"#ffffff\">
<tween at=\"0\" duration=\"1\" to-x=\"48\"/>
</box>
</composition>
";

/// Renders `project` with `options`, the film written to `film.mp4` beside it, and gives the output and the film's path.
fn render(cwd: &Path, project: &Path, options: &[&str]) -> (Output, PathBuf) {
    let film = project.parent().unwrap().join("film.mp4");
    let mut args: Vec<OsString> = vec![
        "render".into(),
        project.as_os_str().to_owned(),
        "--output".into(),
        film.as_os_str().to_owned(),
    ];
    args.extend(options.iter().map(OsString::from));
    let output = framesmith(cwd, args, &[]);
    assert_eq!(output.status.code(), Some(0), "{}{}", out(&output), err(&output));
    (output, film)
}

/// The four letters of the first sample entry of each track, read from the `stsd` boxes of the tables, which lie in front
/// of the media: after `stsd`'s head come its version and flags, the count of entries, then the first entry's size and
/// its four letters.
fn sample_entries(film: &Path) -> Vec<String> {
    let bytes = fs::read(film).unwrap();
    let media = bytes.windows(4).position(|four| four == b"mdat").expect("an mdat box");
    let tables = &bytes[..media];
    (0..tables.len().saturating_sub(16))
        .filter(|&at| &tables[at..at + 4] == b"stsd")
        .map(|at| String::from_utf8_lossy(&tables[at + 16..at + 20]).into_owned())
        .collect()
}

fn summary_codec(output: &Output) -> String {
    let printed = out(output);
    printed
        .lines()
        .find(|line| line.starts_with("codec: "))
        .unwrap_or_else(|| panic!("no codec line in:\n{printed}"))
        .to_string()
}

/// The mark lines `--marks` prints.
fn marks(output: &Output) -> Vec<String> {
    out(output)
        .lines()
        .filter(|line| line.starts_with("mark "))
        .map(str::to_string)
        .collect()
}

#[test]
fn render_with_codec_hevc_writes_an_hevc_film_and_says_so() {
    let (project, cwd) = make(GROUP, "option-hevc", FILM);
    let (output, film) = render(&cwd, &project, &["--codec", "hevc"]);
    assert_eq!(sample_entries(&film), ["hvc1", "mp4a"]);
    assert_eq!(summary_codec(&output), "codec: hevc (option)");
    // The log's `render started` names the codec (`docs/spec/run-log.md`).
    let started = lines(&cwd)
        .into_iter()
        .find(|line| line.component == "render::film" && line.text.starts_with("render started "))
        .expect("a render started line");
    assert!(started.text.split_whitespace().any(|word| word == "codec=hevc"), "{}", started.raw);
}

#[test]
fn render_codec_hevc_in_the_project_file_writes_hevc_and_codec_h264_given_as_well_writes_h264() {
    let (project, cwd) = make(GROUP, "file-hevc", FILM);
    fs::write(project.join("framesmith.yaml"), "render:\n  codec: hevc\n").unwrap();
    let (output, film) = render(&cwd, &project, &[]);
    assert_eq!(sample_entries(&film), ["hvc1", "mp4a"]);
    assert_eq!(summary_codec(&output), "codec: hevc (project file)");

    let (output, film) = render(&cwd, &project, &["--codec", "h264"]);
    assert_eq!(sample_entries(&film), ["avc1", "mp4a"]);
    assert_eq!(summary_codec(&output), "codec: h264 (option)");
}

#[test]
fn with_no_codec_set_the_film_is_h264() {
    let (project, cwd) = make(GROUP, "default", FILM);
    let (output, film) = render(&cwd, &project, &[]);
    assert_eq!(sample_entries(&film), ["avc1", "mp4a"]);
    assert_eq!(summary_codec(&output), "codec: h264 (default)");
}

#[test]
fn the_frame_marks_printed_are_the_same_for_hevc_and_h264() {
    let (project, cwd) = make(GROUP, "marks", FILM);
    let (hevc, _) = render(&cwd, &project, &["--codec", "hevc", "--marks", "--backend", "cpu"]);
    let (h264, _) = render(&cwd, &project, &["--codec", "h264", "--marks", "--backend", "cpu"]);
    let (hevc, h264) = (marks(&hevc), marks(&h264));
    assert_eq!(hevc.len(), 10, "{hevc:?}");
    assert_eq!(hevc, h264);
    // The box moves, so the frames, and their marks, differ.
    let values: Vec<&str> = hevc.iter().map(|line| line.rsplit(' ').next().unwrap()).collect();
    assert!(values.windows(2).any(|pair| pair[0] != pair[1]), "{hevc:?}");
}
