//! T009/T010 end-to-end: pinned self-download, checksum refusal, and
//! disabled/offline refusal. The child runs with a scrubbed `PATH`
//! (`/usr/bin:/bin`: `curl`, `unzip` and `sh` present, no FFmpeg) and a
//! `file://` pinned source served from a fixture zip.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-boot-{}-{}-{}",
        tag,
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

const SCRUBBED_PATH: &str = "/usr/bin:/bin";

fn assert_no_host_ffmpeg() {
    for entry in SCRUBBED_PATH.split(':') {
        for tool in ["ffmpeg", "ffprobe"] {
            let p = Path::new(entry).join(tool);
            assert!(
                !p.is_file(),
                "test assumption broken: {tool} exists at {}",
                p.display()
            );
        }
    }
}

/// Minimal fake `ffmpeg`: answers `-version`/`-encoders` like a suitable
/// build and "encodes" by writing a marker to the last argument.
const FAKE_FFMPEG: &str = "#!/bin/sh\n\
if [ \"$1\" = \"-version\" ]; then\n\
\x20 echo \"ffmpeg version 9.0-test Copyright (c) test\"; exit 0\n\
fi\n\
if [ \"$1\" = \"-hide_banner\" ]; then\n\
\x20 echo \"Encoders:\"\n\
\x20 echo \" V..... libx264   libx264 H.264\"\n\
\x20 echo \" A..... aac        AAC\"\n\
\x20 exit 0\n\
fi\n\
out=\"\"; for a in \"$@\"; do out=\"$a\"; done\n\
printf 'FAKE-MP4-MARKER-0123456789' > \"$out\"\n\
exit 0\n";

/// Build `{script, zip, sha256}` fixture via the host python3 (stdlib
/// zipfile/hashlib only). Returns the SHA256 of the script entry.
fn make_fixture(dir: &Path) -> (PathBuf, String) {
    let script = dir.join("ffmpeg");
    std::fs::write(&script, FAKE_FFMPEG).unwrap();
    let zip = dir.join("ffmpeg-test.zip");
    let helper = dir.join("mkzip.py");
    std::fs::write(
        &helper,
        "import hashlib, sys, zipfile\n\
         script = open(sys.argv[1], 'rb').read()\n\
         zi = zipfile.ZipInfo('ffmpeg')\n\
         zi.external_attr = (0o100755 << 16)\n\
         with zipfile.ZipFile(sys.argv[2], 'w') as z:\n\
         \x20   z.writestr(zi, script)\n\
         print(hashlib.sha256(script).hexdigest())\n",
    )
    .unwrap();
    let out = Command::new("python3")
        .arg(&helper)
        .arg(&script)
        .arg(&zip)
        .output()
        .expect("python3 fixture builder");
    assert!(out.status.success(), "fixture build failed: {out:?}");
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert_eq!(sha.len(), 64, "{sha}");
    (zip, sha)
}

fn run(args: &[&str], cwd: &Path, extra_env: &[(&str, &str)]) -> (i32, String) {
    let mut cmd = Command::new(bin());
    cmd.args(args).current_dir(cwd);
    cmd.env("PATH", SCRUBBED_PATH);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

const VALID: &str = "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1\n}\n";

fn scaffold(root: &Path, name: &str) -> PathBuf {
    let folder = root.join(name);
    std::fs::create_dir_all(&folder).unwrap();
    write(&folder, "composition.kdl", VALID);
    folder
}

#[test]
fn render_self_downloads_verifies_and_renders() {
    assert_no_host_ffmpeg();
    let root = tmp("dl");
    let folder = scaffold(&root, "proj");
    let before = std::fs::read(folder.join("composition.kdl")).unwrap();
    let (zip, sha) = make_fixture(&root);
    let cache = root.join("cache");
    let url = format!("file://{}", zip.display());
    let (code, out) = run(
        &["render", "proj"],
        &root,
        &[
            ("FRAMESMITH_FFMPEG_URL", url.as_str()),
            ("FRAMESMITH_FFMPEG_SHA256", sha.as_str()),
            ("FRAMESMITH_FFMPEG_VERSION", "9.0-t"),
            ("FRAMESMITH_FFMPEG_ENTRY", "ffmpeg"),
            ("FRAMESMITH_FFMPEG_DIR", cache.to_string_lossy().as_ref()),
            ("FRAMESMITH_DOWNLOAD_TIMEOUT_SECS", "30"),
        ],
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("downloaded pinned FFmpeg"), "{out}");
    assert!(out.contains("SHA256 verified"), "{out}");
    let mp4 = folder.join("out.mp4");
    assert!(mp4.is_file(), "render must write an MP4: {out}");
    assert_eq!(
        std::fs::read(&mp4).unwrap(),
        b"FAKE-MP4-MARKER-0123456789",
        "the downloaded binary must be the one that executes"
    );
    // Terminal announcement is mirrored in the run log with all fields.
    let log = std::fs::read_to_string(folder.join("logs").join("framesmith.log"))
        .expect("run log must exist");
    assert!(log.contains("downloaded pinned FFmpeg"), "{log}");
    for field in ["ts=", "level=", "component=", "run="] {
        assert!(log.contains(field), "{log}");
    }
    // The composition itself is untouched by the download.
    assert_eq!(
        std::fs::read(folder.join("composition.kdl")).unwrap(),
        before
    );
    assert!(cache.join("9.0-t").join("bin").join("ffmpeg").is_file());
}

#[test]
fn render_reuses_cache_without_touching_network() {
    assert_no_host_ffmpeg();
    let root = tmp("reuse");
    let folder = scaffold(&root, "proj");
    let (zip, sha) = make_fixture(&root);
    let cache = root.join("cache");
    let url = format!("file://{}", zip.display());
    let cache_str = cache.to_string_lossy().to_string();
    let env: Vec<(&str, &str)> = vec![
        ("FRAMESMITH_FFMPEG_URL", url.as_str()),
        ("FRAMESMITH_FFMPEG_SHA256", sha.as_str()),
        ("FRAMESMITH_FFMPEG_VERSION", "9.0-t"),
        ("FRAMESMITH_FFMPEG_ENTRY", "ffmpeg"),
        ("FRAMESMITH_FFMPEG_DIR", cache_str.as_str()),
    ];
    assert_eq!(run(&["render", "proj"], &root, &env).0, 0);
    // Remove the "network" source: reuse must succeed from cache alone.
    std::fs::remove_file(&zip).unwrap();
    let (code, out) = run(&["render", "proj"], &root, &env);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("reusing cached"), "{out}");
    assert!(folder.join("out.mp4").is_file());
}

