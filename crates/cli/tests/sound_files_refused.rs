//! An `<audio>` that cannot be placed stops `framesmith check` and `framesmith render` before the first frame
//! (`docs/spec/sound.md`, section 5): the report gives the element's line, the file as `src` writes it and the reason,
//! and no film is written. Every sound file is made by the test from bytes of our own.

mod common;

use std::fs;
use std::path::Path;

use common::{err, framesmith, out, project};

/// The bytes of a WAV file of `channels` channels at 48,000 Hz, of format `format` (1 is linear PCM, 7 is µ-law) and
/// `bits` a sample, whose header says it holds `said` bytes of sound and which holds `data`.
fn wav(format: u16, channels: u16, bits: u16, said: u32, data: &[u8]) -> Vec<u8> {
    let width = channels * bits / 8;
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&(36 + said).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&format.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&48_000u32.to_le_bytes());
    bytes.extend_from_slice(&(48_000 * u32::from(width)).to_le_bytes());
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&bits.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&said.to_le_bytes());
    bytes.extend_from_slice(data);
    bytes
}

/// What `check` and then `render` print, and their exit codes, each on a project folder of its own made of `audio` (the
/// `<audio>` on line 2) and the files `files`; and whether `render` wrote a film.
fn check_and_render(name: &str, audio: &str, files: &[(&str, Vec<u8>)]) -> [(Option<i32>, Vec<String>); 2] {
    let text = format!("<composition version=\"1\" width=\"64\" height=\"64\" fps=\"30\" duration=\"1\">\n{audio}\n</composition>\n");
    ["check", "render"].map(|command| {
        let (project, cwd) = project("sound-refused", &format!("{name}-{command}"), &text);
        for (file, bytes) in files {
            fs::write(project.join(file), bytes).unwrap();
        }
        let mut args = vec![command.to_string(), project.to_str().unwrap().to_string()];
        if command == "render" {
            args.extend(["--backend".to_string(), "cpu".to_string()]);
        }
        let output = framesmith(&cwd, &args, &[]);
        let printed = format!("{}{}", out(&output), err(&output));
        if command == "render" {
            assert!(!project.join("renders").exists() || no_film(&project.join("renders")), "{printed}");
        }
        // frx7's reason names the file on the machine; the project folder of each command is written `<project>`.
        let folder = project.to_str().unwrap();
        let lines = printed
            .lines()
            .filter(|line| line.contains("FS-"))
            .map(|line| line.replace(folder, "<project>"))
            .collect();
        (output.status.code(), lines)
    })
}

fn no_film(folder: &Path) -> bool {
    fs::read_dir(folder)
        .unwrap()
        .flatten()
        .all(|entry| entry.path().extension().is_none_or(|ext| ext != "mp4"))
}

/// The one report both commands give, the same from each, with exit code `exit`.
fn one_report(runs: &[(Option<i32>, Vec<String>); 2], exit: i32) -> String {
    for (status, lines) in runs {
        assert_eq!(*status, Some(exit), "{lines:?}");
        assert_eq!(lines.len(), 1, "{lines:?}");
    }
    assert_eq!(runs[0].1, runs[1].1, "check and render report alike");
    runs[0].1[0].clone()
}

fn a_good_wav() -> Vec<u8> {
    wav(1, 2, 16, 4_000, &[0; 4_000])
}

#[test]
fn an_audio_with_no_duration_is_reported_with_its_line_its_file_and_the_reason() {
    let line = one_report(
        &check_and_render(
            "no-duration",
            "<audio src=\"tone.wav\" start=\"0\"/>",
            &[("tone.wav", a_good_wav())],
        ),
        1,
    );
    assert!(line.starts_with("composition.xml, line 2: FS-0067: invalid:"), "{line}");
    assert!(line.contains("`<audio src=\"tone.wav\">` needs a `duration` above 0"), "{line}");
}

#[test]
fn an_audio_with_a_duration_of_0_is_reported_with_its_file_and_the_value() {
    let line = one_report(
        &check_and_render(
            "zero-duration",
            "<audio src=\"tone.wav\" start=\"0\" duration=\"0\"/>",
            &[("tone.wav", a_good_wav())],
        ),
        1,
    );
    assert!(line.starts_with("composition.xml, line 2: FS-0067: invalid:"), "{line}");
    assert!(
        line.contains("`<audio src=\"tone.wav\">` needs a `duration` above 0, the seconds the sound plays, not `duration=\"0\"`"),
        "{line}"
    );
}

#[test]
fn an_audio_with_no_src_and_no_duration_gets_two_reports_on_its_line() {
    let runs = check_and_render("no-src-no-duration", "<audio start=\"0\"/>", &[]);
    for (status, lines) in &runs {
        assert_eq!(*status, Some(1), "{lines:?}");
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("composition.xml, line 2: FS-0067: invalid: `<audio>` needs a `duration` above 0")),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("composition.xml, line 2: FS-0068: invalid: `<audio>` needs the attribute `src`")),
            "{lines:?}"
        );
    }
}

