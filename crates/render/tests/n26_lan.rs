//! Sprint 26 T002: the stateless LAN worker contract.
//!
//! A worker renders exactly its spec (range, source, quality,
//! format) from the spec alone and returns its artifact plus a
//! receipt; an uninvited machine never renders; an unreachable
//! invited worker is reported and its chunk redistributes to a
//! remaining worker.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use render::lan::{ChunkSpec, check_invitation, parse_workers};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-render-n26lan-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const COMP: &str = "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#112233\" version=1 {\n    box x=4 y=4 width=30 height=20 fill=\"#ff0000\" radius=3 border-width=1 border-color=\"#ffffff\" opacity=1 order=1 start=0 duration=1\n}\n";

fn spec(source: &str, name: &str, index: usize, start: u64, end: u64, token: &str) -> ChunkSpec {
    ChunkSpec {
        version: render::lan::LAN_WIRE_VERSION.to_string(),
        token: token.to_string(),
        index,
        total: 2,
        start_frame: start,
        end_frame: end,
        total_frames: 30,
        composition_source: source.to_string(),
        composition_name: name.to_string(),
        variables_json: None,
        strict_variables: false,
        theme: None,
        range_start: None,
        range_end: None,
        backend: "cpu".to_string(),
        preset: "draft".to_string(),
        crf_override: None,
        video_bitrate: None,
        format: "mp4".to_string(),
        prores_profile: "standard".to_string(),
        resolution: "source".to_string(),
        supersample: 1,
        encoder_choice: "software".to_string(),
        keyint: None,
        maxrate: None,
        hdr: "auto".to_string(),
        hdr_transfer: "sdr".to_string(),
        hdr_tone_mapped: false,
        provenance_version: "0.1.0".to_string(),
        provenance_digest: "d".to_string(),
        mix_slice_len: 0,
    }
}

#[test]
fn invitation_list_parses_strictly() {
    let ok = parse_workers("192.168.1.8:7788, workbench.local:7789, 127.0.0.1:7788").unwrap();
    assert_eq!(ok.len(), 3);
    let v6 = parse_workers("[2001:db8::1]:7788").unwrap();
    assert_eq!(v6, vec!["[2001:db8::1]:7788"]);
    // Malformed addresses, empty entries, duplicates, and the
    // empty list all refuse naming the value.
    for raw in ["", "no-port", "host:0", "host:70000", "host:abc", ":7788", "a:1,,b:2", "a:1, a:1"] {
        let e = parse_workers(raw).unwrap_err();
        assert!(e.contains("--workers"), "{raw:?} -> {e}");
        assert!(e.contains("no MP4 was written"), "{e}");
        let _ = &raw;
    }
}

