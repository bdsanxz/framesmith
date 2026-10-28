//! `framesmith render` refuses an output path whose folder cannot be made or written before the first frame: it prints
//! `FS-0072` with the path and the system's reason and a remedy that names `--output`, then the closing line, and exits
//! 2 (decision A19); nothing is written, and a film already at a place that can be written is not touched
//! (`docs/spec/render-command.md`, sections 6 and 7).

mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use common::{err, framesmith, out, project};

const GROUP: &str = "output-place-refusal";

const COMPOSITION: &str = r##"<composition version="1" width="320" height="120" fps="30" duration="1" background="#ffffff">
<box left="0" top="0" width="40" height="40" background="#ff0000"/>
</composition>
"##;

fn names_in(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

#[test]
fn a_render_to_a_folder_that_cannot_be_written_is_refused_before_the_first_frame_with_exit_2_and_writes_nothing() {
    let (folder, cwd) = project(GROUP, "locked", COMPOSITION);
    let locked = folder.join("locked");
    fs::create_dir_all(&locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
    let output = locked.join("film.mp4");

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
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(rendered.status.code(), Some(2), "{}{}", out(&rendered), err(&rendered));
    assert_eq!(err(&rendered), "");
    let printed = out(&rendered);
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines.len(), 2, "one report and the closing line: {printed}");
    assert!(
        lines[0].starts_with(&format!(
            "FS-0072: failure: the output path `{}` cannot be made or written: Permission denied",
            output.display()
        )),
        "{}",
        lines[0]
    );
    assert!(
        lines[0]
            .ends_with("Give `--output` a path in a folder that can be written, or make the folder writable, then run the command again."),
        "{}",
        lines[0]
    );
    assert_eq!(lines[1], "Refused: the video was not made, because its folder cannot be written");
    assert!(names_in(&locked).is_empty(), "the refused folder holds {:?}", names_in(&locked));
    assert!(!folder.join("renders").exists(), "the default folder of the film was made");
}