#[test]
fn render_checksum_mismatch_writes_no_mp4() {
    assert_no_host_ffmpeg();
    let root = tmp("badsum");
    let folder = scaffold(&root, "proj");
    let before = std::fs::read(folder.join("composition.kdl")).unwrap();
    let (zip, _sha) = make_fixture(&root);
    let cache = root.join("cache");
    let url = format!("file://{}", zip.display());
    let (code, out) = run(
        &["render", "proj"],
        &root,
        &[
            ("FRAMESMITH_FFMPEG_URL", url.as_str()),
            ("FRAMESMITH_FFMPEG_SHA256", &"0".repeat(64)),
            ("FRAMESMITH_FFMPEG_VERSION", "9.0-t"),
            ("FRAMESMITH_FFMPEG_ENTRY", "ffmpeg"),
            ("FRAMESMITH_FFMPEG_DIR", cache.to_string_lossy().as_ref()),
        ],
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("checksum mismatch"), "{out}");
    assert!(out.contains("No MP4 was written"), "{out}");
    assert!(!folder.join("out.mp4").exists());
    assert!(!cache.join("9.0-t").join("bin").join("ffmpeg").exists());
    assert_eq!(
        std::fs::read(folder.join("composition.kdl")).unwrap(),
        before
    );
}

#[test]
fn render_disabled_refuses_with_source_and_config() {
    assert_no_host_ffmpeg();
    let root = tmp("disabled");
    let folder = scaffold(&root, "proj");
    write(&folder, "framesmith.toml", "ffmpeg_auto_download = false\n");
    let cache = root.join("cache");
    let (code, out) = run(
        &["render", "proj"],
        &root,
        &[("FRAMESMITH_FFMPEG_DIR", cache.to_string_lossy().as_ref())],
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_NO_FFMPEG"), "{out}");
    assert!(out.contains("framesmith.toml"), "{out}");
    assert!(out.contains("ffmpeg_auto_download = false"), "{out}");
    // The expected source is named: the registry or the override.
    assert!(
        out.contains("ffmpeg-registry") || out.contains("pinned FFmpeg"),
        "{out}"
    );
    assert!(out.contains("No MP4 was written"), "{out}");
    assert!(!folder.join("out.mp4").exists());
    assert!(
        !cache.exists(),
        "a disabled download must not fetch anything"
    );
    // The refusal points at the catalog entry that explains it.
    assert!(out.contains("framesmith errors E_NO_FFMPEG"), "{out}");
}

#[test]
fn render_offline_refuses_with_manual_fix_and_check_still_works() {
    assert_no_host_ffmpeg();
    let root = tmp("offline");
    let folder = scaffold(&root, "proj");
    let cache = root.join("cache");
    let (code, out) = run(
        &["render", "proj"],
        &root,
        &[
            ("FRAMESMITH_OFFLINE", "1"),
            ("FRAMESMITH_FFMPEG_DIR", cache.to_string_lossy().as_ref()),
        ],
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("install FFmpeg"), "{out}");
    assert!(out.contains("No MP4 was written"), "{out}");
    assert!(!folder.join("out.mp4").exists());
    assert!(!cache.exists(), "offline refusal must not fetch anything");
    // Validation keeps working with no FFmpeg available.
    let (code, out) = run(&["check", "proj"], &root, &[("FRAMESMITH_OFFLINE", "1")]);
    assert_eq!(code, 0, "{out}");
}

#[test]
fn render_unreachable_source_fails_safe_without_hanging() {
    assert_no_host_ffmpeg();
    let root = tmp("unreachable");
    let folder = scaffold(&root, "proj");
    let cache = root.join("cache");
    let (code, out) = run(
        &["render", "proj"],
        &root,
        &[
            (
                "FRAMESMITH_FFMPEG_URL",
                "http://127.0.0.1:9/ffmpeg-test.zip",
            ),
            ("FRAMESMITH_FFMPEG_SHA256", &"1".repeat(64)),
            ("FRAMESMITH_FFMPEG_VERSION", "9.0-t"),
            ("FRAMESMITH_FFMPEG_ENTRY", "ffmpeg"),
            ("FRAMESMITH_FFMPEG_DIR", cache.to_string_lossy().as_ref()),
            ("FRAMESMITH_DOWNLOAD_TIMEOUT_SECS", "15"),
        ],
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("download failed"), "{out}");
    assert!(out.contains("No MP4 was written"), "{out}");
    assert!(!folder.join("out.mp4").exists());
}
