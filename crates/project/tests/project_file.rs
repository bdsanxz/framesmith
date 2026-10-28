//! Opening a project that has a `framesmith.yaml`: its keys read and judged, the defaults, the
//! composition it names, and every problem of both files in one run (`docs/spec/project-file.md`).

use project::{Backend, Code, Codec, Encoder, Kind, NoProject, OpenError, Project, Quality, Report, Workers};
use std::fs;
use std::path::{Path, PathBuf};

/// A new folder named `name` for one test, under the tests' scratch folder.
fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("project-file").join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A composition of 5 seconds at the default rate: 150 frames.
const FIVE_SECONDS: &str = r#"<composition version="1" id="demo" width="1280" height="720" duration="5"/>"#;
/// A composition of 2 seconds: 60 frames.
const TWO_SECONDS: &str = r#"<composition version="1" width="640" height="360" duration="2"/>"#;

/// A project folder whose `composition.xml` is `FIVE_SECONDS` and whose `framesmith.yaml` is `yaml`.
fn project_with(name: &str, yaml: &[u8]) -> PathBuf {
    let dir = folder(name);
    fs::write(dir.join("composition.xml"), FIVE_SECONDS).unwrap();
    fs::write(dir.join("framesmith.yaml"), yaml).unwrap();
    dir
}

/// The reports of a project that did not open as valid, or a failure of the test.
fn reports(dir: &Path) -> Vec<Report> {
    match Project::open(dir) {
        Err(OpenError::Invalid(reports)) => reports,
        other => panic!("expected reports, got {other:?}"),
    }
}

/// The reports as (file, line, code) triples.
fn places(reports: &[Report]) -> Vec<(String, u32, Code)> {
    reports
        .iter()
        .map(|report| (report.file().to_string(), report.problem().line(), report.problem().code()))
        .collect()
}

/// The one report of a project file with one wrong thing in it.
fn the_report(name: &str, yaml: &str) -> Report {
    let dir = project_with(name, yaml.as_bytes());
    let mut all = reports(&dir);
    assert_eq!(all.len(), 1, "{name}: {all:#?}");
    all.remove(0)
}

#[test]
fn an_empty_project_file_is_valid_and_the_composition_is_the_default_one() {
    for (name, yaml) in [
        ("empty", ""),
        ("comments", "# nothing yet\n\n# still nothing\n"),
        ("document-marker", "---\n"),
        ("byte-order-mark-only", "\u{feff}"),
        ("blank-lines", "\n\n  \n"),
    ] {
        let dir = project_with(name, yaml.as_bytes());
        let project = Project::open(&dir).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(project.root().frames, 150, "{name}");
        assert_eq!(project.settings().composition(), "composition.xml", "{name}");
        assert_eq!(project.settings().name_written(), None, "{name}");
    }
}

#[test]
fn a_folder_with_no_project_file_has_the_same_settings_as_an_empty_one() {
    let dir = folder("no-project-file");
    fs::write(dir.join("composition.xml"), FIVE_SECONDS).unwrap();
    let without = Project::open(&dir).unwrap();
    let with_empty = Project::open(&project_with("empty-for-comparison", b"")).unwrap();
    assert_eq!(without.settings(), with_empty.settings());
}

#[test]
fn every_setting_has_its_default_when_the_file_writes_none() {
    let dir = folder("defaults").join("my-film");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("composition.xml"), FIVE_SECONDS).unwrap();
    let project = Project::open(&dir).unwrap();
    let settings = project.settings();
    assert_eq!(settings.composition(), "composition.xml");
    assert_eq!(settings.renders(), "renders");
    assert_eq!(settings.backend(), Backend::Auto);
    assert_eq!(settings.quality(), Quality::Standard);
    assert_eq!(settings.encoder(), Encoder::Hardware);
    assert_eq!(settings.codec(), Codec::H264);
    assert_eq!(settings.workers(), Workers::Auto);
    assert_eq!(settings.audio_bitrate(), 192);
    // The name is the folder's, and the output follows from the renders folder and the name.
    assert_eq!(project.name(), "my-film");
    assert_eq!(project.output(), "renders/my-film.mp4");
}

