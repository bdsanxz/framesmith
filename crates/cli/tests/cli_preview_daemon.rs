//! Preview daemon end-to-end (Sprint 14, T006 / CAP-006, J-03, J-04,
//! STATE-01, INT-002, REC-001, REC-003): the full `preview` family
//! through the real binary — background start with a printed URL,
//! status naming project and port, second-start dedupe, busy-port
//! fall-forward, stop, dead-stop, list, stop-all counts, empty
//! stop-all, and broken-project refusal before any server starts.
//!
//! Every command runs with `FRAMESMITH_PREVIEW_REGISTRY` pointed at a
//! per-test file, so suites never share daemon state. A guard stops
//! all of the test's servers on the way out, even on failure.

use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-cli-preview-{}-{}-{}",
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

fn text_project(folder: &Path, name: &str) {
    std::fs::write(
        folder.join("composition.kdl"),
        format!(
            "composition name=\"{name}\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {{\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}}\n"
        ),
    )
    .unwrap();
}

/// One CLI invocation with this test's registry; returns (exit, stdout+stderr).
/// Bounded: a hung child is killed after the timeout and the partial
/// output is returned as a failure, so the suite never stalls and the
/// hung command names itself (the daemon spawns long-lived servers,
/// which output() alone cannot bound). Pipe drains are bounded too: a
/// grandchild that keeps a pipe open must not stall the test.
fn run(reg: &Path, args: &[&str]) -> (i32, String) {
    use std::io::Read;
    use std::sync::mpsc;
    const TIMEOUT: Duration = Duration::from_secs(120);
    eprintln!("run: framesmith {}", args.join(" "));
    let mut child = Command::new(bin())
        .args(args)
        .env("FRAMESMITH_PREVIEW_REGISTRY", reg)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("run framesmith binary");
    let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>();
    {
        let mut pipe = child.stdout.take();
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut pipe) = pipe.take() {
                let _ = pipe.read_to_end(&mut buf);
            }
            let _ = out_tx.send(buf);
        });
    }
    let (err_tx, err_rx) = mpsc::channel::<Vec<u8>>();
    {
        let mut pipe = child.stderr.take();
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut pipe) = pipe.take() {
                let _ = pipe.read_to_end(&mut buf);
            }
            let _ = err_tx.send(buf);
        });
    }
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if start.elapsed() >= TIMEOUT {
                    let _ = child.kill();
                    // Reap with a deadline too: a killed child that
                    // will not die must not stall the test either.
                    let reap = Instant::now();
                    while reap.elapsed() < Duration::from_secs(5) {
                        match child.try_wait() {
                            Ok(Some(_)) => break,
                            _ => std::thread::sleep(Duration::from_millis(50)),
                        }
                    }
                    break None;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => break None,
        }
    };
    // Bounded drains: a grandchild that keeps a pipe open must not
    // stall the test either (genuine EOF then sentinel).
    let stdout = out_rx.recv_timeout(Duration::from_secs(5)).unwrap_or_default();
    let stderr = err_rx.recv_timeout(Duration::from_secs(5)).unwrap_or_default();
    eprintln!(
        "run: drained {} stdout bytes, {} stderr bytes",
        stdout.len(),
        stderr.len()
    );
    let mut text = String::from_utf8_lossy(&stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&stderr));
    match status {
        Some(status) => (status.code().unwrap_or(-1), text),
        None => (
            -1,
            format!(
                "TIMED OUT after {}s: framesmith {}\n{text}",
                TIMEOUT.as_secs(),
                args.join(" ")
            ),
        ),
    }
}

/// Pull the first `http://127.0.0.1:<port>/` URL (plus port) from output.
fn extract_url(out: &str) -> Option<(String, u16)> {
    let marker = "http://127.0.0.1:";
    let at = out.find(marker)?;
    let rest = &out[at + marker.len()..];
    let end = rest.find(|c: char| !c.is_ascii_digit())?;
    let port: u16 = rest[..end].parse().ok()?;
    let url = format!("{marker}{port}/");
    out.contains(&url).then_some((url, port))
}

