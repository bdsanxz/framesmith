//! `framesmith render` writes its picture with the encoder and at the quality the author chose, names them in its summary,
//! never another, and refuses before the first frame a film size the chosen encoder does not take, as `check` does
//! (`docs/spec/render-command.md` sections 5.4 and 7). Run as a program, the way its user runs it. Which encoder wrote a
//! film is read from frx7's own report in the run log, at `debug` (`encoder started` with `hardware=`).

mod common;

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use common::project as make;
use common::*;

const GROUP: &str = "encoder-choice";

/// A box that moves, one second at 10 frames a second.
const FILM: &str = "\
<composition version=\"1\" id=\"film\" width=\"96\" height=\"64\" fps=\"10\" duration=\"1\" background=\"#204060\">
<box left=\"16\" top=\"16\" width=\"32\" height=\"32\" background=\"#ffffff\">
<tween at=\"0\" duration=\"1\" to-x=\"48\"/>
</box>
</composition>
";

/// A film of 1280 by 720 at 30 frames a second, one second long, whose pictures need many bits: 60 small boxes of
/// many colours moving over a gradient, placed by a fixed sequence so that every run draws the same film.
fn busy_film() -> String {
    let mut seed: u32 = 7;
    let mut next = |below: u32| {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (seed >> 8) % below
    };
    let mut text = String::from(
        "<composition version=\"1\" id=\"busy\" width=\"1280\" height=\"720\" fps=\"30\" duration=\"1\" background=\"#204060\">\n\
         <box left=\"0\" top=\"0\" width=\"1280\" height=\"720\" background=\"linear-gradient(45deg, #204060 0%, #a03020 100%)\"/>\n",
    );
    for _ in 0..60 {
        let (left, top) = (next(1200), next(660));
        let (width, height) = (8 + next(32), 8 + next(32));
        let colour = next(0x0100_0000);
        let (to_x, to_y) = (next(1200), next(660));
        text.push_str(&format!(
            "<box left=\"{left}\" top=\"{top}\" width=\"{width}\" height=\"{height}\" background=\"#{colour:06x}\">\n\
             <tween at=\"0\" duration=\"1\" to-x=\"{to_x}\" to-y=\"{to_y}\"/>\n</box>\n"
        ));
    }
    text.push_str("</composition>\n");
    text
}

/// Renders `project` from `cwd` with `options`, at the log level `debug`, the film written to `name` beside the
/// project, and gives the output and the film's path.
fn render(cwd: &Path, project: &Path, name: &str, options: &[&str]) -> (Output, PathBuf) {
    let film = project.parent().unwrap().join(name);
    let mut args: Vec<OsString> = vec![
        "render".into(),
        project.as_os_str().to_owned(),
        "--output".into(),
        film.as_os_str().to_owned(),
    ];
    args.extend(options.iter().map(OsString::from));
    (framesmith(cwd, args, &[("FRAMESMITH_LOG", "debug")]), film)
}

fn rendered(output: &Output) {
    assert_eq!(output.status.code(), Some(0), "{}{}", out(output), err(output));
}

fn summary(output: &Output, label: &str) -> String {
    let printed = out(output);
    printed
        .lines()
        .find(|line| line.starts_with(&format!("{label}: ")))
        .unwrap_or_else(|| panic!("no `{label}` line in:\n{printed}"))
        .to_string()
}

/// What frx7 said of the encoder the last render of `cwd` got: `hardware=true` or `hardware=false`.
fn encoder_used(cwd: &Path) -> String {
    let started: Vec<Line> = lines(cwd)
        .into_iter()
        .filter(|line| line.component == "render::film" && line.text.starts_with("encoder started "))
        .collect();
    let last = started.last().expect("an encoder started line");
    last.text
        .split_whitespace()
        .find(|word| word.starts_with("hardware="))
        .expect("the hardware field")
        .to_string()
}

fn marks(output: &Output) -> Vec<String> {
    out(output)
        .lines()
        .filter(|line| line.starts_with("mark "))
        .map(str::to_string)
        .collect()
}

/// The pictures of a film, counted from the sample table: the `stsz` box of the first track, whose count follows its
/// version, flags and sample size (ISO/IEC 14496-12, 8.7.3).
fn pictures(film: &Path) -> u32 {
    let bytes = fs::read(film).unwrap();
    let at = bytes.windows(4).position(|four| four == b"stsz").expect("an stsz box");
    u32::from_be_bytes(bytes[at + 12..at + 16].try_into().unwrap())
}

