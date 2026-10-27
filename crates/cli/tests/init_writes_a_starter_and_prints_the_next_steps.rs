//! `framesmith init <folder>` in a folder with no such folder creates it with the starter's three files, prints the folder
//! and the two commands that come next, and exits with 0 (`docs/spec/init-command.md`, sections 2 to 4).

mod common;

use std::fs;

use common::{err, framesmith, out, scratch};

#[test]
fn init_of_a_new_folder_writes_the_starter_and_prints_the_next_steps() {
    let cwd = scratch("init", "new-folder");
    let output = framesmith(&cwd, ["init", "demo"], &[]);
    assert_eq!(output.status.code(), Some(0), "{}{}", out(&output), err(&output));
    assert_eq!(
        out(&output),
        "Created the starter project in demo.\nNext, run:\n  framesmith check demo\n  framesmith render demo\n"
    );
    assert_eq!(err(&output), "");
    let folder = cwd.join("demo");
    for file in ["composition.xml", "framesmith.yaml", "audio/tone.wav"] {
        assert!(folder.join(file).is_file(), "{file} is written");
    }
    let composition = fs::read_to_string(folder.join("composition.xml")).unwrap();
    assert!(composition.contains("<composition version=\"1\""), "the root carries version 1");
}
