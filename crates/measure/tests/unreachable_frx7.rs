//! A pin Cargo cannot fetch: the build fails with a message that names the revision and the
//! remote, and the gates add one line that says the same. See `docs/dev/frx7.md`, section 7.
//!
//! The remote is made unreachable on this machine by telling git to read it from a place that does
//! not exist, so the test needs no network and fails the same way with the network up or down.

mod common;

use common::REMOTE;
use std::path::{Path, PathBuf};
use std::process::Command;

const MISSING: &str = "0123456789abcdef0123456789abcdef01234567";

/// A workspace of its own whose one crate takes `base` of frx7 at `MISSING`, with Cargo's home and
/// target folders inside it so that nothing outside is read or written.
fn workspace(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("unreachable-frx7").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("crates/user/src")).expect("the folders are made");
    std::fs::create_dir_all(dir.join("home")).expect("the folder is made");
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[workspace]\nresolver = \"2\"\nmembers = [\"crates/user\"]\n\n[workspace.dependencies]\nbase = {{ git = \"{REMOTE}\", rev = \"{MISSING}\" }}\n"
        ),
    )
    .expect("the manifest is written");
    std::fs::write(
        dir.join("crates/user/Cargo.toml"),
        "[package]\nname = \"user\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nbase = { workspace = true }\n",
    )
    .expect("the manifest is written");
    std::fs::write(dir.join("crates/user/src/lib.rs"), "").expect("the source is written");
    dir
}

/// Runs `program` in `dir` with the remote read from a place that does not exist.
fn run_cut_off(dir: &Path, program: &str, args: &[&str]) -> (i32, String) {
    let nowhere = dir.join("no-such-remote");
    let output = Command::new(program)
        .args(args)
        .current_dir(dir)
        .env("CARGO_HOME", dir.join("home"))
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .env("CARGO_NET_GIT_FETCH_WITH_CLI", "true")
        .env("CARGO_NET_RETRY", "0")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", format!("url.file://{}.insteadOf", nowhere.display()))
        .env("GIT_CONFIG_VALUE_0", REMOTE)
        .output()
        .expect("the program is run");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.code().unwrap_or(-1), text)
}

#[test]
fn a_build_that_cannot_fetch_the_pin_fails_naming_the_revision_and_the_remote() {
    let dir = workspace("build");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let (code, text) = run_cut_off(&dir, &cargo, &["build"]);
    assert_ne!(code, 0, "{text}");
    assert!(text.contains(&format!("unable to update {REMOTE}?rev={MISSING}")), "{text}");
}

#[test]
fn a_gate_that_cannot_fetch_the_pin_says_which_revision_and_which_remote() {
    let dir = workspace("gate");
    std::fs::create_dir_all(dir.join("scripts")).expect("the folder is made");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/gates.sh"),
        dir.join("scripts/gates.sh"),
    )
    .expect("the script is copied");
    let (code, text) = run_cut_off(&dir, "bash", &["scripts/gates.sh", "--gate", "lint"]);
    assert_eq!(code, 1, "{text}");
    assert!(
        text.contains("FAIL static analysis")
            && text.contains(&format!(
                "frx7 revision {MISSING} could not be fetched from {REMOTE}; see docs/dev/frx7.md, section 7"
            )),
        "{text}"
    );
}