#[test]
#[ignore = "reference machine only: needs the machine's own video encoder"]
fn reference_machine_only_the_defaults_use_the_machine_s_own_encoder_and_software_the_processor_s() {
    let (project, cwd) = make(GROUP, "which-encoder", FILM);
    let (output, _) = render(&cwd, &project, "default.mp4", &[]);
    rendered(&output);
    assert_eq!(summary(&output, "encoder"), "encoder: hardware (default)");
    assert_eq!(summary(&output, "quality"), "quality: standard (default)");
    assert_eq!(summary(&output, "codec"), "codec: h264 (default)");
    assert_eq!(encoder_used(&cwd), "hardware=true");

    let (output, _) = render(&cwd, &project, "software.mp4", &["--encoder", "software"]);
    rendered(&output);
    assert_eq!(summary(&output, "encoder"), "encoder: software (option)");
    assert_eq!(summary(&output, "codec"), "codec: h264 (default)");
    assert_eq!(encoder_used(&cwd), "hardware=false");
}

#[test]
fn the_software_encoder_of_the_project_file_writes_every_picture_at_each_quality_and_is_named() {
    let (project, cwd) = make(GROUP, "software-file", FILM);
    fs::write(project.join("framesmith.yaml"), "render:\n  encoder: software\n").unwrap();
    for quality in ["draft", "standard", "high"] {
        let (output, film) = render(&cwd, &project, &format!("{quality}.mp4"), &["--quality", quality]);
        rendered(&output);
        assert_eq!(summary(&output, "encoder"), "encoder: software (project file)");
        assert_eq!(summary(&output, "quality"), format!("quality: {quality} (option)"));
        assert_eq!(encoder_used(&cwd), "hardware=false", "{quality}");
        assert_eq!(pictures(&film), 10, "{quality}: every picture is in the film");
    }
}

#[test]
#[ignore = "reference machine only: needs the machine's own video encoder"]
fn reference_machine_only_the_frame_marks_are_the_same_for_both_encoders() {
    let (project, cwd) = make(GROUP, "marks", FILM);
    let (hardware, _) = render(&cwd, &project, "hardware.mp4", &["--marks", "--backend", "cpu"]);
    rendered(&hardware);
    let (software, _) = render(
        &cwd,
        &project,
        "software.mp4",
        &["--marks", "--backend", "cpu", "--encoder", "software"],
    );
    rendered(&software);
    let (hardware, software) = (marks(&hardware), marks(&software));
    assert_eq!(hardware.len(), 10, "{hardware:?}");
    assert_eq!(hardware, software);
    let values: Vec<&str> = hardware.iter().map(|line| line.rsplit(' ').next().unwrap()).collect();
    assert!(values.windows(2).any(|pair| pair[0] != pair[1]), "the box moves: {hardware:?}");
}

/// The sizes of the busy film at `draft`, `standard` and `high` with `encoder`.
fn sizes(name: &str, encoder: &str) -> Vec<u64> {
    let (project, cwd) = make(GROUP, name, &busy_film());
    ["draft", "standard", "high"]
        .iter()
        .map(|quality| {
            let (output, film) = render(
                &cwd,
                &project,
                &format!("{quality}.mp4"),
                &["--backend", "cpu", "--encoder", encoder, "--quality", quality],
            );
            rendered(&output);
            fs::metadata(film).unwrap().len()
        })
        .collect()
}

#[test]
#[ignore = "reference machine only: needs the machine's own video encoder"]
fn reference_machine_only_with_the_machine_s_own_encoder_draft_is_smaller_than_standard_and_standard_than_high() {
    let sizes = sizes("sizes-hardware", "hardware");
    assert!(sizes[0] < sizes[1] && sizes[1] < sizes[2], "{sizes:?}");
}

#[test]
fn with_the_software_encoder_draft_is_smaller_than_standard_and_standard_than_high() {
    let sizes = sizes("sizes-software", "software");
    assert!(sizes[0] < sizes[1] && sizes[1] < sizes[2], "{sizes:?}");
}

/// One picture of 7680 by 4320, larger than the machine's own H.264 encoder takes (`frx7/docs/spec/os-codecs.md`
/// section 5: on the reference machine it refuses 8192 by 4320 and takes 4096 by 2306).
const EIGHT_K: &str = "\
<composition version=\"1\" id=\"large\" width=\"7680\" height=\"4320\" fps=\"1\" duration=\"1\" background=\"#204060\">
<box left=\"640\" top=\"640\" width=\"1280\" height=\"1280\" background=\"#ffffff\"/>
</composition>
";