#[test]
fn a_file_that_writes_every_key_sets_every_setting() {
    let dir = project_with(
        "complete",
        b"name: launch-film\ncomposition: composition.xml\npaths:\n  renders: out\nrender:\n  output: out/final.mp4\n  backend: cpu\n  quality: high\n  encoder: software\n  codec: hevc\n  workers: 4\n  audio-bitrate: 256\n",
    );
    let project = Project::open(&dir).unwrap();
    let settings = project.settings();
    assert_eq!(project.name(), "launch-film");
    assert_eq!(settings.composition(), "composition.xml");
    assert_eq!(settings.renders(), "out");
    assert_eq!(project.output(), "out/final.mp4");
    assert_eq!(settings.backend(), Backend::Cpu);
    assert_eq!(settings.quality(), Quality::High);
    assert_eq!(settings.encoder(), Encoder::Software);
    assert_eq!(settings.codec(), Codec::Hevc);
    assert_eq!(settings.workers(), Workers::Count(4));
    assert_eq!(settings.audio_bitrate(), 256);
}

#[test]
fn every_word_of_every_key_is_accepted() {
    let cases: [(&str, &[&str]); 4] = [
        ("backend", &["auto", "cpu", "gpu"]),
        ("quality", &["draft", "standard", "high"]),
        ("encoder", &["hardware", "software"]),
        ("codec", &["h264", "hevc"]),
    ];
    for (key, words) in cases {
        for word in words {
            for written in [word.to_string(), format!("\"{word}\""), format!("'{word}'")] {
                let dir = project_with("every-word", format!("render:\n  {key}: {written}\n").as_bytes());
                let project = Project::open(&dir).unwrap_or_else(|error| panic!("{key}: {written}: {error}"));
                let settings = project.settings();
                let taken = match key {
                    "backend" => settings.backend().to_string(),
                    "quality" => settings.quality().to_string(),
                    "encoder" => settings.encoder().to_string(),
                    _ => settings.codec().to_string(),
                };
                assert_eq!(&taken, word, "{key}: {written}");
            }
        }
    }
}

#[test]
fn workers_take_auto_or_a_whole_number_from_1_to_16() {
    for (written, expected) in [
        ("auto", Workers::Auto),
        ("\"auto\"", Workers::Auto),
        ("1", Workers::Count(1)),
        ("8", Workers::Count(8)),
        ("08", Workers::Count(8)),
        ("16", Workers::Count(16)),
    ] {
        let dir = project_with("workers-ok", format!("render:\n  workers: {written}\n").as_bytes());
        let project = Project::open(&dir).unwrap_or_else(|error| panic!("{written}: {error}"));
        assert_eq!(project.settings().workers(), expected, "{written}");
    }
    for written in ["0", "17", "-1", "+8", "0x8", "8.0", "\"8\"", "true", "many", "Auto", "1e1"] {
        let report = the_report("workers-wrong", &format!("render:\n  workers: {written}\n"));
        assert_eq!(report.problem().code(), Code::ValueNotAccepted, "{written}");
        assert_eq!(report.problem().line(), 2, "{written}");
        assert!(
            report
                .problem()
                .message()
                .starts_with("`render.workers` must be auto or a whole number from 1 to 16, not "),
            "{written}: {}",
            report.problem().message()
        );
    }
}

#[test]
fn a_value_outside_its_list_is_reported_with_its_line_and_the_values_taken() {
    let report = the_report("workers-17", "name: demo\nrender:\n  quality: high\n  workers: 17\n");
    assert_eq!(report.file(), "framesmith.yaml");
    assert_eq!(report.problem().line(), 4);
    assert_eq!(report.problem().code(), Code::ValueNotAccepted);
    assert_eq!(report.problem().kind(), Kind::Invalid);
    assert_eq!(
        report.problem().message(),
        "`render.workers` must be auto or a whole number from 1 to 16, not 17"
    );

    let report = the_report("codec-vp9", "render:\n  backend: gpu\n  codec: vp9\n");
    assert_eq!(report.problem().line(), 3);
    assert_eq!(report.problem().message(), "`render.codec` must be one of h264, hevc, not vp9");
    assert_eq!(
        report.to_string(),
        format!(
            "framesmith.yaml, line 3: {}",
            Code::ValueNotAccepted.statement("`render.codec` must be one of h264, hevc, not vp9")
        )
    );
}

