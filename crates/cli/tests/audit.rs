//! T054: the closing audit's check of the support matrix against the fixtures.

mod common;

use common::*;
use std::collections::BTreeSet;
use std::path::Path;

#[test]
fn t054_c3_every_entry_of_the_support_matrix_has_a_fixture_that_check_lists_and_render_refuses() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let matrix = std::fs::read_to_string(root.join("docs/spec/support-matrix.md")).unwrap();
    // The entries: the rows of the matrix whose first cell is an "unsupported" code.
    let entries: BTreeSet<&str> = matrix
        .lines()
        .filter_map(|line| line.strip_prefix("| FS-U"))
        .map(|rest| &rest[..rest.find(' ').unwrap()])
        .collect();
    assert!(entries.len() >= 21, "{entries:?}");

    let mut folders: Vec<String> = std::fs::read_dir(root.join("fixtures/unsupported"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    folders.sort();
    for number in entries {
        let code = format!("FS-U{number}");
        let prefix = format!("fs-u{number}-");
        let own: Vec<&String> = folders.iter().filter(|folder| folder.starts_with(&prefix)).collect();
        assert!(!own.is_empty(), "{code} has no fixture");
        for folder in own {
            let dir = fixture(&format!("t054_c3_{folder}"), &format!("unsupported/{folder}"));
            let check = framesmith(&["check", text(&dir)]);
            assert_eq!(check.code, 2, "{folder}: {}", check.all());
            let listed = check.stdout.lines().any(|line| line.starts_with(&format!("unsupported {code} ")));
            assert!(listed, "{folder}: {code} is not listed in\n{}", check.stdout);
            let render = framesmith(&["render", text(&dir)]);
            assert_eq!(render.code, 2, "{folder}: {}", render.all());
            assert!(
                render.stderr.contains(&format!("unsupported {code} ")),
                "{folder}: {}",
                render.stderr
            );
            assert!(!dir.join("out").exists(), "{folder}: a refused render left an output folder");
        }
    }
}
