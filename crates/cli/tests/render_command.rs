//! The `framesmith render` command, run as a program the way its user runs it: a folder in, an MP4 at
//! `renders/<name>.mp4` out, and nothing written for a project that is not drawn
//! (`docs/spec/render-command.md`).

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::project as make;
use common::*;
use measure::{open_pictures, video_facts};

const GROUP: &str = "render-command";

const FIRST: &str = "\
<composition version=\"1\" id=\"first\" width=\"96\" height=\"54\" fps=\"10\" duration=\"1\" background=\"#102040\">
<box left=\"12\" top=\"8\" width=\"40\" height=\"30\" background=\"#e0a030\"/>
</composition>
";

/// Every file under `dir`, with its bytes, sorted by path.
fn files(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push((path.clone(), fs::read(&path).unwrap()));
            }
        }
    }
    found.sort();
    found
}

fn render(cwd: &Path, project: &Path) -> std::process::Output {
    framesmith(cwd, [std::ffi::OsStr::new("render"), project.as_os_str()], &[])
}

#[test]
fn a_render_writes_the_film_at_renders_name_mp4_and_prints_the_summary_in_order() {
    let (project, cwd) = make(GROUP, "summary", FIRST);
    let output = render(&cwd, &project);
    assert_eq!(output.status.code(), Some(0), "{}{}", out(&output), err(&output));
    assert_eq!(err(&output), "");

    let printed = out(&output);
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines.len(), 14, "{printed}");
    assert_eq!(lines[0], format!("Rendered {}", project.join("renders/demo.mp4").display()));
    assert_eq!(
        &lines[1..6],
        [
            "frames: 10",
            "size: 96x54",
            "frame rate: 10 fps",
            "length: 1.000 s",
            "renderer: processor"
        ]
    );
    assert_eq!(
        &lines[6..13],
        [
            "output: renders/demo.mp4 (default)",
            "backend: auto (default)",
            "quality: standard (default)",
            "encoder: hardware (default)",
            "codec: h264 (default)",
            "workers: auto (default)",
            "audio bitrate: 192 kbit/s (default)",
        ]
    );
    let elapsed = lines[13]
        .strip_prefix("elapsed: ")
        .and_then(|time| time.strip_suffix(" s"))
        .unwrap_or_else(|| panic!("{}", lines[13]));
    assert!(
        elapsed.parse::<f64>().is_ok() && elapsed.split_once('.').unwrap().1.len() == 3,
        "{elapsed}"
    );

    // The file is where the project says, and is a video of the composition.
    let film = project.join("renders").join("demo.mp4");
    assert!(film.is_file());
    let facts = video_facts(&film).unwrap();
    assert_eq!((facts.container.as_str(), facts.codec.as_str()), ("mp4", "h264"));
    assert_eq!((facts.width, facts.height, facts.frames, facts.rate), (96, 54, 10, (10, 1)));
    assert!((facts.seconds() - 1.0).abs() < 1e-9);
}

#[test]
fn the_film_decodes_through_the_system_to_the_last_picture_and_shows_the_box_in_its_place() {
    let (project, cwd) = make(GROUP, "decodes", FIRST);
    assert_eq!(render(&cwd, &project).status.code(), Some(0));
    let mut pictures = open_pictures(&project.join("renders/demo.mp4")).unwrap();
    assert_eq!(pictures.count(), 10);
    let mut seen = 0;
    while let Some(frame) = pictures.next_frame().unwrap() {
        let near = |pixel: [u8; 3], want: [u8; 3]| pixel.iter().zip(want).all(|(a, b)| a.abs_diff(b) <= 12);
        assert!(near(frame.pixel(2, 2), [0x10, 0x20, 0x40]), "frame {seen}: {:?}", frame.pixel(2, 2));
        assert!(
            near(frame.pixel(30, 20), [0xe0, 0xa0, 0x30]),
            "frame {seen}: {:?}",
            frame.pixel(30, 20)
        );
        seen += 1;
    }
    assert_eq!(seen, 10);
}

#[test]
fn the_path_of_the_project_may_be_the_folder_the_project_file_or_the_composition_file() {
    for (name, argument) in [
        ("folder", ""),
        ("project-file", "framesmith.yaml"),
        ("composition-file", "composition.xml"),
    ] {
        let (project, cwd) = make(GROUP, &format!("path-{name}"), FIRST);
        fs::write(project.join("framesmith.yaml"), "name: shown\n").unwrap();
        let path = project.join(argument);
        let output = render(&cwd, if argument.is_empty() { &project } else { &path });
        assert_eq!(output.status.code(), Some(0), "{name}: {}", out(&output));
        assert!(project.join("renders/shown.mp4").is_file(), "{name}");
    }
}