#[test]
fn a_value_of_the_wrong_kind_is_reported_with_the_form_the_key_takes() {
    let cases: [(&str, u32, &str); 20] = [
        ("render:\n  backend: Cpu\n", 2, "`render.backend` must be one of auto, cpu, gpu, not Cpu"),
        ("render:\n  backend: true\n", 2, "`render.backend` must be one of auto, cpu, gpu, not true"),
        (
            "render:\n  quality: [high]\n",
            2,
            "`render.quality` must be one of draft, standard, high, not a list",
        ),
        (
            "render:\n  encoder:\n",
            2,
            "`render.encoder` has no value; it must be one of hardware, software",
        ),
        (
            "render:\n  encoder: ~\n",
            2,
            "`render.encoder` has no value; it must be one of hardware, software",
        ),
        (
            "render:\n  codec: \"vp9\"\n",
            2,
            "`render.codec` must be one of h264, hevc, not \"vp9\"",
        ),
        (
            "render:\n  codec: 264\n",
            2,
            "`render.codec` must be one of h264, hevc, not 264",
        ),
        (
            "render:\n  audio-bitrate: 0\n",
            2,
            "`render.audio-bitrate` must be a whole number of kbit/s from 1 to 4294967295, not 0",
        ),
        (
            "render:\n  audio-bitrate: 192.5\n",
            2,
            "`render.audio-bitrate` must be a whole number of kbit/s from 1 to 4294967295, not 192.5",
        ),
        (
            "render:\n  audio-bitrate: 4294967296\n",
            2,
            "`render.audio-bitrate` must be a whole number of kbit/s from 1 to 4294967295, not 4294967296",
        ),
        (
            "render:\n  audio-bitrate: loud\n",
            2,
            "`render.audio-bitrate` must be a whole number of kbit/s from 1 to 4294967295, not loud",
        ),
        (
            "name: 2026\n",
            1,
            "`name` must be text that is not empty and has no `/`, not 2026, which YAML reads as a number; put it in quotes to use it as text",
        ),
        (
            "name: true\n",
            1,
            "`name` must be text that is not empty and has no `/`, not true, which YAML reads as true or false; put it in quotes to use it as text",
        ),
        (
            "name: a/b\n",
            1,
            "`name` must be text that is not empty and has no `/`, not a/b",
        ),
        (
            "name: \"\"\n",
            1,
            "`name` must be text that is not empty and has no `/`, not \"\"",
        ),
        (
            "name:\n",
            1,
            "`name` has no value; it must be text that is not empty and has no `/`",
        ),
        (
            "composition: 5\n",
            1,
            "`composition` must be a path, which is text that is not empty, not 5, which YAML reads as a number; put it in quotes to use it as text",
        ),
        (
            "paths:\n  renders: [a, b]\n",
            2,
            "`paths.renders` must be a path, which is text that is not empty, not a list",
        ),
        (
            "render:\n  output: {a: b}\n",
            2,
            "`render.output` must be a path, which is text that is not empty, not a group of keys",
        ),
        ("render: fast\n", 1, "`render` must be a group of keys, not fast"),
    ];
    for (yaml, line, message) in cases {
        let report = the_report("wrong-kind", yaml);
        assert_eq!(report.problem().code(), Code::ValueNotAccepted, "{yaml:?}");
        assert_eq!(report.problem().line(), line, "{yaml:?}");
        assert_eq!(report.problem().message(), message, "{yaml:?}");
    }
    // A group with no value holds no key.
    for yaml in ["render:\n", "paths:\n", "render:\npaths:\n"] {
        let dir = project_with("empty-group", yaml.as_bytes());
        Project::open(&dir).unwrap_or_else(|error| panic!("{yaml:?}: {error}"));
    }
}

