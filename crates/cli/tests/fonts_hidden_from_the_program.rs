//! A composition that draws with a font file of the project gives the same frame marks when the machine's installed fonts
//! are hidden from the program: a run under a macOS sandbox profile that denies reading the font folders of the machine
//! prints the marks of a run without it, frame by frame (`docs/spec/text.md`, sections 2.1 and 10;
//! `docs/spec/render-command.md`, section 7.1). A control shows that the profile hides the fonts.
//!
//! A second sandbox profile cannot be applied inside the profile of the core suite (`docs/dev/gates.md`, section 10:
//! `sandbox_apply: Operation not permitted`), so there, where `scripts/gates/core-suite.sh` sets `FRAMESMITH_CORE_TARGET`,
//! these tests are not run and print why; the tests gate runs them in full on every run of the gates.

#![cfg(target_os = "macos")]

mod common;
mod frame_marks;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::{err, out, scratch};
use frame_marks::{fixture_project, mark_lines, marked_render};

/// The folders the machine's installed fonts are kept in.
fn font_folders() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap();
    vec![
        PathBuf::from("/System/Library/Fonts"),
        PathBuf::from("/Library/Fonts"),
        PathBuf::from("/System/Library/AssetsV2"),
        home.join("Library/Fonts"),
    ]
}

/// A sandbox profile that allows everything but reading the font folders, written in `folder`.
fn profile(folder: &Path) -> PathBuf {
    let mut text = String::from("(version 1)\n(allow default)\n");
    for hidden in font_folders() {
        text.push_str(&format!("(deny file-read* (subpath \"{}\"))\n", hidden.display()));
    }
    let path = folder.join("fonts-hidden.sb");
    fs::write(&path, text).unwrap();
    path
}

/// A font file of the machine, the first in `/System/Library/Fonts` by name.
fn a_machine_font() -> PathBuf {
    let mut files: Vec<PathBuf> = fs::read_dir("/System/Library/Fonts")
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    files.into_iter().next().unwrap()
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
fn the_profile_hides_the_machine_s_font_files_from_a_program() {
    if inside_the_core_profile() {
        return;
    }
    let folder = scratch("fonts-hidden", "control");
    let profile = profile(&folder);
    let font = a_machine_font();
    let open = Command::new("/bin/cat").arg(&font).output().unwrap();
    assert!(open.status.success(), "the font is read without the profile");
    let hidden = Command::new("/usr/bin/sandbox-exec")
        .arg("-f")
        .arg(&profile)
        .arg("/bin/cat")
        .arg(&font)
        .output()
        .unwrap();
    assert!(!hidden.status.success(), "the font is not read under the profile");
    assert!(String::from_utf8_lossy(&hidden.stderr).contains("Operation not permitted"));
}

#[test]
fn a_render_with_the_machine_s_fonts_hidden_prints_the_marks_of_a_render_without() {
    if inside_the_core_profile() {
        return;
    }
    let (normal, normal_cwd) = fixture_project("fonts-hidden", "normal", "declared-font-file");
    let (hidden, hidden_cwd) = fixture_project("fonts-hidden", "hidden", "declared-font-file");
    let profile = profile(hidden.parent().unwrap());

    let shown = common::framesmith(&normal_cwd, marked_render(&normal), &[]);
    assert_eq!(shown.status.code(), Some(0), "{}{}", out(&shown), err(&shown));
    let hidden_run = Command::new("/usr/bin/sandbox-exec")
        .arg("-f")
        .arg(&profile)
        .arg(env!("CARGO_BIN_EXE_framesmith"))
        .args(marked_render(&hidden))
        .current_dir(&hidden_cwd)
        .env_remove("FRAMESMITH_LOG")
        .output()
        .unwrap();
    assert_eq!(hidden_run.status.code(), Some(0), "{}{}", out(&hidden_run), err(&hidden_run));

    let shown_marks = mark_lines(&shown);
    assert_eq!(shown_marks.len(), 10, "one mark for each of the ten frames");
    assert_eq!(mark_lines(&hidden_run), shown_marks, "the same mark for every frame");
}