#[test]
fn uninvited_machine_never_renders() {
    assert!(check_invitation("secret", "secret").is_ok());
    let e = check_invitation("wrong", "secret").unwrap_err();
    assert!(e.contains("not invited"), "{e}");
    // The library gate: an empty worker token renders nothing.
    let dir = tmp("token");
    let bad = spec(COMP, "t", 0, 0, 15, "wrong");
    let err = render::lan::render_worker_chunk(&bad, "secret", Path::new("ffmpeg"), &[], &dir)
        .unwrap_err();
    assert!(err.contains("not invited"), "{err}");
    let no_token = render::lan::render_worker_chunk(
        &spec(COMP, "t", 0, 0, 15, "anything"),
        "",
        Path::new("ffmpeg"),
        &[],
        &dir,
    )
    .unwrap_err();
    assert!(no_token.contains("not invited"), "{no_token}");
    assert!(!dir.join("lan-chunk-0").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn worker_renders_exactly_its_range() {
    let dir = tmp("exact");
    let work = dir.join("work");
    let want_token = "invite-secret";
    // Chunk 1 covers only the second half: 15 frames.
    let out = render::lan::render_worker_chunk(
        &spec(COMP, "t", 1, 15, 30, want_token),
        want_token,
        Path::new("ffmpeg"),
        &[],
        &work,
    )
    .expect("worker renders its chunk");
    assert_eq!(out.frames, 15);
    assert!(!out.artifact.is_empty());
    // The artifact is an MP4 (ftyp atom) and decodes to exactly
    // 15 frames over twice 64x48x3 bytes.
    assert!(out.artifact.windows(4).any(|w| w == b"ftyp"));
    let mp4 = dir.join("half.mp4");
    std::fs::write(&mp4, &out.artifact).unwrap();
    let raw = std::process::Command::new("ffmpeg")
        .args([
            "-y", "-v", "error", "-i", &mp4.display().to_string(), "-f",
            "rawvideo", "-pix_fmt", "rgb24", "-",
        ])
        .output()
        .expect("decode");
    assert!(raw.status.success());
    assert_eq!(raw.stdout.len() / (64 * 48 * 3), 15);
    // A spec that would bleed past its neighbour's range refuses.
    let bleed = render::lan::render_worker_chunk(
        &spec(COMP, "t", 1, 20, 31, want_token),
        want_token,
        Path::new("ffmpeg"),
        &[],
        &work,
    )
    .unwrap_err();
    assert!(bleed.contains("runs past"), "{bleed}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Spawn the serve loop on loopback and return its address once
/// it answers connections.
fn spawn_worker(token: &str) -> (String, Arc<AtomicBool>) {
    let announced = Arc::new(Mutex::new(String::new()));
    let announced_clone = announced.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();
    let token_owned = token.to_string();
    std::thread::spawn(move || {
        let _ = render::lan::serve_worker(
            "127.0.0.1:0",
            &token_owned,
            &|| Ok(Path::new("ffmpeg").to_path_buf()),
            &(move |line: &str| {
                if let Ok(mut g) = announced_clone.lock() {
                    *g = line.to_string();
                }
            }),
        );
        let _ = &stop_clone;
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(g) = announced.lock() {
            if let Some(addr) = g
                .strip_prefix("stage workers: serving on ")
                .and_then(|s| s.split_whitespace().next())
            {
                return (addr.to_string(), stop);
            }
        }
        if std::time::Instant::now() > deadline {
            panic!("worker did not announce its address: {:?}", announced.lock().unwrap());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn dead_addr() -> String {
    // A freshly bound then released port: nothing listens there.
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    format!("127.0.0.1:{port}")
}

/// One full coordinator run over the wire against the spawned
/// worker (and optionally a dead address): chunks redistribute and
/// the reassembled file lands through the T001 path.
fn distributed_run(workers: &[String], token: &str, final_path: &Path) -> (String, Vec<String>) {
    let dir = final_path.parent().unwrap().to_path_buf();
    let folder = dir.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("composition.kdl"), COMP).unwrap();
    let v = validate::validate::validate_folder(&folder);
    assert_eq!(format!("{:?}", v.status), "Valid", "{:?}", v.diagnostics);
    let comp = v.composition.clone().unwrap();
    let ffmpeg = Path::new("ffmpeg");
    let mut prep = render::render::prepare_frames(&comp, std::slice::from_ref(&comp), &folder, ffmpeg)
        .expect("prepares");
    assert_eq!(prep.frames(), 30);
    let mix_path = dir.join("mix.pcm");
    let plan =
        render::render::build_audio_plan(&comp, &folder, ffmpeg, &prep, &mix_path).expect("audio");
    assert!(plan.mix_pcm.is_none());
    let chunks = render::chunk::plan_chunks(30, Some(2), None).expect("plans");
    let source = std::fs::read_to_string(folder.join("composition.kdl")).unwrap();
    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let lines_clone = lines.clone();
    let digest = render::outputs::composition_inputs_digest(&v.compositions, &folder);
    let outcome = render::lan::render_distributed(render::lan::LanInputs {
        prep: &mut prep,
        full_plan: &plan,
        opts: &render::render::RenderOptions {
            backend: render::render::Backend::Cpu,
            preset: render::render::Preset::Draft,
            ..Default::default()
        },
        ffmpeg_bin: ffmpeg,
        final_path,
        scale: &render::scale::ScalePlan::identity(64, 48),
        chunks: &chunks,
        composition_source: &source,
        composition_name: &comp.name,
        variables_json: None,
        strict_variables: false,
        theme: None,
        range_start: None,
        range_end: None,
        provenance_version: "0.1.0",
        provenance_digest: &digest,
        workers,
        token,
        timeout: render::lan::worker_chunk_timeout(),
        on_progress: Some(&move |line: &str| {
            lines_clone.lock().unwrap().push(line.to_string());
        }),
    })
    .expect("distributed render completes");
    let lines = lines.lock().unwrap().clone();
    assert_eq!(outcome.receipts.len(), 2);
    assert!(final_path.is_file(), "reassembled file lands");
    (outcome.message, lines)
}

#[test]
fn invited_worker_renders_over_the_wire() {
    let dir = tmp("wire");
    let (addr, _stop) = spawn_worker("wire-secret");
    // Wrong token on the wire is refused and names the invitation.
    let mut stream = TcpStream::connect(&addr).unwrap();
    let bad = spec(COMP, "t", 0, 0, 15, "wrong-secret");
    let header = serde_json::to_vec(&bad).unwrap();
    stream.write_all(&(header.len() as u32).to_be_bytes()).unwrap();
    stream.write_all(&header).unwrap();
    let mut len = [0u8; 4];
    stream.read_exact(&mut len).unwrap();
    let mut payload = vec![0u8; u32::from_be_bytes(len) as usize];
    stream.read_exact(&mut payload).unwrap();
    let resp: render::lan::WorkerResponse = serde_json::from_slice(&payload).unwrap();
    assert!(!resp.ok);
    assert!(resp.message.contains("not invited"), "{}", resp.message);

    let (message, lines) = distributed_run(
        std::slice::from_ref(&addr),
        "wire-secret",
        &dir.join("lan.mp4"),
    );
    assert!(message.contains("chunks=2"), "{message}");
    assert!(message.contains(&format!("workers={addr}")), "{message}");
    assert!(lines.iter().any(|l| l.contains(&format!("chunk 1 ok on {addr}"))), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains(&format!("chunk 2 ok on {addr}"))), "{lines:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dead_worker_redistributes_to_remaining() {
    let dir = tmp("redist");
    let (good, _stop) = spawn_worker("wire-secret");
    let dead = dead_addr();
    let (message, lines) = distributed_run(
        &[dead.clone(), good.clone()],
        "wire-secret",
        &dir.join("redist.mp4"),
    );
    assert!(message.contains("chunks=2"), "{message}");
    // The dead address was reported and the chunk redistributed;
    // every chunk ran on the surviving worker.
    assert!(
        lines.iter().any(|l| l.contains(&format!("worker {dead} unreachable"))),
        "{lines:?}"
    );
    assert!(lines.iter().any(|l| l.contains("redistributing")), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains(&format!("chunk 1 ok on {good}"))), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains(&format!("chunk 2 ok on {good}"))), "{lines:?}");
    assert!(
        !lines.iter().any(|l| l.contains(&format!("ok on {dead}"))),
        "no chunk may have run on the dead worker: {lines:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn all_workers_dead_refuses_naming_the_chunk() {
    let dir = tmp("alldead");
    let dead = dead_addr();
    let folder = dir.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("composition.kdl"), COMP).unwrap();
    let v = validate::validate::validate_folder(&folder);
    let comp = v.composition.clone().unwrap();
    let ffmpeg = Path::new("ffmpeg");
    let mut prep = render::render::prepare_frames(&comp, std::slice::from_ref(&comp), &folder, ffmpeg)
        .expect("prepares");
    let mix_path = dir.join("mix.pcm");
    let plan =
        render::render::build_audio_plan(&comp, &folder, ffmpeg, &prep, &mix_path).expect("audio");
    let chunks = render::chunk::plan_chunks(30, Some(2), None).expect("plans");
    let source = std::fs::read_to_string(folder.join("composition.kdl")).unwrap();
    let digest = render::outputs::composition_inputs_digest(&v.compositions, &folder);
    let err = render::lan::render_distributed(render::lan::LanInputs {
        prep: &mut prep,
        full_plan: &plan,
        opts: &render::render::RenderOptions {
            backend: render::render::Backend::Cpu,
            preset: render::render::Preset::Draft,
            ..Default::default()
        },
        ffmpeg_bin: ffmpeg,
        final_path: &dir.join("never.mp4"),
        scale: &render::scale::ScalePlan::identity(64, 48),
        chunks: &chunks,
        composition_source: &source,
        composition_name: &comp.name,
        variables_json: None,
        strict_variables: false,
        theme: None,
        range_start: None,
        range_end: None,
        provenance_version: "0.1.0",
        provenance_digest: &digest,
        workers: std::slice::from_ref(&dead),
        token: "wire-secret",
        timeout: Duration::from_secs(30),
        on_progress: None,
    })
    .unwrap_err();
    assert!(err.contains("every invited worker is unreachable"), "{err}");
    assert!(err.contains(&dead), "{err}");
    assert!(!dir.join("never.mp4").exists(), "no output may land");
    let _ = std::fs::remove_dir_all(&dir);
}
