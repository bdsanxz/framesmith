//! A render killed outright in its frames stage leaves its hidden staging folder beside the output, and the output path
//! keeps its bytes. The next render into the same folder removes that folder, logs that it did, and completes with no
//! staging file and no part file of the killed run (`docs/spec/render-command.md`, section 6).

mod common;

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use common::{err, framesmith, out, project};

const GROUP: &str = "killed-render-leftovers";
const EARLIER: &[u8] = b"an earlier film, kept until the new one is whole";
/// A minute of a moving box: a render of it runs far longer than the test waits, so the kill lands in the frames stage.
const LONG: &str = r##"<composition version="1" width="320" height="120" fps="30" duration="60" background="#ffffff">
  <box left="0" top="40" width="40" height="40" background="#ff0000">
    <tween duration="60" to-x="260"/>
  </box>
</composition>
"##;
const SHORT: &str = r##"<composition version="1" width="320" height="120" fps="30" duration="4" background="#ffffff">
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
fn a_render_killed_in_its_frames_stage_leaves_nothing_the_next_render_does_not_sweep() {
    let (folder, cwd) = project(GROUP, "killed", LONG);
    let out_dir = folder.parent().unwrap().join("out");
    fs::create_dir_all(&out_dir).unwrap();
    let output = out_dir.join("film.mp4");
    fs::write(&output, EARLIER).unwrap();

    let mut killed = Command::new(env!("CARGO_BIN_EXE_framesmith"))
        .args(["render", folder.to_str().unwrap(), "--output", output.to_str().unwrap()])
        .current_dir(&cwd)
        .env_remove("FRAMESMITH_LOG")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pid = killed.id();
    let staging = out_dir.join(format!(".framesmith-{pid}")).join("film.part.mp4");
    let started = Instant::now();
    loop {
        assert!(killed.try_wait().unwrap().is_none(), "the render ended before it was killed");
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "no frames were written in two minutes"
        );
        if fs::metadata(&staging).is_ok_and(|meta| meta.len() > 4096) {
            break;
        }
        sleep(Duration::from_millis(20));
    }
    killed.kill().unwrap();
    killed.wait().unwrap();

    assert_eq!(
        fs::read(&output).unwrap(),
        EARLIER,
        "the output path changed while the killed render was stopped"
    );
    assert!(staging.is_file(), "the killed render left no staging file to sweep");

    let (short, _) = project(GROUP, "next", SHORT);
    let rendered = framesmith(&cwd, ["render", short.to_str().unwrap(), "--output", output.to_str().unwrap()], &[]);
    assert_eq!(rendered.status.code(), Some(0), "{}{}", out(&rendered), err(&rendered));
    assert_ne!(
        fs::read(&output).unwrap(),
        EARLIER,
        "the next render did not replace the earlier film"
    );
    assert_eq!(
        names_in(&out_dir),
        ["film.mp4"],
        "a staging folder or part file of the killed run is left"
    );

    let log = fs::read_to_string(cwd.join("logs").join("framesmith.log")).unwrap();
    let swept = log
        .lines()
        .any(|line| line.contains("removed a staging folder that no render holds") && line.contains(&format!("pid={pid}")));
    assert!(swept, "the sweep of the killed run is not in the log:\n{log}");
}
