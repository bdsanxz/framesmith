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

/// The position of the one `codec` line whose message is `message`.
fn only(lines: &[Line], message: &str) -> usize {
    let found: Vec<usize> = (0..lines.len())
        .filter(|&at| lines[at].component == "codec" && lines[at].text.starts_with(&format!("{message} ")))
        .collect();
    assert_eq!(found.len(), 1, "one `{message}` line under codec: {found:?}");
    found[0]
}

#[test]
fn a_render_logs_the_start_and_end_of_frx7s_picture_and_sound_encoders_under_its_run_id() {
    let cwd = render("default-level", &[]);
    let lines = lines(&cwd);
    let (first, last) = (0, lines.len() - 1);
    assert!(lines[first].text.starts_with("command started "), "{}", log_text(&cwd));
    assert!(lines[last].text.starts_with("command finished "), "{}", log_text(&cwd));
    for (started, ended) in [
        ("picture encoder started", "picture encoder ended"),
        ("sound encoder started", "sound encoder ended"),
    ] {
        let (start, end) = (only(&lines, started), only(&lines, ended));
        assert!(first < start && start < end && end < last, "{started} at {start}, {ended} at {end}");
        for at in [start, end] {
            assert_eq!(lines[at].level, "INFO", "{}", lines[at].raw);
            assert_eq!(lines[at].run_id, lines[first].run_id, "{}", lines[at].raw);
        }
        let id = field(&lines[start].text, "id").expect("the start has an id");
        assert_eq!(field(&lines[end].text, "id"), Some(id), "{}", lines[end].raw);
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
