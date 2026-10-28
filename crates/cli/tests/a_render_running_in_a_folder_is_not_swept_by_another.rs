//! Two renders into one folder at once: the sweep of the second render leaves the staging folder of the first, which
//! still runs, as it is, and the first completes with its film at its output path (`docs/spec/render-command.md`,
//! section 6).

mod common;

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use common::{err, framesmith, out, project};

const GROUP: &str = "running-render-not-swept";
/// Twelve seconds of a moving box: long enough for a second render to start while it runs.
const LONG: &str = r##"<composition version="1" width="320" height="120" fps="30" duration="12" background="#ffffff">
  <box left="0" top="40" width="40" height="40" background="#ff0000">
    <tween duration="12" to-x="260"/>
  </box>
</composition>
"##;
const SHORT: &str = r##"<composition version="1" width="320" height="120" fps="30" duration="2" background="#ffffff">
  <box left="0" top="40" width="40" height="40" background="#ff0000">
    <tween duration="2" to-x="260"/>
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
fn a_second_render_into_the_folder_leaves_the_running_render_its_staging_folder_and_its_film() {
    let (folder, cwd) = project(GROUP, "first", LONG);
    let out_dir = folder.parent().unwrap().join("out");
    fs::create_dir_all(&out_dir).unwrap();
    let first_output = out_dir.join("first.mp4");

    let mut first = Command::new(env!("CARGO_BIN_EXE_framesmith"))
        .args(["render", folder.to_str().unwrap(), "--output", first_output.to_str().unwrap()])
        .current_dir(&cwd)
        .env_remove("FRAMESMITH_LOG")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let staging = out_dir.join(format!(".framesmith-{}", first.id())).join("first.part.mp4");
    let started = Instant::now();
    loop {
        assert!(
            first.try_wait().unwrap().is_none(),
            "the first render ended before the second one started"
        );
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "the first render wrote no frames in a minute"
        );
        if fs::metadata(&staging).is_ok_and(|meta| meta.len() > 4096) {
            break;
        }
        sleep(Duration::from_millis(20));
    }

    let (short, _) = project(GROUP, "second", SHORT);
    let second_output = out_dir.join("second.mp4");
    let second = framesmith(
        &cwd,
        ["render", short.to_str().unwrap(), "--output", second_output.to_str().unwrap()],
        &[],
    );
    assert_eq!(second.status.code(), Some(0), "{}{}", out(&second), err(&second));
    assert!(first.try_wait().unwrap().is_none(), "the first render ended during the second one");
    assert!(
        staging.is_file(),
        "the second render swept the staging file of the running first render"
    );

    let finished = first.wait().unwrap();
    assert!(finished.success(), "the first render did not complete: {finished}");
    assert_eq!(
        names_in(&out_dir),
        ["first.mp4", "second.mp4"],
        "a staging folder or part file is left"
    );
}
