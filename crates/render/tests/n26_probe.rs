//! Sprint 26 T004: the equivalence probe end to end.
//!
//! A chunked render promotes its reassembled file only after the
//! probe judges it against the single-pass oracle inside the codec
//! noise band, naming chunking in the file's provenance stamp.
//! A tampered file fails naming the disagreeing frames, and the
//! same checker judges single-pass outputs unchanged.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use render::chunk::{plan_chunks, render_chunked};
use render::outputs::{Provenance, ResolvedOutput};
use render::probe::{NoiseBand, container_comment, is_chunked_stamp, probe_files};
use render::render::{RenderOptions, build_audio_plan, prepare_frames, render_staged};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-render-n26probe-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const COMP: &str = "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#112233\" version=1 {\n    box x=4 y=4 width=30 height=20 fill=\"#ff0000\" radius=3 border-width=1 border-color=\"#ffffff\" opacity=1 order=1 start=0 duration=1\n}\n";

fn prepare(dir: &Path) -> (render::render::Prepared, notation::model::Composition) {
    std::fs::write(dir.join("composition.kdl"), COMP).unwrap();
    let v = validate::validate::validate_folder(dir);
    assert_eq!(
        format!("{:?}", v.status),
        "Valid",
        "{:?}",
        v.diagnostics
    );
    let comp = v.composition.unwrap();
    let prep = prepare_frames(&comp, std::slice::from_ref(&comp), dir, Path::new("ffmpeg"))
        .expect("prepares");
    (prep, comp)
}

fn provenance() -> Provenance {
    Provenance {
        version: "0.1.0".to_string(),
        digest: "9e3779b97f4a7c15".to_string(),
    }
}

/// One 64x48 rgb24 frame is 9216 bytes.
fn tamper_frame(src: &Path, dst: &Path, frame: usize) {
    let out = Command::new("ffmpeg")
        .args([
            "-y", "-v", "error", "-i", &src.display().to_string(), "-f",
            "rawvideo", "-pix_fmt", "rgb24", "-",
        ])
        .output()
        .expect("decode video");
    assert!(out.status.success());
    let frame_bytes = 64usize * 48 * 3;
    let mut raw = out.stdout;
    assert!(raw.len() >= (frame + 1) * frame_bytes, "has frame {frame}");
    for b in &mut raw[frame * frame_bytes..(frame + 1) * frame_bytes] {
        *b = 255 - *b;
    }
    // Re-encode the tampered raw into a file with silent audio.
    let mut child = Command::new("ffmpeg")
        .args([
            "-y", "-v", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s",
            "64x48", "-r", "30", "-i", "-", "-f", "lavfi", "-i",
            "anullsrc=r=48000:cl=stereo", "-t", "1", "-c:v", "libx264",
            "-pix_fmt", "yuv420p", "-c:a", "aac", "-ar", "48000", "-ac",
            "2", "-shortest",
        ])
        .arg(dst)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn tamper encode");
    use std::io::Write;
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(&raw)
        .expect("write raw");
    let status = child.wait().expect("tamper encode waits");
    assert!(status.success(), "tamper encode failed: {status}");
}

