//! The lines frx7's library crates write into Framesmith's run log during a render (`docs/spec/run-log.md`,
//! section 4.1). The messages and fields expected are those of frx7's own `docs/spec/run-log.md` section 6,
//! written out here.

mod common;

use common::*;

const GROUP: &str = "frx7-lines";

const SMALL: &str = "<composition version=\"1\" id=\"small\" width=\"96\" height=\"54\" fps=\"10\" duration=\"1\" \
                     background=\"#102040\"/>\n";

/// Renders a small project from a folder of its own, with `env`, and gives that folder.
fn render(name: &str, env: &[(&str, &str)]) -> std::path::PathBuf {
    let (project, _) = project(GROUP, name, SMALL);
    let cwd = project.parent().unwrap().to_path_buf();
    let output = framesmith(&cwd, [std::ffi::OsStr::new("render"), project.as_os_str()], env);
    assert_eq!(output.status.code(), Some(0), "{}", err(&output));
    cwd
}

/// The value of the field `name` in a line's text, as written (`name=value`).
fn field<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.split_whitespace().find_map(|word| word.strip_prefix(name)?.strip_prefix('='))
}

/// The positions of the `codec` lines whose message is `message`, oldest first.
fn positions(lines: &[Line], message: &str) -> Vec<usize> {
    (0..lines.len())
        .filter(|&at| lines[at].component == "codec" && lines[at].text.starts_with(&format!("{message} ")))
        .collect()
}

/// The (start, end) of every pair the messages name, oldest first: the size check makes the encoder to ask it, then
/// the film makes it again, so the picture encoder holds the check's pair before the film's.
fn pairs(lines: &[Line], started: &str, ended: &str, count: usize) -> Vec<(usize, usize)> {
    let starts = positions(lines, started);
    let ends = positions(lines, ended);
    assert_eq!(starts.len(), count, "{started}: {starts:?}");
    assert_eq!(ends.len(), count, "{ended}: {ends:?}");
    let mut paired = Vec::new();
    let mut taken = vec![false; starts.len()];
    for &end in &ends {
        let id = field(&lines[end].text, "id").expect("the end has an id");
        let at = starts
            .iter()
            .position(|&start| {
                start < end && !taken[starts.iter().position(|&s| s == start).unwrap()] && field(&lines[start].text, "id") == Some(id)
            })
            .expect("the end follows an unused start of its id");
        taken[at] = true;
        paired.push((starts[at], end));
    }
    assert!(taken.iter().all(|&used| used), "every start ends");
    paired
}

#[test]
fn a_render_logs_the_start_and_end_of_frx7s_picture_and_sound_encoders_under_its_run_id() {
    let cwd = render("default-level", &[]);
    let lines = lines(&cwd);
    let (first, last) = (0, lines.len() - 1);
    assert!(lines[first].text.starts_with("command started "), "{}", log_text(&cwd));
    assert!(lines[last].text.starts_with("command finished "), "{}", log_text(&cwd));
    let picture = pairs(&lines, "picture encoder started", "picture encoder ended", 2);
    let sound = pairs(&lines, "sound encoder started", "sound encoder ended", 1);
    // The check's pair comes before the film's pair; where the sound pair falls between them is not set.
    assert!(picture[0].1 < picture[1].0, "check {picture:?} before film");
    for (start, end) in picture.into_iter().chain(sound) {
        assert!(first < start && start < end && end < last, "{start} to {end}");
        for at in [start, end] {
            assert_eq!(lines[at].level, "INFO", "{}", lines[at].raw);
            assert_eq!(lines[at].run_id, lines[first].run_id, "{}", lines[at].raw);
        }
    }
    // No line of frx7's holds a path: neither the folder the command ran in nor the film's.
    let folder = cwd.display().to_string();
    for line in lines.iter().filter(|line| line.component == "codec" || line.component == "probe") {
        assert!(!line.text.contains(&folder) && !line.text.contains(".mp4"), "{}", line.raw);
    }
}

#[test]
fn at_warn_a_render_logs_none_of_frx7s_encoder_lines() {
    let cwd = render("at-warn", &[("FRAMESMITH_LOG", "warn")]);
    let text = log_text(&cwd);
    for message in [
        "picture encoder started",
        "picture encoder ended",
        "sound encoder started",
        "sound encoder ended",
    ] {
        assert!(!text.contains(message), "{text}");
    }
}
