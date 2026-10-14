//! `framesmith check` on projects that have a `framesmith.yaml`: what it reads, what it reports
//! and how it ends (`docs/spec/project-file.md` sections 4, 5 and 7).

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use common::*;
use project::Code;

const GROUP: &str = "project-file-check";

/// A composition of 5 seconds: 150 frames at 1280x720.
const FIVE_SECONDS: &str = r#"<composition version="1" width="1280" height="720" duration="5"/>"#;
/// A composition of 2 seconds: 60 frames at 640x360.
const TWO_SECONDS: &str = r#"<composition version="1" width="640" height="360" duration="2"/>"#;

/// A project `demo` with the files given (path from the project, content), and an empty folder to
/// run from. Returns `(project, cwd)`.
fn make(name: &str, files: &[(&str, &str)]) -> (PathBuf, PathBuf) {
    let base = scratch(GROUP, name);
    let project = base.join("demo");
    let cwd = base.join("cwd");
    fs::create_dir_all(&cwd).unwrap();
    for (path, content) in files {
        let at = project.join(path);
        fs::create_dir_all(at.parent().unwrap()).unwrap();
        fs::write(at, content).unwrap();
    }
    (project, cwd)
}

fn check(project: &Path, cwd: &Path) -> Output {
    framesmith(cwd, ["check".as_ref(), project.as_os_str()], &[])
}

/// What a report says after its place: the code, its kind, the sentence and the remedy.
fn statement(code: Code, message: &str) -> String {
    format!("{code}: {}: {message}. {}", code.kind().as_str(), code.remedy())
}

/// Every file under `dir` with its bytes, in order.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push((path.clone(), fs::read(&path).unwrap()));
            }
        }
    }
    files.sort();
    files
}

#[test]
fn a_project_file_that_sets_every_key_to_a_value_of_its_list_exits_0() {
    let yaml = "name: launch-film\ncomposition: scenes/main.xml\npaths:\n  renders: out\nrender:\n  output: out/launch-film.mp4\n  backend: gpu\n  quality: high\n  encoder: software\n  codec: hevc\n  workers: 16\n  audio-bitrate: 320\n";
    let (project, cwd) = make("every-key", &[("framesmith.yaml", yaml), ("scenes/main.xml", TWO_SECONDS)]);
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(0), "{}", out(&output));
    assert_eq!(out(&output), "Ready: the project is valid, 60 frames, 640x360, 30 fps\n");
    assert_eq!(err(&output), "");
}

#[test]
fn every_value_of_every_list_is_accepted_by_the_command() {
    let lists: [(&str, &[&str]); 5] = [
        ("backend", &["auto", "cpu", "gpu"]),
        ("quality", &["draft", "standard", "high"]),
        ("encoder", &["hardware", "software"]),
        ("codec", &["h264", "hevc"]),
        ("workers", &["auto", "1", "2", "4", "8", "16"]),
    ];
    for (key, values) in lists {
        for value in values {
            let yaml = format!("render:\n  {key}: {value}\n");
            let (project, cwd) = make("every-value", &[("framesmith.yaml", &yaml), ("composition.xml", FIVE_SECONDS)]);
            let output = check(&project, &cwd);
            assert_eq!(output.status.code(), Some(0), "{key}: {value}: {}", out(&output));
        }
    }
}

#[test]
fn an_empty_project_file_exits_0_and_reads_composition_xml() {
    for (name, yaml) in [("empty", ""), ("comments", "# nothing here\n")] {
        let (project, cwd) = make(
            name,
            &[
                ("framesmith.yaml", yaml),
                ("composition.xml", FIVE_SECONDS),
                // Another composition is there and is not the one read.
                ("scenes/main.xml", TWO_SECONDS),
            ],
        );
        let output = check(&project, &cwd);
        assert_eq!(output.status.code(), Some(0), "{name}");
        // 150 frames at 1280x720 are those of composition.xml.
        assert_eq!(
            out(&output),
            "Ready: the project is valid, 150 frames, 1280x720, 30 fps\n",
            "{name}"
        );
    }
    // The control: a wrong composition.xml is the one that is reported.
    let (project, cwd) = make(
        "empty-wrong-composition",
        &[
            ("framesmith.yaml", ""),
            (
                "composition.xml",
                r#"<composition version="1" width="641" height="720" duration="5"/>"#,
            ),
        ],
    );
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(1));
    assert!(out(&output).starts_with("composition.xml, line 1: FS-0007: "), "{}", out(&output));
}

