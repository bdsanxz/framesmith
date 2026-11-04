//! Daemon server bookkeeping (Sprint 14, T006 / CAP-006, J-03, J-04,
//! STATE-01, INT-002, REC-001, REC-003).
//!
//! The `framesmith preview` family manages one background preview
//! server per project directory: `preview` starts (background mode,
//! free port, printed URL), `preview-status` names the project and
//! port or reports none, `preview-stop` ends exactly that server,
//! `preview-list` shows every live server, and `preview-stop-all`
//! ends all and reports the count. Starting a second server for the
//! same directory prints the existing URL instead of duplicating; a
//! busy preferred port falls forward to the next free one with the
//! new URL printed; stopping a dead server reports already-stopped;
//! an empty stop-all reports zero and exits 0; a broken project
//! refuses before any server starts, naming the cause.
//!
//! The registry is a small JSON file listing every server this
//! machine started. Liveness is a loopback TCP connect to the
//! recorded port: an entry whose port refuses the connection is
//! dead, so `stop` and `stop-all` report it as already stopped and
//! prune it instead of claiming a kill. The serving itself lives in
//! [`crate::server`]; this module owns validation, port choice,
//! registry reads/writes, liveness, and the graceful-shutdown probe.

use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Environment override for the registry file path (Sprint 14, T006):
/// tests point here at a per-test temp file so parallel suites never
/// share daemon state.
pub const REGISTRY_ENV: &str = "FRAMESMITH_PREVIEW_REGISTRY";

/// Default registry file name under the OS temp dir.
pub const REGISTRY_FILE: &str = "framesmith-preview-servers.json";

/// Preview servers bind loopback only; no remote access is implied.
pub const LOOPBACK: &str = "127.0.0.1";

/// How far a busy preferred port falls forward before giving up.
pub const MAX_PORT_FORWARD: u32 = 200;

/// How long one liveness probe may take.
pub const LIVE_PROBE_TIMEOUT: Duration = Duration::from_millis(200);

/// How long `stop` waits for a server to close after asking it to.
pub const SHUTDOWN_WAIT: Duration = Duration::from_secs(3);

/// Loopback URL for one port.
pub fn url_for_port(port: u16) -> String {
    format!("http://{LOOPBACK}:{port}/")
}

/// Registry file path: the `FRAMESMITH_PREVIEW_REGISTRY` override
/// when set and non-empty, else the shared default under temp.
pub fn registry_path() -> PathBuf {
    match std::env::var_os(REGISTRY_ENV) {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => std::env::temp_dir().join(REGISTRY_FILE),
    }
}

/// One recorded preview server (Sprint 14, T006).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerEntry {
    /// Canonical project directory the server previews.
    pub dir: String,
    /// Project display name (the folder name).
    pub name: String,
    /// Previewed composition name.
    pub composition: String,
    /// Loopback port the server holds.
    pub port: u16,
    /// OS pid of the serving process (best-effort kill fallback).
    pub pid: u32,
    /// Printed loopback URL.
    pub url: String,
}

impl ServerEntry {
    pub fn new(
        dir: String,
        name: String,
        composition: String,
        port: u16,
        pid: u32,
    ) -> Self {
        let url = url_for_port(port);
        Self {
            dir,
            name,
            composition,
            port,
            pid,
            url,
        }
    }
}

/// Escape one JSON string value (quotes, backslashes, controls).
fn json_escape_into(out: &mut String, value: &str) {
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
}

/// Render the registry file body for these entries.
pub fn render_registry(entries: &[ServerEntry]) -> String {
    let mut out = String::from("[");
    for (i, e) in entries.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"dir\":\"");
        json_escape_into(&mut out, &e.dir);
        out.push_str("\",\"name\":\"");
        json_escape_into(&mut out, &e.name);
        out.push_str("\",\"composition\":\"");
        json_escape_into(&mut out, &e.composition);
        out.push_str(&format!(
            "\",\"port\":{},\"pid\":{},\"url\":\"",
            e.port, e.pid
        ));
        json_escape_into(&mut out, &e.url);
        out.push_str("\"}");
    }
    out.push(']');
    out
}