#[test]
fn the_project_file_chooses_the_place_of_the_film() {
    let (project, cwd) = make(GROUP, "place", FIRST);
    fs::write(project.join("framesmith.yaml"), "paths:\n  renders: out\n").unwrap();
    assert_eq!(render(&cwd, &project).status.code(), Some(0));
    assert!(project.join("out/demo.mp4").is_file());
    assert!(!project.join("renders").exists());

    let (project, cwd) = project_again("place-output");
    fs::write(project.join("framesmith.yaml"), "render:\n  output: deep/er/film.mp4\n").unwrap();
    assert_eq!(render(&cwd, &project).status.code(), Some(0));
    assert!(project.join("deep/er/film.mp4").is_file());
}

fn project_again(name: &str) -> (PathBuf, PathBuf) {
    make(GROUP, name, FIRST)
}

#[test]
fn a_render_changes_no_file_of_the_author_and_adds_the_film_and_its_folder_only() {
    let (project, cwd) = make(GROUP, "author-files", FIRST);
    fs::write(project.join("framesmith.yaml"), "name: kept\n").unwrap();
    fs::create_dir_all(project.join("media")).unwrap();
    fs::write(project.join("media/note.txt"), "words").unwrap();
    let before = files(&project);
    assert_eq!(render(&cwd, &project).status.code(), Some(0));
    let after = files(&project);
    let added: Vec<&PathBuf> = after
        .iter()
        .map(|(path, _)| path)
        .filter(|path| !before.iter().any(|(old, _)| old == *path))
        .collect();
    assert_eq!(added, vec![&project.join("renders/kept.mp4")]);
    for (path, bytes) in &before {
        assert_eq!(&fs::read(path).unwrap(), bytes, "{}", path.display());
    }
    // The run log is under the folder the command is run from.
    assert!(log_path(&cwd).is_file());
}

#[test]
fn a_composition_that_is_not_valid_is_reported_as_check_reports_it_and_no_file_or_folder_is_made() {
    let text = "<composition version=\"1\" width=\"96\" height=\"54\" duration=\"1\">\n<box glow=\"1\"/>\n</composition>\n";
    let (project, cwd) = make(GROUP, "invalid", text);
    let before = files(&project);
    let output = render(&cwd, &project);
    assert_eq!(output.status.code(), Some(1));
    let check = framesmith(&cwd, [std::ffi::OsStr::new("check"), project.as_os_str()], &[]);
    assert_eq!(out(&output), out(&check));
    assert!(out(&output).contains("FS-0030"));
    assert_eq!(err(&output), "");
    assert_eq!(files(&project), before);
    assert!(!project.join("renders").exists());
}

#[test]
fn an_unknown_attribute_is_refused_with_its_code_and_line_and_no_file_while_the_twin_without_it_renders() {
    let wrong = FIRST.replace("height=\"30\"", "height=\"30\"\n     glow=\"1\"");
    let (project, cwd) = make(GROUP, "unknown-attribute", &wrong);
    let output = render(&cwd, &project);
    assert_eq!(output.status.code(), Some(1));
    let printed = out(&output);
    assert!(
        printed.contains("composition.xml, line 3: FS-0030: invalid: <box> has no attribute `glow`."),
        "{printed}"
    );
    assert!(
        printed.ends_with("Invalid: the project is not valid (composition.xml)\n"),
        "{printed}"
    );
    assert!(!project.join("renders").exists(), "no file and no folder");

    // The same composition without the attribute is drawn.
    let (twin, cwd) = make(GROUP, "unknown-attribute-twin", FIRST);
    assert_eq!(render(&cwd, &twin).status.code(), Some(0));
    assert!(twin.join("renders/demo.mp4").is_file());
}