#[test]
fn composition_names_the_file_that_is_read_and_its_frame_count_is_reported() {
    let (project, cwd) = make(
        "named-composition",
        &[
            ("framesmith.yaml", "composition: scenes/main.xml\n"),
            // The default composition is wrong, and is not read.
            ("composition.xml", "<composition/>"),
            ("scenes/main.xml", TWO_SECONDS),
        ],
    );
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(0), "{}", out(&output));
    assert_eq!(out(&output), "Ready: the project is valid, 60 frames, 640x360, 30 fps\n");
    // The control: the same project with the default file named reads the other one.
    fs::write(project.join("framesmith.yaml"), "composition: composition.xml\n").unwrap();
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(1));
    assert!(out(&output).starts_with("composition.xml, line 1: FS-0004: "), "{}", out(&output));
}

#[test]
fn a_problem_in_the_named_composition_is_reported_with_its_own_path() {
    let (project, cwd) = make(
        "named-composition-wrong",
        &[
            ("framesmith.yaml", "composition: scenes/main.xml\n"),
            (
                "scenes/main.xml",
                "<composition version=\"1\"\n width=\"640\" height=\"361\" duration=\"2\"/>",
            ),
        ],
    );
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        out(&output),
        format!(
            "scenes/main.xml, line 2: {}\nInvalid: the project is not valid (scenes/main.xml)\n",
            statement(
                Code::SizeNotAccepted,
                "`height` must be an even whole number from 2 to 8192, not 361"
            )
        )
    );
}

#[test]
fn a_key_that_is_not_listed_is_reported_with_its_line_and_its_name_and_the_exit_code_is_1() {
    let (project, cwd) = make(
        "unknown-key",
        &[
            ("framesmith.yaml", "name: demo\n\nspeed: fast\n"),
            ("composition.xml", FIVE_SECONDS),
        ],
    );
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(err(&output), "");
    assert_eq!(
        out(&output),
        format!(
            "framesmith.yaml, line 3: {}\nInvalid: the project is not valid (framesmith.yaml)\n",
            statement(
                Code::KeyNotListed,
                "`speed` is not a key of the project file; the keys are: name, composition, paths.renders, render.output, render.backend, render.quality, render.encoder, render.codec, render.workers, render.audio-bitrate"
            )
        )
    );
}

#[test]
fn a_value_outside_its_list_is_reported_with_its_line_and_the_values_taken() {
    let (project, cwd) = make(
        "values-outside",
        &[
            (
                "framesmith.yaml",
                "name: demo\nrender:\n  workers: 17\n  quality: high\n  codec: vp9\n",
            ),
            ("composition.xml", FIVE_SECONDS),
        ],
    );
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        out(&output),
        format!(
            "framesmith.yaml, line 3: {}\nframesmith.yaml, line 5: {}\nInvalid: the project is not valid (framesmith.yaml)\n",
            statement(
                Code::ValueNotAccepted,
                "`render.workers` must be auto or a whole number from 1 to 16, not 17"
            ),
            statement(Code::ValueNotAccepted, "`render.codec` must be one of h264, hevc, not vp9")
        )
    );
}

#[test]
fn two_wrong_keys_and_a_composition_with_one_wrong_value_are_all_reported_in_one_run() {
    let (project, cwd) = make(
        "three-problems",
        &[
            ("framesmith.yaml", "name: demo\nspeed: fast\nrender:\n  codec: vp9\n"),
            (
                "composition.xml",
                "<composition version=\"1\" width=\"640\" height=\"360\"\n duration=\"soon\"/>",
            ),
        ],
    );
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(1));
    let printed = out(&output);
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines.len(), 4, "{printed}");
    assert!(
        lines[0].starts_with("framesmith.yaml, line 2: FS-0022: invalid: `speed` "),
        "{printed}"
    );
    assert!(
        lines[1].starts_with("framesmith.yaml, line 4: FS-0024: invalid: `render.codec` "),
        "{printed}"
    );
    assert!(lines[2].starts_with("composition.xml, line 2: FS-0010: invalid: "), "{printed}");
    assert_eq!(lines[3], "Invalid: the project is not valid (framesmith.yaml, composition.xml)");
}

