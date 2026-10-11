//! Synthetic videos made with ffmpeg, so no test needs the reference or the product's binary.
#![allow(dead_code)]

use parity::tools::Tools;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const BACKGROUND: [u8; 3] = [0x10, 0x18, 0x20];
pub const CLIP: [u8; 3] = [0xff, 0x5a, 0x36];
pub const MOVER: [u8; 3] = [0x2e, 0xcc, 0x71];
pub const BOX: [u8; 3] = [0x33, 0x66, 0xff];

pub fn tools() -> Tools {
    Tools::find().expect("ffmpeg and ffprobe must be installed to run these tests")
}

/// A folder of this test's own, empty at the start.
pub fn folder(name: &str) -> PathBuf {
    let dir = std::path::absolute(Path::new(env!("CARGO_TARGET_TMPDIR")).join("parity").join(name)).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[derive(Clone, Copy, PartialEq)]
pub enum Audio {
    None,
    AfterVideo,
    BeforeVideo,
}

/// What a synthetic video holds: a 320x180 picture with a clip (a square that shows on
/// frames `clip.0 .. clip.1`), a mover (a square that moves in a straight line) and a still box.
#[derive(Clone)]
pub struct Spec {
    pub fps: u32,
    pub frames: u32,
    pub clip: (u32, u32),
    /// First and last frame of the move, and where the mover starts and ends, in pixels.
    pub mover: (u32, u32, i32, i32),
    /// The still box moves sideways by this many pixels on frames in the range.
    pub box_shift: Option<((u32, u32), i32)>,
    pub audio: Audio,
    /// Writes other colour tags than the usual ones.
    pub other_tags: bool,
}

impl Default for Spec {
    fn default() -> Spec {
        Spec {
            fps: 30,
            frames: 60,
            clip: (20, 40),
            mover: (10, 50, 20, 180),
            box_shift: None,
            audio: Audio::None,
            other_tags: false,
        }
    }
}

fn hex(c: [u8; 3]) -> String {
    format!("0x{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// Makes the video with ffmpeg.
pub fn make(path: &Path, spec: &Spec) {
    let tools = tools();
    let seconds = f64::from(spec.frames) / f64::from(spec.fps);
    let source = |colour: [u8; 3], size: &str| format!("color=c={}:s={size}:r={}:d={seconds}", hex(colour), spec.fps);
    let (s, e, x0, x1) = spec.mover;
    let (shift_range, shift) = spec.box_shift.unwrap_or(((1, 0), 0));
    let graph = format!(
        "[0][1]overlay=format=rgb:x=240:y=20:enable='between(n,{},{})'[a];\
         [a][2]overlay=format=rgb:x='round({x0}+({x1}-{x0})*clip((n-{s})/({e}-{s}),0,1))':y=120[b];\
         [b][3]overlay=format=rgb:x='60+if(between(n,{},{}),{shift},0)':y=10[v]",
        spec.clip.0,
        spec.clip.1 - 1,
        shift_range.0,
        shift_range.1
    );
    let mut command = Command::new(&tools.ffmpeg);
    command.args(["-v", "error", "-y"]);
    for (colour, size) in [(BACKGROUND, "320x180"), (CLIP, "40x40"), (MOVER, "24x24"), (BOX, "140x90")] {
        command.args(["-f", "lavfi", "-i"]).arg(source(colour, size));
    }
    if spec.audio != Audio::None {
        command
            .args(["-f", "lavfi", "-i"])
            .arg(format!("sine=frequency=440:duration={seconds}"));
    }
    command.args(["-filter_complex", &graph]);
    match spec.audio {
        Audio::None => command.args(["-map", "[v]"]),
        Audio::AfterVideo => command.args(["-map", "[v]", "-map", "4:a", "-c:a", "aac"]),
        Audio::BeforeVideo => command.args(["-map", "4:a", "-map", "[v]", "-c:a", "aac"]),
    };
    command.args([
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-crf",
        "10",
        "-preset",
        "veryfast",
        "-r",
        &spec.fps.to_string(),
    ]);
    if spec.other_tags {
        command.args(["-colorspace", "bt470bg", "-color_primaries", "bt470bg", "-color_trc", "smpte170m"]);
    } else {
        command.args(["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709"]);
    }
    let output = command.arg(path).output().expect("ffmpeg could not be started");
    assert!(
        output.status.success(),
        "ffmpeg failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Makes two videos side by side in the folder: `a.mp4` and `b.mp4`.
pub fn pair(dir: &Path, a: &Spec, b: &Spec) -> (PathBuf, PathBuf) {
    let (pa, pb) = (dir.join("a.mp4"), dir.join("b.mp4"));
    make(&pa, a);
    make(&pb, b);
    (pa, pb)
}