#[test]
fn a_length_that_is_wrong_is_invalid_with_fs_0034_and_a_form_that_is_not_drawn_is_unsupported_with_exit_2() {
    let wrong = FIRST.replace("width=\"40\"", "width=\"-3\"");
    let (project, cwd) = make(GROUP, "length-invalid", &wrong);
    let output = render(&cwd, &project);
    assert_eq!(output.status.code(), Some(1));
    let printed = out(&output);
    assert!(printed.starts_with("composition.xml, line 2: FS-0034: invalid: "), "{printed}");
    assert!(printed.contains("`width=\"-3\"` is below 0; a size is 0 or more"), "{printed}");
    assert!(
        printed.ends_with("Invalid: the project is not valid (composition.xml)\n"),
        "{printed}"
    );
    assert!(!project.join("renders").exists());

    let text = FIRST.replace("<box ", "<text/>\n<box ");
    let (project, cwd) = make(GROUP, "form-unsupported", &text);
    let output = render(&cwd, &project);
    assert_eq!(output.status.code(), Some(2));
    let printed = out(&output);
    assert!(
        printed.starts_with("composition.xml, line 2: FS-0035: unsupported: <text> is not supported"),
        "{printed}"
    );
    assert!(
        printed.ends_with("Unsupported: the project uses what this version does not support (composition.xml)\n"),
        "{printed}"
    );
    assert!(!project.join("renders").exists());
}

#[test]
fn a_setting_that_is_not_applied_is_refused_with_the_line_of_its_value_in_the_project_file() {
    let (project, cwd) = make(GROUP, "setting", FIRST);
    fs::write(project.join("framesmith.yaml"), "name: demo\nrender:\n  quality: high\n").unwrap();
    let output = render(&cwd, &project);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        out(&output).starts_with("framesmith.yaml, line 3: FS-0035: unsupported: `render.quality: high` is not supported"),
        "{}",
        out(&output)
    );
    assert!(out(&output).ends_with("Unsupported: the project uses what this version does not support (framesmith.yaml)\n"));
    assert!(!project.join("renders").exists());
}

#[test]
fn a_path_that_is_not_a_project_gives_what_check_gives_with_no_file() {
    let base = scratch(GROUP, "no-project");
    let output = framesmith(&base, ["render", "nothing"], &[]);
    let check = framesmith(&base, ["check", "nothing"], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(out(&output), out(&check));
    assert!(out(&output).starts_with("No project: FS-0027: "));
    assert!(!base.join("nothing").exists());
}

#[test]
fn a_film_that_cannot_be_written_is_a_failure_with_fs_0036_and_exit_1_and_no_file() {
    let (project, cwd) = make(GROUP, "failure", FIRST);
    fs::write(project.join("framesmith.yaml"), "render:\n  output: composition.xml/film.mp4\n").unwrap();
    let output = render(&cwd, &project);
    assert_eq!(output.status.code(), Some(1));
    let printed = out(&output);
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines.len(), 2, "{printed}");
    assert!(lines[0].starts_with("FS-0036: failure: the video could not be made: "), "{printed}");
    assert!(lines[0]
        .ends_with("Put right what the message names (the folder, the disk or the system's video encoder), then run the command again."));
    assert_eq!(lines[1], "Failed: the video could not be made");
    assert_eq!(err(&output), "");
    assert!(!project.join("composition.xml/film.mp4").exists());
    assert_eq!(fs::read_to_string(project.join("composition.xml")).unwrap(), FIRST);
}

#[test]
fn a_command_line_that_cannot_be_read_is_told_on_standard_error_with_the_usage_line() {
    let base = scratch(GROUP, "command-line");
    for (args, message) in [
        (vec!["render"], "`render` needs the path of a project."),
        (
            vec!["render", "a", "b"],
            "`render` takes the path of one project, and b is one word too many.",
        ),
        (
            vec!["render", "--fast", "a"],
            "`--fast` is not an option of `render`; the options are: --output, --backend, --quality, --encoder, --codec, --workers.",
        ),
    ] {
        let output = framesmith(&base, args.clone(), &[]);
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        assert_eq!(out(&output), "", "{args:?}");
        let told = err(&output);
        assert!(
            told.starts_with(&format!("framesmith: FS-0017: invalid: {message} ")),
            "{args:?}: {told}"
        );
        assert!(told.ends_with("\nusage: framesmith render <path>\n"), "{args:?}: {told}");
    }
    assert!(!base.join("renders").exists());
}

#[test]
fn no_command_and_an_unknown_command_are_told_with_both_usage_lines() {
    let base = scratch(GROUP, "no-command");
    for args in [vec![], vec!["play", "demo"]] {
        let output = framesmith(&base, args.clone(), &[]);
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        assert!(
            err(&output).ends_with("\nusage: framesmith check <path>\nusage: framesmith render <path>\n"),
            "{args:?}: {}",
            err(&output)
        );
    }
}

