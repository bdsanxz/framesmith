//! When the reference is reported as absent: its command is not installed in the folder, or the
//! runtime its command needs is on no folder of the search path (`blackbox::absence`). Looking
//! starts nothing, so these tests start no program.

mod common;

use common::folder;
use parity::blackbox;
use std::ffi::OsString;
use std::path::Path;

fn install(under: &Path) {
    let bin = under.join("node_modules").join(".bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join(blackbox::COMMAND), "").unwrap();
}

fn search_path(folders: &[&Path]) -> OsString {
    std::env::join_paths(folders).unwrap()
}

#[test]
fn a_reference_that_is_not_installed_in_the_folder_is_absent_and_the_folder_is_named() {
    let dir = folder("reference_absence_not_installed");
    let runtime = dir.join("runtime");
    std::fs::create_dir_all(&runtime).unwrap();
    std::fs::write(runtime.join("node"), "").unwrap();
    let why = blackbox::absence(&dir.join("nowhere"), &search_path(&[&runtime])).unwrap_err();
    assert!(why.starts_with("the reference is not installed under "), "{why}");
    assert!(why.contains("nowhere"), "{why}");
}

#[test]
fn an_installed_reference_whose_runtime_is_on_no_folder_of_the_search_path_is_absent_and_the_runtime_is_named() {
    let dir = folder("reference_absence_no_runtime");
    install(&dir.join("reference"));
    let empty = dir.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let why = blackbox::absence(&dir.join("reference"), &search_path(&[&empty])).unwrap_err();
    assert!(
        why.contains("its runtime, node, was not found in any folder of the search path"),
        "{why}"
    );
}

#[test]
fn an_installed_reference_with_its_runtime_on_the_search_path_is_not_absent() {
    let dir = folder("reference_absence_present");
    install(&dir.join("reference"));
    let runtime = dir.join("runtime");
    std::fs::create_dir_all(&runtime).unwrap();
    std::fs::write(runtime.join("node"), "").unwrap();
    assert_eq!(blackbox::absence(&dir.join("reference"), &search_path(&[&runtime])), Ok(()));
}