#[test]
fn text_and_paths_that_are_numbers_are_accepted_when_quoted() {
    let dir = project_with(
        "quoted-text",
        b"name: \"2026\"\ncomposition: composition.xml\nrender:\n  output: 'out/2026.mp4'\n",
    );
    let project = Project::open(&dir).unwrap();
    assert_eq!(project.name(), "2026");
    assert_eq!(project.output(), "out/2026.mp4");
}

#[test]
fn a_key_that_is_not_listed_is_reported_with_its_line_and_its_name() {
    let report = the_report("unknown-top", "name: demo\n\nspeed: fast\n");
    assert_eq!(report.file(), "framesmith.yaml");
    assert_eq!(report.problem().line(), 3);
    assert_eq!(report.problem().code(), Code::KeyNotListed);
    assert_eq!(report.problem().kind(), Kind::Invalid);
    assert_eq!(
        report.problem().message(),
        "`speed` is not a key of the project file; the keys are: name, composition, paths.renders, render.output, render.backend, render.quality, render.encoder, render.codec, render.workers, render.audio-bitrate"
    );

    // Inside a group the key is named with its group.
    let report = the_report("unknown-nested", "render:\n  codec: h264\n  colour: red\n");
    assert_eq!(report.problem().line(), 3);
    assert!(report
        .problem()
        .message()
        .starts_with("`render.colour` is not a key of the project file; "));
    let report = the_report("unknown-nested-paths", "paths:\n  cache: x\n");
    assert!(report
        .problem()
        .message()
        .starts_with("`paths.cache` is not a key of the project file; "));

    // A key that is not a word is an unknown key too, shown as it was written.
    let report = the_report("unknown-number-key", "7: x\n");
    assert_eq!(report.problem().code(), Code::KeyNotListed);
    assert!(report.problem().message().starts_with("`7` is not a key "));
    let report = the_report("unknown-list-key", "? [a, b]\n: x\n");
    assert_eq!(report.problem().code(), Code::KeyNotListed);
    assert!(report
        .problem()
        .message()
        .starts_with("this key is not a word, and the keys of the project file are: name"));

    // A key of a setting that would change the picture is not a key of the project file.
    for key in ["width", "fps", "duration", "background"] {
        let report = the_report("picture-key", &format!("{key}: 10\n"));
        assert_eq!(report.problem().code(), Code::KeyNotListed, "{key}");
    }
}

#[test]
fn a_key_of_a_group_written_flat_says_how_to_write_it() {
    let report = the_report("flat-key", "render.workers: 17\n");
    assert_eq!(report.problem().code(), Code::KeyNotListed);
    assert_eq!(report.problem().line(), 1);
    assert_eq!(
        report.problem().message(),
        "`render.workers` is not a key at the top of the file; write `render` as a group and `workers` under it"
    );
    // A dotted name that is not a key of a group gets the plain message.
    let report = the_report("flat-unknown", "render.colour: red\n");
    assert!(report
        .problem()
        .message()
        .starts_with("`render.colour` is not a key of the project file; "));
}

#[test]
fn the_keys_compositions_and_timeline_are_reported_as_not_supported_and_their_values_are_not_judged() {
    let dir = project_with(
        "unsupported",
        b"name: demo\ncompositions:\n  - intro.xml\n  - 42\ntimeline: {at: nowhere}\n",
    );
    let all = reports(&dir);
    assert_eq!(
        places(&all),
        [
            ("framesmith.yaml".to_string(), 2, Code::KeyNotSupported),
            ("framesmith.yaml".to_string(), 5, Code::KeyNotSupported),
        ]
    );
    assert!(all.iter().all(|report| report.problem().kind() == Kind::Unsupported));
    assert_eq!(
        all[0].problem().message(),
        "the project file has `compositions`, which this version does not support (support table S11: The project file names one composition; waits until Sprint 2)"
    );
    assert_eq!(
        all[1].problem().message(),
        "the project file has `timeline`, which this version does not support (support table S11: The project file names one composition; waits until Sprint 2)"
    );
}

