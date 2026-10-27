//! `framesmith init` on a folder that exists and holds nothing writes the starter into it, as it does into a new folder
//! (`docs/spec/init-command.md`, section 2).

mod common;

use std::fs;

use common::{err, framesmith, out, scratch};

#[test]
fn init_of_an_empty_folder_writes_the_starter_into_it() {
    let cwd = scratch("init", "empty-folder");
    let folder = cwd.join("demo");
    fs::create_dir(&folder).unwrap();
    let output = framesmith(&cwd, ["init", "demo"], &[]);
    assert_eq!(output.status.code(), Some(0), "{}{}", out(&output), err(&output));
    assert_eq!(
        out(&output),
        "Created the starter project in demo.\nNext, run:\n  framesmith check demo\n  framesmith render demo\n"
    );
    for file in ["composition.xml", "framesmith.yaml", "audio/tone.wav"] {
        assert!(folder.join(file).is_file(), "{file} is written into the empty folder");
    }
}