#[test]
fn the_same_project_rendered_twice_gives_files_that_show_the_same_pictures() {
    let (project, cwd) = make(GROUP, "twice", FIRST);
    let film = project.join("renders/demo.mp4");
    let mut runs = Vec::new();
    for _ in 0..2 {
        assert_eq!(render(&cwd, &project).status.code(), Some(0));
        let mut pictures = open_pictures(&film).unwrap();
        let mut frames = Vec::new();
        while let Some(frame) = pictures.next_frame().unwrap() {
            frames.push(frame);
        }
        runs.push(frames);
    }
    assert_eq!(runs[0], runs[1]);
}

#[test]
fn a_render_logs_its_stages_in_order_under_one_run_id_with_counts_and_codes_only() {
    let (project, cwd) = make(GROUP, "log", FIRST);
    assert_eq!(
        framesmith(
            &cwd,
            [std::ffi::OsStr::new("render"), project.as_os_str()],
            &[("FRAMESMITH_LOG", "debug")]
        )
        .status
        .code(),
        Some(0)
    );
    let lines = lines(&cwd);
    let stages: Vec<(&str, String)> = lines
        .iter()
        .map(|line| (line.component.as_str(), line.text.split(' ').take(3).collect::<Vec<_>>().join(" ")))
        .collect();
    let position = |component: &str, text: &str| {
        stages
            .iter()
            .position(|(c, t)| *c == component && t.starts_with(text))
            .unwrap_or_else(|| panic!("{component} {text}: {}", log_text(&cwd)))
    };
    let order = [
        position("cli", "command started"),
        position("render::scene", "scene made"),
        position("render::film", "render started"),
        position("render::film", "encoder started"),
        position("render::film", "sound encoder started"),
        position("render::film", "sound encoder finished"),
        position("render::film", "render finished"),
        position("cli", "command finished"),
    ];
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}\n{}", log_text(&cwd));
    assert!(lines.iter().all(|line| line.run_id == lines[0].run_id && !line.run_id.is_empty()));
    assert!(
        lines.iter().all(|line| line.level != "ERROR" && line.level != "WARN"),
        "{}",
        log_text(&cwd)
    );

    let started = &lines[order[2]].text;
    assert!(
        started.contains("frames=10") && started.contains("width=96") && started.contains("height=54") && started.contains("fps=10"),
        "{started}"
    );
    assert!(
        started.contains("renderer=\"processor\"") || started.contains("renderer=processor"),
        "{started}"
    );
    let finished = &lines[order[6]].text;
    assert!(finished.contains("frames=10") && finished.contains("bytes="), "{finished}");
    let last = &lines[order[7]];
    assert_eq!(last.level, "INFO");
    assert!(
        last.text.contains("outcome=\"rendered\"") && last.text.contains("exit=0") && last.text.contains("frames=10"),
        "{}",
        last.raw
    );

    // Counts, stage names and codes only: nothing of the composition, no path.
    // (The first line holds the command line as it was typed; the lines of the render hold none of it.)
    let written = lines[1..].iter().map(|line| line.text.as_str()).collect::<Vec<_>>().join("\n");
    for word in ["#102040", "#e0a030", "composition.xml", "renders", "demo", "first", ".mp4"] {
        assert!(!written.contains(word), "{word} is in the log:\n{written}");
    }
    assert!(!written.contains(project.to_str().unwrap()));
}

#[test]
fn a_render_that_is_refused_or_fails_logs_its_outcome_at_the_level_of_the_spec() {
    let (project, cwd) = make(GROUP, "log-refused", &FIRST.replace("<box ", "<text/>\n<box "));
    assert_eq!(render(&cwd, &project).status.code(), Some(2));
    let last = lines(&cwd).pop().unwrap();
    assert_eq!(last.level, "WARN");
    assert!(
        last.text.contains("outcome=\"unsupported\"") && last.text.contains("exit=2") && last.text.contains("problems=1"),
        "{}",
        last.raw
    );

    let (project, cwd) = make(GROUP, "log-failed", FIRST);
    fs::write(project.join("framesmith.yaml"), "render:\n  output: composition.xml/film.mp4\n").unwrap();
    assert_eq!(render(&cwd, &project).status.code(), Some(1));
    let last = lines(&cwd).pop().unwrap();
    assert_eq!(last.level, "ERROR");
    assert!(
        last.text.contains("outcome=\"video not made\"") && last.text.contains("exit=1") && last.text.contains("code=\"FS-0036\"")
            || last.text.contains("code=FS-0036"),
        "{}",
        last.raw
    );
    assert!(
        !after_run_id(&log_text(&cwd)).contains("File exists"),
        "the reason of the system is not copied into the log"
    );
}