#[test]
fn a_file_that_cannot_be_read_as_yaml_is_reported_once_with_the_readers_reason_and_line() {
    let (project, cwd) = make(
        "not-yaml",
        &[
            ("framesmith.yaml", "name: demo\nrender:\n\tcodec: h264\n"),
            // Wrong too, and not read: nothing says which file is meant.
            ("composition.xml", "<composition/>"),
        ],
    );
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(err(&output), "");
    let printed = out(&output);
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines.len(), 2, "{printed}");
    assert!(
        lines[0].starts_with("framesmith.yaml, line 3: FS-0019: invalid: the file is not well-formed YAML: "),
        "{printed}"
    );
    assert!(lines[0].ends_with(Code::ProjectFileNotYaml.remedy()), "{printed}");
    assert_eq!(lines[1], "Invalid: the project is not valid (framesmith.yaml)");
    // A key written twice is a YAML fault with the reader's own words.
    fs::write(project.join("framesmith.yaml"), "name: a\nname: b\n").unwrap();
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        out(&output).lines().next().unwrap(),
        format!(
            "framesmith.yaml, line 2: {}",
            statement(
                Code::ProjectFileNotYaml,
                "the file is not well-formed YAML: duplicated key in mapping"
            )
        )
    );
}

#[test]
fn the_keys_compositions_and_timeline_are_listed_as_not_supported_and_end_with_exit_2() {
    let (project, cwd) = make(
        "unsupported-keys",
        &[
            ("framesmith.yaml", "name: demo\ncompositions:\n  - intro.xml\ntimeline: []\n"),
            ("composition.xml", FIVE_SECONDS),
        ],
    );
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(err(&output), "");
    assert_eq!(
        out(&output),
        format!(
            "framesmith.yaml, line 2: {}\nframesmith.yaml, line 4: {}\nUnsupported: the project uses what this version does not support (framesmith.yaml)\n",
            statement(
                Code::KeyNotSupported,
                "`compositions` is not supported in this version; it comes in Sprint 2"
            ),
            statement(
                Code::KeyNotSupported,
                "`timeline` is not supported in this version; it comes in Sprint 2"
            )
        )
    );
    // Anything invalid in the same run makes the exit code 1.
    fs::write(project.join("framesmith.yaml"), "timeline: []\nspeed: fast\n").unwrap();
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        out(&output).ends_with("Invalid: the project is not valid (framesmith.yaml)\n"),
        "{}",
        out(&output)
    );
}

#[test]
fn a_composition_that_is_named_and_missing_is_no_project_after_the_reports_of_the_file() {
    let (project, cwd) = make("named-missing", &[("framesmith.yaml", "composition: nothing.xml\n")]);
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(1));
    let printed = out(&output);
    assert_eq!(printed.lines().count(), 1, "{printed}");
    assert!(
        printed.starts_with("No project: FS-0025: invalid: the composition `nothing.xml`, named by `composition` on line 1 of framesmith.yaml, does not exist in `"),
        "{printed}"
    );
    assert!(printed.trim_end().ends_with(Code::NamedCompositionMissing.remedy()), "{printed}");

    fs::write(project.join("framesmith.yaml"), "composition: nothing.xml\nspeed: fast\n").unwrap();
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(1));
    let printed = out(&output);
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines.len(), 2, "{printed}");
    assert!(lines[0].starts_with("framesmith.yaml, line 2: FS-0022: "), "{printed}");
    assert!(lines[1].starts_with("No project: FS-0025: "), "{printed}");
}

#[test]
fn a_project_file_that_cannot_be_read_is_no_project_with_a_failure_code() {
    let (project, cwd) = make("project-file-unreadable", &[("composition.xml", FIVE_SECONDS)]);
    fs::create_dir(project.join("framesmith.yaml")).unwrap();
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(1));
    let printed = out(&output);
    assert_eq!(printed.lines().count(), 1, "{printed}");
    assert!(
        printed.starts_with("No project: FS-0020: failure: framesmith.yaml in `"),
        "{printed}"
    );
    assert!(printed.trim_end().ends_with(Code::ProjectFileUnreadable.remedy()), "{printed}");
}

#[test]
fn check_changes_nothing_of_a_project_that_has_a_project_file() {
    let (project, cwd) = make(
        "unchanged",
        &[
            ("framesmith.yaml", "name: demo\nrender:\n  workers: 17\n"),
            ("composition.xml", FIVE_SECONDS),
            ("media/clip.bin", "bytes"),
        ],
    );
    let before = snapshot(&project);
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(snapshot(&project), before);
    fs::write(project.join("framesmith.yaml"), "name: demo\n").unwrap();
    let before = snapshot(&project);
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(snapshot(&project), before);
    assert!(!project.join("renders").exists());
}
