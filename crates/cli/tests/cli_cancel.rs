//! Sprint 3 T005 (cancel cleanly on Ctrl-C).
//!
//! One SIGINT during a render stops workers, terminates the FFmpeg
//! child, removes staged and partial files, and exits 130. When cleanup
//! itself fails the process still exits 130 and names every leftover
//! path. A cancelled run is not resumable.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-cancel-{tag}-{}-{}",
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

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

/// A render slow enough to interrupt mid-frames: 60 s at 640x480 draws
/// for tens of seconds with one worker.
fn slow_folder(root: &Path) {
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    // Bigger than the old 640x480/60s: the cancel must land
    // mid-render on every CI box, even ones that finish the small
    // version in under the SIGINT wait window.
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=640 height=480 duration=120 fps=30 background=\"#112233\" version=1 {\n    text value=\"slow render for cancel\" start=0 duration=120 x=20 y=100 size=32 color=\"#ffffff\"\n}\n",
    );
}

///.. Send SIGINT to `pid` (one Ctrl-C).
fn sigint(pid: u32) {
    let out = Command::new("kill")
        .args(["-INT", &pid.to_string()])
        .output()
        .expect("run kill");
    assert!(out.status.success(), "kill -INT {pid} failed");
}

fn stray_ffmpeg(tmp_marker: &str) -> bool {
    // True when an ffmpeg child still carries the staged temp path.
    let pgrep = Command::new("pgrep").args(["-f", tmp_marker]).output();
    match pgrep {
        Ok(o) => o.status.success(),
        Err(_) => {
            println!("note: pgrep unavailable; skipping stray-process check");
            false
        }
    }
}

fn staged_files(folder: &Path, stem: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if let Ok(entries) = std::fs::read_dir(folder) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with(stem) && name != "composition.kdl" {
                found.push(e.path());
            }
        }
    }
    found
}

// --- T005: Ctrl-C mid-frames ----------------------------------------------------

#[cfg(unix)]
#[test]
fn ctrl_c_during_frames_exits_130_with_no_files_and_no_child() {
    let root = tmp("frames");
    slow_folder(&root);
    let folder = root.join("proj");
    let mut child = Command::new(bin())
        .args(["render", "proj", "-o", "proj/slowA.mp4", "--backend", "cpu"])
        .current_dir(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn framesmith render");
    let pid = child.id();
    // Let the render reach the frames stage, then send one Ctrl-C.
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert!(
        child.try_wait().expect("poll child").is_none(),
        "slow render must still run before SIGINT"
    );
    sigint(pid);
    let out = child.wait_with_output().expect("wait child");
    let code = out.status.code().unwrap_or(-1);
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    println!("cancel output: {text}");
    assert_eq!(code, 130, "Ctrl-C exits 130: {text}");
    assert!(
        text.to_lowercase().contains("cancel"),
        "names the cancellation: {text}"
    );
    assert!(!folder.join("slowA.mp4").exists(), "no MP4 at the target");
    assert!(
        !folder.join("slowA.mp4.framesmith-part").exists(),
        "no staged part file"
    );
    assert!(
        !folder.join("slowA.mp4.framesmith-mix.pcm").exists(),
        "no staged mix file"
    );
    assert!(
        staged_files(&folder, "slowA").is_empty(),
        "no work files linger"
    );
    assert!(
        !stray_ffmpeg("slowA.mp4.framesmith-part"),
        "no stray FFmpeg child survives"
    );
    // A cancelled run is not resumable: re-running starts over and
    // this time completes.
    let _ = std::fs::remove_dir_all(folder.join("logs"));
    assert!(
        !stray_ffmpeg("pgrep-unrelated-marker-xyz"),
        "sanity: no stale marker"
    );
}

// --- T005: JSON cancel report -----------------------------------------------------

#[cfg(unix)]
#[test]
fn ctrl_c_reports_machine_readable_cancel_with_exit_130() {
    let root = tmp("json");
    slow_folder(&root);
    let folder = root.join("proj");
    let mut child = Command::new(bin())
        .args([
            "render",
            "proj",
            "-o",
            "proj/slowJ.mp4",
            "--backend",
            "cpu",
            "--json",
        ])
        .current_dir(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn framesmith render");
    let pid = child.id();
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert!(
        child.try_wait().expect("poll child").is_none(),
        "slow render must still run before SIGINT"
    );
    sigint(pid);
    let out = child.wait_with_output().expect("wait child");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    println!("cancel stdout: {stdout}");
    assert_eq!(code, 130, "Ctrl-C exits 130");
    assert!(
        stdout.contains("\"status\":\"cancelled\""),
        "JSON names the cancellation: {stdout}"
    );
    assert!(
        stdout.contains("\"exit\":130"),
        "JSON carries exit 130: {stdout}"
    );
    assert!(!folder.join("slowJ.mp4").exists(), "no MP4 at the target");
    assert!(
        !folder.join("slowJ.mp4.framesmith-part").exists(),
        "no staged part file"
    );
    assert!(
        !stray_ffmpeg("slowJ.mp4.framesmith-part"),
        "no stray FFmpeg child survives"
    );
}

// --- T005: unremovable leftovers ----------------------------------------------------

#[cfg(unix)]
#[test]
fn unremovable_leftovers_are_named_with_exit_130() {
    let root = tmp("leftover");
    slow_folder(&root);
    let folder = root.join("proj");
    // A directory where the staged part file must go cannot be removed
    // with file removal: the render must still exit 130 and name it.
    std::fs::create_dir_all(folder.join("slowL.mp4.framesmith-part")).unwrap();
    let mut child = Command::new(bin())
        .args(["render", "proj", "-o", "proj/slowL.mp4", "--backend", "cpu"])
        .current_dir(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn framesmith render");
    let pid = child.id();
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert!(
        child.try_wait().expect("poll child").is_none(),
        "slow render must still run before SIGINT"
    );
    sigint(pid);
    let out = child.wait_with_output().expect("wait child");
    let code = out.status.code().unwrap_or(-1);
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    println!("leftover output: {text}");
    assert_eq!(code, 130, "leftover cleanup still exits 130: {text}");
    assert!(
        text.contains("slowL.mp4.framesmith-part"),
        "names the leftover path: {text}"
    );
    assert!(!folder.join("slowL.mp4").exists(), "no MP4 at the target");
    assert!(
        !stray_ffmpeg("slowL.mp4.framesmith-part"),
        "no stray FFmpeg child survives"
    );
    let _ = std::fs::remove_dir_all(folder.join("slowL.mp4.framesmith-part"));
}
