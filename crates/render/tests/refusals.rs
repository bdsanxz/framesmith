//! What this version does not draw or apply is refused before the first frame, each with its code,
//! its file and its line (`docs/spec/render-command.md` sections 2 and 3).

mod common;

use common::{composition, project_with, scratch};
use project::{Code, Kind, OpenError, Project, Report, COMPOSITION_FILE, PROJECT_FILE};
use render::Scene;
use std::fs;

/// The reports of a composition with `inside` in its root, as `file line code message`.
fn refused(name: &str, inside: &str) -> Vec<String> {
    let folder = project_with("refusals", name, &composition(inside));
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    match Scene::from_project(&project) {
        Ok(_) => Vec::new(),
        Err(reports) => reports.iter().map(shown).collect(),
    }
}

fn shown(report: &Report) -> String {
    format!(
        "{} {} {} {}",
        report.file(),
        report.problem().line(),
        report.problem().code(),
        report.problem().message()
    )
}

fn only(name: &str, inside: &str) -> String {
    let mut reports = refused(name, inside);
    assert_eq!(reports.len(), 1, "{inside}: {reports:#?}");
    reports.remove(0)
}

#[test]
fn a_composition_of_boxes_with_what_this_version_draws_has_a_scene() {
    let folder = project_with(
        "refusals",
        "drawn",
        &composition(
            "<box id=\"a\" left=\"-5\" top=\"1e1\" width=\"+40\" height=\"0\" background=\"#aabbcc\"/>\n<box/>\n<box background=\"#aabbccff\" left=\"5.\" width=\"1.5e1\"/>",
        ),
    );
    let scene = Scene::from_project(&Project::open(&folder).unwrap()).unwrap();
    assert_eq!(scene.boxes(), 3);
}

#[test]
fn an_element_other_than_a_box_in_the_root_is_unsupported_and_named_with_its_line() {
    for (index, (element, tag)) in [
        ("<audio src=\"a.wav\" duration=\"1\"/>", "<audio>"),
        ("<rect/>", "<rect>"),
        ("<image src=\"i.png\"/>", "<image>"),
        ("<video src=\"v.mp4\" width=\"2\" height=\"2\"/>", "<video>"),
        ("<shape src=\"s.svg\"/>", "<shape>"),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            only(&format!("element-{index}"), element),
            format!(
                "composition.xml 2 FS-0035 {tag} is not supported by this version, which draws <box>, <text> and <composition src> only"
            ),
            "{element}"
        );
    }
}

#[test]
fn an_element_inside_a_box_is_unsupported_and_what_is_inside_an_element_that_is_reported_is_not_looked_into() {
    assert_eq!(
        only("in-box", "<box>\n<tween at=\"0\" duration=\"1\" to-x=\"5\"/>\n</box>"),
        "composition.xml 3 FS-0035 <tween> inside a <box> is not supported by this version, which draws a <box>, a <text> or a <composition src> inside a <box> and no other element"
    );
    // A box inside a box is drawn (section 4.1), so it is not refused.
    assert_eq!(refused("box-in-box", "<box>\n<box/>\n</box>"), Vec::<String>::new());
    // A text that names no font of the composition is refused for its family, once; the span inside it is not looked
    // into, since the text is not drawn. A span inside a text that is drawn is refused in `text_frame_marks.rs`.
    assert_eq!(
        only("inside-reported", "<text>words <span color=\"#ffffff\">x</span></text>"),
        "composition.xml 2 FS-0035 <text> is not drawn by this version: no font of the composition is named sans-serif"
    );
    // A rect inside a box is reported once; the box inside that rect is not looked into.
    let reports = refused("inside-inside", "<box>\n<rect>\n<box/>\n</rect>\n</box>");
    assert_eq!(reports.len(), 1, "{reports:#?}");
    assert_eq!(
        reports[0],
        "composition.xml 3 FS-0035 <rect> inside a <box> is not supported by this version, which draws a <box>, a <text> or a <composition src> inside a <box> and no other element"
    );
}

#[test]
fn a_refusal_names_what_this_version_draws_in_the_root_and_inside_a_box() {
    // The root draws a <box>, a <text> and a <composition src>, and a message that names only the box is wrong.
    let root = only("names-root", "<rect/>");
    assert_eq!(
        root,
        "composition.xml 2 FS-0035 <rect> is not supported by this version, which draws <box>, <text> and <composition src> only"
    );
    assert!(root.contains("<box>") && root.contains("<composition src>"), "{root}");
    // Inside a box it draws a <box> or a <composition src>, and no other element.
    let inside = only("names-box", "<box>\n<rect/>\n</box>");
    assert_eq!(
        inside,
        "composition.xml 3 FS-0035 <rect> inside a <box> is not supported by this version, which draws a <box>, a <text> or a <composition src> inside a <box> and no other element"
    );
    assert!(inside.contains("<box>") && inside.contains("<composition src>"), "{inside}");
}

