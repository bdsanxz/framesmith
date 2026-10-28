//! A render that shows footage starts no other program: its clips are made with frx7's own writer, and the command renders
//! them under a macOS sandbox profile that refuses to start any program but `framesmith` itself, and the film it writes
//! shows the footage (`docs/spec/render-command.md`, section 4.8). A control shows that the profile refuses a program.
//!
//! A second sandbox profile cannot be applied inside the profile of the core suite (`docs/dev/gates.md`, section 10), so
//! there, where `scripts/gates/core-suite.sh` sets `FRAMESMITH_CORE_TARGET`, these tests are not run and print why; the
//! tests gate runs them in full on every run of the gates. The core suite's own profile refuses every program start too.

#![cfg(target_os = "macos")]

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::{err, out, project};
use measure::Frame;

const TEAL: [u8; 3] = [20, 128, 120];

/// A sandbox profile that allows everything but starting a program other than `framesmith`, written in `folder`.
fn profile(folder: &Path) -> PathBuf {
    let text = format!(
        "(version 1)\n(allow default)\n(deny process-exec*)\n(allow process-exec (literal \"{}\"))\n",
        fs::canonicalize(env!("CARGO_BIN_EXE_framesmith")).unwrap().display()
    );
    let path = folder.join("no-program.sb");
    fs::write(&path, text).unwrap();
    path
}

fn sandboxed(profile: &Path, program: &Path, args: &[&std::ffi::OsStr], cwd: &Path) -> Output {
    Command::new("/usr/bin/sandbox-exec")
        .arg("-f")
        .arg(profile)
        .arg(program)
        .args(args)
        .current_dir(cwd)
        .env_remove("FRAMESMITH_LOG")
        .output()
        .unwrap()
}

/// Whether the test runs inside the profile of the core suite, where no second profile can be applied.
fn inside_the_core_profile() -> bool {
    if std::env::var_os("FRAMESMITH_CORE_TARGET").is_some() {
        println!("not run inside the core suite: a second sandbox profile cannot be applied there; the tests gate runs it");
        return true;
    }
    false
}

#[test]
fn the_profile_refuses_to_start_another_program() {
    if inside_the_core_profile() {
        return;
    }
    let (folder, cwd) = project("no-program", "control", "");
    let profile = profile(folder.parent().unwrap());
    let refused = sandboxed(&profile, Path::new("/usr/bin/true"), &[], &cwd);
    assert!(!refused.status.success(), "/usr/bin/true is not started under the profile");
    // framesmith itself starts: it answers a word that is not a command with its own report.
    let allowed = sandboxed(
        &profile,
        Path::new(env!("CARGO_BIN_EXE_framesmith")),
        &["no-such-command".as_ref()],
        &cwd,
    );
    assert!(
        err(&allowed).contains("FS-0017"),
        "framesmith is started under the profile: {}{}",
        out(&allowed),
        err(&allowed)
    );
}

#[test]
fn a_render_of_footage_under_the_profile_writes_a_film_that_shows_it() {
    if inside_the_core_profile() {
        return;
    }
    let composition = "<composition version=\"1\" width=\"96\" height=\"54\" fps=\"10\" duration=\"1\" background=\"#102040\">\n<video src=\"clip.mp4\" left=\"8\" top=\"4\" width=\"40\" height=\"24\"/>\n</composition>\n";
    let (folder, cwd) = project("no-program", "render", composition);
    let picture = Frame {
        width: 40,
        height: 24,
        rgb: TEAL.iter().copied().cycle().take(40 * 24 * 3).collect(),
    };
    measure::write_video(&folder.join("clip.mp4"), (10, 1), &vec![picture; 10]).unwrap();
    let profile = profile(folder.parent().unwrap());
    let output = folder.parent().unwrap().join("film.mp4");
    let run = sandboxed(
        &profile,
        Path::new(env!("CARGO_BIN_EXE_framesmith")),
        &["render".as_ref(), folder.as_os_str(), "--output".as_ref(), output.as_os_str()],
        &cwd,
    );
    assert_eq!(run.status.code(), Some(0), "{}{}", out(&run), err(&run));
    let mut reader = measure::open_pictures(&output).unwrap();
    let first = reader.next_frame().unwrap().unwrap();
    let pixel = first.pixel(28, 16);
    assert!(
        (0..3).all(|i| pixel[i].abs_diff(TEAL[i]) <= 12),
        "the film shows {pixel:?} where the footage is"
    );
    let canvas = first.pixel(2, 2);
    assert!(
        (0..3).all(|i| canvas[i].abs_diff([0x10, 0x20, 0x40][i]) <= 12),
        "the canvas is {canvas:?}"
    );
}
