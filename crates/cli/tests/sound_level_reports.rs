//! The reports of a sound's level (`docs/spec/sound.md`, section 3a): points out of time order (the first of each sound),
//! a point with no `value`, a level below 0, and a point with no `at`, which is at the start of the sound and is not
//! reported. A sound that cannot be placed is reported before the renderer is chosen (section 5).

mod common;

use std::fs;

use common::{err, framesmith, out};
use testkit::sound_files::{wav_bytes, Pcm};

/// The bytes of a stereo WAV file of 16-bit samples at 48,000 Hz, of a second of a tone at 0.25.
fn tone_wav() -> Vec<u8> {
    wav_bytes(48_000, 2, Pcm::Int(16), &vec![0.25; 2 * 48_000])
}

/// Runs `check` and `render --backend cpu` on a project of a one-second film with the `body` lines, which begin at line 2
/// (the `<audio>` in `tone.wav`, or the `<audio>` and its points). Gives the exit code of `check`, the lines `check` prints
/// and the exit code of `render`, and whether `render` wrote a film.
fn check_and_render(name: &str, body: &str) -> (Option<i32>, Vec<String>, Option<i32>, bool) {
    let text = format!("<composition version=\"1\" width=\"64\" height=\"64\" fps=\"30\" duration=\"1\">\n{body}\n</composition>\n");
    let (project, cwd) = common::project("sound-level-reports", name, &text);
    fs::write(project.join("tone.wav"), tone_wav()).unwrap();
    let folder = project.to_str().unwrap().to_string();
    let checked = framesmith(&cwd, ["check", &folder], &[("NO_COLOR", "1")]);
    let lines = out(&checked).lines().map(|line| line.replace(&folder, "<project>")).collect();
    let (rendered, film) = {
        let (project, cwd) = common::project("sound-level-reports-render", name, &text);
        fs::write(project.join("tone.wav"), tone_wav()).unwrap();
        let output = framesmith(
            &cwd,
            ["render", project.to_str().unwrap(), "--backend", "cpu"],
            &[("NO_COLOR", "1")],
        );
        let written = fs::read_dir(project.join("renders")).is_ok_and(|entries| {
            entries
                .flatten()
                .any(|entry| entry.path().extension().is_some_and(|ext| ext == "mp4"))
        });
        (output.status.code(), written)
    };
    (checked.status.code(), lines, rendered, film)
}

/// The report lines of `check`: those that name an error code.
fn reports(lines: &[String]) -> Vec<&String> {
    lines.iter().filter(|line| line.contains(": invalid: ")).collect()
}

#[test]
fn points_out_of_time_order_are_reported_once_on_the_first_that_is_out_of_order() {
    // The points are at 2, 1 and 0 seconds: the one at 1 (line 4) is the first out of order; the one at 0 is after it and
    // is not reported again.
    let body = "<audio src=\"tone.wav\" start=\"0\" duration=\"1\">\n<volume at=\"2\" value=\"1\"/>\n<volume at=\"1\" value=\"0.5\"/>\n<volume at=\"0\" value=\"0\"/>\n</audio>";
    let (checked, lines, rendered, film) = check_and_render("out-of-order", body);
    assert_eq!(checked, Some(1), "{lines:?}");
    let found = reports(&lines);
    assert_eq!(found.len(), 1, "{lines:?}");
    assert!(
        found[0].starts_with("composition.xml, line 4: FS-0087: invalid: the <volume> points of an <audio> must be in order of time"),
        "{}",
        found[0]
    );
    assert!(found[0].contains("the point at 1 s is before the one at 2 s"), "{}", found[0]);
    assert_eq!(rendered, Some(1));
    assert!(!film, "a refused sound writes no film");
}

#[test]
fn a_point_with_no_value_is_reported_with_its_line() {
    let body = "<audio src=\"tone.wav\" start=\"0\" duration=\"1\">\n<volume at=\"0\" value=\"1\"/>\n<volume at=\"0.5\"/>\n</audio>";
    let (checked, lines, rendered, _) = check_and_render("no-value", body);
    assert_eq!(checked, Some(1), "{lines:?}");
    let found = reports(&lines);
    assert_eq!(found.len(), 1, "{lines:?}");
    assert!(
        found[0].starts_with("composition.xml, line 4: FS-0088: invalid: `<volume>` needs a `value`"),
        "{}",
        found[0]
    );
    assert_eq!(rendered, Some(1));
}

#[test]
fn a_point_below_0_and_a_volume_below_0_are_each_reported_as_fs_0034_on_their_lines() {
    let body = "<audio src=\"tone.wav\" start=\"0\" duration=\"1\" volume=\"-1\">\n<volume at=\"0\" value=\"-0.5\"/>\n</audio>";
    let (checked, lines, rendered, _) = check_and_render("below-zero", body);
    assert_eq!(checked, Some(1), "{lines:?}");
    let found = reports(&lines);
    assert_eq!(found.len(), 2, "{lines:?}");
    assert!(
        found[0].starts_with("composition.xml, line 2: FS-0034: invalid: `volume=\"-1\"` is below 0"),
        "{}",
        found[0]
    );
    assert!(
        found[1].starts_with("composition.xml, line 3: FS-0034: invalid: `value=\"-0.5\"` is below 0"),
        "{}",
        found[1]
    );
    assert_eq!(rendered, Some(1));
}

#[test]
fn a_point_with_no_time_is_at_the_start_of_the_sound_and_is_not_reported() {
    // The first point has no `at`: it is at 0, the start of the sound, and the second is at 0.5 s, in order after it.
    let body = "<audio src=\"tone.wav\" start=\"0\" duration=\"1\">\n<volume value=\"1\"/>\n<volume at=\"0.5\" value=\"0\"/>\n</audio>";
    let (checked, lines, rendered, film) = check_and_render("no-time", body);
    assert_eq!(checked, Some(0), "{lines:?}");
    assert!(reports(&lines).is_empty(), "{lines:?}");
    assert_eq!(rendered, Some(0));
    assert!(film, "a sound with a level that is in order writes its film");
}

#[test]
fn a_sound_that_cannot_be_placed_is_reported_before_the_renderer_is_chosen() {
    // The file is missing, and the card is asked for: the report of the sound comes first, not a refusal of the card.
    let text = "<composition version=\"1\" width=\"64\" height=\"64\" fps=\"30\" duration=\"1\">\n<audio src=\"missing.wav\" start=\"0\" duration=\"1\"/>\n</composition>\n";
    let (project, cwd) = common::project("sound-level-reports", "judged-first", text);
    let output = framesmith(
        &cwd,
        ["render", project.to_str().unwrap(), "--backend", "gpu"],
        &[("NO_COLOR", "1")],
    );
    let printed = format!("{}{}", out(&output), err(&output));
    assert!(printed.contains("FS-0077"), "{printed}");
    assert!(!printed.contains("FS-0038"), "the card was judged before the sound: {printed}");
}