#[test]
fn an_attribute_this_version_does_not_draw_is_unsupported_on_its_own_line() {
    let reports = refused(
        "attributes",
        "<box id=\"a\" border-radius=\"4\"\n     x=\"3\" flex-grow=\"1\" left=\"2\" opacity=\"0.5\"/>",
    );
    assert_eq!(
        reports,
        vec![
            "composition.xml 2 FS-0035 <box> has an attribute this version does not support: `border-radius`",
            "composition.xml 3 FS-0035 <box> has an attribute this version does not support: `x`",
            "composition.xml 3 FS-0035 <box> has an attribute this version does not support: `flex-grow`",
            "composition.xml 3 FS-0035 <box> has an attribute this version does not support: `opacity`",
        ]
    );
    // The root takes its seven attributes and `class`, which names styles of the file: a name that no
    // style of the file has is reported on its line, and is not taken as an attribute this version lacks.
    let folder = project_with(
        "refusals",
        "root-class",
        "<composition version=\"1\" id=\"x\"\n class=\"a\" width=\"96\" height=\"54\" duration=\"1\"/>\n",
    );
    let Err(OpenError::Invalid(reports)) = Project::open(&folder) else {
        panic!("a `class` that names no style of the file is refused when the project opens");
    };
    assert_eq!(
        shown(&reports[0]),
        "composition.xml 2 FS-0043 `class` names the style `a`, and this file has no <style> with that id"
    );
    assert_eq!(reports.len(), 1);
}

#[test]
fn a_length_that_is_a_length_and_is_not_a_whole_number_of_pixels_in_range_is_unsupported() {
    let edge = "a whole number of pixels from -1000000 to 1000000, or a percentage such as 50%";
    let size = "a whole number of pixels from 0 to 1000000, or a percentage such as 50%";
    for (name, attribute, written, takes) in [
        ("left-fraction", "left", "10.5", edge),
        ("left-large", "left", "1000001", edge),
        ("top-negative-large", "top", "-1e7", edge),
        ("right-fraction", "right", "-0.5", edge),
        ("bottom-large", "bottom", "1e7", edge),
        ("width-fraction", "width", "0.5", size),
        ("height-large", "height", "2e6", size),
    ] {
        assert_eq!(
            only(name, &format!("<box {attribute}=\"{written}\"/>")),
            format!("composition.xml 2 FS-0035 `{attribute}=\"{written}\"` on <box> is not a form this version supports: {takes}")
        );
    }
}

#[test]
fn the_limits_themselves_are_drawn() {
    for inside in [
        "<box left=\"-1000000\" top=\"-1000000\" width=\"1000000\" height=\"0\"/>",
        "<box width=\"1000000\" height=\"0\"/>",
        "<box left=\"-0\" width=\"-0\"/>",
    ] {
        assert_eq!(refused("limit", inside), Vec::<String>::new(), "{inside}");
    }
}

#[test]
fn a_place_that_works_out_beyond_the_limit_is_reported_with_its_line() {
    // The default size of a box at top 1000000 lies beyond the limit at its bottom edge.
    assert_eq!(
        only("beyond-default-size", "<box top=\"1000000\"/>"),
        "composition.xml 2 FS-0035 a place or a size of <box> works out beyond 1000000 pixels, which this version does not place"
    );
    // A near edge at the limit and a size that runs past it.
    assert_eq!(
        only("beyond-far-edge", "<box left=\"1000000\" width=\"1\"/>"),
        "composition.xml 2 FS-0035 a place or a size of <box> works out beyond 1000000 pixels, which this version does not place"
    );
}

