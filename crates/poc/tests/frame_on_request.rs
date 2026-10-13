//! Frames asked for in any order, through `poc seek`: each has the pixels of the same frame of
//! a render in order. These tests need `ffmpeg`, which makes
//! their clip; `poc` does not start it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A new, empty folder for one test, under the build's own temporary folder.
fn folder(name: &str) -> PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("frame-on-request").join(name);
    if folder.exists() {
        std::fs::remove_dir_all(&folder).expect("the old test folder is removed");
    }
    std::fs::create_dir_all(&folder).expect("the test folder is made");
    folder
}

/// Writes a clip of our own making: two seconds at 30 pictures a second, each picture one flat
/// grey that is brighter than the one before, kept all but exactly.
fn clip(folder: &Path) {
    let target = folder.join("clip.mp4");
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i"])
        .arg("color=c=black:s=64x32:r=30:d=2,geq=lum='16+3*N':cb=128:cr=128")
        .args([
            "-c:v",
            "libx264",
            "-qp",
            "1",
            "-profile:v",
            "high",
            "-pix_fmt",
            "yuv420p",
            "-color_range",
            "tv",
        ])
        .args(["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709"])
        .arg(&target)
        .status()
        .expect("ffmpeg runs");
    assert!(status.success(), "ffmpeg made the clip");
}

/// Two clips over a moving shape, so that a frame differs from every other by its footage and
/// by its motion.
const SCENE: &str = r##"<composition width="64" height="32" fps="30" duration="2">
  <video id="a" src="clip.mp4" left="0" top="0" width="64" height="32" start="0" duration="1"/>
  <video id="b" src="clip.mp4" left="0" top="0" width="64" height="32" start="1" duration="1" media-start="0.5"/>
  <rect id="dot" left="0" top="12" width="8" height="8" background="#ff0000">
    <tween at="0" duration="2" from-x="0" to-x="56"/>
  </rect>
</composition>"##;

fn poc(args: &[&str], folder: &Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_poc"))
        .args(args)
        .current_dir(folder)
        .output()
        .expect("poc runs");
    assert!(output.status.success(), "poc {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).expect("poc prints text")
}

/// The mark of each frame of a render in order.
fn marks_in_order(folder: &Path, backend: &str) -> HashMap<u64, String> {
    poc(
        &[
            "render",
            "composition.xml",
            "--backend",
            backend,
            "--no-encode",
            "--marks",
            "marks.txt",
        ],
        folder,
    );
    let text = std::fs::read_to_string(folder.join("marks.txt")).expect("the marks are written");
    text.lines()
        .map(|line| {
            let (frame, mark) = line.split_once(' ').expect("a number and a mark");
            (frame.parse().expect("a frame number"), mark.to_string())
        })
        .collect()
}

/// The frame and the mark of each line `seek` prints for a frame, in the order printed.
fn marks_on_request(folder: &Path, backend: &str, asked: &[u64]) -> Vec<(u64, String)> {
    let list: String = asked.iter().map(|frame| format!("{frame}\n")).collect();
    std::fs::write(folder.join("list.txt"), list).expect("the list is written");
    let printed = poc(&["seek", "composition.xml", "--list", "list.txt", "--backend", backend], folder);
    printed
        .lines()
        .filter(|line| line.starts_with("frame "))
        .map(|line| {
            let words: Vec<&str> = line.split_whitespace().collect();
            (words[1].parse().expect("a frame number"), words[words.len() - 1].to_string())
        })
        .collect()
}

/// Forwards, backwards, the same frame twice, a long jump, and both sides of the cut.
const ASKED: [u64; 14] = [40, 3, 3, 4, 2, 59, 0, 29, 30, 29, 31, 45, 44, 12];

fn frames_on_request_are_those_in_order(name: &str, backend: &str) {
    let folder = folder(name);
    clip(&folder);
    std::fs::write(folder.join("composition.xml"), SCENE).expect("the composition is written");
    let in_order = marks_in_order(&folder, backend);
    assert_eq!(in_order.len(), 60, "a mark for every frame");
    let distinct: std::collections::HashSet<&String> = in_order.values().collect();
    assert_eq!(distinct.len(), 60, "every frame of the scene differs from every other");
    let on_request = marks_on_request(&folder, backend, &ASKED);
    assert_eq!(on_request.len(), ASKED.len(), "a line for every frame asked for");
    for ((frame, mark), asked) in on_request.iter().zip(ASKED) {
        assert_eq!(*frame, asked, "frames are answered in the order asked");
        assert_eq!(
            mark, &in_order[frame],
            "frame {frame} on request has the pixels of frame {frame} in order"
        );
    }
}

#[test]
fn the_cpu_renderer_gives_any_frame_as_a_render_in_order_does() {
    frames_on_request_are_those_in_order("cpu", "cpu");
}

#[test]
fn the_gpu_renderer_gives_any_frame_as_a_render_in_order_does() {
    frames_on_request_are_those_in_order("gpu", "gpu");
}

#[test]
fn a_frame_the_composition_does_not_have_is_refused() {
    let folder = folder("refused");
    clip(&folder);
    std::fs::write(folder.join("composition.xml"), SCENE).expect("the composition is written");
    std::fs::write(folder.join("list.txt"), "5\n60\n").expect("the list is written");
    let output = Command::new(env!("CARGO_BIN_EXE_poc"))
        .args(["seek", "composition.xml", "--list", "list.txt", "--backend", "cpu"])
        .current_dir(&folder)
        .output()
        .expect("poc runs");
    assert!(!output.status.success(), "the list names frame 60 of a composition of 60 frames");
    let said = String::from_utf8_lossy(&output.stderr);
    assert!(
        said.contains("line 2") && said.contains("`60`") && said.contains("frames 0 to 59"),
        "the message names the line, the number and the frames there are: {said}"
    );
}