#[test]
fn the_composition_the_project_names_is_the_one_that_is_read() {
    let dir = project_with("named-composition", b"composition: scenes/main.xml\n");
    fs::create_dir(dir.join("scenes")).unwrap();
    fs::write(dir.join("scenes").join("main.xml"), TWO_SECONDS).unwrap();
    // The default composition is wrong, and is not read.
    fs::write(dir.join("composition.xml"), "<composition/>").unwrap();
    let project = Project::open(&dir).unwrap();
    assert_eq!(project.root().frames, 60);
    assert_eq!((project.root().width, project.root().height), (640, 360));
    assert_eq!(project.settings().composition(), "scenes/main.xml");
}

#[test]
fn a_problem_of_the_named_composition_names_its_path_and_its_line() {
    let dir = project_with("named-composition-wrong", b"composition: scenes/main.xml\n");
    fs::create_dir(dir.join("scenes")).unwrap();
    fs::write(
        dir.join("scenes").join("main.xml"),
        "<composition version=\"1\"\n width=\"641\" height=\"360\" duration=\"2\"/>",
    )
    .unwrap();
    assert_eq!(places(&reports(&dir)), [("scenes/main.xml".to_string(), 2, Code::SizeNotAccepted)]);
}

#[test]
fn two_wrong_keys_and_a_wrong_composition_value_are_all_reported_in_one_run() {
    let dir = project_with("three-problems", b"name: demo\nspeed: fast\nrender:\n  codec: vp9\n");
    fs::write(
        dir.join("composition.xml"),
        "<composition version=\"1\" width=\"640\" height=\"360\"\n duration=\"soon\"/>",
    )
    .unwrap();
    let all = reports(&dir);
    assert_eq!(
        places(&all),
        [
            ("framesmith.yaml".to_string(), 2, Code::KeyNotListed),
            ("framesmith.yaml".to_string(), 4, Code::ValueNotAccepted),
            ("composition.xml".to_string(), 2, Code::DurationNotATime),
        ]
    );
    let text = OpenError::Invalid(all).to_string();
    assert_eq!(text.lines().count(), 3, "{text}");
}

#[test]
fn the_reports_of_a_project_file_are_in_the_order_of_its_lines_whatever_the_order_they_are_judged_in() {
    let dir = project_with(
        "report-order",
        b"render:\n  workers: 17\n  speed: fast\nname: 5\npaths:\n  renders: []\ncomposition:\n",
    );
    assert_eq!(
        places(&reports(&dir))
            .iter()
            .map(|(_, line, code)| (*line, *code))
            .collect::<Vec<_>>(),
        [
            (2, Code::ValueNotAccepted),
            (3, Code::KeyNotListed),
            (4, Code::ValueNotAccepted),
            (6, Code::ValueNotAccepted),
            (7, Code::ValueNotAccepted),
        ]
    );
}

#[test]
fn a_file_that_is_not_yaml_is_reported_once_with_the_readers_reason_and_line() {
    let cases: [(&str, &str, u32); 4] = [
        ("duplicate-key", "name: a\nname: b\n", 2),
        ("unclosed-list", "render: [a, b\nname: x\n", 2),
        ("tab-indent", "render:\n\tcodec: h264\n", 2),
        ("unclosed-quote", "name: \"abc\n", 1),
    ];
    for (name, yaml, line) in cases {
        let dir = project_with(name, yaml.as_bytes());
        // The composition is wrong too, and is not read: nothing says which file is meant.
        fs::write(dir.join("composition.xml"), "<composition/>").unwrap();
        let all = reports(&dir);
        assert_eq!(all.len(), 1, "{name}: {all:#?}");
        assert_eq!(all[0].file(), "framesmith.yaml", "{name}");
        assert_eq!(all[0].problem().code(), Code::ProjectFileNotYaml, "{name}");
        assert_eq!(all[0].problem().kind(), Kind::Invalid, "{name}");
        assert!(
            all[0].problem().message().starts_with("the file is not well-formed YAML: "),
            "{name}: {}",
            all[0].problem().message()
        );
        assert!(
            all[0].problem().message().len() > "the file is not well-formed YAML: ".len() + 5,
            "{name}"
        );
        assert_eq!(all[0].problem().line(), line, "{name}: {}", all[0]);
    }
    let report = the_report("duplicate-key-reason", "name: a\nname: b\n");
    assert_eq!(
        report.problem().message(),
        "the file is not well-formed YAML: duplicated key in mapping"
    );
}

