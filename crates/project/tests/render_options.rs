//! The options of `render` and the rule that takes every setting: an option wins over the project
//! file, which wins over the default (`docs/spec/render-command.md` sections 1 and 1.1).

use project::{Backend, Codec, Encoder, Options, Project, Quality, Source, Workers, OPTION_NAMES};
use std::fs;
use std::path::{Path, PathBuf};

const COMPOSITION: &str = r#"<composition version="1" width="64" height="32" duration="1"/>"#;

/// A new project folder `name` with a composition and, when given, a `framesmith.yaml`.
fn project_with(name: &str, yaml: Option<&str>) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("render-options").join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("composition.xml"), COMPOSITION).unwrap();
    if let Some(yaml) = yaml {
        fs::write(dir.join("framesmith.yaml"), yaml).unwrap();
    }
    dir
}

fn options(pairs: &[(&str, &str)]) -> Options {
    let mut options = Options::default();
    for (name, value) in pairs {
        options.set(name, value).unwrap_or_else(|why| panic!("{name} {value}: {why}"));
    }
    options
}

#[test]
fn the_options_are_the_six_of_the_spec() {
    assert_eq!(
        OPTION_NAMES,
        ["--output", "--backend", "--quality", "--encoder", "--codec", "--workers"]
    );
    for name in OPTION_NAMES {
        assert!(Options::is_option(name));
    }
    for name in ["--json", "--audio-bitrate", "--strict", "-o", "--"] {
        assert!(!Options::is_option(name), "{name}");
    }
}

#[test]
fn an_unknown_option_is_told_with_the_options_taken() {
    assert_eq!(
        Options::unknown("--colour"),
        "`--colour` is not an option of `render`; the options are: --output, --backend, --quality, --encoder, --codec, --workers"
    );
    let mut options = Options::default();
    assert_eq!(options.set("--colour", "red"), Err(Options::unknown("--colour")));
}

#[test]
fn a_value_outside_the_list_of_its_option_is_told_with_the_values_taken() {
    for (name, value, told) in [
        ("--quality", "ultra", "`--quality` must be one of draft, standard, high, not ultra"),
        ("--backend", "metal", "`--backend` must be one of auto, cpu, gpu, not metal"),
        ("--encoder", "fast", "`--encoder` must be one of hardware, software, not fast"),
        ("--codec", "vp9", "`--codec` must be one of h264, hevc, not vp9"),
        ("--codec", "H264", "`--codec` must be one of h264, hevc, not H264"),
        ("--workers", "17", "`--workers` must be auto or a whole number from 1 to 16, not 17"),
        ("--workers", "0", "`--workers` must be auto or a whole number from 1 to 16, not 0"),
        ("--workers", "+4", "`--workers` must be auto or a whole number from 1 to 16, not +4"),
        (
            "--workers",
            "2.0",
            "`--workers` must be auto or a whole number from 1 to 16, not 2.0",
        ),
        ("--output", "", "`--output` must be a path that is not empty"),
    ] {
        let mut options = Options::default();
        assert_eq!(options.set(name, value), Err(told.to_string()), "{name} {value}");
        assert_eq!(options, Options::default(), "nothing is taken from {name} {value}");
    }
}

#[test]
fn an_option_with_no_value_and_an_option_given_twice_are_told() {
    assert_eq!(
        Options::no_value("--quality"),
        "`--quality` needs a value: one of draft, standard, high"
    );
    assert_eq!(Options::no_value("--output"), "`--output` needs a value: a path that is not empty");
    let mut options = options(&[("--quality", "standard")]);
    assert_eq!(
        options.set("--quality", "high"),
        Err("`--quality` is given twice; give it once".to_string())
    );
}

#[test]
fn every_value_of_the_lists_is_taken() {
    let given = options(&[
        ("--output", "out/film.mp4"),
        ("--backend", "gpu"),
        ("--quality", "draft"),
        ("--encoder", "software"),
        ("--codec", "hevc"),
        ("--workers", "08"),
    ]);
    assert_eq!(given.count(), 6);
    let mut project = Project::open(&project_with("every-value", None)).unwrap();
    project.apply(&given);
    let settings = project.settings();
    assert_eq!(project.output(), "out/film.mp4");
    assert_eq!(settings.backend(), Backend::Gpu);
    assert_eq!(settings.quality(), Quality::Draft);
    assert_eq!(settings.encoder(), Encoder::Software);
    assert_eq!(settings.codec(), Codec::Hevc);
    assert_eq!(settings.workers(), Workers::Count(8));
    assert_eq!(options(&[("--workers", "auto")]).count(), 1);
}

