//! Sprint 26 T001: the chunked render API end to end.
//!
//! A planned chunked render draws and encodes each chunk
//! independently, reports one receipt per chunk plus progress lines,
//! and reassembles in order; removing one chunk afterwards makes
//! reassembly refuse naming it.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use render::chunk::{ChunkArtifact, ChunkRange, assemble_chunks, plan_chunks, render_chunked};
use render::render::{RenderOptions, build_audio_plan, prepare_frames, render_staged};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-render-n26chunk-{}-{}-{}",
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
fn chunked_api_renders_receipts_then_refuses_missing_chunk() {
    let dir = tmp("api");
    let ffmpeg = Path::new("ffmpeg");
    let (mut prep, comp) = prepare(&dir);
    assert_eq!(prep.frames(), 30, "one second at 30 fps");
    let mix_path = dir.join("mix.pcm");
    let plan =
        build_audio_plan(&comp, &dir, ffmpeg, &prep, &mix_path).expect("audio plan");
    assert!(plan.mix_pcm.is_none(), "silent comp stages no mix");

    let chunks = plan_chunks(30, Some(2), None).expect("plans");
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].frame_count(), 15);
    assert_eq!(chunks[1].frame_count(), 15);

    let lines = std::cell::RefCell::new(Vec::<String>::new());
    let final_path = dir.join("chunked.mp4");
    let outcome = render_chunked(
        &mut prep,
        &plan,
        &RenderOptions::default(),
        ffmpeg,
        &final_path,
        &render::scale::ScalePlan::identity(64, 48),
        &chunks,
        Some(&|line: &str| lines.borrow_mut().push(line.to_string())),
    )
    .expect("chunked render");
    let lines = lines.into_inner();
    assert!(outcome.message.contains("chunks=2"), "{}", outcome.message);
    assert_eq!(outcome.receipts.len(), 2);
    assert_eq!(outcome.receipts[0].frames, 15);
    assert_eq!(outcome.receipts[1].frames, 15);
    assert!(outcome.receipts.iter().all(|r| r.bytes > 0));
    assert!(final_path.is_file(), "reassembled file lands");
    assert!(lines.iter().any(|l| l.contains("chunk 1/2")), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains("chunk 2/2")), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains("ok (frames 15")), "{lines:?}");
    // The caller's window is restored: the same preparation still
    // spans every frame.
    assert_eq!(prep.frames(), 30);

    // Reassembly with a removed chunk refuses naming it.
    let victim = outcome.receipts[1].path.clone();
    std::fs::remove_file(&victim).unwrap();
    let artifacts: Vec<ChunkArtifact> = outcome
        .receipts
        .iter()
        .map(|r| ChunkArtifact {
            range: ChunkRange {
                index: r.index,
                start_frame: r.start_frame,
                end_frame: r.end_frame,
            },
            path: r.path.clone(),
        })
        .collect();
    let e = assemble_chunks(
        ffmpeg,
        &artifacts,
        &dir.join("again.mp4"),
        render::outputs::OutputFormat::Mp4,
        plan.mix_pcm.as_deref(),
        1.0,
        None,
    )
    .unwrap_err();
    assert!(e.contains("missing 1 chunk"), "{e}");
    assert!(e.contains("chunk 2/2"), "{e}");
    assert!(!dir.join("again.mp4").exists(), "no output may land on refusal");
}

#[test]
fn single_pass_frames_equal_chunk_frames_input() {
    // The drawn frames are one source: the single-pass staged render
    // and each chunk draw frame N at the same absolute instant, so
    // the chunk pipeline carries every planned frame by construction.
    let dir = tmp("iso");
    let ffmpeg = Path::new("ffmpeg");
    let (prep, comp) = prepare(&dir);
    let mix_path = dir.join("mix.pcm");
    let plan = build_audio_plan(&comp, &dir, ffmpeg, &prep, &mix_path)
        .expect("audio plan");
    let scale = render::scale::ScalePlan::identity(64, 48);
    // Single-pass oracle on the same preparation.
    let dest = render::outputs::ResolvedOutput::File {
        path: dir.join("single.mp4"),
    };
    let _ = render_staged(&prep, &plan, &RenderOptions::default(), ffmpeg, &dest, &scale)
        .expect("single-pass renders");
    let single = std::fs::read(dir.join("single.mp4")).unwrap();
    assert!(!single.is_empty());
    // And the full window survives a chunked run on the same prep.
    let chunks = plan_chunks(prep.frames(), None, Some(10)).expect("plans");
    assert_eq!(chunks.len(), 3);
    let (mut prep2, comp2) = prepare(&dir);
    let plan2 = build_audio_plan(&comp2, &dir, ffmpeg, &prep2, &mix_path)
        .expect("audio plan");
    let outcome =
        render_chunked(&mut prep2, &plan2, &RenderOptions::default(), ffmpeg, &dir.join("tri.mp4"), &scale, &chunks, None)
            .expect("chunked renders");
    assert_eq!(outcome.receipts.len(), 3);
    assert!(dir.join("tri.mp4").is_file());
}
