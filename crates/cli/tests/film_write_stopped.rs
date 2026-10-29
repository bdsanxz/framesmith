//! A film whose write stops after its first frames is a failure: `FS-0036`, exit 1, the earlier film kept at the output
//! path and no staging file left (`docs/spec/render-command.md`, sections 5 and 6). The stop is the file-size limit of
//! the shell, which the command meets as an ordinary write failure.

mod common;

use std::fs;
use std::path::Path;

use common::{err, framesmith_with_file_limit, out, project};

const GROUP: &str = "film-write-stopped";
const EARLIER: &[u8] = b"an earlier film, kept until the new one is whole";
/// Four seconds of a moving box: the sound alone of the film is larger than the limit, and the log is not.
const COMPOSITION: &str = r##"<composition version="1" width="320" height="120" fps="30" duration="4" background="#ffffff">
  <box left="0" top="40" width="40" height="40" background="#ff0000">
    <tween duration="4" to-x="260"/>
  </box>
</composition>
"##;

fn names_in(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

#[test]
fn a_film_whose_write_stops_after_its_first_frames_is_fs_0036_with_exit_1_and_keeps_the_earlier_film() {
    let (folder, cwd) = project(GROUP, "stopped", COMPOSITION);
    let out_dir = folder.parent().unwrap().join("out");
    fs::create_dir_all(&out_dir).unwrap();
    let output = out_dir.join("film.mp4");
    fs::write(&output, EARLIER).unwrap();

    let rendered = framesmith_with_file_limit(&cwd, ["render", folder.to_str().unwrap(), "--output", output.to_str().unwrap()]);

    assert_eq!(rendered.status.code(), Some(1), "{}{}", out(&rendered), err(&rendered));
    // The run tells its progress on standard error before it fails (section 5.5): every line there is
    // a stage count or a stage with nothing to do.
    for line in err(&rendered).lines() {
        if line.ends_with("nothing to do") {
            line.strip_suffix(": nothing to do").expect(line);
            continue;
        }
        let (_, rest) = line.split_once(": ").expect(line);
        rest.strip_suffix(" frames").expect(line).split_once('/').expect(line);
    }
    let printed = out(&rendered);
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines.len(), 2, "{printed}");
    assert!(lines[0].starts_with("FS-0036: failure: the video could not be made: "), "{printed}");
    assert!(
        lines[0].ends_with(
            "Put right what the message names (the folder, the disk or the system's video encoder), then run the command again."
        ),
        "{printed}"
    );
    assert_eq!(lines[1], "Failed: the video could not be made");
    assert_eq!(fs::read(&output).unwrap(), EARLIER, "the earlier film keeps its bytes");
    assert_eq!(names_in(&out_dir), ["film.mp4"], "no staging file is left");
    let log = fs::read_to_string(cwd.join("logs").join("framesmith.log")).unwrap();
    assert!(log.contains("outcome=\"video not made\" exit=1 code=\"FS-0036\""), "{log}");
}
