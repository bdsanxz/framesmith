//! Sprint 21 T003: native 3D scenes end-to-end through the
//! `framesmith check` binary.
//!
//! Covers the user-visible half of `T003-render-3d-scenes-natively.md`:
//! a valid orbiting scene checks clean, an unbuildable scene refuses
//! naming the object and the cause, and a scene with no light rig
//! checks clean (the named default light renders, never darkness).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n21scene3d-{tag}-{}-{}",
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

fn run(args: &[&str], cwd: &Path) -> (i32, String) {
    let mut cmd = Command::new(bin());
    cmd.args(args).current_dir(cwd);
    let out = cmd.output().expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn proj(root: &Path, body: &str) -> PathBuf {
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("composition.kdl"), body).unwrap();
    folder
}

const OPEN: &str =
    "composition name=\"main\" width=640 height=480 duration=4 background=\"#000\" version=1 {\n";

fn scene(body: &str) -> String {
    format!("{OPEN}{body}\n}}\n")
}

#[test]
fn orbiting_scene_checks_clean() {
    let root = tmp("valid");
    let folder = proj(
        &root,
        &scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\" size=\"2,2,2\" color=\"#c02020\"\n    camera mode=\"orbit\" distance=6 azimuth=10 elevation=15 {\n        key at=0 azimuth=0\n        key at=4 azimuth=90\n    }\n    light name=\"sun\" kind=\"directional\" direction=\"-0.3,-1,-0.4\"\n}\n",
        ),
    );
    let (code, out) = run(&["check", folder.to_str().unwrap()], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("is valid"), "{out}");
    assert!(out.contains("1 clip(s)"), "{out}");
}

#[test]
fn unbuildable_scene_refuses_naming_object_and_cause() {
    let root = tmp("bad");
    let folder = proj(
        &root,
        &scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"wheel\" kind=\"torus\"\n}\n",
        ),
    );
    let (code, out) = run(&["check", folder.to_str().unwrap()], &root);
    assert_ne!(code, 0, "{out}");
    assert!(out.contains("E_BAD_SCENE3D"), "{out}");
    assert!(out.contains("\"wheel\""), "{out}");
    assert!(out.contains("torus"), "{out}");
}

#[test]
fn missing_light_rig_checks_clean() {
    let root = tmp("norix");
    let folder = proj(
        &root,
        &scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\"\n}\n",
        ),
    );
    let (code, out) = run(&["check", folder.to_str().unwrap()], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("is valid"), "{out}");
}
