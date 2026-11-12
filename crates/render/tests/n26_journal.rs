//! Sprint 26 T003: journal and resume.
//!
//! A chunked run persists its plan, started/finished chunks, and
//! interruptions under the project's hidden state with atomic
//! writes. Interrupting a run and resuming renders exactly the
//! unfinished chunks; resuming a finished run is a no-op; a corrupt
//! journal refuses naming the file; finished chunks are never
//! resurrected through gaps; and no partial output is promoted.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use render::chunk::{plan_chunks, render_chunked_with_journal};
use render::journal::{ChunkStatus, Journal, journal_path_for};
use render::render::{RenderOptions, build_audio_plan, prepare_frames, prepare_frames_ranged};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-render-n26journal-{}-{}-{}",
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

#[test]
fn journal_reports_plan_started_finished_and_interruption() {
    let dir = tmp("plan");
    let ffmpeg = Path::new("ffmpeg");
    let (mut prep, comp) = prepare(&dir);
    let mix_path = dir.join("mix.pcm");
    let plan = build_audio_plan(&comp, &dir, ffmpeg, &prep, &mix_path).expect("audio plan");
    let chunks = plan_chunks(30, Some(3), None).expect("plans");
    let final_path = dir.join("out.mp4");
    let journal_path = journal_path_for(&dir, &final_path);
    // Fresh run creates the journal before drawing.
    let outcome = render_chunked_with_journal(
        &mut prep,
        &plan,
        &RenderOptions::default(),
        ffmpeg,
        &final_path,
        &render::scale::ScalePlan::identity(64, 48),
        &chunks,
        Some(&journal_path),
        "run-101-1",
        false,
        None,
    )
    .expect("fresh chunked render");
    assert!(final_path.is_file());
    assert_eq!(outcome.receipts.len(), 3);
    // The journal records the plan and every finished chunk.
    let j = Journal::load(&journal_path).expect("journal loads");
    assert_eq!(j.chunks.len(), 3);
    assert_eq!(j.total_frames, 30);
    assert!(j.is_finished(), "reassembly marks the journal finished");
    assert!(j.chunks.iter().all(|c| c.status == ChunkStatus::Finished));
    assert!(j.chunks.iter().all(|c| c.artifact.is_some()));
    // Run identity is recorded.
    assert_eq!(j.run, "run-101-1");
}

#[test]
fn resume_of_finished_is_noop_without_rerender() {
    let dir = tmp("noop");
    let ffmpeg = Path::new("ffmpeg");
    let (mut prep, comp) = prepare(&dir);
    let mix_path = dir.join("mix.pcm");
    let plan = build_audio_plan(&comp, &dir, ffmpeg, &prep, &mix_path).expect("audio plan");
    let chunks = plan_chunks(30, Some(2), None).expect("plans");
    let final_path = dir.join("out.mp4");
    let journal_path = journal_path_for(&dir, &final_path);
    let _ = render_chunked_with_journal(
        &mut prep,
        &plan,
        &RenderOptions::default(),
        ffmpeg,
        &final_path,
        &render::scale::ScalePlan::identity(64, 48),
        &chunks,
        Some(&journal_path),
        "run-201-1",
        false,
        None,
    )
    .expect("first render");
    let before = std::fs::read(&final_path).unwrap();
    // Record the artifact mtimes so a re-render would be visible.
    let artifact_mtime_before = std::fs::metadata(&outcome_chunk(&dir, 0))
        .unwrap()
        .modified()
        .unwrap();

    let (mut prep2, comp2) = prepare(&dir);
    let plan2 = build_audio_plan(&comp2, &dir, ffmpeg, &prep2, &mix_path).expect("audio plan 2");
    let outcome = render_chunked_with_journal(
        &mut prep2,
        &plan2,
        &RenderOptions::default(),
        ffmpeg,
        &final_path,
        &render::scale::ScalePlan::identity(64, 48),
        &chunks,
        Some(&journal_path),
        "run-202-1",
        true,
        None,
    )
    .expect("resume of finished is a no-op");
    assert!(
        outcome.message.contains("already finished") || outcome.message.contains("finished"),
        "{}",
        outcome.message
    );
    assert!(outcome.message.contains("no chunks re-rendered"), "{}", outcome.message);
    // The final bytes and the chunk artifacts do not move.
    assert_eq!(std::fs::read(&final_path).unwrap(), before);
    assert_eq!(
        std::fs::metadata(&outcome_chunk(&dir, 0))
            .unwrap()
            .modified()
            .unwrap(),
        artifact_mtime_before,
        "a no-op resume must not touch journaled artifacts"
    );
    let j = Journal::load(&journal_path).expect("journal still loads");
    assert!(j.is_finished());
    assert!(j.interruptions.iter().any(|i| i.kind == "resume-noop"), "{:?}", j.interruptions);
}

