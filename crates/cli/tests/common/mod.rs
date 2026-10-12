//! Shared by the command tests: scratch projects, running the binary, and
//! reading back what it made.
#![allow(dead_code)]

pub mod live;
pub mod sprint1;

use measure::{Facts, Frame, Ran};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub const BIN: &str = env!("CARGO_BIN_EXE_framesmith");

/// A fresh, empty folder for one test.
pub fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli").join(test);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn write(dir: &Path, name: &str, content: impl AsRef<[u8]>) {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

/// A page with one composition. `attrs` are the composition's own
/// attributes after its name; `rest` follows the composition element.
pub fn page(attrs: &str, style: &str, body: &str, rest: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>Test</title>\n<style>\n{style}\n</style>\n</head>\n<body>\n\
<div data-fs-composition=\"main\" {attrs}>\n{body}\n</div>\n{rest}\n</body>\n</html>\n"
    )
}

/// A motion block holding the given tweens, written as JSON objects.
pub fn motion(tweens: &str) -> String {
    format!("<script type=\"application/framesmith-motion+json\">\n[{tweens}]\n</script>")
}

/// A project folder named `folder` inside the test's scratch folder.
pub fn project(test: &str, folder: &str, index: &str) -> PathBuf {
    let dir = scratch(test).join(folder);
    write(&dir, "index.html", index);
    dir
}

/// A copy of a fixture from the repository, inside the test's scratch folder.
pub fn fixture(test: &str, name: &str) -> PathBuf {
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name);
    let to = scratch(test).join(Path::new(name).file_name().unwrap());
    copy_tree(&from, &to);
    to
}

pub fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap_or_else(|e| panic!("{}: {e}", from.display())) {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            if entry.file_name() != "out" {
                copy_tree(&entry.path(), &target);
            }
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

pub fn run_in(dir: &Path, args: &[&str], env: &[(&str, &OsStr)]) -> Ran {
    measure::run(Path::new(BIN), args, dir, env)
}

pub fn framesmith(args: &[&str]) -> Ran {
    run_in(Path::new(env!("CARGO_TARGET_TMPDIR")), args, &[])
}

pub fn ffmpeg() -> PathBuf {
    measure::tool("ffmpeg").expect("ffmpeg is needed for this test")
}

pub fn ffprobe() -> PathBuf {
    measure::tool("ffprobe").expect("ffprobe is needed for this test")
}

pub fn facts(video: &Path) -> Facts {
    measure::facts(&ffprobe(), video).unwrap()
}

pub fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// Renders the project and returns what was printed with the exact
/// pictures the browser painted, in frame order.
pub fn render_frames(project: &Path, args: &[&str], env: &[(&str, &OsStr)]) -> (Ran, Vec<Frame>) {
    let frames = project
        .parent()
        .unwrap()
        .join(format!("{}-frames", project.file_name().unwrap().to_string_lossy()));
    let _ = std::fs::remove_dir_all(&frames);
    let mut all = vec!["render", text(project)];
    all.extend_from_slice(args);
    let mut with: Vec<(&str, &OsStr)> = vec![("FRAMESMITH_DEBUG_FRAMES", frames.as_os_str())];
    with.extend_from_slice(env);
    let ran = run_in(project.parent().unwrap(), &all, &with);
    let pictures = if ran.code == 0 {
        measure::frames_from_pngs(&ffmpeg(), &frames).unwrap()
    } else {
        Vec::new()
    };
    (ran, pictures)
}

/// Every file under a folder with its size and a hash of its content.
pub fn tree(dir: &Path) -> BTreeMap<PathBuf, (u64, u64)> {
    fn walk(root: &Path, dir: &Path, found: &mut BTreeMap<PathBuf, (u64, u64)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, found);
            } else {
                let bytes = std::fs::read(&path).unwrap();
                let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| {
                    (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
                });
                found.insert(path.strip_prefix(root).unwrap().to_path_buf(), (bytes.len() as u64, hash));
            }
        }
    }
    let mut found = BTreeMap::new();
    walk(dir, dir, &mut found);
    found
}

/// A stand-in for a program: it notes that it was started in `marker`,
/// then becomes the real program.
pub fn spy(dir: &Path, name: &str, real: &Path) -> (PathBuf, PathBuf) {
    let marker = dir.join(format!("{name}.started"));
    let script = dir.join(name);
    let body = format!(
        "#!/bin/sh\necho started >> '{}'\nexec '{}' \"$@\"\n",
        marker.display(),
        real.display()
    );
    std::fs::write(&script, body).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    (script, marker)
}

/// The browser the tests use.
pub fn browser() -> PathBuf {
    browser::find(&browser::Places::from_process()).expect("a Chromium-based browser is needed for this test")
}

/// The spec files of the sprint.
pub fn spec(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/spec").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The JSON document a `--json` run printed on standard output.
pub fn document(ran: &Ran) -> serde_json::Value {
    serde_json::from_str(ran.stdout.trim()).unwrap_or_else(|e| panic!("standard output is not one JSON document ({e}): {:?}", ran.stdout))
}

/// Whether anything of a run's browser is left: a process that carries
/// the run's profile folder on its command line, or the folder itself.
pub fn browser_left(run_id: &str) -> bool {
    let profile = format!("framesmith-{run_id}");
    let found = measure::run(Path::new("/usr/bin/pgrep"), &["-f", &profile], Path::new("/"), &[]);
    found.code == 0 || std::env::temp_dir().join(profile).exists()
}
