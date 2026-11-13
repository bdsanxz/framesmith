//! T001 acceptance: third-party notices carry the GPL ffmpeg-registry
//! note, installers verify checksums before install and refuse a
//! mismatched package, and the package set covers every required OS.

use std::path::PathBuf;

fn framesmith_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn read(path: &str) -> String {
    std::fs::read_to_string(framesmith_dir().join(path))
        .unwrap_or_else(|e| panic!("read {path}: {e}"))
}

#[test]
fn third_party_notices_include_gpl_registry_note() {
    let body = read("dist/THIRD-PARTY-NOTICES.md");
    assert!(body.contains("GPL"), "notices must name GPL");
    assert!(
        body.contains("ffmpeg-registry"),
        "notices must name ffmpeg-registry"
    );
    assert!(body.contains("libx264"), "notices must explain GPL cause");
    assert!(body.contains("9.0.2"), "notices must pin registry version");
}

#[test]
fn unix_installer_verifies_before_install() {
    let sh = read("dist/install.sh");
    assert!(
        sh.contains("checksum mismatch"),
        "install.sh must refuse on mismatch"
    );
    assert!(
        sh.contains("checksums.txt"),
        "install.sh must read release metadata checksums.txt"
    );
    assert!(
        sh.contains("supported platforms"),
        "install.sh must name the supported set on refusal"
    );
    assert!(
        sh.contains("macos-arm64"),
        "install.sh must list the required OS slugs"
    );
    // Installer installs via one command and renders immediately via bootstrap.
    assert!(sh.contains("bootstrap") || sh.contains("FFmpeg"),
        "install.sh must mention the bootstrap path");
}
#[test]
fn windows_installer_verifies_before_install() {
    let ps = read("dist/install.ps1");
    assert!(ps.contains("checksum mismatch"), "install.ps1 must refuse on mismatch");
    assert!(ps.contains("checksums.txt"), "install.ps1 must read checksums.txt");
    assert!(ps.contains("framesmith.exe"), "install.ps1 must install the .exe");
    // Windows path joins use Join-Path, never string concat for PATH.
    assert!(ps.contains("Join-Path"), "install.ps1 must use Join-Path");
}

#[test]
fn build_script_gates_tagged_and_complete() {
    let sh = read("tools/build-package.sh");
    assert!(
        sh.contains("packages build only from tagged releases"),
        "build must gate on tags"
    );
    assert!(
        sh.contains("incomplete OS set"),
        "build must refuse an incomplete set naming the missing OS"
    );
    assert!(sh.contains("macos-arm64"), "script must know all four slugs");
    assert!(sh.contains("windows-x64"), "script must know windows slug");
    assert!(sh.contains("linux-x64"), "script must know linux slug");
}

#[test]
fn release_workflow_builds_all_four_and_publishes_checksums() {
    let yml = read(".github/workflows/release.yml");
    assert!(yml.contains("v*"), "release triggers on tags");
    for slug in ["macos-arm64", "macos-x64", "windows-x64", "linux-x64"] {
        assert!(yml.contains(slug), "release must build {slug}");
    }
    assert!(yml.contains("checksums.txt"), "release must publish checksums");
    assert!(
        yml.contains("incomplete OS set") || yml.contains("verify-set"),
        "release must check the complete set"
    );
}