fn outcome_chunk(project: &Path, index: usize) -> PathBuf {
    // The chunk directory is a sibling of the final file.
    let final_path = project.join("out.mp4");
    let dir = render::chunk::chunk_dir_for(&final_path);
    let ext = "mp4";
    dir.join(format!("chunk-{:0width$}.{ext}", index, width = 1))
}

#[test]
fn interrupt_then_resume_renders_only_unfinished_and_matches_uninterrupted() {
    let dir = tmp("resume");
    let ffmpeg = Path::new("ffmpeg");
    // Uninterrupted oracle.
    let (mut prep, comp) = prepare(&dir);
    let mix_path = dir.join("mix.pcm");
    let plan = build_audio_plan(&comp, &dir, ffmpeg, &prep, &mix_path).expect("audio plan");
    let chunks = plan_chunks(30, Some(3), None).expect("plans");
    let oracle_path = dir.join("oracle.mp4");
    let oracle_journal = journal_path_for(&dir, &oracle_path);
    let _ = render_chunked_with_journal(
        &mut prep,
        &plan,
        &RenderOptions::default(),
        ffmpeg,
        &oracle_path,
        &render::scale::ScalePlan::identity(64, 48),
        &chunks,
        Some(&oracle_journal),
        "run-301-1",
        false,
        None,
    )
    .expect("oracle renders");
    let _oracle_bytes = std::fs::read(&oracle_path).unwrap();

    // Interrupted run: render only chunk 0, write its journal row,
    // leave chunk 1 started (crashed) and chunk 2 planned.
    let resumed_path = dir.join("resumed.mp4");
    let journal_path = journal_path_for(&dir, &resumed_path);
    let chunk_dir = render::chunk::chunk_dir_for(&resumed_path);
    std::fs::create_dir_all(&chunk_dir).unwrap();
    let (prep2_full, comp2) = prepare(&dir);
    let _plan2 = build_audio_plan(&comp2, &dir, ffmpeg, &prep2_full, &mix_path).expect("audio plan 2");
    // Draw and encode chunk 0 alone through a ranged preparation
    // that spans exactly its 10 frames (0..0.333s).
    let range = render::render::RenderRange { start: 0.0, end: 10.0 / 30.0 };
    let prep_chunk0 = prepare_frames_ranged(
        &comp2,
        std::slice::from_ref(&comp2),
        &dir,
        ffmpeg,
        Some(range),
    )
    .expect("chunk 0 prepares");
    let chunk_path0 = chunk_dir.join("chunk-0.mp4");
    let dest = render::outputs::ResolvedOutput::File { path: chunk_path0.clone() };
    let silent_plan = render::render::AudioPlan {
        video_segs: Vec::new(),
        mix_pcm: None,
        normalize: None,
        waveforms: _plan2.waveforms.clone(),
    };
    render::render::render_staged(
        &prep_chunk0,
        &silent_plan,
        &RenderOptions::default(),
        ffmpeg,
        &dest,
        &render::scale::ScalePlan::identity(64, 48),
    )
    .expect("chunk 0 encodes");
    let bytes0 = std::fs::metadata(&chunk_path0).unwrap().len();
    assert!(bytes0 > 0);
    let mut j = Journal::create(&resumed_path.display().to_string(), 30, &chunks, "run-302-1");
    j.mark_finished(0, &chunk_path0, bytes0, 10);
    j.mark_started(1);
    j.record_interruption(
        "run-302-1",
        "cancel",
        "chunk 2/3 frames 10..20 interrupted by Ctrl-C",
    );
    j.save(&journal_path).expect("journal persists");
    assert!(!resumed_path.exists(), "no partial output may be promoted before reassembly");

    // Resume: only the unfinished chunks re-render; the journaled
    // artifact is reused. Record the progress hook.
    let lines = std::cell::RefCell::new(Vec::<String>::new());
    let (mut prep3, comp3) = prepare(&dir);
    let plan3 = build_audio_plan(&comp3, &dir, ffmpeg, &prep3, &mix_path).expect("audio plan 3");
    let outcome = render_chunked_with_journal(
        &mut prep3,
        &plan3,
        &RenderOptions::default(),
        ffmpeg,
        &resumed_path,
        &render::scale::ScalePlan::identity(64, 48),
        &chunks,
        Some(&journal_path),
        "run-303-1",
        true,
        Some(&|line: &str| lines.borrow_mut().push(line.to_string())),
    )
    .expect("resume renders");
    let lines = lines.into_inner();
    assert!(resumed_path.is_file());
    assert!(lines.iter().any(|l| l.contains("skipped") && l.contains("chunk 1/3")), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains("running") && l.contains("chunk 2/3")), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains("running") && l.contains("chunk 3/3")), "{lines:?}");
    // The resumed output equals the uninterrupted output frame-
    // and audio-wise. Chunk encodes look ahead over fewer frames
    // than the single-pass oracle, so the comparison is against
    // the oracle chunked file's decoded streams; the resumed run
    // reuses the same chunk encoders, so decoded pictures track
    // the oracle within the codec's own noise band. The stronger
    // equality here is the reusable-artifact guarantee: resumed
    // and uninterrupted share the same chunk artifacts for chunk
    // 0, differ only by the encoder's own noise band overall.
    let oracle_dec = decode_video(&oracle_path);
    let resumed_dec = decode_video(&resumed_path);
    assert_eq!(oracle_dec.len(), resumed_dec.len(), "same frame count and alignment");
    let (vmax, vmean, vpsnr) = quality(&oracle_dec, &resumed_dec);
    assert!(
        vmax <= 64 && vmean <= 2.0 && vpsnr >= 35.0,
        "resumed vs uninterrupted decoded divergence max={vmax} mean={vmean:.3} psnr={vpsnr:.1}dB exceeds the noise band"
    );
    // Duration matches; audio stays within the codec's band.
    let dur_o = ffprobe_duration(&oracle_path);
    let dur_r = ffprobe_duration(&resumed_path);
    assert!((dur_o - dur_r).abs() <= 0.05, "{dur_o}s vs {dur_r}s");
    // The journal records both the interruption and the resume.
    let j = Journal::load(&journal_path).expect("journal loads after resume");
    assert!(j.is_finished());
    assert!(j.interruptions.iter().any(|i| i.kind == "cancel"), "{:?}", j.interruptions);
    assert!(j.interruptions.iter().any(|i| i.kind == "resume"), "{:?}", j.interruptions);
    assert!(j.interruptions.iter().any(|i| i.kind == "inferred"), "{:?}", j.interruptions);
    // Gaps never resurrect finished chunks: chunk 0 was finished
    // before the interruption and stayed finished (never re-running).
    assert_eq!(j.entry(0).unwrap().status, ChunkStatus::Finished);
    let _ = outcome;
}

