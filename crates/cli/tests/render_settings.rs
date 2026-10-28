//! `framesmith render` takes every setting by one rule, an option over the project file over the
//! default, and its summary names what was used (`docs/spec/render-command.md` sections 1, 1.1, 6
//! and 7). Run as a program, the way its user runs it.

mod common;

use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::Output;

use common::project as make;
use common::*;

const GROUP: &str = "render-settings";

const FILM: &str = "\
<composition version=\"1\" id=\"film\" width=\"64\" height=\"48\" fps=\"10\" duration=\"0.5\" background=\"#204060\">
<box left=\"8\" top=\"8\" width=\"16\" height=\"16\" background=\"#ffffff\"/>
</composition>
";

fn render(cwd: &Path, project: &Path, options: &[&str]) -> Output {
    let mut args: Vec<OsString> = vec!["render".into(), project.as_os_str().to_owned()];
    args.extend(options.iter().map(OsString::from));
    framesmith(cwd, args, &[])
}

/// The summary line that starts with `label: `.
fn summary_line(output: &Output, label: &str) -> String {
    let printed = out(output);
    printed
        .lines()
        .find(|line| line.starts_with(&format!("{label}: ")))
        .unwrap_or_else(|| panic!("no `{label}` line in:\n{printed}"))
        .to_string()
}

fn assert_rendered(output: &Output) {
    assert_eq!(output.status.code(), Some(0), "{}{}", out(output), err(output));
    assert_eq!(err(output), "");
}

/// Every file under `dir`, as paths from it.
fn files(dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push(path.strip_prefix(dir).unwrap().to_string_lossy().into_owned());
            }
        }
    }
    found.sort();
    found
}

#[test]
fn the_output_of_the_project_file_is_used_and_the_output_option_wins_over_it() {
    let (project, cwd) = make(GROUP, "output", FILM);
    fs::write(project.join("framesmith.yaml"), "render:\n  output: from-file/film.mp4\n").unwrap();

    let output = render(&cwd, &project, &[]);
    assert_rendered(&output);
    assert_eq!(
        out(&output).lines().next().unwrap(),
        format!("Rendered {}", project.join("from-file/film.mp4").display())
    );
    assert!(project.join("from-file/film.mp4").is_file());
    assert_eq!(summary_line(&output, "output"), "output: from-file/film.mp4 (project file)");

    fs::remove_dir_all(project.join("from-file")).unwrap();
    let output = render(&cwd, &project, &["--output", "from-option/film.mp4"]);
    assert_rendered(&output);
    assert_eq!(
        out(&output).lines().next().unwrap(),
        format!("Rendered {}", project.join("from-option/film.mp4").display())
    );
    assert!(project.join("from-option/film.mp4").is_file());
    assert!(
        !project.join("from-file").exists(),
        "nothing is written at the project file's output"
    );
    assert_eq!(summary_line(&output, "output"), "output: from-option/film.mp4 (option)");

    // The `--output=<path>` form is the same option.
    let output = render(&cwd, &project, &["--output=equals/film.mp4"]);
    assert_rendered(&output);
    assert!(project.join("equals/film.mp4").is_file());
}

#[test]
fn with_no_output_the_film_is_at_paths_renders_name_mp4_and_paths_renders_changes_the_folder() {
    let (project, cwd) = make(GROUP, "renders", FILM);
    let output = render(&cwd, &project, &[]);
    assert_rendered(&output);
    assert_eq!(files(&project), ["composition.xml", "renders/demo.mp4"]);
    assert_eq!(summary_line(&output, "output"), "output: renders/demo.mp4 (default)");

    fs::remove_dir_all(project.join("renders")).unwrap();
    fs::write(project.join("framesmith.yaml"), "name: launch\npaths:\n  renders: finished\n").unwrap();
    let output = render(&cwd, &project, &[]);
    assert_rendered(&output);
    assert_eq!(files(&project), ["composition.xml", "finished/launch.mp4", "framesmith.yaml"]);
    assert_eq!(summary_line(&output, "output"), "output: finished/launch.mp4 (project file)");
}

#[test]
fn a_relative_output_is_taken_from_the_project_folder_and_its_missing_folders_are_made() {
    let (project, cwd) = make(GROUP, "relative", FILM);
    fs::write(project.join("framesmith.yaml"), "render:\n  output: a/b/file.mp4\n").unwrap();
    assert!(!project.join("a").exists());
    let output = render(&cwd, &project, &[]);
    assert_rendered(&output);
    assert!(project.join("a/b/file.mp4").is_file());

    let output = render(&cwd, &project, &["--output", "c/d/option.mp4"]);
    assert_rendered(&output);
    assert!(project.join("c/d/option.mp4").is_file());
    // Nothing is written under the folder the command is run from but the log.
    assert_eq!(files(&cwd), ["logs/framesmith.log"]);
}

#[test]
fn the_sound_bit_rate_of_the_project_file_is_used_and_the_default_is_192() {
    let (project, cwd) = make(GROUP, "bitrate", FILM);
    let output = render(&cwd, &project, &[]);
    assert_rendered(&output);
    assert_eq!(summary_line(&output, "audio bitrate"), "audio bitrate: 192 kbit/s (default)");

    fs::write(project.join("framesmith.yaml"), "render:\n  audio-bitrate: 128\n").unwrap();
    let output = render(&cwd, &project, &[]);
    assert_rendered(&output);
    assert_eq!(summary_line(&output, "audio bitrate"), "audio bitrate: 128 kbit/s (project file)");
}