/// Parse one JSON string starting at `bytes[i]` (which must be `"`).
/// Returns the value and the index just past the closing quote.
fn parse_json_string(bytes: &[u8], mut i: usize) -> Result<(String, usize), String> {
    debug_assert_eq!(bytes.get(i), Some(&b'"'));
    i += 1;
    let mut out = String::new();
    loop {
        let c = *bytes.get(i).ok_or("registry: unterminated string")?;
        i += 1;
        match c {
            b'"' => return Ok((out, i)),
            b'\\' => {
                let e = *bytes.get(i).ok_or("registry: dangling escape")?;
                i += 1;
                match e {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        if i + 4 > bytes.len() {
                            return Err("registry: short \\u escape".to_string());
                        }
                        let hex = std::str::from_utf8(&bytes[i..i + 4])
                            .map_err(|_| "registry: bad \\u escape".to_string())?;
                        let n = u32::from_str_radix(hex, 16)
                            .map_err(|_| "registry: bad \\u escape".to_string())?;
                        i += 4;
                        out.push(char::from_u32(n).ok_or("registry: bad codepoint")?);
                    }
                    _ => return Err("registry: bad escape".to_string()),
                }
            }
            _ => {
                // `c` starts one UTF-8 code point; decode it whole so
                // non-ASCII paths round-trip.
                let s = std::str::from_utf8(&bytes[i - 1..])
                    .map_err(|_| "registry: bad utf-8".to_string())?;
                let ch = s.chars().next().ok_or("registry: bad utf-8")?;
                out.push(ch);
                i += ch.len_utf8() - 1;
            }
        }
    }
}

/// Skip JSON whitespace at `i`.
fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while matches!(bytes.get(i), Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')) {
        i += 1;
    }
    i
}

/// Parse one JSON number (unsigned) at `i`.
fn parse_json_number(bytes: &[u8], mut i: usize) -> Result<(u64, usize), String> {
    let start = i;
    while matches!(bytes.get(i), Some(b'0'..=b'9')) {
        i += 1;
    }
    if start == i {
        return Err("registry: expected a number".to_string());
    }
    std::str::from_utf8(&bytes[start..i])
        .map_err(|_| "registry: bad number".to_string())?
        .parse::<u64>()
        .map(|n| (n, i))
        .map_err(|_| "registry: bad number".to_string())
}

/// Parse the registry file body into entries. A missing body parses
/// as empty; anything malformed errors naming the file.
pub fn parse_registry(body: &str) -> Result<Vec<ServerEntry>, String> {
    let bytes = body.as_bytes();
    let mut i = skip_ws(bytes, 0);
    if i >= bytes.len() {
        return Ok(Vec::new());
    }
    if bytes[i] != b'[' {
        return Err("registry: expected an array".to_string());
    }
    i = skip_ws(bytes, i + 1);
    let mut entries = Vec::new();
    if bytes.get(i) == Some(&b']') {
        return Ok(entries);
    }
    loop {
        i = skip_ws(bytes, i);
        if bytes.get(i) != Some(&b'{') {
            return Err("registry: expected an object".to_string());
        }
        i = skip_ws(bytes, i + 1);
        let (mut dir, mut name, mut composition, mut url) = (
            None::<String>,
            None::<String>,
            None::<String>,
            None::<String>,
        );
        let (mut port, mut pid) = (None::<u16>, None::<u32>);
        loop {
            i = skip_ws(bytes, i);
            if bytes.get(i) != Some(&b'"') {
                return Err("registry: expected a key".to_string());
            }
            let (key, j) = parse_json_string(bytes, i)?;
            i = skip_ws(bytes, j);
            if bytes.get(i) != Some(&b':') {
                return Err("registry: expected a colon".to_string());
            }
            i = skip_ws(bytes, i + 1);
            match key.as_str() {
                k @ ("dir" | "name" | "composition" | "url") => {
                    if bytes.get(i) != Some(&b'"') {
                        return Err(format!("registry: expected a string for {k:?}"));
                    }
                    let (v, j) = parse_json_string(bytes, i)?;
                    match k {
                        "dir" => dir = Some(v),
                        "name" => name = Some(v),
                        "composition" => composition = Some(v),
                        _ => url = Some(v),
                    }
                    i = j;
                }
                k @ ("port" | "pid") => {
                    let (n, j) = parse_json_number(bytes, i)?;
                    if k == "port" {
                        port = Some(u16::try_from(n).map_err(|_| "registry: bad port")?);
                    } else {
                        pid = Some(u32::try_from(n).map_err(|_| "registry: bad pid")?);
                    }
                    i = j;
                }
                _ => return Err(format!("registry: unknown key {key:?}")),
            }
            i = skip_ws(bytes, i);
            match bytes.get(i) {
                Some(b',') => i = skip_ws(bytes, i + 1),
                Some(b'}') => {
                    i += 1;
                    break;
                }
                _ => return Err("registry: expected `,` or `}`".to_string()),
            }
        }
        entries.push(ServerEntry {
            dir: dir.ok_or("registry: entry without `dir`")?,
            name: name.ok_or("registry: entry without `name`")?,
            composition: composition.unwrap_or_default(),
            port: port.ok_or("registry: entry without `port`")?,
            pid: pid.unwrap_or(0),
            url: url.unwrap_or_default(),
        });
        // Fill a missing URL from the port so hand-edited entries
        // still print somewhere useful.
        if entries.last().expect("just pushed").url.is_empty() {
            let port = entries.last().expect("just pushed").port;
            entries.last_mut().expect("just pushed").url = url_for_port(port);
        }
        i = skip_ws(bytes, i);
        match bytes.get(i) {
            Some(b',') => i = skip_ws(bytes, i + 1),
            Some(b']') => return Ok(entries),
            _ => return Err("registry: expected `,` or `]`".to_string()),
        }
    }
}

