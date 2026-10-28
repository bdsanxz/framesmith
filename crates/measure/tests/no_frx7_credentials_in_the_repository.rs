//! Framesmith fetches frx7 from its public remote with no credentials (`docs/dev/frx7.md`, section 2), so no tracked file
//! names a secret of the hosted runner, the command that once installed it, or the host name it was written under. The
//! patterns are put together at run time, so that this file does not match itself.

use std::path::Path;
use std::process::Command;

/// The words of the old frx7 secret that must appear in no tracked file, case not counted.
fn forbidden() -> Vec<String> {
    vec![
        format!("{}{}", "deploy", "[-_ .]key"),
        format!("{}{}", "DEPLOY", "_KEY"),
        format!("{}{}", "frx7", "-github"),
    ]
}

/// The repository root: the folder above `crates/measure`.
fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the repository root")
}

/// The lines of the tracked files that match `pattern` (case not counted), as `git grep` prints them.
fn matches(pattern: &str) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root())
        .args(["grep", "-n", "-i", "-E", pattern])
        .output()
        .expect("git grep runs");
    // git grep exits 1 when nothing matches, which is the answer wanted here.
    assert!(
        matches!(output.status.code(), Some(0) | Some(1)),
        "git grep failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn no_tracked_file_names_a_frx7_secret() {
    for pattern in forbidden() {
        let found = matches(&pattern);
        assert!(found.is_empty(), "a tracked file names a frx7 secret ({pattern}):\n{found}");
    }
}
