//! Sprint 26 T002: LAN worker distribution over the wire.
//!
//! A chunked render completes with invited workers and reports
//! the worker per chunk; an uninvited machine never joins; an
//! unreachable machine is reported rather than waited on; a
//! killed worker's in-flight chunks redistribute and complete.

use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-lan-{}-{}-{}",
        tag,
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
    let out = Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

const COMP: &str = "composition name=\"t\" width=160 height=120 duration=1 fps=30 background=\"#112233\" version=1 {\n    box x=10 y=10 width=60 height=40 fill=\"#ff0000\" radius=4 border-width=2 border-color=\"#ffffff\" opacity=1 order=1 start=0 duration=1\n    text value=\"Hi\" start=0 duration=1 x=70 y=20 size=20 color=\"#ffffff\"\n}\n";

fn valid_proj(root: &Path, tag: &str) -> PathBuf {
    let folder = root.join(tag);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("composition.kdl"), COMP).unwrap();
    folder
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn spawn_worker(
    port: u16,
    token: &str,
    env: Option<(&str, &str)>,
) -> std::process::Child {
    let mut cmd = Command::new(bin());
    cmd.arg("worker-serve")
        .arg("--bind")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("--token")
        .arg(token)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some((k, v)) = env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().expect("spawn worker");
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        if child.try_wait().unwrap().is_some() {
            panic!("worker on port {port} exited before listening");
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            panic!("worker on port {port} never became connectable");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    child
}

/// A port where nothing listens (a worker that is gone).
fn dead_port() -> String {
    let port = free_port();
    format!("127.0.0.1:{port}")
}

#[test]
fn invited_worker_chunk_render_end_to_end() {
    let root = tmp("e2e");
    valid_proj(&root, "proj");
    let port = free_port();
    let mut worker = spawn_worker(port, "lan-secret", None);
    let addr = format!("127.0.0.1:{port}");
    let (code, out) = run(
        &[
            "render", "proj", "-o", "lan.mp4", "--backend", "cpu",
            "--preset", "draft", "--chunks", "2",
            "--workers", &addr, "--worker-token", "lan-secret",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("stage workers: dispatching"), "{out}");
    assert!(out.contains("chunk 1/2") && out.contains("chunk 2/2"), "{out}");
    assert!(out.contains(&format!("workers={addr}")), "{out}");
    assert!(out.contains(&format!("chunk 1 rendered on {addr}")), "{out}");
    assert!(out.contains(&format!("chunk 2 rendered on {addr}")), "{out}");
    assert!(root.join("lan.mp4").is_file());

    // Decoded pictures: 30 frames, aligned with the single-pass
    // render inside the encoder's noise band.
    let (code, single) = run(
        &[
            "render", "proj", "-o", "single.mp4", "--backend", "cpu",
            "--preset", "draft",
        ],
        &root,
    );
    assert_eq!(code, 0, "{single}");
    let video = |mp4: &Path| {
        let out = Command::new("ffmpeg")
            .args([
                "-y", "-v", "error", "-i", &mp4.display().to_string(), "-f",
                "rawvideo", "-pix_fmt", "rgb24", "-",
            ])
            .output()
            .expect("decode video");
        assert!(out.status.success());
        out.stdout
    };
    let sv = video(&root.join("single.mp4"));
    let cv = video(&root.join("lan.mp4"));
    assert_eq!(sv.len(), cv.len(), "LAN render must carry every frame");
    assert_eq!(sv.len() / (160usize * 120 * 3), 30);
    let mut max = 0u8;
    let mut sum = 0u64;
    for (x, y) in sv.iter().zip(cv.iter()) {
        max = max.max(x.abs_diff(*y));
        sum += x.abs_diff(*y) as u64;
    }
    let mean = sum as f64 / sv.len() as f64;
    assert!(max <= 80 && mean <= 3.0, "max={max} mean={mean} exceeds the noise band");
    let _ = worker.kill();
    let _ = worker.wait();
}

#[test]
fn uninvited_machines_refuse() {
    let root = tmp("refuse");
    valid_proj(&root, "proj");
    // A worker never starts without the invitation token.
    let (code, out) = run(&["worker-serve", "--port", "0"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("never serves uninvited"), "{out}");

    let port = free_port();
    let mut worker = spawn_worker(port, "lan-secret", None);
    let addr = format!("127.0.0.1:{port}");
    // A wrong invitation secret is refused naming the invitation.
    let (code, out) = run(
        &[
            "render", "proj", "-o", "never.mp4", "--backend", "cpu",
            "--preset", "draft", "--chunks", "2",
            "--workers", &addr, "--worker-token", "wrong-secret",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("not invited"), "{out}");
    assert!(!root.join("never.mp4").exists(), "no output may land");

    // Workers without a token, a token without workers, workers
    // without a chunk target, garbage addresses, and duplicates
    // all refuse before the first frame.
    let cases: &[(&[&str], &str)] = &[
        (
            &["render", "proj", "--chunks", "2", "--workers", &addr],
            "--worker-token",
        ),
        (
            &["render", "proj", "--worker-token", "lan-secret"],
            "--workers host:port",
        ),
        (
            &[
                "render", "proj", "--workers", &addr,
                "--worker-token", "lan-secret",
            ],
            "--chunks",
        ),
        (
            &[
                "render", "proj", "--chunks", "2", "--workers", "not-an-addr",
                "--worker-token", "lan-secret",
            ],
            "not-an-addr",
        ),
    ];
    for (args, needle) in cases {
        let (code, out) = run(args, &root);
        assert_eq!(code, 1, "{out}");
        assert!(out.contains(needle), "args={args:?}: {out}");
    }
    let _ = worker.kill();
    let _ = worker.wait();
}

#[test]
fn unreachable_worker_reported_and_redistributed() {
    let root = tmp("unreach");
    valid_proj(&root, "proj");
    let port = free_port();
    let mut worker = spawn_worker(port, "lan-secret", None);
    let good = format!("127.0.0.1:{port}");
    let dead = dead_port();
    // The dead address is reported and the run completes on the
    // surviving worker: reported, never waited on forever.
    let (code, out) = run(
        &[
            "render", "proj", "-o", "lan.mp4", "--backend", "cpu",
            "--preset", "draft", "--chunks", "2",
            "--workers", &format!("{dead},{good}"), "--worker-token",
            "lan-secret",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains(&format!("worker {dead} unreachable")), "{out}");
    assert!(out.contains("redistributing"), "{out}");
    assert!(out.contains(&format!("chunk 1 rendered on {good}")), "{out}");
    assert!(out.contains(&format!("chunk 2 rendered on {good}")), "{out}");
    assert!(root.join("lan.mp4").is_file());
    let _ = worker.kill();
    let _ = worker.wait();
}

#[test]
fn killed_worker_chunks_redistribute_exactly_once() {
    let root = tmp("kill");
    valid_proj(&root, "proj");
    let slow_port = free_port();
    let good_port = free_port();
    // The slow worker stalls every request; the kill wakes the
    // coordinator the moment the process dies.
    let mut slow = spawn_worker(
        slow_port,
        "lan-secret",
        Some(("FRAMESMITH_WORKER_STALL_MS", "60000")),
    );
    let mut good = spawn_worker(good_port, "lan-secret", None);
    let slow_addr = format!("127.0.0.1:{slow_port}");
    let good_addr = format!("127.0.0.1:{good_port}");
    let root_clone = root.clone();
    let slow_clone = slow_addr.clone();
    let good_clone = good_addr.clone();
    let render = std::thread::spawn(move || {
        run(
            &[
                "render", "proj", "-o", "lan.mp4", "--backend", "cpu",
                "--preset", "draft", "--chunks", "3",
                "--workers", &format!("{slow_clone},{good_clone}"),
                "--worker-token", "lan-secret",
            ],
            &root_clone,
        )
    });
    // Give the coordinator time to dispatch a chunk to the
    // stalled worker, then kill it mid-chunk.
    std::thread::sleep(Duration::from_secs(8));
    slow.kill().expect("kill the slow worker");
    let _ = slow.wait();
    let (code, out) = render.join().unwrap();
    assert_eq!(code, 0, "{out}");
    assert!(out.contains(&slow_addr), "{out}");
    assert!(out.contains("redistributing"), "{out}");
    assert!(out.contains(&format!("chunk 1 rendered on {good_addr}")), "{out}");
    assert!(out.contains(&format!("chunk 2 rendered on {good_addr}")), "{out}");
    assert!(out.contains(&format!("chunk 3 rendered on {good_addr}")), "{out}");
    let out_file = root.join("lan.mp4");
    assert!(out_file.is_file());
    let raw = Command::new("ffmpeg")
        .args([
            "-y", "-v", "error", "-i", &out_file.display().to_string(), "-f",
            "rawvideo", "-pix_fmt", "rgb24", "-",
        ])
        .output()
        .expect("decode video");
    assert!(raw.status.success());
    assert_eq!(raw.stdout.len() / (160usize * 120 * 3), 30, "every frame exactly once");
    let _ = good.kill();
    let _ = good.wait();
}