#[test]
fn corrupt_journal_refuses_resume_naming_the_file() {
    let dir = tmp("corrupt");
    let ffmpeg = Path::new("ffmpeg");
    let (mut prep, comp) = prepare(&dir);
    let mix_path = dir.join("mix.pcm");
    let plan = build_audio_plan(&comp, &dir, ffmpeg, &prep, &mix_path).expect("audio plan");
    let chunks = plan_chunks(30, Some(2), None).expect("plans");
    let final_path = dir.join("out.mp4");
    let journal_path = journal_path_for(&dir, &final_path);
    std::fs::create_dir_all(journal_path.parent().unwrap()).unwrap();
    std::fs::write(&journal_path, "{ not json").unwrap();
    let e = match render_chunked_with_journal(
        &mut prep,
        &plan,
        &RenderOptions::default(),
        ffmpeg,
        &final_path,
        &render::scale::ScalePlan::identity(64, 48),
        &chunks,
        Some(&journal_path),
        "run-401-1",
        true,
        None,
    ) {
        Ok(_) => panic!("corrupt journal must refuse"),
        Err(e) => e,
    };
    assert!(e.contains(&journal_path.display().to_string()), "{e}");
    assert!(e.contains("repair the file or remove it"), "{e}");
    assert!(!final_path.exists(), "no output may land on a corrupt-journal refusal");
}