#[test]
fn a_file_that_is_not_utf_8_text_is_reported_once_on_the_line_of_the_first_bad_byte() {
    let dir = project_with("not-utf-8", b"name: demo\nrender:\n  codec: \xff\n");
    let all = reports(&dir);
    assert_eq!(places(&all), [("framesmith.yaml".to_string(), 3, Code::ProjectFileNotUtf8)]);
    assert_eq!(all[0].problem().message(), "the file is not valid UTF-8 text");
}

#[test]
fn a_file_that_is_not_one_mapping_is_reported_once() {
    let cases: [(&str, &str, u32, &str); 4] = [
        (
            "list",
            "# keys\n- name\n- composition\n",
            2,
            "the project file must be one mapping of keys, such as `name: launch-film`, not a list",
        ),
        (
            "single-value",
            "launch-film\n",
            1,
            "the project file must be one mapping of keys, such as `name: launch-film`, not a single value",
        ),
        (
            "two-documents",
            "name: a\n---\nname: b\n",
            3,
            "the project file must hold one YAML document, and a second one starts here",
        ),
        (
            "empty-second-document",
            "---\n---\n",
            2,
            "the project file must hold one YAML document, and a second one starts here",
        ),
    ];
    for (name, yaml, line, message) in cases {
        let dir = project_with(name, yaml.as_bytes());
        fs::write(dir.join("composition.xml"), "<composition/>").unwrap();
        let all = reports(&dir);
        assert_eq!(all.len(), 1, "{name}: {all:#?}");
        assert_eq!(all[0].problem().code(), Code::ProjectFileNotAMapping, "{name}");
        assert_eq!(all[0].problem().line(), line, "{name}");
        assert_eq!(all[0].problem().message(), message, "{name}");
    }
}

#[test]
fn a_byte_order_mark_and_windows_line_ends_change_no_key_and_no_line() {
    let dir = project_with(
        "bom-and-crlf",
        "\u{feff}name: demo\r\nrender:\r\n  workers: 17\r\n  codec: hevc\r\n".as_bytes(),
    );
    let all = reports(&dir);
    assert_eq!(places(&all), [("framesmith.yaml".to_string(), 3, Code::ValueNotAccepted)]);
    let dir = project_with("bom-valid", "\u{feff}name: demo\n".as_bytes());
    assert_eq!(Project::open(&dir).unwrap().name(), "demo");
}

#[test]
fn a_wrong_composition_key_stops_the_composition_from_being_read() {
    let dir = project_with("composition-key-wrong", b"composition: []\nspeed: fast\n");
    // The default composition is wrong, and is not read in place of the one that was meant.
    fs::write(dir.join("composition.xml"), "<composition/>").unwrap();
    assert_eq!(
        places(&reports(&dir)),
        [
            ("framesmith.yaml".to_string(), 1, Code::ValueNotAccepted),
            ("framesmith.yaml".to_string(), 2, Code::KeyNotListed),
        ]
    );
}

