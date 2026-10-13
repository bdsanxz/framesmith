//! The project file, `framesmith.yaml`: its keys, its defaults, and which project a target of
//! `render` belongs to.

use poc::project::{self, Codec, Coder, Project, Quality, Renderer, Workers};
use std::path::{Path, PathBuf};

/// A new, empty folder for one test, under the build's own temporary folder.
fn folder(name: &str) -> PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("project-file").join(name);
    if folder.exists() {
        std::fs::remove_dir_all(&folder).expect("the old test folder is removed");
    }
    std::fs::create_dir_all(&folder).expect("the test folder is made");
    folder
}

#[test]
fn an_empty_file_is_a_project_of_defaults() {
    let project = project::parse("", Path::new("/films/launch")).expect("an empty file is valid");
    assert_eq!(project.composition, Path::new("composition.xml"));
    assert_eq!(project.renders, Path::new("renders"));
    assert_eq!(
        (
            project.renderer,
            project.quality,
            project.coder,
            project.codec,
            project.workers,
            project.audio_bitrate
        ),
        (Renderer::Auto, Quality::Standard, Coder::Hardware, Codec::H264, Workers::Auto, 192)
    );
    assert_eq!(project, Project::defaults(Path::new("/films/launch")));
}

#[test]
fn every_key_is_read() {
    let source = "name: launch-film\ncomposition: cuts/film.xml\npaths:\n  renders: out\nrender:\n  output: out/final.mp4\n  backend: cpu\n  quality: high\n  encoder: software\n  codec: hevc\n  workers: 3\n  audio-bitrate: 256\n";
    let project = project::parse(source, Path::new("/films/launch")).expect("the file is valid");
    assert_eq!(project.name, "launch-film");
    assert_eq!(project.composition, Path::new("cuts/film.xml"));
    assert_eq!(project.renders, Path::new("out"));
    assert_eq!(
        (
            project.renderer,
            project.quality,
            project.coder,
            project.codec,
            project.workers,
            project.audio_bitrate
        ),
        (Renderer::Cpu, Quality::High, Coder::Software, Codec::Hevc, Workers::Count(3), 256)
    );
    assert_eq!(project.output_path(), Path::new("/films/launch/out/final.mp4"));
}

#[test]
fn the_finished_file_is_named_after_the_project_in_its_renders_folder() {
    let project = project::parse("name: launch-film\npaths:\n  renders: out\n", Path::new("/films/launch")).expect("valid");
    assert_eq!(project.output_path(), Path::new("/films/launch/out/launch-film.mp4"));
}

#[test]
fn every_problem_is_reported_with_its_line() {
    let source = "name: x\nrender:\n  quality: best\n  threads: 4\nfoo: 1\ncomposition: ../up.xml\npaths: here\n";
    let problems = project::parse(source, Path::new(".")).expect_err("the file has problems");
    let found: Vec<(usize, &str)> = problems.iter().map(|p| (p.line, p.message.as_str())).collect();
    assert_eq!(
        found,
        [
            (3, "`render.quality` must be one of: draft, standard, high"),
            (
                4,
                "`render` has no key `threads`; it has: output, backend, quality, encoder, codec, workers, audio-bitrate"
            ),
            (5, "the project file has no key `foo`; it has: name, composition, paths, render"),
            (6, "`composition: ../up.xml` must be a path inside the project folder"),
            (7, "`paths` must be a list of `key: value` lines"),
        ]
    );
    let broken = project::parse("name: [unclosed\n", Path::new(".")).expect_err("the file is not YAML");
    assert!(broken[0].message.starts_with("the file is not well-formed YAML"), "{broken:?}");
}

#[test]
fn a_folder_a_project_file_and_a_composition_all_lead_to_the_project() {
    let root = folder("targets");
    std::fs::create_dir_all(root.join("cuts")).expect("a sub-folder");
    std::fs::write(root.join("framesmith.yaml"), "name: film\ncomposition: cuts/main.xml\n").expect("the project file");
    std::fs::write(root.join("cuts/main.xml"), "<composition/>").expect("a composition");
    std::fs::write(root.join("cuts/other.xml"), "<composition/>").expect("another composition");

    let (project, composition) = project::open(&root).expect("a folder is a target");
    assert_eq!((project.name.as_str(), composition), ("film", root.join("cuts/main.xml")));

    let (project, composition) = project::open(&root.join("framesmith.yaml")).expect("a project file is a target");
    assert_eq!((project.name.as_str(), composition), ("film", root.join("cuts/main.xml")));

    // A composition in a sub-folder belongs to the project above it, and is the one rendered.
    let (project, composition) = project::open(&root.join("cuts/other.xml")).expect("a composition is a target");
    assert_eq!(project.name, "film");
    assert_eq!(composition, root.join("cuts/other.xml"));
    assert_eq!(project.folder.canonicalize().ok(), root.canonicalize().ok());
}

#[test]
fn a_folder_without_a_project_file_is_a_project_named_after_itself() {
    let root = folder("plain");
    let (project, composition) = project::open(&root).expect("a folder is a target");
    assert_eq!(project.name, "plain");
    assert_eq!(composition, root.join("composition.xml"));
    assert!(project::open(&root.join("missing.xml")).is_err());
}

#[test]
fn an_encoder_a_codec_or_a_number_of_workers_that_is_not_known_is_a_problem() {
    let source = "render:\n  encoder: fastest\n  codec: av1\n  workers: 0\n";
    let problems = project::parse(source, Path::new(".")).expect_err("the file has problems");
    let found: Vec<(usize, &str)> = problems.iter().map(|p| (p.line, p.message.as_str())).collect();
    assert_eq!(
        found,
        [
            (2, "`render.encoder` must be one of: hardware, software"),
            (3, "`render.codec` must be one of: h264, hevc"),
            (4, "`render.workers` must be `auto` or a whole number from 1 to 16"),
        ]
    );
    for written in ["many", "17", "2.5"] {
        let source = format!("render:\n  workers: {written}\n");
        assert!(project::parse(&source, Path::new(".")).is_err(), "`workers: {written}` is refused");
    }
    let project = project::parse("render:\n  workers: auto\n", Path::new(".")).expect("`auto` is valid");
    assert_eq!(project.workers, Workers::Auto);
}

#[test]
fn each_quality_has_its_encoder_settings() {
    // The software encoder's bits for each pixel, and the hardware encoder's quality.
    assert_eq!(Quality::Draft.bits_a_pixel(), 0.06);
    assert_eq!(Quality::Standard.bits_a_pixel(), 0.13);
    assert_eq!(Quality::High.bits_a_pixel(), 0.20);
    assert_eq!(
        [Quality::Draft, Quality::Standard, Quality::High].map(Quality::hardware),
        [45, 65, 80]
    );
}
