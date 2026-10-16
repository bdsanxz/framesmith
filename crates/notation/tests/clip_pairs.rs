//! `start` and `duration` are written both or neither on an element that takes them as a clip, and
//! one written without the other is reported with the line of its element (`docs/spec/notation.md`
//! section 5.2; `docs/spec/check-command.md` section 3a).

mod common;

use common::{document, problems_in, worded};
use notation::Code;
use std::fs;
use std::path::Path;

/// The reports of the pair of a clip in a valid root with `body` in it.
fn pair_reports(body: &str) -> Vec<notation::Problem> {
    problems_in(body)
        .into_iter()
        .filter(|problem| problem.code() == Code::ClipNotPaired)
        .collect()
}

#[test]
fn a_start_without_a_duration_is_reported_on_the_line_of_its_box() {
    let body = "<box id=\"first\" width=\"8\" height=\"8\"/>\n<box start=\"1\" width=\"8\" height=\"8\"/>";
    let problems = problems_in(body);
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(
        worded(&problems[0]),
        "line 3: FS-0040: <box> needs both start and duration, or neither"
    );
}

#[test]
fn a_duration_without_a_start_is_reported_on_the_line_of_its_box() {
    let body = "<box id=\"first\" width=\"8\" height=\"8\"/>\n<box duration=\"2\" width=\"8\" height=\"8\"/>";
    let problems = problems_in(body);
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(
        worded(&problems[0]),
        "line 3: FS-0040: <box> needs both start and duration, or neither"
    );
}

#[test]
fn a_box_with_both_or_with_neither_is_not_reported_for_its_pair() {
    let body = "<box start=\"1\" duration=\"2\" width=\"8\" height=\"8\"/>\n<box width=\"8\" height=\"8\"/>";
    assert!(problems_in(body).is_empty(), "{:#?}", problems_in(body));
}

#[test]
fn the_pair_is_reported_for_every_element_that_takes_a_clip() {
    let elements = [
        ("<rect start=\"1\" width=\"8\" height=\"8\"/>", "<rect>"),
        ("<text start=\"1\" width=\"8\" height=\"8\">\nhello\n</text>", "<text>"),
        ("<text width=\"8\" height=\"8\">\n<span start=\"1\">hello</span>\n</text>", "<span>"),
        ("<image src=\"images/logo.png\" start=\"1\" width=\"8\" height=\"8\"/>", "<image>"),
        ("<video src=\"clips/intro.mp4\" width=\"8\" height=\"8\" start=\"1\"/>", "<video>"),
        ("<shape d=\"M0 0\" start=\"1\" width=\"8\" height=\"8\"/>", "<shape>"),
    ];
    for (body, tag) in elements {
        let reports = pair_reports(body);
        assert_eq!(reports.len(), 1, "{body}: {reports:#?}");
        assert_eq!(
            reports[0].message(),
            format!("{tag} needs both start and duration, or neither"),
            "{body}"
        );
    }
}

#[test]
fn an_audio_and_a_nested_composition_are_not_judged_for_the_pair() {
    let audio = pair_reports("<audio src=\"audio/bed.wav\" start=\"1\"/>");
    assert!(audio.is_empty(), "{audio:#?}");
    let nested = pair_reports("<composition src=\"scenes/other.xml\" start=\"1\"/>");
    assert!(nested.is_empty(), "{nested:#?}");
}

#[test]
fn the_message_of_the_pair_is_the_one_the_notation_spec_shows() {
    let spec = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/spec/notation.md")).unwrap();
    let message = "<box> needs both start and duration, or neither";
    assert!(spec.contains(message), "section 5.2 does not hold: {message}");
    let reports = pair_reports("<box start=\"1\"/>");
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].message(), message);
    // The document helper is the one the other tests use: its body starts on line 2.
    assert_eq!(reports[0].line(), 2, "{}", document("<box start=\"1\"/>"));
}
