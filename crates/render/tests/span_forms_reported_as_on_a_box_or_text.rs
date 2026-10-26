//! A span's window written wrongly is reported in the words a box's is, and a dressing value not listed is reported on a
//! span in the words it is on a `<text>`, each on its own line (`docs/spec/text.md`, sections 6.5 rule 10 and 6.6;
//! `docs/spec/notation.md`, section 5.2).

mod common;

use project::{OpenError, Project, Report};
use render::Scene;
use std::fs;

const FONT: &str = "<font src=\"fonts/framesmith-test-stroke-400.ttf\" family=\"Framesmith Test Stroke\"/>";

/// The reports of a composition holding `body` on line 3, each as "line code message".
fn reports_of(name: &str, body: &str) -> Vec<String> {
    let text =
        format!("<composition version=\"1\" width=\"40\" height=\"20\" fps=\"10\" duration=\"1\">\n{FONT}\n{body}\n</composition>\n");
    let folder = common::project_with("span-forms-reported", name, &text);
    fs::create_dir_all(folder.join("fonts")).unwrap();
    let font = "framesmith-test-stroke-400.ttf";
    fs::copy(common::fixtures().join("fonts").join(font), folder.join("fonts").join(font)).unwrap();
    // A window is read when the project is opened, a dressing when its scene is made.
    let project = match Project::open(&folder) {
        Ok(project) => project,
        Err(OpenError::Invalid(reports)) => return reports.iter().map(shown).collect(),
        Err(_) => panic!("{name}: no project"),
    };
    match Scene::from_project(&project) {
        Ok(_) => Vec::new(),
        Err(reports) => reports.iter().map(shown).collect(),
    }
}

fn shown(report: &Report) -> String {
    format!(
        "{} {} {}",
        report.problem().line(),
        report.problem().code(),
        report.problem().message()
    )
}

#[test]
fn a_span_s_window_written_wrongly_is_reported_as_a_box_s_is() {
    for (name, window, words) in [
        ("start-alone", "start=\"1\"", "<span> needs both start and duration, or neither"),
        (
            "duration-alone",
            "duration=\"1\"",
            "<span> needs both start and duration, or neither",
        ),
        ("negative", "start=\"-1\" duration=\"1\"", "`start=\"-1\"` is not a time in seconds"),
        (
            "not-a-number",
            "start=\"1\" duration=\"1x\"",
            "`duration=\"1x\"` is not a time in seconds",
        ),
    ] {
        let span = reports_of(
            &format!("span-{name}"),
            &format!("<text font-family=\"Framesmith Test Stroke\">A<span {window}>B</span></text>"),
        );
        let boxed = reports_of(&format!("box-{name}"), &format!("<box {window} width=\"4\" height=\"4\"/>"));
        assert_eq!(span.len(), 1, "{name}: {span:?}");
        assert!(span[0].starts_with("3 FS-00") && span[0].contains(words), "{name}: {span:?}");
        assert_eq!(span[0].replace("<span>", "<box>"), boxed[0], "{name}");
    }
}

#[test]
fn a_dressing_value_not_listed_is_reported_on_a_span_as_on_a_text() {
    for (name, attribute) in [
        ("transform", "text-transform=\"lowercase\""),
        ("white-space", "white-space=\"pre\""),
        ("figures", "font-variant-numeric=\"slashed-zero\""),
        ("stroke", "text-stroke=\"thick #000000\""),
        ("shadow", "text-shadow=\"red\""),
    ] {
        let span = reports_of(
            &format!("span-{name}"),
            &format!("<text font-family=\"Framesmith Test Stroke\">A<span {attribute}>B</span></text>"),
        );
        let text = reports_of(
            &format!("text-{name}"),
            &format!("<text font-family=\"Framesmith Test Stroke\" {attribute}>AB</text>"),
        );
        assert_eq!(span.len(), 1, "{name}: {span:?}");
        assert_eq!(span, text, "{name}");
    }
}