#[test]
fn an_unknown_option_and_a_value_outside_its_list_are_told_with_the_values_taken_and_nothing_is_written() {
    for (name, options, message) in [
        (
            "unknown",
            vec!["--colour", "red"],
            "`--colour` is not an option of `render`; the options are: --output, --backend, --quality, --encoder, --codec, --workers, --marks.",
        ),
        (
            "json",
            vec!["--json"],
            "`--json` is not an option of `render`; the options are: --output, --backend, --quality, --encoder, --codec, --workers, --marks.",
        ),
        (
            "quality",
            vec!["--quality", "ultra"],
            "`--quality` must be one of draft, standard, high, not ultra.",
        ),
        ("codec", vec!["--codec=vp9"], "`--codec` must be one of h264, hevc, not vp9."),
        (
            "backend",
            vec!["--backend", "vulkan"],
            "`--backend` must be one of auto, cpu, gpu, not vulkan.",
        ),
        (
            "workers",
            vec!["--workers", "17"],
            "`--workers` must be auto or a whole number from 1 to 16, not 17.",
        ),
        ("no-value", vec!["--backend"], "`--backend` needs a value: one of auto, cpu, gpu."),
        (
            "twice",
            vec!["--quality", "standard", "--quality", "draft"],
            "`--quality` is given twice; give it once.",
        ),
    ] {
        let (project, cwd) = make(GROUP, &format!("refused-{name}"), FILM);
        let output = render(&cwd, &project, &options);
        assert_eq!(output.status.code(), Some(1), "{options:?}");
        assert_eq!(out(&output), "", "{options:?}");
        let told = err(&output);
        assert!(
            told.starts_with(&format!(
                "framesmith: FS-0037: invalid: {message} Give one of the options the message lists, \
                 with one of the values it names, or leave the option out.\n"
            )),
            "{options:?}: {told}"
        );
        assert!(
            !told.contains("framesmith check <path>` or"),
            "{options:?}: the remedy is not about the path: {told}"
        );
        assert!(told.ends_with("\nusage: framesmith render <path> [--output <path>] [--backend auto|cpu|gpu] [--quality draft|standard|high] [--encoder hardware|software] [--codec h264|hevc] [--workers auto|1-16] [--marks]\n"), "{options:?}: {told}");
        assert_eq!(files(&project), ["composition.xml"], "{options:?}: no file is written");
    }
}

#[test]
fn the_summary_names_each_setting_used_and_where_its_value_came_from() {
    let (project, cwd) = make(GROUP, "summary", FILM);
    fs::write(
        project.join("framesmith.yaml"),
        "render:\n  backend: auto\n  quality: high\n  audio-bitrate: 128\n",
    )
    .unwrap();
    let output = render(&cwd, &project, &["--backend", "cpu", "--quality", "standard", "--workers", "1"]);
    assert_rendered(&output);
    let printed = out(&output);
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines.len(), 14, "{printed}");
    assert_eq!(lines[5], "renderer: processor");
    assert_eq!(
        &lines[6..13],
        [
            "output: renders/demo.mp4 (default)",
            "backend: cpu (option)",
            "quality: standard (option)",
            "encoder: hardware (default)",
            "codec: h264 (default)",
            "workers: 1 (option)",
            "audio bitrate: 128 kbit/s (project file)",
        ]
    );
    assert!(lines[13].starts_with("elapsed: "), "{printed}");
}

#[test]
fn a_value_an_option_gives_that_this_version_does_not_apply_is_refused_before_the_first_frame() {
    let (project, cwd) = make(GROUP, "unsupported-option", FILM);
    let output = render(&cwd, &project, &["--encoder", "software"]);
    assert_eq!(output.status.code(), Some(2), "{}", out(&output));
    assert_eq!(
        out(&output),
        "command line: FS-0035: unsupported: `--encoder software` is given, which this version does not support \
         (support table S49: So far the film is made by the system's hardware encoder at its standard quality only; waits until Sprint 1, T085). \
         Remove what the message names, or wait for the sprint that brings it.\n\
         Unsupported: the project uses what this version does not support (command line)\n"
    );
    assert_eq!(files(&project), ["composition.xml"]);
}

#[test]
fn after_a_render_with_options_the_project_file_and_the_composition_have_the_same_bytes() {
    let (project, cwd) = make(GROUP, "unchanged", FILM);
    let yaml = "# kept as written\nname: launch\nrender:\n  output: out/film.mp4   # a comment\n  quality: draft\n  audio-bitrate: 128\n";
    fs::write(project.join("framesmith.yaml"), yaml).unwrap();
    let before = (
        fs::read(project.join("framesmith.yaml")).unwrap(),
        fs::read(project.join("composition.xml")).unwrap(),
    );
    let output = render(
        &cwd,
        &project,
        &["--output", "option.mp4", "--quality", "standard", "--workers", "auto"],
    );
    assert_rendered(&output);
    let after = (
        fs::read(project.join("framesmith.yaml")).unwrap(),
        fs::read(project.join("composition.xml")).unwrap(),
    );
    assert_eq!(before, after);
    assert_eq!(files(&project), ["composition.xml", "framesmith.yaml", "option.mp4"]);
}

#[test]
fn a_backend_option_wins_over_the_project_file_and_the_summary_names_the_option() {
    let (project, cwd) = make(GROUP, "backend-option-over-file", FILM);
    fs::write(project.join("framesmith.yaml"), "render:\n  backend: gpu\n").unwrap();
    let output = render(&cwd, &project, &["--backend", "cpu"]);
    assert_rendered(&output);
    assert_eq!(summary_line(&output, "renderer"), "renderer: processor");
    assert_eq!(summary_line(&output, "backend"), "backend: cpu (option)");
}
