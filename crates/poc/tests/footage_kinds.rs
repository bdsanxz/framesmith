//! Footage of kinds other than plain video at the composition's rate, end to end through the
//! `poc` command: which picture each frame shows, and which kinds are refused. See
//! `docs/spec/native-render-pipeline.md`, section 4. The clips are made with `ffmpeg`, which
//! these tests need; `poc` itself does not start it.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A new, empty folder for one test, under the build's own temporary folder.
fn folder(name: &str) -> PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("footage-kinds").join(name);
    if folder.exists() {
        std::fs::remove_dir_all(&folder).expect("the old test folder is removed");
    }
    std::fs::create_dir_all(&folder).expect("the test folder is made");
    folder
}

fn ffmpeg(args: &[&str]) -> Vec<u8> {
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
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

/// The pictures of a clip of our own making, as a source for `ffmpeg`: 64 by 32, each picture
/// one flat grey, picture N of luma `16 + 3 (N mod 70)`, marked as ordinary BT.709 video.
fn pictures(rate: &str, seconds: u32) -> String {
    let marked = "setparams=range=tv:colorspace=bt709:color_primaries=bt709:color_trc=bt709";
    format!("color=c=black:s=64x32:r={rate}:d={seconds},geq=lum='16+3*mod(N,70)':cb=128:cr=128,{marked}")
}

/// Writes `clip.mp4`: the pictures after `filter` when one is given, encoded so finely that a
/// flat grey keeps its value, as H.264 of a kind every decoder reads.
fn clip(folder: &Path, rate: &str, filter: Option<&str>, more: &[&str]) {
    let source = pictures(rate, 2);
    let target = folder.join("clip.mp4");
    let mut args = vec!["-f", "lavfi", "-i", &source];
    if let Some(filter) = filter {
        args.extend(["-vf", filter]);
    }
    args.extend(["-c:v", "libx264", "-qp", "1", "-profile:v", "high", "-pix_fmt", "yuv420p"]);
    args.extend(more);
    args.push(target.to_str().expect("the path is text"));
    ffmpeg(&args);
}

/// Which picture of the clip each frame of a render shows, counted up to 70 and then from 0
/// again, from the grey at its centre. The frames are drawn without encoding, so the grey is
/// exact.
fn shown(folder: &Path, backend: &str, composition: &str) -> Vec<u32> {
    std::fs::write(folder.join("composition.xml"), composition).expect("the composition is written");
    let dump = folder.join(format!("frames-{backend}"));
    let output = Command::new(env!("CARGO_BIN_EXE_poc"))
        .args(["render", "composition.xml", "--no-encode", "--backend", backend, "--dump"])
        .arg(&dump)
        .current_dir(folder)
        .output()
        .expect("poc runs");
    assert!(output.status.success(), "poc render: {}", String::from_utf8_lossy(&output.stderr));
    let frames = std::fs::read_dir(&dump).expect("the frames are kept").count();
    (0..frames)
        .map(|frame| {
            let pixels = std::fs::read(dump.join(format!("{frame}.rgba"))).expect("a frame is read");
            let grey = f64::from(pixels[(16 * 64 + 32) * 4]);
            // Luma from the grey, then the picture from the luma.
            ((grey / 1.164_384 / 3.0).round()) as u32
        })
        .collect()
}

const WHOLE: &str = r#"<composition width="64" height="32" fps="30" duration="2">
  <video src="clip.mp4" left="0" top="0" width="64" height="32"/>
</composition>"#;

/// The same clip from 0.5 s of the source on, shown for a second from 0.5 s.
const FROM_INSIDE: &str = r#"<composition width="64" height="32" fps="30" duration="2">
  <video src="clip.mp4" left="0" top="0" width="64" height="32" start="0.5" duration="1" media-start="0.5"/>
</composition>"#;

/// The latest picture of a source of `rate` pictures a second at or before frame `frame`.
fn at_or_before(frame: u32, rate: (u32, u32)) -> u32 {
    // frame / 30 seconds, times rate.0 / rate.1 pictures a second, rounded down.
    frame * rate.0 / (30 * rate.1)
}

#[test]
fn a_source_at_another_rate_shows_the_latest_picture_at_or_before_each_frame() {
    let rates: [(&str, &str, (u32, u32)); 3] = [("25", "25", (25, 1)), ("24", "24000/1001", (24000, 1001)), ("60", "60", (60, 1))];
    for (name, rate, exact) in rates {
        let folder = folder(&format!("rate-{name}"));
        clip(&folder, rate, None, &[]);
        for backend in ["cpu", "gpu"] {
            let frames = shown(&folder, backend, WHOLE);
            assert_eq!(frames.len(), 60);
            for (frame, picture) in (0_u32..).zip(&frames) {
                // Two seconds of pictures; the last one stays once the source has ended.
                let last = (2 * exact.0).div_ceil(exact.1) - 1;
                let expected = at_or_before(frame, exact).min(last);
                assert_eq!(*picture, expected % 70, "{rate} pictures a second, {backend}: frame {frame}");
            }
        }
    }
}

#[test]
fn a_clip_that_starts_inside_a_source_at_another_rate_starts_at_the_right_picture() {
    let folder = folder("inside");
    clip(&folder, "25", None, &[]);
    for backend in ["cpu", "gpu"] {
        let frames = shown(&folder, backend, FROM_INSIDE);
        // Frames 15 to 44 show the source from 0.5 s on: the source's time is the frame's.
        for frame in 15..45_u32 {
            assert_eq!(frames[frame as usize], at_or_before(frame, (25, 1)), "{backend}: frame {frame}");
        }
    }
}

#[test]
fn a_source_whose_rate_changes_shows_the_latest_picture_at_or_before_each_frame() {
    let folder = folder("changing");
    // Every picture of the first half of each second, every third of the second half.
    clip(
        &folder,
        "30",
        Some("select='lt(mod(n,30),15)+not(mod(n,3))'"),
        &["-fps_mode", "vfr"],
    );
    let kept: Vec<u32> = (0..60).filter(|n| n % 30 < 15 || n % 3 == 0).collect();
    for backend in ["cpu", "gpu"] {
        for (composition, frames) in [(WHOLE, 0..60_u32), (FROM_INSIDE, 15..45)] {
            let found = shown(&folder, backend, composition);
            for frame in frames {
                let expected = *kept.iter().rfind(|picture| **picture <= frame).expect("picture 0 is kept");
                assert_eq!(found[frame as usize], expected, "{backend}: frame {frame}");
            }
        }
    }
}

#[test]
fn an_interlaced_source_gives_one_picture_in_every_frame() {
    let folder = folder("interlaced");
    // Sixty pictures a second woven into thirty frames: without deinterlacing, the rows of a
    // frame hold two pictures in turn.
    clip(
        &folder,
        "60",
        Some("tinterlace=interleave_top,fieldorder=tff"),
        &["-flags", "+ilme+ildct", "-x264opts", "tff=1"],
    );
    std::fs::write(folder.join("composition.xml"), WHOLE).expect("the composition is written");
    let dump = folder.join("frames");
    let output = Command::new(env!("CARGO_BIN_EXE_poc"))
        .args([
            "render",
            "composition.xml",
            "--no-encode",
            "--backend",
            "cpu",
            "--frames",
            "10:20",
            "--dump",
        ])
        .arg(&dump)
        .current_dir(&folder)
        .output()
        .expect("poc runs");
    assert!(output.status.success(), "poc render: {}", String::from_utf8_lossy(&output.stderr));
    for frame in 10..20 {
        let pixels = std::fs::read(dump.join(format!("{frame}.rgba"))).expect("a frame is read");
        let column: Vec<u8> = (4..28).map(|row| pixels[(row * 64 + 32) * 4]).collect();
        assert!(
            column.iter().all(|grey| *grey == column[0]),
            "frame {frame} holds one grey: {column:?}"
        );
    }
}

/// What `poc render` says when it refuses the clip of `folder`, and how many frames it drew.
fn refused(folder: &Path) -> String {
    std::fs::write(folder.join("composition.xml"), WHOLE.replace("<video ", "<video id=\"shot\" ")).expect("the composition is written");
    let output = Command::new(env!("CARGO_BIN_EXE_poc"))
        .args(["render", "composition.xml", "--backend", "cpu", "--output", "out.mp4"])
        .current_dir(folder)
        .output()
        .expect("poc runs");
    assert!(!output.status.success(), "the render is refused");
    assert!(!folder.join("out.mp4").exists(), "no file is written");
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn a_video_marked_as_hdr_is_refused_before_a_frame_is_drawn() {
    let folder = folder("hdr");
    let source = pictures("30", 1)
        .replace("bt709", "bt2020")
        .replace("colorspace=bt2020", "colorspace=bt2020nc")
        .replace("color_trc=bt2020", "color_trc=smpte2084");
    let target = folder.join("clip.mp4");
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        &source,
        "-c:v",
        "libx265",
        "-x265-params",
        "lossless=1:log-level=error:colorprim=bt2020:transfer=smpte2084:colormatrix=bt2020nc",
        "-pix_fmt",
        "yuv420p10le",
        "-tag:v",
        "hvc1",
        target.to_str().expect("the path is text"),
    ]);
    let said = refused(&folder);
    assert!(
        said.contains("line 2")
            && said.contains("<video id=\"shot\">")
            && said.contains("clip.mp4")
            && said.contains("HDR video is not supported yet"),
        "{said}"
    );
}

