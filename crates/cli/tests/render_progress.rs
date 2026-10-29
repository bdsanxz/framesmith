//! Progress while the film is written (`docs/spec/render-command.md`, section 5.5): one line per stage on
//! standard error, with the frames done and the frames in all; the frames line ends at the frame count; a stage with
//! nothing to do says so once. A slow encoding stage holds the frames stage to two finished pictures ahead.

mod common;

use common::{err, framesmith, out, project};
use testkit::sound_files::{write_wav, Pcm};

/// The lines of one telling: the stage counts of one moment of the run, in the order of the spec.
type Telling = Vec<(String, u64, u64)>;

/// Renders the composition with the files `extra` written into the project folder first (`("silent.wav",
/// Sound)` writes a silent second) and gives the standard error of the run: the render ends 0.
fn render_stderr(name: &str, text: &str, extra: &[(&str, &str)], args: &[&str]) -> String {
    let (project, cwd) = project("render-progress", name, text);
    for (file, kind) in extra {
        assert_eq!(*kind, "silent", "only a silent second is made here");
        let samples = vec![0.0; 48_000 * 2];
        write_wav(&project.join(file), 48_000, 2, Pcm::Float, &samples).unwrap();
    }
    let mut command = vec!["render", project.to_str().unwrap(), "--backend", "cpu"];
    command.extend(args);
    let output = framesmith(&cwd, command, &[]);
    assert_eq!(output.status.code(), Some(0), "{}{}", out(&output), err(&output));
    err(&output)
}

/// Groups the count lines into tellings of `stages` lines: every line names one of the four stages, carries no
/// colour, and the stages of a telling come in the order of the spec.
fn tellings(stderr: &str, stages: &[&str]) -> Vec<Telling> {
    let mut tellings: Vec<Telling> = Vec::new();
    for line in stderr.lines().filter(|line| !line.ends_with("nothing to do")) {
        assert!(!line.contains('\u{1b}'), "{line}");
        let (stage, rest) = line.split_once(": ").expect(line);
        let (done, total) = rest.strip_suffix(" frames").expect(line).split_once('/').expect(line);
        let telling = if tellings.is_empty() || tellings.last().unwrap().len() == stages.len() {
            tellings.push(Vec::new());
            tellings.last_mut().unwrap()
        } else {
            tellings.last_mut().unwrap()
        };
        telling.push((stage.to_string(), done.parse().expect(line), total.parse().expect(line)));
    }
    for telling in &tellings {
        assert_eq!(telling.len(), stages.len(), "{telling:?}");
        for (got, wanted) in telling.iter().zip(stages.iter()) {
            assert_eq!(&got.0, wanted, "{telling:?}");
        }
    }
    tellings
}

fn boxes(seconds: u32) -> String {
    format!(
        "<composition version=\"1\" width=\"96\" height=\"64\" fps=\"30\" duration=\"{seconds}\" background=\"#102040\">\n<box left=\"8\" top=\"8\" width=\"16\" height=\"16\" background=\"#ffffff\"/>\n</composition>\n"
    )
}

#[test]
fn every_stage_is_told_by_name_with_its_frames_and_the_frames_line_ends_at_the_count() {
    // Boxes with a silent sound: footage has nothing to do (once), the other three count to 30, and the
    // last telling reads 30/30 on every stage.
    let stderr = render_stderr(
        "counted",
        &boxes(1).replace(
            "</composition>",
            "<audio src=\"silent.wav\" start=\"0\" duration=\"1\"/>\n</composition>",
        ),
        &[("silent.wav", "silent")],
        &[],
    );
    assert_eq!(
        stderr.lines().filter(|line| *line == "footage: nothing to do").count(),
        1,
        "{stderr}"
    );
    let tellings = tellings(&stderr, &["frames", "encoding", "sound"]);
    assert!(!tellings.is_empty(), "{stderr}");
    for telling in &tellings {
        for (_, _, total) in telling {
            assert_eq!(*total, 30);
        }
    }
    for (stage, done, _) in tellings.last().unwrap() {
        assert_eq!(*done, 30, "{stage}");
    }
}

#[test]
fn a_slow_encoding_stage_holds_the_frames_stage_to_two_finished_pictures_ahead() {
    // A two-second film at 1280 by 720 through the software encoder at high quality, with its footage
    // a small picture in the corner: encoding each full frame takes far longer than drawing it, so the
    // frames stage waits on the bound. Every telling then holds the frames count to the encoding count
    // plus the two waiting pictures and the one in encoding, and footage to two frames ahead of drawing;
    // and the wait really engaged, the frames count standing two or more ahead at least once.
    let text = "<composition version=\"1\" width=\"1280\" height=\"720\" fps=\"30\" duration=\"2\" background=\"#102040\">\n<box left=\"700\" top=\"100\" width=\"400\" height=\"300\" background=\"#ffffff\"/>\n<video src=\"clip.mp4\" width=\"64\" height=\"48\" left=\"8\" top=\"8\"/>\n<audio src=\"silent.wav\" start=\"0\" duration=\"2\"/>\n</composition>\n";
    let (project, cwd) = project("render-progress", "bound", text);
    let pictures: Vec<measure::Frame> = (0..60)
        .map(|_| measure::Frame {
            width: 64,
            height: 48,
            rgb: vec![99; 64 * 48 * 3],
        })
        .collect();
    measure::write_video(&project.join("clip.mp4"), (30, 1), &pictures).unwrap();
    let samples = vec![0.0; 48_000 * 2 * 2];
    write_wav(&project.join("silent.wav"), 48_000, 2, Pcm::Float, &samples).unwrap();
    let output = framesmith(
        &cwd,
        [
            "render",
            project.to_str().unwrap(),
            "--backend",
            "cpu",
            "--encoder",
            "software",
            "--quality",
            "high",
        ],
        &[],
    );
    assert_eq!(output.status.code(), Some(0), "{}{}", out(&output), err(&output));
    let tellings = tellings(&err(&output), &["footage", "frames", "encoding", "sound"]);
    assert!(!tellings.is_empty());
    let mut engaged = false;
    for telling in &tellings {
        let at = |stage: &str| telling.iter().find(|(name, _, _)| name == stage).unwrap().1;
        let (footage, frames, encoding) = (at("footage"), at("frames"), at("encoding"));
        for (_, _, total) in telling {
            assert_eq!(*total, 60);
        }
        assert!(frames <= encoding + 3, "{telling:?}");
        assert!(footage <= frames + 2, "{telling:?}");
        engaged = engaged || frames >= encoding + 2;
    }
    assert!(engaged, "the bound never engaged");
    let last = tellings.last().unwrap();
    for (stage, done, _) in last {
        assert_eq!(*done, 60, "{stage}");
    }
}

#[test]
fn stages_with_nothing_to_do_say_so_once() {
    // No footage and no sound: footage says it has nothing to do, once, while frames and encoding count
    // on. (A film always holds a silent sound track, so the sound stage still counts its frames even with
    // no sound in the composition.)
    let stderr = render_stderr("idle", &boxes(1), &[], &[]);
    assert_eq!(
        stderr.lines().filter(|line| *line == "footage: nothing to do").count(),
        1,
        "{stderr}"
    );
    assert!(
        stderr.lines().filter(|line| *line == "sound: nothing to do").count() == 0,
        "{stderr}"
    );
    let tellings = tellings(&stderr, &["frames", "encoding", "sound"]);
    assert!(!tellings.is_empty(), "{stderr}");
    for (stage, done, _) in tellings.last().unwrap() {
        assert_eq!(*done, 30, "{stage}");
    }
}
