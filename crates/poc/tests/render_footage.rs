//! A render with footage, end to end through the `poc` command: every frame of the finished
//! file shows the picture of the source that its time names. These tests need `ffmpeg` and
//! `ffprobe`: they make the clip and look into the finished file. `poc` starts neither.

use std::path::{Path, PathBuf};
use std::process::Command;

/// How much brighter each picture of the test clip is than the one before, in luma, on average.
const STEP: u32 = 3;

/// A new, empty folder for one test, under the build's own temporary folder.
fn folder(name: &str) -> PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("render-footage").join(name);
    if folder.exists() {
        std::fs::remove_dir_all(&folder).expect("the old test folder is removed");
    }
    std::fs::create_dir_all(&folder).expect("the test folder is made");
    folder
}

fn ffmpeg(args: &[&str]) -> Vec<u8> {
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error"])
        .args(args)
        .output()
        .expect("ffmpeg runs");
    assert!(
        output.status.success(),
        "ffmpeg {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

/// Writes a clip of our own making: two seconds at 30 pictures a second, each picture one flat
/// grey that is brighter than the one before, kept all but exactly.
fn clip(folder: &Path, width: u32, height: u32) {
    let source = format!("color=c=black:s={width}x{height}:r=30:d=2,geq=lum='16+{STEP}*N':cb=128:cr=128");
    let target = folder.join("clip.mp4");
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        &source,
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
        "-colorspace",
        "bt709",
        "-color_primaries",
        "bt709",
        "-color_trc",
        "bt709",
        target.to_str().expect("the path is text"),
    ]);
}

/// The luma at the centre of each frame of a video file, as the file holds it.
fn lumas(file: &Path, (width, height): (usize, usize)) -> Vec<u8> {
    let planes = ffmpeg(&[
        "-i",
        file.to_str().expect("the path is text"),
        "-f",
        "rawvideo",
        "-pix_fmt",
        "yuv420p",
        "pipe:1",
    ]);
    planes
        .chunks_exact(width * height * 3 / 2)
        .map(|frame| frame[(height / 2) * width + width / 2])
        .collect()
}

/// Renders `composition.xml` of `folder` with the given options and returns the luma at the
/// centre of each frame of the finished file.
fn rendered(folder: &Path, size: (usize, usize), options: &[&str]) -> Vec<u8> {
    let output = folder.join("out.mp4");
    let status = Command::new(env!("CARGO_BIN_EXE_poc"))
        .arg("render")
        .arg(folder.join("composition.xml"))
        .args(["--backend", "cpu", "--output"])
        .arg(&output)
        .args(options)
        .output()
        .expect("poc runs");
    assert!(status.status.success(), "poc render: {}", String::from_utf8_lossy(&status.stderr));
    lumas(&output, size)
}

/// Checks that the frames show these pictures of the clip in `folder`: each frame has the
/// luma of its picture, to within one level. Neighbouring pictures are at least two apart.
fn shows(folder: &Path, found: &[u8], expected: &[usize]) {
    let clip = lumas(&folder.join("clip.mp4"), (64, 32));
    assert_eq!(clip.len(), 60, "the clip has two seconds of pictures");
    assert!(
        clip.windows(2).all(|pair| pair[1] >= pair[0] + 2),
        "every picture of the clip is brighter than the one before: {clip:?}"
    );
    assert_eq!(found.len(), expected.len(), "the number of frames: {found:?}");
    for (frame, (found, expected)) in found.iter().zip(expected).enumerate() {
        assert!(
            found.abs_diff(clip[*expected]) <= 1,
            "frame {frame} has luma {found}; picture {expected} of the clip has {}",
            clip[*expected]
        );
    }
}

const TWO_CLIPS: &str = r#"<composition width="64" height="32" fps="30" duration="2">
  <video id="a" src="clip.mp4" left="0" top="0" width="64" height="32" start="0" duration="1"/>
  <video id="b" src="clip.mp4" left="0" top="0" width="64" height="32" start="1" duration="1" media-start="0.5"/>
</composition>"#;

#[test]
fn every_frame_shows_the_picture_its_time_names() {
    let folder = folder("whole");
    clip(&folder, 64, 32);
    std::fs::write(folder.join("composition.xml"), TWO_CLIPS).expect("the composition is written");
    // The first clip shows pictures 0 to 29; the second starts at picture 15 of the source.
    let expected: Vec<usize> = (0..30).chain(15..45).collect();
    shows(&folder, &rendered(&folder, (64, 32), &[]), &expected);
}

