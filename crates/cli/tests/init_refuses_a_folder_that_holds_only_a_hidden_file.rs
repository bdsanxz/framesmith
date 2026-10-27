//! `framesmith init` on a folder whose only entry is a hidden file refuses it as it refuses any other folder that is not
//! empty: it writes nothing, says that the folder is not empty, exits with 2, and keeps the hidden file's bytes
//! (`docs/spec/init-command.md`, sections 2 and 5).

mod common;

use std::fs;

use common::{err, framesmith, out, scratch};

#[test]
fn init_of_a_folder_with_only_a_hidden_file_writes_nothing_and_keeps_its_bytes() {
    let cwd = scratch("init", "only-hidden");
    let folder = cwd.join("demo");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join(".hidden"), [0_u8, 1, 2, 255]).unwrap();
    let output = framesmith(&cwd, ["init", "demo"], &[]);
    assert_eq!(output.status.code(), Some(2), "{}{}", out(&output), err(&output));
    assert_eq!(out(&output), "", "nothing is printed on standard output");
    assert!(
        err(&output).starts_with("framesmith: FS-0082: failure: `demo` is not empty, so nothing was written. "),
        "{}",
        err(&output)
    );
    assert_eq!(fs::read(folder.join(".hidden")).unwrap(), [0_u8, 1, 2, 255]);
    let names: Vec<String> = fs::read_dir(&folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, [".hidden"], "no file of the starter is written");
}