const REFUSAL: &str = "FS-0073: unsupported: the machine's own H.264 encoder does not take a film of 7680x4320 \
     (render.encoder: hardware, render.codec: h264); set codec: hevc, which the machine's own encoder takes at this size, \
     or encoder: software. Set the setting the message names: `render.codec` (or `--codec`) to `hevc` where the message \
     offers it, or `render.encoder` (or `--encoder`) to `software`.";

#[test]
#[ignore = "reference machine only: the size limits are those of the reference machine's own encoder"]
fn reference_machine_only_an_8k_film_with_the_defaults_is_refused_before_the_first_frame_and_writes_nothing() {
    let (project, cwd) = make(GROUP, "large-refused", EIGHT_K);
    let (output, film) = render(&cwd, &project, "large.mp4", &[]);
    assert_eq!(output.status.code(), Some(2), "{}{}", out(&output), err(&output));
    assert_eq!(
        out(&output),
        format!("{REFUSAL}\nRefused: the video was not made, because the chosen encoder does not take the film's size\n")
    );
    assert!(!film.exists(), "no file is written");
    assert!(
        !lines(&cwd).iter().any(|line| line.text.starts_with("render started ")),
        "no frame is drawn"
    );
}

#[test]
#[ignore = "reference machine only: the size limits are those of the reference machine's own encoder"]
fn reference_machine_only_check_reports_the_same_refusal_of_an_8k_film() {
    let (project, cwd) = make(GROUP, "large-check", EIGHT_K);
    let output = framesmith(&cwd, [OsString::from("check"), project.as_os_str().to_owned()], &[]);
    assert_eq!(output.status.code(), Some(2), "{}{}", out(&output), err(&output));
    assert_eq!(
        out(&output),
        format!("{REFUSAL}\nRefused: the video would not be made, because the chosen encoder does not take the film's size\n")
    );
}

#[test]
#[ignore = "reference machine only: needs the machine's own HEVC encoder at 8K"]
fn reference_machine_only_an_8k_film_completes_with_each_setting_the_refusal_names() {
    for (name, options, codec, encoder) in [
        (
            "large-hevc",
            ["--codec", "hevc"],
            "codec: hevc (option)",
            "encoder: hardware (default)",
        ),
        (
            "large-software",
            ["--encoder", "software"],
            "codec: h264 (default)",
            "encoder: software (option)",
        ),
    ] {
        let (project, cwd) = make(GROUP, name, EIGHT_K);
        let (output, film) = render(&cwd, &project, "large.mp4", &options);
        rendered(&output);
        assert_eq!(summary(&output, "size"), "size: 7680x4320");
        assert_eq!(summary(&output, "codec"), codec);
        assert_eq!(summary(&output, "encoder"), encoder);
        assert_eq!(pictures(&film), 1);
    }
}

#[test]
fn an_encoder_or_a_quality_outside_its_list_is_told_with_the_values_taken() {
    for (options, message) in [
        (["--encoder", "gpu"], "`--encoder` must be one of hardware, software, not gpu."),
        (["--quality", "best"], "`--quality` must be one of draft, standard, high, not best."),
    ] {
        let (project, cwd) = make(GROUP, &format!("outside-{}", options[1]), FILM);
        let (output, film) = render(&cwd, &project, "film.mp4", &options);
        assert_eq!(output.status.code(), Some(1), "{options:?}");
        assert!(
            err(&output).starts_with(&format!("framesmith: FS-0037: invalid: {message} ")),
            "{}",
            err(&output)
        );
        assert!(!film.exists());
    }
    for (yaml, message) in [
        (
            "render:\n  encoder: gpu\n",
            "`render.encoder` must be one of hardware, software, not gpu",
        ),
        (
            "render:\n  quality: best\n",
            "`render.quality` must be one of draft, standard, high, not best",
        ),
    ] {
        let (project, cwd) = make(GROUP, &format!("outside-file-{}", yaml.len()), FILM);
        fs::write(project.join("framesmith.yaml"), yaml).unwrap();
        let output = framesmith(&cwd, [OsString::from("check"), project.as_os_str().to_owned()], &[]);
        assert_eq!(output.status.code(), Some(1), "{yaml}");
        assert!(out(&output).contains(message), "{}", out(&output));
    }
}