#[test]
fn a_render_of_some_frames_starts_at_the_right_picture() {
    let folder = folder("part");
    clip(&folder, 64, 32);
    std::fs::write(folder.join("composition.xml"), TWO_CLIPS).expect("the composition is written");
    // The last two frames of the first clip and the first two of the second.
    shows(&folder, &rendered(&folder, (64, 32), &["--frames", "28:32"]), &[28, 29, 15, 16]);
    // A range that starts inside a clip.
    shows(&folder, &rendered(&folder, (64, 32), &["--frames", "40:43"]), &[25, 26, 27]);
}

#[test]
fn several_workers_give_the_frames_of_one() {
    let folder = folder("workers");
    clip(&folder, 64, 32);
    std::fs::write(folder.join("composition.xml"), TWO_CLIPS).expect("the composition is written");
    // Two seconds of film are two stretches of a second each; a third worker has nothing to do.
    let expected: Vec<usize> = (0..30).chain(15..45).collect();
    shows(&folder, &rendered(&folder, (64, 32), &["--workers", "2"]), &expected);
    shows(&folder, &rendered(&folder, (64, 32), &["--workers", "3"]), &expected);
    // A stretch that begins inside a clip.
    shows(
        &folder,
        &rendered(&folder, (64, 32), &["--workers", "2", "--frames", "0:50"]),
        &expected[..50],
    );
    let left: Vec<String> = std::fs::read_dir(&folder)
        .expect("the folder is read")
        .map(|entry| entry.expect("an entry").file_name().to_string_lossy().to_string())
        .filter(|name| name.contains("part"))
        .collect();
    assert!(left.is_empty(), "the part files are removed: {left:?}");
}

#[test]
fn a_video_drawn_at_an_odd_size_shows_the_same_pictures() {
    let folder = folder("odd");
    clip(&folder, 64, 32);
    let composition = r#"<composition width="64" height="32" fps="30" duration="1">
  <video src="clip.mp4" left="0" top="0" width="63" height="31" media-start="0.2"/>
</composition>"#;
    std::fs::write(folder.join("composition.xml"), composition).expect("the composition is written");
    shows(&folder, &rendered(&folder, (64, 32), &["--frames", "0:4"]), &[6, 7, 8, 9]);
}

#[test]
fn the_finished_file_says_what_it_holds() {
    let folder = folder("tags");
    clip(&folder, 64, 32);
    std::fs::write(folder.join("composition.xml"), TWO_CLIPS).expect("the composition is written");
    rendered(&folder, (64, 32), &["--frames", "0:3"]);
    let probed = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0"])
        .args([
            "-show_entries",
            "stream=codec_name,profile,pix_fmt,color_range,color_space,color_primaries,color_transfer",
        ])
        .args(["-of", "default=nw=1"])
        .arg(folder.join("out.mp4"))
        .output()
        .expect("ffprobe runs");
    let text = String::from_utf8_lossy(&probed.stdout);
    for line in [
        "codec_name=h264",
        "profile=High",
        "pix_fmt=yuv420p",
        "color_range=tv",
        "color_space=bt709",
        "color_primaries=bt709",
        "color_transfer=bt709",
    ] {
        assert!(text.lines().any(|found| found == line), "the file lacks `{line}`:\n{text}");
    }
}

/// With the GPU renderer the footage goes to the card as planes and the frames come back as
/// planes. This test needs a graphics adapter.
#[cfg(feature = "gpu")]
#[test]
fn the_gpu_renderer_shows_the_same_pictures() {
    let folder = folder("gpu");
    clip(&folder, 64, 32);
    std::fs::write(folder.join("composition.xml"), TWO_CLIPS).expect("the composition is written");
    let expected: Vec<usize> = (0..30).chain(15..45).collect();
    // A later `--backend` counts.
    shows(&folder, &rendered(&folder, (64, 32), &["--backend", "gpu"]), &expected);
    shows(
        &folder,
        &rendered(&folder, (64, 32), &["--backend", "gpu", "--frames", "28:32"]),
        &expected[28..32],
    );
}
