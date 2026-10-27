//! `framesmith init` on a folder that exists and holds at least one entry writes nothing, says that the folder is not empty
//! and exits with 1; the entries of the folder keep their bytes (`docs/spec/init-command.md`, sections 2 and 5).

mod common;

use std::fs;

use common::{err, framesmith, out, scratch};

#[test]
fn init_of_a_folder_with_files_writes_nothing_and_keeps_their_bytes() {
    let cwd = scratch("init", "not-empty");
    let folder = cwd.join("demo");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("notes.txt"), b"keep these notes exactly\n").unwrap();
    fs::write(folder.join(".hidden"), [0_u8, 1, 2, 255]).unwrap();
    let output = framesmith(&cwd, ["init", "demo"], &[]);
    assert_eq!(output.status.code(), Some(1), "{}{}", out(&output), err(&output));
    assert_eq!(out(&output), "", "nothing is printed on standard output");
    assert!(
        err(&output).starts_with("framesmith: FS-0082: failure: `demo` is not empty, so nothing was written. "),
        "{}",
        err(&output)
    );
    assert_eq!(fs::read(folder.join("notes.txt")).unwrap(), b"keep these notes exactly\n");
    assert_eq!(fs::read(folder.join(".hidden")).unwrap(), [0_u8, 1, 2, 255]);
    let mut names: Vec<String> = fs::read_dir(&folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, [".hidden", "notes.txt"], "no file of the starter is written");
}