#[test]
fn a_value_that_is_no_length_and_a_size_below_zero_are_invalid_and_not_unsupported() {
    for (name, attribute, written, message) in [
        (
            "left-word",
            "left",
            "abc",
            "is not a length: a number of pixels or a percentage, such as 40 or 50%",
        ),
        (
            "top-unit",
            "top",
            "10px",
            "is not a length: a number of pixels or a percentage, such as 40 or 50%",
        ),
        (
            "width-empty",
            "width",
            "",
            "is not a length: a number of pixels or a percentage, such as 40 or 50%",
        ),
        (
            "height-word",
            "height",
            "tall",
            "is not a length: a number of pixels or a percentage, such as 40 or 50%",
        ),
        ("width-negative", "width", "-5", "is below 0; a size is 0 or more"),
        ("height-negative-percent", "height", "-10%", "is below 0; a size is 0 or more"),
    ] {
        assert_eq!(
            only(name, &format!("<box {attribute}=\"{written}\"/>")),
            format!("composition.xml 2 FS-0034 `{attribute}=\"{written}\"` {message}")
        );
    }
    // A negative edge is not wrong: a box may lie partly outside, and a percentage is a length.
    assert_eq!(
        refused("negative-edge", "<box left=\"-5\" top=\"-5%\"/>"),
        Vec::<String>::new(),
        "a negative distance and a percentage are both taken"
    );
    assert_eq!(Code::LengthNotAccepted.kind(), Kind::Invalid);
    assert_eq!(Code::FormNotSupported.kind(), Kind::Unsupported);
}

#[test]
fn a_background_that_is_not_one_opaque_colour_is_unsupported() {
    for (name, written) in [
        ("alpha", "#ff000080"),
        ("gradient", "linear-gradient(90deg, #ff0000 0%, #0000ff 100%)"),
        ("layers", "#ff0000, #0000ff"),
        ("name", "red"),
        ("short", "#f00"),
    ] {
        assert_eq!(
            only(name, &format!("<box background=\"{written}\"/>")),
            format!("composition.xml 2 FS-0035 `background=\"{written}\"` on <box> is not a form this version supports: one opaque colour, #rrggbb")
        );
    }
}

#[test]
fn a_root_background_with_an_alpha_below_ff_is_unsupported() {
    let folder = project_with(
        "refusals",
        "root-alpha",
        "<composition version=\"1\" width=\"96\" height=\"54\" duration=\"1\"\n background=\"#10203080\"/>\n",
    );
    let reports = Scene::from_project(&Project::open(&folder).unwrap()).unwrap_err();
    assert_eq!(reports.len(), 1);
    assert_eq!(
        shown(&reports[0]),
        "composition.xml 2 FS-0035 `background=\"#10203080\"` on <composition> is not a form this version supports: one opaque colour, #rrggbb"
    );
    let folder = project_with(
        "refusals",
        "root-alpha-ff",
        "<composition version=\"1\" width=\"96\" height=\"54\" duration=\"1\" background=\"#102030ff\"/>\n",
    );
    assert!(Scene::from_project(&Project::open(&folder).unwrap()).is_ok());
}

#[test]
fn a_project_the_notation_does_not_accept_is_not_opened_so_nothing_is_drawn_from_it() {
    let folder = project_with("refusals", "not-opened", &composition("<box glow=\"1\"/>"));
    assert!(Project::open(&folder).is_err());
}

#[test]
fn the_refusals_of_a_composition_come_in_the_order_of_its_lines() {
    let reports = refused(
        "order",
        "<rect/>\n<box left=\"abc\" border-radius=\"4\"/>\n<box>\n<tween at=\"0\" duration=\"1\"/>\n</box>\n<image src=\"i.png\"/>",
    );
    let places: Vec<(String, String)> = reports
        .iter()
        .map(|report| {
            let mut words = report.split(' ');
            (words.nth(1).unwrap().to_string(), words.next().unwrap().to_string())
        })
        .collect();
    assert_eq!(
        places,
        vec![
            ("2".to_string(), "FS-0035".to_string()),
            ("3".to_string(), "FS-0034".to_string()),
            ("3".to_string(), "FS-0035".to_string()),
            ("5".to_string(), "FS-0035".to_string()),
            ("7".to_string(), "FS-0035".to_string()),
        ]
    );
}

fn with_project_file(name: &str, yaml: &str) -> Result<Scene, Vec<String>> {
    let folder = project_with("refusals", name, &composition("<box background=\"#ff0000\"/>"));
    fs::write(folder.join(PROJECT_FILE), yaml).unwrap();
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    Scene::from_project(&project).map_err(|reports| reports.iter().map(shown).collect())
}