/// Load the registry file; a missing file is an empty registry, a
/// malformed one errors naming the file.
pub fn load_registry(path: &Path) -> Result<Vec<ServerEntry>, String> {
    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("preview registry {} cannot be read: {e}", path.display())),
        Ok(body) => parse_registry(&body)
            .map_err(|why| format!("preview registry {} is malformed: {why}", path.display())),
    }
}

/// Next per-save tmp-file sequence (process-unique together with the
/// pid, so a stopping parent and its gracefully exiting child can
/// save the registry concurrently without sharing a tmp file).
static SAVE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Save the registry atomically (unique temp file plus rename),
/// creating the parent directory when missing.
pub fn save_registry(path: &Path, entries: &[ServerEntry]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let seq = SAVE_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let tmp = path.with_extension(format!("json.tmp-{}-{seq}", std::process::id()));
    std::fs::write(&tmp, render_registry(entries))?;
    let renamed = std::fs::rename(&tmp, path);
    if renamed.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    renamed?;
    Ok(())
}

/// Is anything answering on this loopback port right now?
pub fn is_live(port: u16) -> bool {
    let addr: SocketAddr = match format!("{LOOPBACK}:{port}").parse() {
        Ok(a) => a,
        Err(_) => return false,
    };
    TcpStream::connect_timeout(&addr, LIVE_PROBE_TIMEOUT).is_ok()
}

/// Drop every dead entry, keeping only live servers.
pub fn live_only(entries: &[ServerEntry]) -> Vec<ServerEntry> {
    entries.iter().filter(|e| is_live(e.port)).cloned().collect()
}

/// Canonical project directory string for registry keys.
pub fn canonical_dir(folder: &Path) -> Result<String, String> {
    folder
        .canonicalize()
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|_| {
            format!(
                "preview refused: {} has no project directory; pass an existing project folder. No preview was started.",
                folder.display()
            )
        })
}

/// Why a preview start refused (Sprint 14, T006 / REC-001): the
/// project is broken, so no server starts and nothing is recorded.
#[derive(Debug, Clone)]
pub struct DaemonRefusal {
    pub message: String,
}

impl std::fmt::Display for DaemonRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// A validated project ready to serve (Sprint 14, T006): validation
/// matches `check` exactly, so a broken project refuses here before
/// any port is bound or any server starts.
#[derive(Debug, Clone)]
pub struct ValidatedProject {
    /// Canonical directory string (the registry key).
    pub dir: String,
    /// Project display name (the folder name).
    pub name: String,
    /// Previewed composition name.
    pub composition: String,
}