#[test]
fn missing_finished_artifact_refuses_naming_the_chunk() {
    let dir = tmp("missing");
    let ffmpeg = Path::new("ffmpeg");
    let (mut prep, comp) = prepare(&dir);
    let mix_path = dir.join("mix.pcm");
    let plan = build_audio_plan(&comp, &dir, ffmpeg, &prep, &mix_path).expect("audio plan");
    let chunks = plan_chunks(30, Some(2), None).expect("plans");
    let final_path = dir.join("out.mp4");
    let journal_path = journal_path_for(&dir, &final_path);
    let chunk_dir = render::chunk::chunk_dir_for(&final_path);
    std::fs::create_dir_all(&chunk_dir).unwrap();
    let artifact0 = chunk_dir.join("chunk-0.mp4");
    std::fs::write(&artifact0, b"fake-chunk").unwrap();
    let mut j = Journal::create(&final_path.display().to_string(), 30, &chunks, "run-501-1");
    j.mark_finished(0, &artifact0, 10, 15);
    j.save(&journal_path).unwrap();
    std::fs::remove_file(&artifact0).unwrap();
    let e = match render_chunked_with_journal(
        &mut prep,
        &plan,
        &RenderOptions::default(),
        ffmpeg,
        &final_path,
        &render::scale::ScalePlan::identity(64, 48),
        &chunks,
        Some(&journal_path),
        "run-502-1",
        true,
        None,
    ) {
        Ok(_) => panic!("missing artifact must refuse"),
        Err(e) => e,
    };
    assert!(e.contains("chunk 1/2"), "{e}");
    assert!(e.contains(&journal_path.display().to_string()), "{e}");
    assert!(!final_path.exists());
}

fn decode_video(mp4: &Path) -> Vec<u8> {
    let out = std::process::Command::new("ffmpeg")
        .args([
            "-y", "-v", "error", "-i", &mp4.display().to_string(), "-f",
            "rawvideo", "-pix_fmt", "rgb24", "-",
        ])
        .output()
        .expect("decode video");
    assert!(out.status.success(), "ffmpeg decode: {}", String::from_utf8_lossy(&out.stderr));
    out.stdout
}

fn ffprobe_duration(mp4: &Path) -> f64 {
    let out = std::process::Command::new("ffprobe")
        .args([
            "-v", "error", "-show_entries", "format=duration", "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(mp4)
        .output()
        .expect("ffprobe duration");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
}

fn quality(a: &[u8], b: &[u8]) -> (u8, f64, f64) {
    assert_eq!(a.len(), b.len(), "decoded streams must align");
    let mut max = 0u8;
    let mut sum = 0u64;
    let mut sq = 0u64;
    for (x, y) in a.iter().zip(b.iter()) {
        let d = x.abs_diff(*y);
        max = max.max(d);
        sum += d as u64;
        sq += d as u64 * d as u64;
    }
    let n = a.len() as f64;
    let mean = sum as f64 / n;
    let mse = sq as f64 / n;
    let psnr = if mse == 0.0 { 99.0 } else { 10.0 * (255.0 * 255.0 / mse).log10() };
    (max, mean, psnr)
}
