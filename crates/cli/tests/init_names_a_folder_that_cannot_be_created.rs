//! `framesmith init` where the folder cannot be created names the path and the reason the system gives, without its error
//! number, and exits with 1 (`docs/spec/init-command.md`, sections 2 and 5).

mod common;

use std::fs;

use common::{err, framesmith, out, scratch};

#[test]
fn init_where_the_parent_folder_is_missing_names_the_path_and_the_system_reason() {
    let cwd = scratch("init", "missing-parent");
    let output = framesmith(&cwd, ["init", "missing/demo"], &[]);
    assert_eq!(output.status.code(), Some(1), "{}{}", out(&output), err(&output));
    assert_eq!(out(&output), "");
    assert_eq!(
        err(&output).lines().next().unwrap(),
        "framesmith: FS-0083: failure: `missing/demo` cannot be created: No such file or directory. Put right the cause the system names (the folder's parent, a file of that name, or the folder's permissions), then run the command again."
    );
    assert!(!cwd.join("missing").exists(), "no folder is made where the path cannot be created");
}

#[test]
fn init_where_a_file_stands_in_the_path_names_the_path_and_the_system_reason() {
    let cwd = scratch("init", "file-in-the-way");
    fs::write(cwd.join("blocker"), b"a file, not a folder\n").unwrap();
    for folder in ["blocker/demo", "blocker"] {
        let output = framesmith(&cwd, ["init", folder], &[]);
        assert_eq!(output.status.code(), Some(1), "{folder}: {}{}", out(&output), err(&output));
        assert_eq!(out(&output), "", "{folder}");
        assert!(
            err(&output).starts_with(&format!(
                "framesmith: FS-0083: failure: `{folder}` cannot be created: Not a directory. "
            )),
            "{folder}: {}",
            err(&output)
        );
        assert!(
            !err(&output).contains("os error"),
            "the error number is not printed: {}",
            err(&output)
        );
    }
    assert_eq!(
        fs::read(cwd.join("blocker")).unwrap(),
        b"a file, not a folder\n",
        "the file is not changed"
    );
}
