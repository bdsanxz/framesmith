//! What the output path may end in: a film is written to `.mp4` or `.m4v`; a path ending in
//! `.mov`, or in an extension no writer claims, is refused before the first frame, with no file left
//! (`docs/spec/render-command.md`, section 6; FS-0036 until S34 is built).

mod common;

use common::{err, framesmith, out, project};
use measure::video_facts;

const GROUP: &str = "film-output-extension";

const COMPOSITION: &str = r##"<composition version="1" width="32" height="18" fps="30" duration="1" background="#204060">
<box left="4" top="4" width="8" height="6" background="#e0a030"/>
</composition>
"##;

fn render_to(name: &str, file: &str) -> (std::path::PathBuf, std::process::Output) {
    let (folder, cwd) = project(GROUP, name, COMPOSITION);
    let output = folder.join(file);
    let rendered = framesmith(
        &cwd,
        [
            "render",
            folder.to_str().unwrap(),
            "--backend",
            "cpu",
            "--output",
            output.to_str().unwrap(),
        ],
        &[("NO_COLOR", "1")],
    );
    (output, rendered)
}

#[test]
fn a_render_to_a_mov_path_is_refused_before_the_first_frame_with_exit_1_and_writes_nothing() {
    let (output, rendered) = render_to("mov-refused", "film.mov");
    assert_eq!(rendered.status.code(), Some(1), "{}{}", out(&rendered), err(&rendered));
    assert_eq!(err(&rendered), "");
    let printed = out(&rendered);
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines.len(), 2, "one report and the closing line: {:?}", lines);
    assert!(
        lines[0].starts_with("FS-0036: failure: the video could not be made: no container format of this build writes `.mov` files"),
        "{:?}",
        lines
    );
    assert_eq!(lines[1], "Failed: the video could not be made", "{:?}", lines);
    assert!(!output.exists(), "a refused render leaves no file at its path");
}

#[test]
fn mp4_and_m4v_paths_write_films_that_read_back() {
    for (name, file, frames) in [("mp4-written", "film.mp4", 30), ("m4v-written", "film.m4v", 30)] {
        let (output, rendered) = render_to(name, file);
        assert_eq!(rendered.status.code(), Some(0), "{}{}", out(&rendered), err(&rendered));
        assert!(output.is_file(), "the film is written");
        let facts = video_facts(&output).expect("the film reads back");
        assert_eq!(facts.container, "mp4", "the film reads as MP4");
        assert_eq!(facts.frames, frames, "the film holds its frames");
        assert_eq!((facts.width, facts.height), (32, 18), "the film holds its size");
    }
}
