//! Sprint 27 T002 end-to-end: version reporting and the update check.
//!
//! * `version [--json]` names the running version, the build, and the
//!   FFmpeg registry pin; `--version` stays the unchanged single line
//!   proof scripts parse (`framesmith 0.1.0`).
//! * `update check` / `upgrade` read the release metadata the package
//!   builder publishes (latest-release JSON). With a `file://`
//!   metadata fixture (the same `curl` fetch path production uses for
//!   HTTPS) the binary reports current with no banner, names a newer
//!   release with where to get it, and reports unavailable (never
//!   current) when the channel is unreachable — renders and validation
//!   are unaffected by the failed check.
//! * `update apply` without `--yes` refuses (updates never install
//!   without consent). The successful apply path is covered by unit
//!   tests for the atomic replace and by staged manual verification:
//!   applying here would replace the test-harness binary itself, and
//!   a real newer binary needs a bumped rebuild.
//! * A checksum mismatch refuses naming the mismatch and leaves the
//!   running binary working (REC-002).
//!
//! Tests that fetch skip (bootstrap-style early return) when `curl`
//! is absent from PATH.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n27t002-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_framesmith"))
}

fn on_path(name: &str) -> Option<PathBuf> {
    for dir in std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default()
    {
        for cand in [name.to_string(), format!("{name}.exe")] {
            let p = dir.join(&cand);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Run the binary, returning (exit, stdout, stderr).
fn run(args: &[&str], cwd: &Path, envs: &[(&str, &str)]) -> (i32, String, String) {
    let out = Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .envs(envs.iter().copied())
        .output()
        .expect("run framesmith binary");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn running_version() -> String {
    let (code, stdout, stderr) = run(&["--version"], &tmp("ver"), &[]);
    assert_eq!(code, 0, "framesmith --version exit: {stderr}");
    let line = stdout.trim().to_string();
    assert!(line.starts_with("framesmith "), "single-line --version: {line:?}");
    assert!(!line.contains('\n'), "single-line --version: {line:?}");
    line["framesmith ".len()..].to_string()
}

/// (slug, archive extension) for this host, or None when unsupported.
fn host_slug() -> Option<(&'static str, &'static str)> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some(("macos-arm64", "tar.gz")),
        ("macos", "x86_64") => Some(("macos-x64", "tar.gz")),
        ("linux", "x86_64") => Some(("linux-x64", "tar.gz")),
        ("windows", "x86_64") => Some(("windows-x64", "zip")),
        _ => None,
    }
}

/// Write a latest-release JSON fixture pointing at `file://` asset
/// URLs under `dir`. `archive_bytes` backs the platform archive (or
/// the bare name for check-only fixtures); `archive_hex` is the hex
/// the fixture's `checksums.txt` claims.
fn write_release(
    dir: &Path,
    tag: &str,
    archive_bytes: &[u8],
    archive_hex: &str,
) -> PathBuf {
    let (slug, ext) = host_slug().expect("supported test host");
    let version = tag.strip_prefix('v').unwrap_or(tag);
    let asset_name = format!("framesmith-{version}-{slug}.{ext}");
    let archive_path = dir.join(&asset_name);
    std::fs::write(&archive_path, archive_bytes).unwrap();
    let sums = dir.join("checksums.txt");
    std::fs::write(&sums, format!("{archive_hex}  {asset_name}\n")).unwrap();
    let file_url = |p: &Path| format!("file://{}", p.display());
    // Serialized (not hand-formatted): bare Windows paths inside a
    // JSON string are invalid escapes.
    let body = serde_json::to_string(&serde_json::json!({
        "tag_name": tag,
        "html_url": format!("https://example.invalid/releases/{tag}"),
        "assets": [
            {
                "name": asset_name,
                "browser_download_url": file_url(&archive_path),
            },
            {
                "name": "checksums.txt",
                "browser_download_url": file_url(&sums),
            },
        ]
    }))
    .unwrap();
    let meta = dir.join("release.json");
    std::fs::write(&meta, body).unwrap();
    meta
}

#[test]
fn version_names_version_build_and_pin() {
    let dir = tmp("version");
    let (code, stdout, stderr) = run(&["version"], &dir, &[]);
    assert_eq!(code, 0, "version exit: {stderr}");
    let version = running_version();
    let mut lines = stdout.lines();
    assert_eq!(lines.next(), Some(format!("framesmith {version}").as_str()));
    let build = lines.next().expect("version prints a build line");
    assert!(build.starts_with("build: "), "build line: {build:?}");
    assert!(build.contains(std::env::consts::OS), "build naming OS: {build:?}");
    assert!(build.contains(std::env::consts::ARCH), "build naming arch: {build:?}");
    let pin = lines.next().expect("version prints a pin line");
    assert!(pin.contains("9.0.2"), "pin line: {pin:?}");
    assert!(pin.contains("ffmpeg-registry"), "pin line: {pin:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn version_json_carries_build_and_pin() {
    let dir = tmp("version-json");
    let (code, stdout, stderr) = run(&["version", "--json"], &dir, &[]);
    assert_eq!(code, 0, "version --json exit: {stderr}");
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("version --json parses");
    assert_eq!(v["tool"], "framesmith");
    assert_eq!(v["version"], running_version());
    assert_eq!(v["ffmpeg_registry_pin"], "9.0.2");
    assert_eq!(v["os"], std::env::consts::OS);
    assert_eq!(v["arch"], std::env::consts::ARCH);
    // The unknown flag must agree with the build string, one way or
    // the other — never a guessed commit.
    let build = v["build"].as_str().expect("build is a string");
    assert_eq!(v["unknown_build"], build.contains("unknown build"));
    // Usage misuse exits 2.
    let (code, _, _) = run(&["version", "--bogus"], &dir, &[]);
    assert_eq!(code, 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn update_check_reports_current_and_newer() {
    if on_path("curl").is_none() {
        return; // curl absent: fetch-path tests skip (bootstrap precedent).
    }
    let Some((slug, _ext)) = host_slug() else {
        // Unsupported host must refuse naming the supported set.
        let dir = tmp("unsupported");
        let (code, _, stderr) = run(
            &["update", "check", "--api-url", "file:///nonexistent-n27-probe"],
            &dir,
            &[],
        );
        assert_eq!(code, 1, "unsupported/unreachable check fails: {stderr}");
        let _ = std::fs::remove_dir_all(&dir);
        return;
    };
    let _ = slug;
    let dir = tmp("check");
    let version = running_version();
    // Current: metadata names the running version -> current, no banner.
    let meta = write_release(&dir, &format!("v{version}"), b"irrelevant", "00");
    let (code, stdout, stderr) = run(
        &["update", "check", "--api-url", &format!("file://{}", meta.display())],
        &dir,
        &[],
    );
    assert_eq!(code, 0, "current check exit: {stderr}");
    assert!(stdout.contains("current"), "current report: {stdout:?}");
    assert!(!stdout.contains("update-available"), "no banner: {stdout:?}");

    // Newer: v99.0.0 names the release with where to get it.
    let dir2 = tmp("check-newer");
    let meta = write_release(&dir2, "v99.0.0", b"irrelevant", "00");
    let url = format!("file://{}", meta.display());
    let (code, stdout, stderr) = run(&["update", "check", "--api-url", &url], &dir2, &[]);
    assert_eq!(code, 0, "newer check exit: {stderr}");
    assert!(stdout.contains("update-available"), "newer report: {stdout:?}");
    assert!(stdout.contains("99.0.0"), "newer names the release: {stdout:?}");
    assert!(stdout.contains("example.invalid/releases/v99.0.0"), "where to get it: {stdout:?}");
    // JSON form agrees.
    let (code, stdout, stderr) = run(
        &["update", "check", "--json", "--api-url", &url],
        &dir2,
        &[],
    );
    assert_eq!(code, 0, "newer check --json exit: {stderr}");
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("check --json parses");
    assert_eq!(v["status"], "update-available");
    assert_eq!(v["latest"], "99.0.0");
    assert_eq!(v["current"], version);
    assert!(v["asset"].as_str().unwrap().contains("framesmith-99.0.0-"));
    // The `upgrade` parity spelling checks too.
    let (code, stdout, stderr) = run(&["upgrade", "--api-url", &url], &dir2, &[]);
    assert_eq!(code, 0, "upgrade check exit: {stderr}");
    assert!(stdout.contains("update-available"), "upgrade report: {stdout:?}");

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&dir2);
}

#[test]
fn update_check_unreachable_is_unavailable_never_current() {
    if on_path("curl").is_none() {
        return;
    }
    let dir = tmp("unreachable");
    // Missing file: unavailable, exit 1, and never the word "current"
    // as a status (renders proceed normally, unaffected by the check).
    let (code, stdout, stderr) = run(
        &["update", "check", "--api-url", "file:///nonexistent-n27-probe"],
        &dir,
        &[],
    );
    assert_eq!(code, 1, "unreachable check fails: {stdout} {stderr}");
    let all = format!("{stdout}{stderr}");
    assert!(all.contains("unavailable"), "names unavailable: {all:?}");
    assert!(!all.contains(" current ("), "never claims current: {all:?}");
    // JSON form agrees: status unavailable, no latest.
    let (code, stdout, _) = run(
        &["update", "check", "--json", "--api-url", "file:///nonexistent-n27-probe"],
        &dir,
        &[],
    );
    assert_eq!(code, 1);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("unavailable --json parses");
    assert_eq!(v["status"], "unavailable");
    assert_eq!(v["latest"], serde_json::Value::Null);
    // Offline switch forces unavailable without fetching.
    let (code, _, stderr) = run(
        &["update", "check", "--api-url", "file:///nonexistent-n27-probe"],
        &dir,
        &[("FRAMESMITH_OFFLINE", "1")],
    );
    assert_eq!(code, 1);
    assert!(stderr.contains("unavailable"), "offline check: {stderr:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn update_apply_refuses_without_consent() {
    if on_path("curl").is_none() {
        return;
    }
    let dir = tmp("consent");
    let meta = write_release(&dir, "v99.0.0", b"irrelevant", "00");
    let url = format!("file://{}", meta.display());
    // `update apply` without --yes refuses naming the consent flag.
    let (code, _, stderr) = run(
        &["update", "apply", "--api-url", &url],
        &dir,
        &[],
    );
    assert_eq!(code, 2, "apply without --yes refuses: {stderr}");
    assert!(stderr.contains("--yes"), "names the consent flag: {stderr:?}");
    // Bare `upgrade` is a check (no --yes): it reports, never installs.
    let (code, _, stderr) = run(&["upgrade", "--api-url", &url], &dir, &[]);
    assert_eq!(code, 0, "bare upgrade checks: {stderr}");
    // Unknown subverb exits 2.
    let (code, _, _) = run(&["update", "frobnicate"], &dir, &[]);
    assert_eq!(code, 2);
    // The binary still works: refusal changed nothing.
    let (code, stdout, _) = run(&["--version"], &dir, &[]);
    assert_eq!(code, 0);
    assert!(stdout.starts_with("framesmith "));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn update_apply_checksum_mismatch_leaves_binary_working() {
    if on_path("curl").is_none() {
        return;
    }
    let dir = tmp("mismatch");
    // The checksums.txt claims zeros; the asset is garbage bytes, so
    // verification must refuse before any replace (REC-002).
    let meta = write_release(&dir, "v99.0.0", b"garbage-bytes", "0000000000000000000000000000000000000000000000000000000000000000");
    let url = format!("file://{}", meta.display());
    let (code, _, stderr) = run(
        &["update", "apply", "--yes", "--api-url", &url],
        &dir,
        &[],
    );
    assert_eq!(code, 1, "mismatch refuses: {stderr}");
    assert!(stderr.contains("checksum mismatch"), "names the mismatch: {stderr:?}");
    // Old binary still works, and validation still works: the failed
    // update changed nothing.
    let (code, stdout, _) = run(&["--version"], &dir, &[]);
    assert_eq!(code, 0);
    assert!(stdout.starts_with("framesmith "));
    let proj = dir.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(
        proj.join("composition.kdl"),
        "composition name=\"t\" width=640 height=480 duration=2 fps=30 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=2\n}\n",
    )
    .unwrap();
    let (code, stdout, stderr) = run(&["check", proj.to_str().unwrap()], &dir, &[]);
    assert_eq!(code, 0, "check still valid: {stdout} {stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}
