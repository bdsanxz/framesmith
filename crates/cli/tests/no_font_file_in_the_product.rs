//! No font file is in the product: the crates and the specification folder hold none. The test fonts of the tests are made by
//! us and are kept in `fixtures/fonts` and in the fixtures that copy them (decision A8; `DECISIONS.md`, D7).

use std::fs;
use std::path::{Path, PathBuf};

const FONT_EXTENSIONS: [&str; 5] = ["ttf", "otf", "ttc", "woff", "woff2"];

/// Every file under `folder`, the folders first, in a stable order.
fn files_under(folder: &Path, found: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(folder).unwrap().flatten().map(|entry| entry.path()).collect();
    entries.sort();
    for entry in entries {
        if entry.is_dir() {
            files_under(&entry, found);
        } else {
            found.push(entry);
        }
    }
}

#[test]
fn no_font_file_is_in_the_crates_or_the_specification() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for folder in ["crates", "docs"] {
        files_under(&root.join(folder), &mut files);
    }
    let fonts: Vec<&PathBuf> = files
        .iter()
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| FONT_EXTENSIONS.contains(&extension))
        })
        .collect();
    assert!(fonts.is_empty(), "font files in the product: {fonts:?}");
    assert!(!files.is_empty(), "the folders hold files to look at");
}
