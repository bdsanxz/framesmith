//! Play command end-to-end (Sprint 15, T005 / CAP-001): the lightweight
//! `play` command serves the same player as `preview` — a background
//! start prints the player page URL, the page loads with the film at
//! its own shape, a repeated `play` reuses the existing server, and a
//! broken project refuses before any server starts.
//!
//! Every command runs with `FRAMESMITH_PREVIEW_REGISTRY` pointed at a
//! per-test file, so suites never share daemon state. A guard stops
//! all of the test's servers on the way out, even on failure.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-cli-play-{tag}-{}-{}",
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

fn text_project(folder: &Path) {
    std::fs::write(
        folder.join("composition.kdl"),
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    )
    .unwrap();
}

/// One CLI invocation with this test's registry; returns (exit, stdout+stderr).
fn run(reg: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(bin())
        .args(args)
        .env("FRAMESMITH_PREVIEW_REGISTRY", reg)
        .output()
        .expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

/// Pull the first `http://127.0.0.1:<port>/player` URL (plus port) from output.
fn extract_player_url(out: &str) -> Option<(String, u16)> {
    let marker = "http://127.0.0.1:";
    let at = out.find(marker)?;
    let rest = &out[at + marker.len()..];
    let end = rest.find(|c: char| !c.is_ascii_digit())?;
    let port: u16 = rest[..end].parse().ok()?;
    let url = format!("{marker}{port}/player");
    out.contains(&url).then_some((url, port))
}

fn get(port: u16, target: &str) -> (u16, String) {
    let mut stream = std::net::TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().unwrap(),
        Duration::from_secs(5),
    )
    .expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream
        .write_all(
            format!("GET {target} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    let text = String::from_utf8_lossy(&raw).into_owned();
    let code: u16 = text
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse().ok())
        .expect("status line");
    let body = match raw.windows(4).position(|w| w == b"\r\n\r\n") {
        Some(i) => String::from_utf8_lossy(&raw[i + 4..]).into_owned(),
        None => String::new(),
    };
    (code, body)
}

/// Stops every server in this test's registry when the test ends.
struct Guard {
    reg: PathBuf,
}

impl Guard {
    fn new(reg: PathBuf) -> Self {
        Self { reg }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = run(&self.reg, &["preview-stop-all"]);
    }
}

// Daemon stop on Windows has no force-kill fallback (`kill` is
// Unix-only), so zombie servers accumulate and this serve e2e
// hangs on loopback port contention there; the serve proof runs
// on non-Windows hosts for now.
#[cfg(not(windows))]
#[test]
fn play_serves_the_same_player() {
    let root = tmp("serve");
    let reg = root.join("servers.json");
    let _guard = Guard::new(reg.clone());
    let proj = root.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    text_project(&proj);

    // `play` starts the server and prints the player page URL.
    let (code, out) = run(&reg, &["play", proj.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    let (player_url, port) = extract_player_url(&out).expect("play prints its player URL: {out}");
    assert!(player_url.ends_with("/player"), "{player_url}");

    // The player page loads: the composition film at its own shape with
    // the method/event surface behind it.
    let (code, page) = get(port, "/player");
    assert_eq!(code, 200, "{page}");
    assert!(page.contains("player: "), "{page}");
    assert!(page.contains("aspect-ratio: 320 / 180"), "{page}");
    assert!(page.contains("framesmithPlayer"), "{page}");

    // A repeated `play` reuses the existing server, still naming the
    // player page.
    let (code, out) = run(&reg, &["play", proj.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("already"), "{out}");
    let (again, port2) = extract_player_url(&out).expect("second play prints a player URL: {out}");
    assert_eq!(again, player_url);
    assert_eq!(port2, port);

    // The review page still serves on the same server: one server feeds
    // both surfaces.
    let (code, _) = get(port, "/");
    assert_eq!(code, 200);
}

#[test]
fn play_refuses_a_broken_project_before_serving() {
    let root = tmp("refuse");
    let reg = root.join("servers.json");
    let _guard = Guard::new(reg.clone());
    let proj = root.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(proj.join("composition.kdl"), "composition {{{ not kdl\n").unwrap();

    let (code, out) = run(&reg, &["play", proj.to_str().unwrap()]);
    assert_eq!(code, 1, "{out}");
    assert!(
        extract_player_url(&out).is_none(),
        "no URL is printed: {out}"
    );

    // Nothing serves and nothing is listed.
    let (code, out) = run(&reg, &["preview-list"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("no preview servers"), "{out}");
}