/// Debug dump for the start-failure panic: the registry body plus
/// the tails of any preview-serve daemon logs in the temp dir.
fn read_to_string_for_debug(reg: &Path) -> String {
    std::fs::read_to_string(reg).unwrap_or_else(|e| format!("<unreadable: {e}>"))
}

fn daemon_logs_for_debug() -> String {
    let dir = std::env::temp_dir();
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .map(|name| name.to_string_lossy().starts_with("framesmith-preview-serve-"))
                        .unwrap_or(false)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    names.sort();
    let mut out = String::new();
    for path in names {
        let body = std::fs::read_to_string(&path).unwrap_or_default();
        let lines: Vec<&str> = body.lines().collect();
        let start = lines.len().saturating_sub(5);
        out.push_str(&format!("\n--- {} ---\n{}", path.display(), lines[start..].join("\n")));
    }
    if out.is_empty() {
        out.push_str("<no daemon logs>");
    }
    out
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

#[test]
fn full_daemon_lifecycle_through_the_binary() {
    let root = tmp("lifecycle");
    let reg = root.join("servers.json");
    let _guard = Guard::new(reg.clone());
    let p1 = root.join("proj-one");
    let p2 = root.join("proj-two");
    std::fs::create_dir_all(&p1).unwrap();
    std::fs::create_dir_all(&p2).unwrap();
    text_project(&p1, "one");
    text_project(&p2, "two");

    // Start binds a free port and prints its URL.
    let (code, out) = run(&reg, &["preview", p1.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    let (url1, port1) = match extract_url(&out) {
        Some(pair) => pair,
        None => panic!("start prints its URL, got: {out}\nregistry: {}\ndaemon logs: {}", read_to_string_for_debug(&reg), daemon_logs_for_debug()),
    };

    // Status names the project and port.
    let (code, out) = run(&reg, &["preview-status", p1.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("proj-one"), "{out}");
    assert!(out.contains(&port1.to_string()), "{out}");
    assert!(out.contains(&url1), "{out}");

    // A second start for one directory prints the existing URL.
    let (code, out) = run(&reg, &["preview", p1.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("already"), "{out}");
    let (url1b, _) = match extract_url(&out) {
        Some(pair) => pair,
        None => panic!("second start prints a URL, got: {out}"),
    };
    assert_eq!(url1b, url1, "the existing URL is reused, not duplicated");

    // A busy preferred port falls forward with the new URL printed.
    let (code, out) = run(
        &reg,
        &[
            "preview",
            "--port",
            &port1.to_string(),
            p2.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 0, "{out}");
    let (url2, port2) = match extract_url(&out) {
        Some(pair) => pair,
        None => panic!("fall-forward prints its URL, got: {out}"),
    };
    assert_ne!(port2, port1, "the busy port falls forward: {out}");
    assert_ne!(url2, url1);

    // List shows every live server.
    let (code, out) = run(&reg, &["preview-list"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("proj-one") && out.contains("proj-two"),
        "{out}"
    );
    assert!(out.contains(&url1) && out.contains(&url2), "{out}");

    // Stop ends exactly that server; the other is untouched.
    let (code, out) = run(&reg, &["preview-stop", p1.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("stopped"), "{out}");
    assert!(out.contains(&url1), "{out}");
    let (code, out) = run(&reg, &["preview-status", p1.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("no preview server"), "{out}");
    let (code, out) = run(&reg, &["preview-status", p2.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains(&url2), "the other server is untouched: {out}");

    // Stopping a dead server reports already-stopped and exits 0.
    let (code, out) = run(&reg, &["preview-stop", p1.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("already stopped"), "{out}");

    // Stop-all ends all and reports the count.
    let (code, out) = run(&reg, &["preview-stop-all"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("stopped 1 preview server(s)."), "{out}");

    // Empty stop-all reports zero and exits 0.
    let (code, out) = run(&reg, &["preview-stop-all"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("stopped 0 preview server(s)."), "{out}");
    let (code, out) = run(&reg, &["preview-list"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("no preview servers"), "{out}");
}

#[test]
fn broken_project_refuses_before_any_server_starts() {
    let root = tmp("refusal");
    let reg = root.join("servers.json");
    let _guard = Guard::new(reg.clone());
    let bad = root.join("broken");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::write(bad.join("composition.kdl"), "composition {{{ not kdl\n").unwrap();

    let (code, out) = run(&reg, &["preview", bad.to_str().unwrap()]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("preview refused"), "{out}");
    assert!(out.contains("composition.kdl"), "{out}");
    assert!(out.contains("fix"), "{out}");
    assert!(out.contains("No preview was started"), "{out}");

    // Nothing was recorded: status and list both report none.
    let (code, out) = run(&reg, &["preview-status", bad.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("no preview server"), "{out}");
    let (code, out) = run(&reg, &["preview-list"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("no preview servers"), "{out}");

    // A missing directory refuses the same way.
    let (code, out) = run(&reg, &["preview", root.join("absent").to_str().unwrap()]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("no project directory"), "{out}");
    assert!(out.contains("No preview was started"), "{out}");
}

#[test]
fn preview_arg_errors_exit_2() {
    let root = tmp("args");
    let reg = root.join("servers.json");
    let _guard = Guard::new(reg.clone());
    let p = root.join("proj");
    std::fs::create_dir_all(&p).unwrap();
    text_project(&p, "t");

    let (code, _) = run(&reg, &["preview"]);
    assert_eq!(code, 2);
    let (code, _) = run(&reg, &["preview", "--port", "abc", p.to_str().unwrap()]);
    assert_eq!(code, 2);
    let (code, _) = run(&reg, &["preview-stop"]);
    assert_eq!(code, 2);
    let (code, _) = run(&reg, &["preview-list", "extra"]);
    assert_eq!(code, 2);
    let (code, _) = run(&reg, &["preview-stop-all", "extra"]);
    assert_eq!(code, 2);
}

/// Read one numeric registry field (`"port"` / `"pid"`) for a directory.
fn registry_number(reg: &Path, dir: &str, key: &str) -> Option<u64> {
    let body = std::fs::read_to_string(reg).ok()?;
    let at = body.find(dir)?;
    let window = &body[at..];
    let key = format!("\"{key}\":");
    let k = window.find(&key)?;
    let rest = &window[k + key.len()..];
    let end = rest.find(|c: char| !c.is_ascii_digit())?;
    rest[..end].parse().ok()
}

#[test]
#[cfg(unix)]
fn killed_server_reports_already_stopped() {
    let root = tmp("dead");
    let reg = root.join("servers.json");
    let _guard = Guard::new(reg.clone());
    let p = root.join("proj");
    std::fs::create_dir_all(&p).unwrap();
    text_project(&p, "t");

    let (code, out) = run(&reg, &["preview", p.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    let dir = p.canonicalize().unwrap().to_string_lossy().into_owned();
    let port = registry_number(&reg, &dir, "port").expect("registry records the port");
    let pid = registry_number(&reg, &dir, "pid").expect("registry records the pid");

    // Kill the daemon behind the registry's back: the server is dead.
    let status = Command::new("kill")
        .arg("-9")
        .arg(pid.to_string())
        .status()
        .expect("kill runs");
    assert!(status.success());
    let start = Instant::now();
    while TcpStream::connect(format!("127.0.0.1:{port}")).is_ok() {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "the killed port goes quiet"
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    let (code, out) = run(&reg, &["preview-stop", p.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("already stopped"), "{out}");
    let (code, out) = run(&reg, &["preview-status", p.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("no preview server"), "{out}");
}