#[test]
fn chunked_render_passes_probe_and_names_chunking() {
    let dir = tmp("pass");
    let ffmpeg = Path::new("ffmpeg");
    let (mut prep, comp) = prepare(&dir);
    let mix_path = dir.join("mix.pcm");
    let plan =
        build_audio_plan(&comp, &dir, ffmpeg, &prep, &mix_path).expect("audio plan");
    let mut opts = RenderOptions::default();
    opts.provenance = Some(provenance());

    // Single-pass oracle on the same preparation.
    let dest = ResolvedOutput::File {
        path: dir.join("single.mp4"),
    };
    let scale = render::scale::ScalePlan::identity(64, 48);
    render_staged(&prep, &plan, &opts, ffmpeg, &dest, &scale).expect("single-pass renders");

    // Chunked render: the report carries the measured verdict.
    let chunks = plan_chunks(30, Some(2), None).expect("plans");
    let outcome = render_chunked(
        &mut prep,
        &plan,
        &opts,
        ffmpeg,
        &dir.join("chunked.mp4"),
        &scale,
        &chunks,
        None,
    )
    .expect("chunked render");
    assert!(outcome.message.contains("probe=within-noise"), "{}", outcome.message);
    // The verdict reports the measurement at frame alignment.
    let probe = &outcome.probe;
    assert!(probe.within_noise, "{probe:?}");
    assert_eq!(probe.frames_candidate, 30);
    assert_eq!(probe.frames_oracle, 30);
    assert!(probe.video_psnr >= 35.0, "psnr {}", probe.video_psnr);
    assert!(probe.duration_candidate - probe.duration_oracle <= 0.05);
    let verdict = probe.verdict_line(true);
    assert!(verdict.contains("chunked reassembly"), "{verdict}");
    assert!(verdict.contains("not single-pass byte identity"), "{verdict}");
    let lines = probe.measurement_lines();
    assert!(lines.iter().any(|l| l.contains("probe video")), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains("probe audio")), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains("probe duration")), "{lines:?}");

    // The standalone checker reaches the same verdict.
    let report =
        probe_files(ffmpeg, &dir.join("chunked.mp4"), &dir.join("single.mp4"), &NoiseBand::default())
            .expect("probe files");
    assert!(report.within_noise, "{report:?}");

    // Provenance: the chunked file names chunking, the oracle
    // single-pass.
    let chunked_comment =
        container_comment(ffmpeg, &dir.join("chunked.mp4")).expect("chunked comment");
    assert!(is_chunked_stamp(&chunked_comment), "{chunked_comment}");
    assert!(chunked_comment.contains("chunks=2"), "{chunked_comment}");
    assert!(chunked_comment.contains("frames=30"), "{chunked_comment}");
    let single_comment =
        container_comment(ffmpeg, &dir.join("single.mp4")).expect("single comment");
    assert!(!is_chunked_stamp(&single_comment), "{single_comment}");
}

#[test]
fn tampered_output_fails_naming_the_frame_and_single_pass_still_judges() {
    let dir = tmp("tamper");
    let ffmpeg = Path::new("ffmpeg");
    let (mut prep, comp) = prepare(&dir);
    let mix_path = dir.join("mix.pcm");
    let plan =
        build_audio_plan(&comp, &dir, ffmpeg, &prep, &mix_path).expect("audio plan");
    let opts = RenderOptions::default();
    let scale = render::scale::ScalePlan::identity(64, 48);

    let dest = ResolvedOutput::File {
        path: dir.join("single.mp4"),
    };
    render_staged(&prep, &plan, &opts, ffmpeg, &dest, &scale).expect("single-pass renders");

    let chunks = plan_chunks(30, Some(3), None).expect("plans");
    render_chunked(
        &mut prep,
        &plan,
        &opts,
        ffmpeg,
        &dir.join("chunked.mp4"),
        &scale,
        &chunks,
        None,
    )
    .expect("chunked render");

    // Tamper with one frame of the chunked output and probe it
    // against the oracle: the refusal names the frame.
    let tampered = dir.join("tampered.mp4");
    tamper_frame(&dir.join("chunked.mp4"), &tampered, 7);
    let report =
        probe_files(ffmpeg, &tampered, &dir.join("single.mp4"), &NoiseBand::default())
            .expect("tampered probes");
    assert!(!report.within_noise, "{report:?}");
    assert!(
        report.disagreeing_frames.contains(&7),
        "disagreeing {:?} must name frame 7",
        report.disagreeing_frames
    );
    let refusal = report.refusal(&tampered, &dir.join("single.mp4"));
    assert!(refusal.contains("[7]"), "{refusal}");
    assert!(refusal.contains("file withheld"), "{refusal}");

    // The oracle checker judges single-pass outputs too: the
    // oracle against itself is exact.
    let self_report =
        probe_files(ffmpeg, &dir.join("single.mp4"), &dir.join("single.mp4"), &NoiseBand::default())
            .expect("self probes");
    assert!(self_report.within_noise, "{self_report:?}");
    assert_eq!(self_report.video_max, 0);
    assert!(!self_report.verdict_line(false).contains("chunked"), "{}", self_report.verdict_line(false));
}