/// Validate the project exactly like `check` (Sprint 14, T006 /
/// REC-001): broken projects refuse with file, line, and fix before
/// any server starts.
pub fn validate_project(folder: &Path) -> Result<ValidatedProject, DaemonRefusal> {
    let dir = canonical_dir(folder).map_err(|message| DaemonRefusal { message })?;
    let verdict = validate::validate::validate_folder_with_variables_strict_themed(
        Path::new(&dir),
        &validate::validate::VariableOverrides::new(),
        false,
        None,
    );
    match verdict.status {
        validate::validate::Status::Invalid => {
            let mut message = format!(
                "preview refused: {dir} is invalid; fix the problems `check` reports and re-run. No preview was started."
            );
            for d in &verdict.diagnostics {
                message.push_str(&format!(
                    "\n{}:{}:{} {} {} (fix: {})",
                    d.file, d.line, d.column, d.code, d.message, d.fix
                ));
            }
            return Err(DaemonRefusal { message });
        }
        validate::validate::Status::Unsupported => {
            let names: Vec<String> =
                verdict.diagnostics.iter().map(|d| d.message.clone()).collect();
            return Err(DaemonRefusal {
                message: format!(
                    "preview refused: {dir} uses later-sprint element(s): {}. No preview was started.",
                    names.join("; ")
                ),
            })
        }
        validate::validate::Status::Valid => {}
    }
    let composition = verdict
        .composition
        .as_ref()
        .map(|c| c.name.clone())
        .ok_or_else(|| DaemonRefusal {
            message: format!(
                "preview refused: {dir} validated but names no composition to preview. No preview was started."
            ),
        })?;
    let name = Path::new(&dir)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.clone());
    Ok(ValidatedProject { dir, name, composition })
}

/// Pick a loopback port to serve on (Sprint 14, T006 / REC-003): no
/// preference binds an ephemeral free port; a preferred port that is
/// busy falls forward to the next free one, and the returned port is
/// always free at return time (the caller binds it promptly).
pub fn pick_port(preferred: Option<u16>) -> io::Result<u16> {
    match preferred {
        // `0` (or none) means "any free port".
        None | Some(0) => {
            let listener = std::net::TcpListener::bind((LOOPBACK, 0))?;
            Ok(listener.local_addr()?.port())
        }
        Some(first) => {
            let last = first.saturating_add(MAX_PORT_FORWARD as u16).max(first);
            let mut port = first;
            loop {
                match std::net::TcpListener::bind((LOOPBACK, port)) {
                    Ok(listener) => {
                        let found = listener.local_addr()?.port();
                        return Ok(found);
                    }
                    Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
                        if port >= last || port == 65535 {
                            return Err(io::Error::new(
                                io::ErrorKind::AddrInUse,
                                format!(
                                    "preview refused: ports {first}..={last} are all busy; free one and re-run"
                                ),
                            ));
                        }
                        port += 1;
                    }
                    Err(e) => return Err(e),
                }
            }
        }
    }
}

/// What `preview` should do for one folder (Sprint 14, T006): reuse
/// the live server for the directory, or start fresh on a free port.
#[derive(Debug, Clone)]
pub enum StartPlan {
    /// A live server already previews this directory: print its URL.
    AlreadyRunning(ServerEntry),
    /// No live server: start one on this free port and print its URL.
    ToStart {
        dir: String,
        name: String,
        composition: String,
        port: u16,
        url: String,
    },
}

/// Plan a start: validate first (a broken project refuses before
/// anything starts), prune dead entries, then either reuse the live
/// server for the directory or pick a free port — falling forward
/// past a busy preferred port.
pub fn plan_start(
    registry_path: &Path,
    folder: &Path,
    preferred_port: Option<u16>,
) -> Result<StartPlan, DaemonRefusal> {
    let project = validate_project(folder)?;
    let entries = load_registry(registry_path).map_err(|message| DaemonRefusal { message })?;
    let live: Vec<ServerEntry> = entries.iter().filter(|e| is_live(e.port)).cloned().collect();
    if let Some(existing) = live.iter().find(|e| e.dir == project.dir) {
        return Ok(StartPlan::AlreadyRunning(existing.clone()));
    }
    let port = pick_port(preferred_port).map_err(|e| DaemonRefusal {
        message: format!("{}; no preview was started", previewize_daemon(e.to_string())),
    })?;
    Ok(StartPlan::ToStart {
        url: url_for_port(port),
        dir: project.dir,
        name: project.name,
        composition: project.composition,
        port,
    })
}

/// Restate an OS error as a preview refusal.
fn previewize_daemon(mut e: String) -> String {
    for (from, to) in [
        ("Address already in use", "the port is busy"),
        ("Permission denied", "binding the port was denied"),
    ] {
        e = e.replace(from, to);
    }
    if e.starts_with("preview ") {
        e
    } else {
        format!("preview refused: {e}")
    }
}