#[test]
fn a_video_of_a_codec_that_is_not_decoded_is_refused_before_a_frame_is_drawn() {
    let folder = folder("not-decoded");
    let source = pictures("30", 1);
    // The test composition names `clip.mp4`; the file holds QuickTime animation with alpha.
    let target = folder.join("clip.mov");
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        &source,
        "-vf",
        "format=argb",
        "-c:v",
        "qtrle",
        "-pix_fmt",
        "argb",
        target.to_str().expect("the path is text"),
    ]);
    std::fs::rename(&target, folder.join("clip.mp4")).expect("the clip is named as the composition names it");
    let said = refused(&folder);
    assert!(
        said.contains("line 2") && said.contains("clip.mp4") && said.contains("does not decode"),
        "{said}"
    );
}

#[test]
fn a_stream_the_system_does_not_decode_is_refused_before_a_frame_is_drawn() {
    let folder = folder("stream-not-decoded");
    let source = pictures("60", 1);
    // H.264 kept without loss, its frames holding two fields: a codec the system has, in a
    // form it does not decode.
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        &source,
        "-vf",
        "tinterlace=interleave_top,fieldorder=tff",
        "-c:v",
        "libx264",
        "-qp",
        "0",
        "-pix_fmt",
        "yuv420p",
        "-flags",
        "+ilme+ildct",
        "-x264opts",
        "tff=1",
        folder.join("clip.mp4").to_str().expect("the path is text"),
    ]);
    let said = refused(&folder);
    assert!(
        said.contains("line 2") && said.contains("clip.mp4") && said.contains("could not be decoded"),
        "{said}"
    );
}

#[test]
fn a_file_that_is_no_video_is_refused_before_a_frame_is_drawn() {
    let folder = folder("no-video");
    std::fs::write(folder.join("clip.mp4"), b"this is not a film at all, only some words in a file").expect("the file is written");
    let said = refused(&folder);
    assert!(
        said.contains("line 2") && said.contains("clip.mp4") && said.contains("is not read"),
        "{said}"
    );
}