#[test]
fn an_audio_whose_file_is_missing_is_reported_with_its_line_its_file_and_the_reason() {
    let line = one_report(
        &check_and_render("missing", "<audio src=\"sounds/gone.wav\" start=\"0\" duration=\"1\"/>", &[]),
        1,
    );
    assert!(line.starts_with("composition.xml, line 2: FS-0068: invalid:"), "{line}");
    assert!(line.contains("`src=\"sounds/gone.wav\"` names no file"), "{line}");
}

#[test]
fn an_audio_whose_file_cannot_be_decoded_is_reported_with_its_line_its_file_and_the_reason() {
    // The header says 4,000 bytes of sound; the file holds 2,000.
    let cut = wav(1, 2, 16, 4_000, &[0; 2_000]);
    let line = one_report(
        &check_and_render(
            "damaged",
            "<audio src=\"cut.wav\" start=\"0\" duration=\"1\"/>",
            &[("cut.wav", cut)],
        ),
        1,
    );
    assert!(line.starts_with("composition.xml, line 2: FS-0071: invalid:"), "{line}");
    assert!(line.contains("`src=\"cut.wav\"` cannot be decoded:"), "{line}");
    assert!(line.contains("`<project>/cut.wav` is damaged"), "{line}");
}

#[test]
fn a_sound_file_of_a_kind_not_read_is_reported_with_its_line_and_the_kinds_read() {
    // µ-law sound in a WAV file: a kind of sound file this version does not read.
    let ulaw = wav(7, 1, 8, 4_000, &[0x7f; 4_000]);
    let line = one_report(
        &check_and_render(
            "ulaw",
            "<audio src=\"ulaw.wav\" start=\"0\" duration=\"1\"/>",
            &[("ulaw.wav", ulaw)],
        ),
        2,
    );
    assert!(line.starts_with("composition.xml, line 2: FS-0070: unsupported:"), "{line}");
    assert!(line.contains("`src=\"ulaw.wav\"` is not sound this version reads"), "{line}");
    assert!(line.contains("The kinds read are wav, aiff, mp3 and aac-in-mp4."), "{line}");
}

#[test]
fn a_media_start_past_the_end_of_the_source_is_reported() {
    // 4,000 bytes of two 16-bit channels are 1,000 sample frames: 0.021 s.
    let line = one_report(
        &check_and_render(
            "past-end",
            "<audio src=\"tone.wav\" start=\"0\" duration=\"1\" media-start=\"0.5\"/>",
            &[("tone.wav", a_good_wav())],
        ),
        1,
    );
    assert!(line.starts_with("composition.xml, line 2: FS-0072: invalid:"), "{line}");
    assert!(
        line.contains("`media-start=\"0.5\"` is at or past the end of `src=\"tone.wav\"`, which lasts 0.021 s"),
        "{line}"
    );
}

#[test]
fn a_volume_on_an_audio_is_refused_by_render_as_not_applied() {
    let runs = check_and_render(
        "volume",
        "<audio src=\"tone.wav\" start=\"0\" duration=\"1\" volume=\"0.5\"/>",
        &[("tone.wav", a_good_wav())],
    );
    let (status, lines) = &runs[1];
    assert_eq!(*status, Some(2), "{lines:?}");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("composition.xml, line 2: FS-0035: unsupported:"),
        "{}",
        lines[0]
    );
    assert!(lines[0].contains("`volume=\"0.5\"` on <audio> is not applied"), "{}", lines[0]);
}

#[test]
fn a_volume_point_inside_an_audio_is_refused_by_render_as_not_applied() {
    let runs = check_and_render(
        "volume-point",
        "<audio src=\"tone.wav\" start=\"0\" duration=\"1\">\n<volume at=\"0\" value=\"0.5\"/>\n</audio>",
        &[("tone.wav", a_good_wav())],
    );
    let (status, lines) = &runs[1];
    assert_eq!(*status, Some(2), "{lines:?}");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("composition.xml, line 3: FS-0035: unsupported: <volume> is not applied"),
        "{}",
        lines[0]
    );
}

#[test]
fn an_audio_in_a_composition_shown_from_another_file_is_refused_by_render() {
    let shown = b"<composition version=\"1\" width=\"64\" height=\"64\" fps=\"30\" duration=\"1\">\n<audio src=\"tone.wav\" start=\"0\" duration=\"1\"/>\n</composition>\n".to_vec();
    let runs = check_and_render(
        "in-shown-file",
        "<composition src=\"part.xml\" start=\"0\" duration=\"1\" left=\"0\" top=\"0\"/>",
        &[("part.xml", shown), ("tone.wav", a_good_wav())],
    );
    let (status, lines) = &runs[1];
    assert_eq!(*status, Some(2), "{lines:?}");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("part.xml, line 2: FS-0035: unsupported: <audio> in a composition shown from another file"),
        "{}",
        lines[0]
    );
}