/// Record one server, replacing any entry for the same directory.
pub fn add_entry(registry_path: &Path, entry: &ServerEntry) -> Result<(), String> {
    let mut entries = load_registry(registry_path)?;
    entries.retain(|e| e.dir != entry.dir);
    entries.push(entry.clone());
    save_registry(registry_path, &entries)
        .map_err(|e| format!("preview registry {} cannot be written: {e}", registry_path.display()))
}

/// Forget one directory's entry, if present.
pub fn remove_entry(registry_path: &Path, dir: &str) -> Result<(), String> {
    let mut entries = load_registry(registry_path)?;
    let before = entries.len();
    entries.retain(|e| e.dir != dir);
    if entries.len() != before {
        save_registry(registry_path, &entries).map_err(|e| {
            format!("preview registry {} cannot be written: {e}", registry_path.display())
        })?;
    }
    Ok(())
}

/// Every live server, pruning dead entries (Sprint 14, T006): the
/// pruned file is written back best-effort so later commands stop
/// tripping over dead servers.
pub fn list_servers(registry_path: &Path) -> Result<Vec<ServerEntry>, String> {
    let entries = load_registry(registry_path)?;
    let live = live_only(&entries);
    if live.len() != entries.len() {
        let _ = save_registry(registry_path, &live);
    }
    Ok(live)
}

/// Ask a live server to shut itself down through its loopback
/// shutdown route. Returns whether the port has gone quiet.
pub fn request_shutdown(port: u16) -> bool {
    let addr: SocketAddr = match format!("{LOOPBACK}:{port}").parse() {
        Ok(a) => a,
        Err(_) => return false,
    };
    let mut stream = match TcpStream::connect_timeout(&addr, LIVE_PROBE_TIMEOUT) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let _ = stream.set_write_timeout(Some(LIVE_PROBE_TIMEOUT));
    let _ = stream.set_read_timeout(Some(LIVE_PROBE_TIMEOUT));
    let request = format!(
        "POST /api/shutdown HTTP/1.0\r\nHost: {LOOPBACK}:{port}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    if std::io::Write::write_all(&mut stream, request.as_bytes()).is_err() {
        return false;
    }
    // Drain the reply so the server side can close cleanly.
    let mut buf = [0u8; 512];
    use std::io::Read;
    let _ = stream.read(&mut buf);
    wait_closed(port, SHUTDOWN_WAIT)
}

/// Wait until nothing answers on this port, up to `timeout`.
pub fn wait_closed(port: u16, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if !is_live(port) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    !is_live(port)
}

