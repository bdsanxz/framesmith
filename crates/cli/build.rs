//! Capture the build commit for the `framesmith version` command
//! (Sprint 27 T002 / CAP-002).
//!
//! The commit is baked in at compile time from `git rev-parse
//! --short HEAD` run in the package directory (which sits inside the
//! Framesmith checkout). When git is unavailable or the sources are
//! vendored without a `.git` dir, the commit is empty and the version
//! command reports the build as unknown rather than guessing. The
//! build never fails on a missing git binary.

use std::process::Command;

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(&manifest)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_default();
    println!("cargo:rustc-env=FRAMEWORK_GIT_COMMIT={commit}");
    let head = std::path::Path::new(&manifest)
        .join("..")
        .join("..")
        .join(".git")
        .join("HEAD");
    println!("cargo:rerun-if-changed={}", head.display());
}