#[test]
fn an_option_wins_over_the_project_file_which_wins_over_the_default() {
    let yaml = "render:\n  output: file/out.mp4\n  quality: draft\n  codec: hevc\n  audio-bitrate: 128\n";
    let mut project = Project::open(&project_with("precedence", Some(yaml))).unwrap();
    assert_eq!(project.output(), "file/out.mp4");
    assert_eq!(project.settings().source("render.output"), Source::ProjectFile);

    project.apply(&options(&[("--output", "option/out.mp4"), ("--quality", "standard")]));
    let settings = project.settings();
    // The option's value, for the settings it names.
    assert_eq!(project.output(), "option/out.mp4");
    assert_eq!(settings.source("render.output"), Source::Option);
    assert_eq!(settings.quality(), Quality::Standard);
    assert_eq!(settings.source("render.quality"), Source::Option);
    // The project file's value, for the settings it writes and no option names.
    assert_eq!(settings.codec(), Codec::Hevc);
    assert_eq!(settings.source("render.codec"), Source::ProjectFile);
    assert_eq!(settings.audio_bitrate(), 128);
    assert_eq!(settings.source("render.audio-bitrate"), Source::ProjectFile);
    // The default, for the rest.
    assert_eq!(settings.backend(), Backend::Auto);
    assert_eq!(settings.source("render.backend"), Source::Default);
    assert_eq!(settings.encoder(), Encoder::Hardware);
    assert_eq!(settings.workers(), Workers::Auto);
    assert_eq!(settings.source("render.workers"), Source::Default);
    // The line of the value the file wrote is still known; the option says it is not in use.
    assert_eq!(settings.line_of("render.quality"), Some(3));
    assert!(settings.from_option("render.quality"));
    assert!(!settings.from_option("render.codec"));
}

#[test]
fn the_output_is_from_the_project_file_when_it_writes_a_key_the_default_is_made_from() {
    let mut project = Project::open(&project_with("output-default", None)).unwrap();
    assert_eq!(project.output(), "renders/output-default.mp4");
    assert_eq!(project.settings().source("render.output"), Source::Default);
    assert_eq!(project.settings().audio_bitrate(), 192);
    assert_eq!(project.settings().source("render.audio-bitrate"), Source::Default);
    project.apply(&Options::default());
    assert_eq!(project.output(), "renders/output-default.mp4");

    let project = Project::open(&project_with("output-renders", Some("paths:\n  renders: finished\n"))).unwrap();
    assert_eq!(project.output(), "finished/output-renders.mp4");
    assert_eq!(project.settings().source("render.output"), Source::ProjectFile);

    let project = Project::open(&project_with("output-name", Some("name: launch\n"))).unwrap();
    assert_eq!(project.output(), "renders/launch.mp4");
    assert_eq!(project.settings().source("render.output"), Source::ProjectFile);
}

#[test]
fn a_relative_output_of_an_option_is_taken_from_the_project_folder() {
    let folder = project_with("relative-option", None);
    let mut project = Project::open(&folder).unwrap();
    project.apply(&options(&[("--output", "deep/er/film.mp4")]));
    assert_eq!(project.resolve(&project.output()), folder.join("deep/er/film.mp4"));
}

#[test]
fn applying_options_writes_nothing_to_the_project_file() {
    let yaml = "render:\n  quality: draft\n";
    let folder = project_with("unchanged", Some(yaml));
    let mut project = Project::open(&folder).unwrap();
    project.apply(&options(&[("--quality", "standard"), ("--output", "x.mp4")]));
    assert_eq!(fs::read_to_string(folder.join("framesmith.yaml")).unwrap(), yaml);
    assert_eq!(fs::read_to_string(folder.join("composition.xml")).unwrap(), COMPOSITION);
}
