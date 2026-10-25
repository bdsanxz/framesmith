//! What the tests of a `<font>` file share: a composition of one `<font>` on line 2 and one `<text>` on line 3 that names the
//! family of that font, the bytes of the test font, and the report that `check` and `render` give on a project of them.
//! Each command runs on a project folder of its own, and `render` is given `--backend cpu`.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use crate::common::{err, framesmith, out, project};

/// The family the faces of the test font are named by.
pub const FAMILY: &str = "Framesmith Test Stroke";

/// The bytes of the test font of the repository (`fixtures/fonts`, made by us).
pub fn test_font() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts/framesmith-test-stroke-400.ttf");
    fs::read(path).unwrap()
}

/// A composition whose line 2 is a `<font>` with `src` and whose line 3 is a `<text>` in the family of that font.
pub fn composition(src: &str) -> String {
    format!(
        "<composition version=\"1\" width=\"200\" height=\"60\" fps=\"10\" duration=\"1\" background=\"#ffffff\">\n\
         <font src=\"{src}\" family=\"{FAMILY}\"/>\n\
         <text left=\"10\" top=\"12\" font-family=\"{FAMILY}\" font-size=\"30\" color=\"#202020\">HIIH</text>\n\
         </composition>\n"
    )
}

/// What one command printed and wrote.
pub struct Run {
    /// The project folder the command ran on.
    pub project: PathBuf,
    pub status: Option<i32>,
    pub printed: String,
    /// The lines of the report that name a code.
    pub lines: Vec<String>,
    /// Whether a video file is in the project's `renders` folder.
    pub film: bool,
}

fn film_written(project: &Path) -> bool {
    fs::read_dir(project.join("renders"))
        .map(|entries| {
            entries
                .flatten()
                .any(|entry| entry.path().extension().is_some_and(|ext| ext == "mp4"))
        })
        .unwrap_or(false)
}

/// Runs `check` and then `render` on a project of `composition_text` and of the `files` (each a path from the project folder
/// and its bytes), each in a project folder of its own, and returns the two runs in that order.
pub fn check_and_render(group: &str, name: &str, composition_text: &str, files: &[(&str, Vec<u8>)]) -> (Run, Run) {
    let mut runs = Vec::new();
    for command in ["check", "render"] {
        let (project, cwd) = project(group, &format!("{name}-{command}"), composition_text);
        for (path, bytes) in files {
            let file = project.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, bytes).unwrap();
        }
        let project_text = project.to_str().unwrap().to_string();
        let args: Vec<String> = if command == "check" {
            vec!["check".into(), project_text]
        } else {
            vec!["render".into(), project_text, "--backend".into(), "cpu".into()]
        };
        let output = framesmith(&cwd, args, &[]);
        let printed = format!("{}{}", out(&output), err(&output));
        let lines = printed.lines().filter(|line| line.contains("FS-")).map(str::to_string).collect();
        let film = film_written(&project);
        runs.push(Run {
            project,
            status: output.status.code(),
            printed,
            lines,
            film,
        });
    }
    let render = runs.pop().unwrap();
    let check = runs.pop().unwrap();
    (check, render)
}

/// Asserts that both commands end with the exit code `status`, print the same report of codes, and that `render` prints no
/// film and writes none. Returns the first line of that report.
pub fn refused_the_same_way(check: &Run, render: &Run, status: i32) -> String {
    assert_eq!(check.status, Some(status), "check ends with {status}: {}", check.printed);
    assert_eq!(render.status, Some(status), "render ends with {status}: {}", render.printed);
    assert!(!render.printed.contains("Rendered "), "render prints no film: {}", render.printed);
    assert!(!render.film, "render writes no film: {}", render.printed);
    assert!(
        !render.project.join("renders").exists(),
        "render makes no renders folder: {}",
        render.printed
    );
    assert_eq!(check.lines, render.lines, "render gives the report that check gives");
    assert!(!check.lines.is_empty(), "the report names a code: {}", check.printed);
    check.lines[0].clone()
}
