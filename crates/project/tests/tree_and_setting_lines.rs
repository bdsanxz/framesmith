//! An opened project holds the tree of its composition, and the lines of the render settings its
//! project file wrote (`docs/spec/render-command.md` sections 3 and 11); a report can be made by a
//! part that judges what the project means.

use project::{Code, Element, Problem, Project, Report, COMPOSITION_FILE, PROJECT_FILE};
use std::fs;
use std::path::{Path, PathBuf};

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("tree-and-setting-lines").join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

const COMPOSITION: &str = "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n<box left=\"4\"/>\n</composition>\n";

#[test]
fn an_opened_project_holds_the_elements_of_its_composition() {
    let dir = folder("tree");
    fs::write(dir.join(COMPOSITION_FILE), COMPOSITION).unwrap();
    let project = Project::open(&dir).unwrap();
    let nodes = project.tree().nodes();
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[1].element(), Element::Box);
    assert_eq!(nodes[1].line(), 2);
    assert_eq!(nodes[1].attribute("left").unwrap().value(), "4");
}

#[test]
fn the_tree_is_that_of_the_composition_the_project_file_names() {
    let dir = folder("named-tree");
    fs::create_dir_all(dir.join("scenes")).unwrap();
    fs::write(dir.join("scenes/main.xml"), COMPOSITION).unwrap();
    fs::write(dir.join(PROJECT_FILE), "composition: scenes/main.xml\n").unwrap();
    let project = Project::open(&dir).unwrap();
    assert_eq!(project.tree().nodes().len(), 2);
    assert_eq!(project.composition(), "scenes/main.xml");
}

#[test]
fn the_line_of_each_render_setting_the_file_wrote_is_known_and_the_others_have_none() {
    let dir = folder("lines");
    fs::write(dir.join(COMPOSITION_FILE), COMPOSITION).unwrap();
    fs::write(
        dir.join(PROJECT_FILE),
        "name: demo\nrender:\n  output: out.mp4\n  backend: cpu\n\n  quality: high\n  encoder: software\n  codec: hevc\n  workers: 3\n  audio-bitrate: 128\n",
    )
    .unwrap();
    let settings = Project::open(&dir).unwrap().settings().clone();
    assert_eq!(settings.line_of("render.backend"), Some(4));
    assert_eq!(settings.line_of("render.quality"), Some(6));
    assert_eq!(settings.line_of("render.encoder"), Some(7));
    assert_eq!(settings.line_of("render.codec"), Some(8));
    assert_eq!(settings.line_of("render.workers"), Some(9));
    assert_eq!(settings.line_of("render.audio-bitrate"), Some(10));
    // The output's line is kept too, so that a film of another kind is refused on it (support table S34).
    assert_eq!(settings.line_of("render.output"), Some(3));
    assert_eq!(settings.line_of("name"), None);
}

#[test]
fn a_project_file_that_writes_no_render_setting_has_no_line_for_any() {
    let dir = folder("no-lines");
    fs::write(dir.join(COMPOSITION_FILE), COMPOSITION).unwrap();
    let settings = Project::open(&dir).unwrap().settings().clone();
    for key in [
        "render.backend",
        "render.quality",
        "render.encoder",
        "render.codec",
        "render.workers",
        "render.audio-bitrate",
    ] {
        assert_eq!(settings.line_of(key), None, "{key}");
    }
}

#[test]
fn a_report_made_by_a_part_that_judges_meaning_prints_like_one_of_the_reader() {
    let report = Report::new(
        "composition.xml",
        Problem::new(Code::FormNotSupported, 3, "<text> is not supported".to_string()),
    );
    assert_eq!(report.file(), "composition.xml");
    assert_eq!(report.problem().line(), 3);
    assert_eq!(
        report.to_string(),
        format!(
            "composition.xml, line 3: {}",
            Code::FormNotSupported.statement("<text> is not supported")
        )
    );
}