#[test]
fn a_render_setting_this_version_does_not_apply_is_unsupported_with_the_line_of_its_value() {
    let yaml = "name: demo\nrender:\n  quality: high\n  encoder: software\n  codec: hevc\n  workers: 4\n  audio-bitrate: 32\n";
    let reports = with_project_file("settings", yaml).unwrap_err();
    assert_eq!(
        reports,
        vec![
            "framesmith.yaml 3 FS-0035 `render.quality: high` is not supported by this version, which takes standard",
            "framesmith.yaml 4 FS-0035 `render.encoder: software` is not supported by this version, which takes hardware",
            "framesmith.yaml 5 FS-0035 `render.codec: hevc` is not supported by this version, which takes h264",
            "framesmith.yaml 6 FS-0035 `render.workers: 4` is not supported by this version, which takes auto or 1",
            "framesmith.yaml 7 FS-0035 `render.audio-bitrate: 32` is not supported by this version, which takes 64 to 320",
        ]
    );
}

#[test]
fn a_render_setting_this_version_applies_is_accepted_whether_written_or_not() {
    let yaml = "name: demo\nrender:\n  backend: cpu\n  quality: standard\n  encoder: hardware\n  codec: h264\n  workers: 1\n  audio-bitrate: 192\n";
    assert!(with_project_file("settings-applied", yaml).is_ok());
    assert!(with_project_file("settings-auto", "render:\n  backend: auto\n  workers: auto\n").is_ok());
    assert!(with_project_file("settings-card", "render:\n  backend: gpu\n").is_ok());
    assert!(with_project_file("settings-none", "name: demo\n").is_ok());
}

#[test]
fn the_reports_of_the_project_file_come_before_those_of_the_composition() {
    let folder = project_with("refusals", "both-files", &composition("<rect/>"));
    fs::write(folder.join(PROJECT_FILE), "render:\n  quality: draft\n").unwrap();
    let project = Project::open(&folder).unwrap();
    let reports = Scene::from_project(&project).unwrap_err();
    let files: Vec<&str> = reports.iter().map(Report::file).collect();
    assert_eq!(files, vec![PROJECT_FILE, COMPOSITION_FILE]);
    assert!(reports.iter().all(|report| report.problem().kind() == Kind::Unsupported));
}

#[test]
fn the_composition_that_is_read_is_the_one_the_project_names() {
    let folder = scratch("refusals", "named-composition");
    fs::create_dir_all(folder.join("scenes")).unwrap();
    fs::write(folder.join("scenes/main.xml"), composition("<rect/>")).unwrap();
    fs::write(folder.join(PROJECT_FILE), "composition: scenes/main.xml\n").unwrap();
    let reports = Scene::from_project(&Project::open(&folder).unwrap()).unwrap_err();
    assert_eq!(
        shown(&reports[0]),
        "scenes/main.xml 2 FS-0035 <rect> is not supported by this version, which draws <box>, <text> and <composition src> only"
    );
}

#[test]
fn a_setting_an_option_gives_that_this_version_does_not_apply_is_unsupported_on_the_command_line() {
    let folder = project_with("refusals", "options", &composition("<box background=\"#ff0000\"/>"));
    fs::write(folder.join(PROJECT_FILE), "render:\n  quality: high\n  codec: hevc\n").unwrap();
    let mut project = Project::open(&folder).unwrap();
    let mut options = project::Options::default();
    // The option replaces the file's `quality: high`, which is then not in use and not refused.
    options.set("--quality", "standard").unwrap();
    options.set("--workers", "4").unwrap();
    project.apply(&options);
    let reports = Scene::from_project(&project).unwrap_err();
    assert_eq!(
        reports.iter().map(shown).collect::<Vec<_>>(),
        vec![
            "command line 0 FS-0035 `--workers 4` is not supported by this version, which takes auto or 1",
            "framesmith.yaml 3 FS-0035 `render.codec: hevc` is not supported by this version, which takes h264",
        ]
    );
    assert_eq!(
        reports[0].to_string(),
        "command line: FS-0035: unsupported: `--workers 4` is not supported by this version, which takes auto or 1. \
         Remove what the message names, or wait for the sprint that brings it."
    );
}

#[test]
fn a_sound_bit_rate_the_system_encoder_takes_is_accepted_and_one_it_does_not_is_unsupported() {
    for (name, rate) in [("rate-64", 64), ("rate-128", 128), ("rate-320", 320)] {
        assert!(
            with_project_file(name, &format!("render:\n  audio-bitrate: {rate}\n")).is_ok(),
            "{rate}"
        );
    }
    for (name, rate) in [("rate-63", 63), ("rate-321", 321)] {
        assert_eq!(
            with_project_file(name, &format!("render:\n  audio-bitrate: {rate}\n")).unwrap_err(),
            vec![format!(
                "framesmith.yaml 2 FS-0035 `render.audio-bitrate: {rate}` is not supported by this version, which takes 64 to 320"
            )]
        );
    }
}