#[test]
fn a_named_composition_that_is_not_there_is_no_project_with_the_line_of_the_key() {
    let dir = project_with("named-missing", b"name: demo\ncomposition: scenes/nothing.xml\n");
    match Project::open(&dir) {
        Err(OpenError::NoProject(reason @ NoProject::NamedCompositionMissing(..))) => {
            assert_eq!((reason.code(), reason.kind()), (Code::NamedCompositionMissing, Kind::Invalid));
            assert_eq!(
                reason.to_string(),
                format!(
                    "the composition `scenes/nothing.xml`, named by `composition` on line 2 of framesmith.yaml, does not exist in `{}`",
                    dir.display()
                )
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_named_composition_that_cannot_be_read_is_a_failure_with_the_reason() {
    let dir = project_with("named-unreadable", b"composition: scenes\n");
    fs::create_dir(dir.join("scenes")).unwrap();
    match Project::open(&dir) {
        Err(OpenError::NoProject(reason @ NoProject::NamedCompositionUnreadable(..))) => {
            assert_eq!((reason.code(), reason.kind()), (Code::NamedCompositionUnreadable, Kind::Failure));
            let text = reason.to_string();
            assert!(
                text.starts_with("the composition `scenes`, named by `composition` on line 1 of framesmith.yaml, in `"),
                "{text}"
            );
            assert!(text.contains("could not be read: "), "{text}");
            assert!(!text.contains("os error"), "{text}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_reports_of_the_project_file_come_before_the_reason_there_is_no_composition() {
    // A composition named and not there.
    let dir = project_with("reports-then-named-missing", b"render:\n  workers: 17\ncomposition: nothing.xml\n");
    match Project::open(&dir) {
        Err(OpenError::InvalidAndNoProject(all, reason)) => {
            assert_eq!(places(&all), [("framesmith.yaml".to_string(), 2, Code::ValueNotAccepted)]);
            assert_eq!(reason.code(), Code::NamedCompositionMissing);
        }
        other => panic!("{other:?}"),
    }
    // The default composition is not there.
    let dir = project_with("reports-then-default-missing", b"speed: fast\n");
    fs::remove_file(dir.join("composition.xml")).unwrap();
    let error = Project::open(&dir).unwrap_err();
    let text = error.to_string();
    match error {
        OpenError::InvalidAndNoProject(all, reason) => {
            assert_eq!(places(&all), [("framesmith.yaml".to_string(), 1, Code::KeyNotListed)]);
            assert_eq!(reason.code(), Code::NoComposition);
        }
        other => panic!("{other:?}"),
    }
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "{text}");
    assert!(lines[0].starts_with("framesmith.yaml, line 1: FS-0022: invalid: "), "{text}");
    assert!(lines[1].starts_with("No project: FS-0015: invalid: "), "{text}");
}

#[test]
fn a_project_file_that_cannot_be_read_is_no_project_with_the_reason() {
    let dir = folder("project-file-unreadable");
    fs::write(dir.join("composition.xml"), FIVE_SECONDS).unwrap();
    fs::create_dir(dir.join("framesmith.yaml")).unwrap();
    match Project::open(&dir) {
        Err(OpenError::NoProject(reason @ NoProject::ProjectFileUnreadable(..))) => {
            assert_eq!((reason.code(), reason.kind()), (Code::ProjectFileUnreadable, Kind::Failure));
            let text = reason.to_string();
            assert!(text.starts_with("framesmith.yaml in `"), "{text}");
            assert!(text.contains("could not be read: "), "{text}");
            assert!(!text.contains("os error"), "{text}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_default_output_follows_the_name_and_the_renders_folder_in_use() {
    let dir = project_with("output-default", b"name: film\npaths:\n  renders: build/videos/\n");
    let project = Project::open(&dir).unwrap();
    assert_eq!(project.output(), "build/videos/film.mp4");
    let dir = project_with("output-default-renders-only", b"paths:\n  renders: build\n");
    let project = Project::open(&dir).unwrap();
    assert_eq!(project.output(), format!("build/{}.mp4", project.name()));
    assert_eq!(project.name(), "output-default-renders-only");
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
fn opening_a_project_with_a_project_file_changes_nothing_in_the_folder() {
    let dir = project_with("unchanged", b"name: demo\nrender:\n  workers: 17\n");
    fs::create_dir(dir.join("renders")).unwrap();
    let before = snapshot(&dir);
    assert!(Project::open(&dir).is_err());
    assert_eq!(snapshot(&dir), before);
    fs::write(dir.join("framesmith.yaml"), "name: demo\n").unwrap();
    let before = snapshot(&dir);
    Project::open(&dir).unwrap();
    assert_eq!(snapshot(&dir), before);
    assert!(!dir.join("renders").join("demo.mp4").exists());
}