/// Best-effort OS kill of one pid (Unix only): the graceful shutdown
/// route is the primary stop path; this only hurries a server that
/// answered liveness but ignored the route. Never fails the stop.
#[cfg(unix)]
fn kill_pid(pid: u32) {
    let _ = std::process::Command::new("kill")
        .arg(pid.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// What `preview-stop` did for one folder (Sprint 14, T006): ended
/// exactly that server, or reported it as already stopped when the
/// entry is missing or dead.
#[derive(Debug, Clone)]
pub enum StopOutcome {
    Stopped(ServerEntry),
    AlreadyStopped { dir: String },
}

/// Stop one directory's server (Sprint 14, T006 / REC-003): a live
/// server is asked to shut down and forgotten; a missing or dead
/// entry reports already-stopped and is pruned.
pub fn stop_server(registry_path: &Path, dir: &str) -> Result<StopOutcome, String> {
    let entries = load_registry(registry_path)?;
    let hit = entries.iter().find(|e| e.dir == dir).cloned();
    let Some(entry) = hit else {
        return Ok(StopOutcome::AlreadyStopped { dir: dir.to_string() });
    };
    if !is_live(entry.port) {
        let _ = remove_entry(registry_path, dir);
        return Ok(StopOutcome::AlreadyStopped { dir: dir.to_string() });
    }
    request_shutdown(entry.port);
    if is_live(entry.port) {
        #[cfg(unix)]
        kill_pid(entry.pid);
        #[cfg(unix)]
        let _ = wait_closed(entry.port, SHUTDOWN_WAIT);
    }
    if is_live(entry.port) {
        return Err(format!(
            "preview stop failed: {} at {} (port {}) is still answering; stop it manually and re-run",
            entry.name, entry.url, entry.port
        ));
    }
    let _ = remove_entry(registry_path, dir);
    Ok(StopOutcome::Stopped(entry))
}

/// What `preview-stop-all` ended (Sprint 14, T006): every live server
/// plus the dead entries it pruned along the way.
#[derive(Debug, Clone, Default)]
pub struct StopAllOutcome {
    pub stopped: Vec<ServerEntry>,
    pub already: Vec<String>,
}

/// Stop every recorded server (Sprint 14, T006): live servers are
/// asked to shut down, dead entries prune as already-stopped, and an
/// empty registry stops zero and still succeeds.
pub fn stop_all_servers(registry_path: &Path) -> Result<StopAllOutcome, String> {
    let entries = load_registry(registry_path)?;
    let mut outcome = StopAllOutcome::default();
    let mut failed: Vec<ServerEntry> = Vec::new();
    for entry in &entries {
        if !is_live(entry.port) {
            outcome.already.push(entry.dir.clone());
            continue;
        }
        request_shutdown(entry.port);
        if is_live(entry.port) {
            #[cfg(unix)]
            kill_pid(entry.pid);
            #[cfg(unix)]
            let _ = wait_closed(entry.port, SHUTDOWN_WAIT);
        }
        if is_live(entry.port) {
            failed.push(entry.clone());
        } else {
            outcome.stopped.push(entry.clone());
        }
    }
    save_registry(registry_path, &failed)
    .map_err(|e| format!("preview registry {} cannot be written: {e}", registry_path.display()))?;
    if let Some(first) = failed.first() {
        return Err(format!(
            "preview stop-all stopped {} server(s) but {} at {} (port {}) is still answering; stop it manually and re-run",
            outcome.stopped.len(),
            first.name,
            first.url,
            first.port
        ));
    }
    Ok(outcome)
}

/// Human report lines shared by the CLI and the tests, so every
/// lifecycle command reports exactly what changed (Sprint 14, T006 /
/// INT-002).
pub fn msg_started(entry: &ServerEntry) -> String {
    format!(
        "preview serving {} (composition {:?}) at {} (port {}).",
        entry.name, entry.composition, entry.url, entry.port
    )
}

pub fn msg_existing(entry: &ServerEntry) -> String {
    format!(
        "preview already serving {} (composition {:?}) at {} (port {}); using the existing server.",
        entry.name, entry.composition, entry.url, entry.port
    )
}

pub fn msg_status_some(entry: &ServerEntry) -> String {
    format!(
        "preview status: project {} (composition {:?}) serving at {} (port {}).",
        entry.name, entry.composition, entry.url, entry.port
    )
}

pub fn msg_status_none(dir: &str) -> String {
    format!("preview status: no preview server for {dir}.")
}

pub fn msg_stopped(entry: &ServerEntry) -> String {
    format!(
        "preview stopped {} at {} (port {}).",
        entry.name, entry.url, entry.port
    )
}

pub fn msg_already_stopped(dir: &str) -> String {
    format!("preview already stopped for {dir}; no server was running.")
}

pub fn msg_list(entries: &[ServerEntry]) -> String {
    if entries.is_empty() {
        return "no preview servers running.".to_string();
    }
    let mut out = format!("preview servers ({}):", entries.len());
    for e in entries {
        out.push_str(&format!(
            "\nproject {} (composition {:?}) at {} (port {})",
            e.name, e.composition, e.url, e.port
        ));
    }
    out
}

pub fn msg_stop_all(outcome: &StopAllOutcome) -> String {
    format!("stopped {} preview server(s).", outcome.stopped.len())
}

/// Readiness probe for a freshly spawned server: the registry entry
/// for this directory on this port whose port answers.
pub fn find_ready(registry_path: &Path, dir: &str, port: u16) -> Option<ServerEntry> {
    let entries = load_registry(registry_path).ok()?;
    entries
        .into_iter()
        .find(|e| e.dir == dir && e.port == port && is_live(e.port))
}

/// Any live entry for this directory, whatever its port.
pub fn find_live_for(registry_path: &Path, dir: &str) -> Option<ServerEntry> {
    let entries = load_registry(registry_path).ok()?;
    entries.into_iter().find(|e| e.dir == dir && is_live(e.port))
}

/// Bind a loopback listener, used by tests to hold ports live or
/// busy without running a server.
#[doc(hidden)]
pub fn test_bind(port: u16) -> io::Result<TcpListener> {
    TcpListener::bind((LOOPBACK, port))
}
