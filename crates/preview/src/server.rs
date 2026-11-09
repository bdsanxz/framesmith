//! Loopback preview server (Sprint 14, T006 / CAP-001..006, J-01,
//! STATE-02..04, REC-002).
//!
//! A `preview-serve` child process validates and opens one
//! [`PreviewSession`](crate::PreviewSession) — a broken project
//! refuses with file, line, and fix before any port is bound — then
//! binds its loopback port, records itself in the daemon registry,
//! and serves the composition until `/api/shutdown` (or a signal)
//! ends it:
//!
//! * `GET /` — the review page: true frames, transport controls with
//!   the time/frame readout, the render mix as the playback clock,
//!   and the measured latency medians plus the last reload report.
//! * `GET /api/frame?i=N&action=step|scrub|jump` — frame N as exact
//!   PNG bytes through the same pipeline as a full render, timed
//!   into the session latency record under that action.
//! * `GET /api/mix.wav` — the render mix as WAV bytes the page audio
//!   element plays; a soundless project answers 204 (silent video,
//!   no error, no fake track); an undecodable mix names its cause.
//! * `GET /api/status` — project, composition, port, readout,
//!   latency medians, and the last reload report as JSON.
//! * `POST /api/play|pause|scrub|step|loop|speed|sync` — transport
//!   controls over the server-owned playhead, answered with the
//!   readout; play checks the mix first, so an undecodable mix
//!   blocks play naming the cause and the playhead never moves.
//! * `POST /api/reload` — hot-reload the folder now, keeping the
//!   playhead; a broken edit lists its diagnostics while last good
//!   frames keep showing.
//! * `POST /api/shutdown` — graceful stop for `preview-stop` and
//!   `preview-stop-all`.
//!
//! Saving a project file also reloads automatically: before serving
//! a frame or status the server compares the composition file mtime
//! against the last reload and re-prepares when it moved, so edits
//! appear hot with the playhead kept. Everything binds 127.0.0.1
//! only. The server is single-threaded and owns its session: local
//! preview needs no concurrency, and serialization keeps the cache
//! and the playhead exact.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::daemon;
use crate::{PreviewOptions, PreviewSession, TransportAction};

/// Exit code when the chosen port is already bound (Sprint 14, T006 /
/// REC-003): the parent `preview` command falls forward to the next
/// free port and retries instead of failing.
pub const PORT_BUSY_EXIT: i32 = 3;

/// What to serve (Sprint 14, T006).
#[derive(Debug, Clone)]
pub struct ServeOptions {
    pub folder: PathBuf,
    pub port: u16,
    pub registry: PathBuf,
    pub composition: Option<String>,
}

/// Bind one loopback port exactly (Sprint 14, T006): no fall-forward
/// here — the parent picked a free port, and a race shows up as
/// `AddrInUse` so the parent can fall forward and retry.
pub fn bind_exact(port: u16) -> std::io::Result<TcpListener> {
    TcpListener::bind((daemon::LOOPBACK, port))
}

/// Run a serving child to completion (Sprint 14, T006): open the
/// session first (a broken project refuses before any server starts,
/// exit 1), bind the port (busy exits [`PORT_BUSY_EXIT`] for the
/// parent's fall-forward), record the registry entry, serve until
/// shutdown, forget the entry, exit 0.
pub fn run_serve(opts: &ServeOptions) -> i32 {
    // Canonicalize first so the registry key, the session folder, and
    // the status reports all name the same directory.
    let folder = match daemon::canonical_dir(&opts.folder) {
        Ok(d) => PathBuf::from(d),
        Err(message) => {
            eprintln!("{message}");
            return 1;
        }
    };
    let session = match PreviewSession::open(
        &folder,
        PreviewOptions {
            composition: opts.composition.clone(),
            ..PreviewOptions::default()
        },
    ) {
        Ok(s) => s,
        Err(refusal) => {
            eprintln!("{refusal}");
            for d in &refusal.diagnostics {
                eprintln!(
                    "{}:{}:{} {} {} (fix: {})",
                    d.file, d.line, d.column, d.code, d.message, d.fix
                );
            }
            return 1;
        }
    };
    let listener = match bind_exact(opts.port) {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            eprintln!(
                "preview serve refused: port {} is busy; the parent falls forward to a free port",
                opts.port
            );
            return PORT_BUSY_EXIT;
        }
        Err(e) => {
            eprintln!("preview serve failed: cannot bind port {}: {e}", opts.port);
            return 1;
        }
    };
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(opts.port);
    let dir = folder.to_string_lossy().into_owned();
    let name = Path::new(&dir)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.clone());
    let entry = daemon::ServerEntry::new(
        dir,
        name,
        session.composition_name().to_string(),
        port,
        std::process::id(),
    );
    if let Err(message) = daemon::add_entry(&opts.registry, &entry) {
        eprintln!("{message}");
        return 1;
    }
    eprintln!("preview serving {} at {}", entry.name, entry.url);
    let mut state = ServerState::new(session, folder, port);
    serve(listener, &mut state);
    let _ = daemon::remove_entry(&opts.registry, &state.dir_key);
    0
}

/// Mutable serving state: the session, the page playhead, and the
/// reload watch.
struct ServerState {
    session: PreviewSession,
    transport: crate::Transport,
    folder: PathBuf,
    dir_key: String,
    port: u16,
    kdl_disk: crate::studio::DiskVersion,
    conflict: Option<crate::studio::StudioConflict>,
    last_reload: Option<String>,
    shutdown: Arc<AtomicBool>,
    /// Studio autosave queue (Sprint 16, T002): while paused, studio
    /// saves stage here with nothing applied; resuming drains every
    /// staged edit through the SDK patch and history machinery.
    save_queue: crate::studio::SaveQueue,
    /// Canvas transport extras (Sprint 17, T005): shuttle memory, the
    /// sub-frame carry, and the still-picture watch. Pure playback
    /// state layered over the serving-session playhead.
    studio_transport_live: crate::studio_transport::StudioTransportLive,
}

impl ServerState {
    fn new(session: PreviewSession, folder: PathBuf, port: u16) -> Self {
        let mut transport = session.transport();
        let dir_key = session.project_folder().to_string_lossy().into_owned();
        let kdl_disk = crate::studio::disk_version_of(&folder);
        // Canvas transport extras (Sprint 17, T005): restore the
        // persisted menu speed onto the fresh playhead; a refused
        // speed (for example on an empty timeline) keeps 1x.
        let saved = crate::studio_transport::load_transport(&folder);
        let _ = transport.set_speed(saved.settings.speed);
        if saved.settings.loop_enabled {
            transport.set_loop(true);
        }
        let live = crate::studio_transport::StudioTransportLive::new(
            transport.position(),
            std::time::Instant::now(),
        );
        let shutdown = Arc::new(AtomicBool::new(false));
        // Renders queue worker (Sprint 18, T006): one background
        // thread per server running queued jobs in order while the
        // handlers answer reads; it stops on shutdown.
        crate::studio_renders::spawn_queue_worker(folder.clone(), Arc::clone(&shutdown));
        Self {
            session,
            transport,
            folder,
            dir_key,
            port,
            kdl_disk,
            conflict: None,
            last_reload: None,
            shutdown,
            save_queue: crate::studio::SaveQueue::default(),
            studio_transport_live: live,
        }
    }

    /// Re-prepare when the composition file moved since the last
    /// check (Sprint 14, T005; Sprint 16, T003): valid edits apply
    /// with the playhead kept, broken edits list diagnostics while
    /// last good frames keep showing. Runs before frames and status
    /// so edits appear hot without a manual reload call. With no
    /// unsaved work held the disk version reloads automatically; with
    /// staged edits held the change banners as a conflict instead —
    /// never an automatic merge, never a silent win.
    fn maybe_reload(&mut self) {
        if !crate::studio::project_reachable(&self.folder) {
            // REC-003: a moved or deleted project stays read-only with
            // every staged edit kept — never reload, never banner, and
            // never move the known fingerprint, so restoring the folder
            // resumes exactly where the shell left off.
            return;
        }
        if self.save_queue.staged.is_empty() {
            if crate::studio::disk_stat_sig(&self.folder) == self.kdl_disk.stat_sig() {
                return;
            }
            let now = crate::studio::disk_version_of(&self.folder);
            if now.hash == self.kdl_disk.hash && now.present == self.kdl_disk.present {
                // Same bytes under a new stamp (a touch, not an edit):
                // note the stamp, reload nothing, banner nothing.
                self.kdl_disk = now;
                return;
            }
            let report = self.session.reload_keep_playhead(&mut self.transport);
            self.last_reload = Some(report.to_string());
            self.kdl_disk = now;
            self.conflict = None;
            return;
        }
        // Unsaved staged work is held: never auto-reload. Full
        // fingerprint (same-tick rewrites count) so the banner names
        // the true disk version.
        let now = crate::studio::disk_version_of(&self.folder);
        if now.hash == self.kdl_disk.hash && now.present == self.kdl_disk.present {
            // The known bytes are back under the staged work (a touch,
            // or the disk restored after a scare): note the stamp, clear
            // any banner, reload nothing — the pre-change view is
            // current again.
            self.kdl_disk = now;
            self.conflict = None;
            return;
        }
        if self
            .conflict
            .as_ref()
            .is_some_and(|pending| pending.disk == now)
        {
            return;
        }
        self.conflict = Some(crate::studio::StudioConflict { disk: now });
    }
}

/// Refusal naming the pending external change (Sprint 16, T003 /
/// STATE-04): editing pauses until the maker's pick lands, so the
/// disk version never wins silently over staged work. Callers check
/// `state.conflict.is_some()` first.
fn conflict_blocker_message(staged: usize) -> String {
    format!(
        "studio: an external change is waiting for your pick: composition.kdl changed on disk while {staged} staged edit(s) wait. Resolve it at POST /api/studio/conflict with {{\"pick\":\"disk\"}} or {{\"pick\":\"unsaved\"}} before editing. Nothing was applied or staged."
    )
}

/// Accept connections until `/api/shutdown` flips the flag.
fn serve(listener: TcpListener, state: &mut ServerState) {
    let shutdown = Arc::clone(&state.shutdown);
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        handle_one(stream, state);
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
    }
}

/// One HTTP exchange: read the head plus any body, route, answer,
/// close. Bodies arrive only on layout saves; every other route keeps
/// its query-only contract.
fn handle_one(stream: TcpStream, state: &mut ServerState) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let Some((method, target, body)) = read_request(&stream) else {
        return;
    };
    // Hot paths watch the file; the shutdown route must answer even
    // mid-edit, so it skips the reload check.
    if !(method == "POST" && strip_query(&target).0 == "/api/shutdown") {
        state.maybe_reload();
    }
    route(stream, state, &method, &target, &body);
}

fn strip_query(target: &str) -> (&str, &str) {
    match target.split_once('?') {
        Some((p, q)) => (p, q),
        None => (target, ""),
    }
}

/// Read the request head (`METHOD TARGET HTTP/1.x` plus headers) and,
/// when `Content-Length` names one, the body. Returns the method, the
/// target, and the body bytes as text.
fn read_request(stream: &TcpStream) -> Option<(String, String, String)> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    // `&TcpStream` shares the socket: reads through the reference
    // land in the same socket the reply later writes to.
    let mut stream = stream;
    loop {
        let n = match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => return None,
        };
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > 65536 {
            return None;
        }
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let end = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&buf[..end]).ok()?;
    let mut lines = head.lines();
    let line = lines.next()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let mut length = 0usize;
    for header in lines {
        if let Some(value) = header.strip_prefix("Content-Length:") {
            length = value.trim().parse().unwrap_or(0);
        } else if let Some(value) = header.strip_prefix("content-length:") {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    length = length.min(1 << 20);
    let mut body = buf[end + 4..].to_vec();
    while body.len() < length {
        let n = match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);
    let body = String::from_utf8_lossy(&body).into_owned();
    Some((method, target, body))
}

/// Decode one query string into pairs (`+` is a space, `%XX`
/// unescapes; malformed escapes stay literal).
fn query_pairs(query: &str) -> Vec<(String, String)> {
    fn decode(s: &str) -> String {
        let mut out = String::new();
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'+' => {
                    out.push(' ');
                    i += 1;
                }
                b'%' if i + 2 < bytes.len() => match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    Ok(b) => {
                        out.push(b as char);
                        i += 3;
                    }
                    Err(_) => {
                        out.push('%');
                        i += 1;
                    }
                },
                _ => {
                    out.push(bytes[i] as char);
                    i += 1;
                }
            }
        }
        out
    }
    query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, v)) => (decode(k), decode(v)),
            None => (decode(p), String::new()),
        })
        .collect()
}

/// Query pairs for sibling modules (Sprint 16, T001): the Studio shell
/// parses deep links from the same decoding the frame routes use.
pub(crate) fn query_pairs_pub(query: &str) -> Vec<(String, String)> {
    query_pairs(query)
}

fn query_get(query: &str, key: &str) -> Option<String> {
    query_pairs(query)
        .into_iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
}

/// One Studio composition lookup (Sprint 16, T007): the serving
/// session copy by name, or the session composition when unnamed.
fn find_studio_composition(
    state: &ServerState,
    want: Option<&str>,
) -> Option<notation::model::Composition> {
    match want {
        None => Some(state.session.composition().clone()),
        Some(name) => state
            .session
            .all_compositions()
            .iter()
            .find(|c| c.name == name)
            .cloned(),
    }
}

/// Composition behind the Studio readouts (Sprint 16, T003): while a
/// conflict pends, the frozen pre-change session copy shows — the disk
/// version must not leak into rows, lanes, or visuals before the
/// maker's pick. Otherwise the SDK effective composition with the
/// persisted delta replayed, exactly as before.
fn readout_composition(
    state: &ServerState,
    base: &notation::model::Composition,
    want: Option<&str>,
) -> notation::model::Composition {
    if state.conflict.is_some() {
        return base.clone();
    }
    crate::studio::effective_composition_for(&state.folder, base, want)
}

/// The unknown-composition refusal shared by the Studio readouts
/// (Sprint 16, T007): names the project holdings, like the timeline
/// route.
fn unknown_composition(state: &ServerState, want: Option<String>) -> String {
    let available: Vec<String> = state
        .session
        .all_compositions()
        .iter()
        .map(|c| c.name.clone())
        .collect();
    format!(
        "studio: the deep link names unknown composition {:?}: the project holds {}; showing the project root {:?} instead.",
        want.unwrap_or_default(),
        available
            .iter()
            .map(|n| format!("{n:?}"))
            .collect::<Vec<_>>()
            .join(", "),
        state.session.composition_name()
    )
}

fn json_escape(value: &str) -> String {
    let mut out = String::new();
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
    out
}

fn respond(mut stream: TcpStream, status: &str, content_type: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.0 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

fn respond_text(stream: TcpStream, status: &str, body: &str) {
    respond(stream, status, "text/plain; charset=utf-8", body.as_bytes());
}

fn respond_json(stream: TcpStream, status: &str, body: &str) {
    respond(stream, status, "application/json", body.as_bytes());
}

fn route(stream: TcpStream, state: &mut ServerState, method: &str, target: &str, body: &str) {
    let (path, query) = strip_query(target);
    match (method, path) {
        ("GET", "/") => {
            let page = render_page(state);
            respond(
                stream,
                "200 OK",
                "text/html; charset=utf-8",
                page.as_bytes(),
            );
        }
        // Embeddable player (Sprint 15, T005): the same frames and mix
        // behind a player page implementing the method/event surface.
        // Embed attributes arrive hyphenated in the query string
        // (`?autoplay=1&click-to-play=0&no-load-timeout=1`); with no
        // height given the film shows at its own shape.
        ("GET", "/player") => {
            let attrs = crate::player::PlayerAttributes::from_query(query);
            let page = render_player(state, &attrs);
            respond(
                stream,
                "200 OK",
                "text/html; charset=utf-8",
                page.as_bytes(),
            );
        }
        // Host embed snippet (Sprint 15, T005 / INT-001): an iframe
        // pointing at `/player` with the hyphenated attributes.
        ("GET", "/embed") => {
            let attrs = crate::player::PlayerAttributes::from_query(query);
            let port = state.port;
            let snippet = crate::player::embed_snippet(
                &format!("http://{}:{port}/player", daemon::LOOPBACK),
                state.session.composition_name(),
                &attrs,
                state.session.canvas().0,
                state.session.canvas().1,
                None,
            );
            respond(
                stream,
                "200 OK",
                "text/html; charset=utf-8",
                snippet.as_bytes(),
            );
        }
        ("GET", "/api/status") => {
            let body = render_status(state);
            respond_json(stream, "200 OK", &body);
        }
        ("GET", "/api/frame") => {
            let index = match query_get(query, "i").map(|v| v.parse::<u64>()) {
                Some(Ok(i)) => i,
                _ => {
                    respond_text(
                        stream,
                        "400 Bad Request",
                        "preview refused: /api/frame needs `?i=N` (a frame index); pass the frame to show",
                    );
                    return;
                }
            };
            let action = match query_get(query, "action").as_deref() {
                Some("scrub") => TransportAction::Scrub,
                Some("jump") => TransportAction::Jump,
                _ => TransportAction::Step,
            };
            match state.session.frame_png_timed(action, index) {
                Ok((frame, _)) => {
                    respond(stream, "200 OK", "image/png", &frame.png);
                }
                Err(e) => {
                    let status = match e.kind {
                        crate::FrameErrorKind::OutOfRange => "404 Not Found",
                        crate::FrameErrorKind::RenderFailed => "500 Internal Server Error",
                    };
                    respond_text(stream, status, &e.to_string());
                }
            }
        }
        ("GET", "/api/mix.wav") => match state.session.mix_wav() {
            Ok(None) => respond(stream, "204 No Content", "audio/wav", b""),
            Ok(Some(wav)) => respond(stream, "200 OK", "audio/wav", &wav),
            Err(e) => respond_text(stream, "502 Bad Gateway", &e.to_string()),
        },
        ("POST", "/api/play") => match state.session.play_with_audio(&mut state.transport) {
            Ok(play) => {
                let audio = match play {
                    crate::AudioPlay::Silent => "silent",
                    crate::AudioPlay::Sound(_) => "sound",
                };
                respond_json(
                    stream,
                    "200 OK",
                    &format!("{{\"audio\":\"{audio}\",{}}}", readout_json(state)),
                );
            }
            Err(e) => respond_text(stream, "409 Conflict", &e.to_string()),
        },
        ("POST", "/api/pause") => {
            state.transport.pause();
            respond_json(stream, "200 OK", &format!("{{{}}}", readout_json(state)));
        }
        ("POST", "/api/scrub") => {
            let index = match query_get(query, "i").map(|v| v.parse::<u64>()) {
                Some(Ok(i)) => i,
                _ => {
                    respond_text(
                        stream,
                        "400 Bad Request",
                        "preview refused: /api/scrub needs `?i=N` (a frame index); pass the frame to show",
                    );
                    return;
                }
            };
            match state.transport.scrub_to(index) {
                Ok(()) => respond_json(stream, "200 OK", &format!("{{{}}}", readout_json(state))),
                Err(e) => respond_text(stream, "409 Conflict", &e.to_string()),
            }
        }
        ("POST", "/api/step") => {
            let delta: i64 = query_get(query, "d")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1);
            let result = if delta < 0 {
                state.transport.step_back()
            } else {
                state.transport.step_forward()
            };
            match result {
                Ok(()) => respond_json(stream, "200 OK", &format!("{{{}}}", readout_json(state))),
                Err(e) => respond_text(stream, "409 Conflict", &e.to_string()),
            }
        }
        ("POST", "/api/loop") => {
            let on = query_get(query, "on").as_deref() != Some("0")
                && query_get(query, "on").as_deref() != Some("off");
            state.transport.set_loop(on);
            respond_json(stream, "200 OK", &format!("{{{}}}", readout_json(state)));
        }
        ("POST", "/api/speed") => {
            let speed = match query_get(query, "v").map(|v| v.parse::<f64>()) {
                Some(Ok(v)) => v,
                _ => {
                    respond_text(
                        stream,
                        "400 Bad Request",
                        "preview refused: /api/speed needs `?v=RATE` (a rate above zero); pass the playback rate",
                    );
                    return;
                }
            };
            match state.transport.set_speed(speed) {
                Ok(()) => respond_json(stream, "200 OK", &format!("{{{}}}", readout_json(state))),
                Err(e) => respond_text(stream, "409 Conflict", &e.to_string()),
            }
        }
        ("POST", "/api/sync") => {
            let time = match query_get(query, "time").map(|v| v.parse::<f64>()) {
                Some(Ok(v)) => v,
                _ => {
                    respond_text(
                        stream,
                        "400 Bad Request",
                        "preview refused: /api/sync needs `?time=SECS` (mix seconds); pass the audio clock time",
                    );
                    return;
                }
            };
            state.transport.sync_to_mix_time(time);
            let frame = state.session.frame_for_mix_time(time);
            respond_json(
                stream,
                "200 OK",
                &format!("{{\"frame\":{frame}, {}}}", readout_json(state)),
            );
        }
        // Studio shell (Sprint 16, T002 / CAP-002): one dockable page
        // grown from the preview and player pages, with per-project
        // layout and theme save/restore, deep links, and autosave: the
        // edit route applies visual and code saves through the SDK
        // patch and history machinery, the queue route pauses and
        // resumes the save queue, and the timeline rows replay the
        // persisted delta.
        ("GET", "/studio") => {
            let page = render_studio(state, query);
            respond(
                stream,
                "200 OK",
                "text/html; charset=utf-8",
                page.as_bytes(),
            );
        }
        ("GET", "/api/studio/layout") => {
            let load = crate::studio::load_layout(&state.folder);
            respond_json(stream, "200 OK", &crate::studio::layout_json(&load));
        }
        ("POST", "/api/studio/layout") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            match crate::studio::save_layout(&state.folder, body) {
                Ok(layout) => {
                    let raw = serde_json::to_string(&layout).unwrap_or_else(|_| "{}".to_string());
                    respond_json(
                        stream,
                        "200 OK",
                        &format!("{{\"saved\":true,\"layout\":{raw}}}"),
                    );
                }
                Err(message) => respond_text(stream, "400 Bad Request", &message),
            }
        }
        ("POST", "/api/studio/reset") => {
            crate::studio::reset_layout(&state.folder);
            let load = crate::studio::load_layout(&state.folder);
            respond_json(stream, "200 OK", &crate::studio::layout_json(&load));
        }
        ("GET", "/api/studio/theme") => {
            let load = crate::studio::load_theme(&state.folder);
            respond_json(stream, "200 OK", &crate::studio::theme_json(&load));
        }
        ("POST", "/api/studio/theme") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            match crate::studio::save_theme(&state.folder, body) {
                Ok(theme) => respond_json(
                    stream,
                    "200 OK",
                    &format!("{{\"saved\":true,\"theme\":\"{}\"}}", theme.as_str()),
                ),
                Err(message) => respond_text(stream, "400 Bad Request", &message),
            }
        }
        ("GET", "/api/studio/save-state") => {
            let read_only = !crate::studio::project_reachable(&state.folder);
            respond_json(
                stream,
                "200 OK",
                &crate::studio::save_state_json(&state.save_queue, read_only, None),
            );
        }
        ("POST", "/api/studio/queue") => {
            let paused = match serde_json::from_str::<serde_json::Value>(body)
                .ok()
                .and_then(|v| v.get("paused").and_then(|p| p.as_bool()))
            {
                Some(p) => p,
                None => {
                    respond_text(
                        stream,
                        "400 Bad Request",
                        "studio refused: the queue switch needs a JSON queue body (`{\"paused\":true}` or `{\"paused\":false}`); pass the pause switch from the shell. Nothing changed.",
                    );
                    return;
                }
            };
            if !paused && state.conflict.is_some() {
                // STATE-04: a pending pick owns the drain — resuming
                // would merge the staged edits over the disk version
                // without an explicit choice.
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            state.save_queue.set_paused(paused);
            if paused || state.save_queue.staged.is_empty() {
                let read_only = !crate::studio::project_reachable(&state.folder);
                respond_json(
                    stream,
                    "200 OK",
                    &crate::studio::save_state_json(&state.save_queue, read_only, None),
                );
                return;
            }
            // Resuming replays every staged edit through the SDK patch
            // and history machinery; a failed drain keeps every staged
            // edit intact.
            let composition = state.session.composition_name().to_string();
            match state.save_queue.drain(&state.folder, Some(&composition)) {
                Ok(None) => {
                    let read_only = !crate::studio::project_reachable(&state.folder);
                    respond_json(
                        stream,
                        "200 OK",
                        &crate::studio::save_state_json(&state.save_queue, read_only, None),
                    );
                }
                Ok(Some(receipt)) => {
                    let mut out = crate::studio::edit_receipt_json(&receipt);
                    out.pop();
                    out.push_str(&format!(
                        ",\"paused\":{},\"staged\":{}}}",
                        state.save_queue.paused,
                        state.save_queue.staged.len()
                    ));
                    respond_json(stream, "200 OK", &out);
                }
                Err(message) => respond_text(stream, "409 Conflict", &message),
            }
        }
        ("POST", "/api/studio/edit") => {
            let edit = match crate::studio::parse_edit_body(body) {
                Ok(edit) => edit,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            if !crate::studio::project_reachable(&state.folder) {
                // REC-003: an unreachable project stays read-only and
                // keeps every staged edit; nothing applies.
                let staged = state.save_queue.stage(edit);
                let notice = crate::studio::unreachable_notice(&state.folder);
                let out = format!(
                    "{{\"saved\":false,\"read_only\":true,\"staged\":{},\"notice\":\"{}\"}}",
                    staged,
                    crate::studio::json_escape(&notice)
                );
                respond_text(stream, "409 Conflict", &out);
                return;
            }
            if state.conflict.is_some() {
                // STATE-04: editing pauses until the maker's pick lands.
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            if state.save_queue.paused {
                let staged = state.save_queue.stage(edit);
                let out = format!(
                    "{{\"saved\":false,\"paused\":true,\"staged\":{},\"notice\":\"save queue paused with {} staged edit(s); resume to apply.\"}}",
                    staged, staged
                );
                respond_json(stream, "200 OK", &out);
                return;
            }
            // Every save applies through the SDK patch and history
            // machinery, so visual edits land as validated turns.
            // Studio edits never touch the composition sources, so the
            // preview session needs no reload: it keeps serving the
            // same frames while readouts replay the persisted delta.
            let composition = state.session.composition_name().to_string();
            match crate::studio::apply_studio_edit(&state.folder, Some(&composition), &edit) {
                Ok(receipt) => respond_json(
                    stream,
                    "200 OK",
                    &crate::studio::edit_receipt_json(&receipt),
                ),
                Err(message) => respond_text(stream, "409 Conflict", &message),
            }
        }
        // External-change conflicts (Sprint 16, T003 / CAP-003, REC-002,
        // STATE-04): the shell banners the disk version against the
        // unsaved Studio version and the maker's pick applies with the
        // loser kept in history. Every pick lands as a validated turn
        // through the SDK patch and history machinery.
        ("GET", "/api/studio/conflict") => {
            respond_json(
                stream,
                "200 OK",
                &crate::studio::conflict_json(
                    &state.folder,
                    state.conflict.as_ref(),
                    &state.save_queue.summaries(),
                ),
            );
        }
        ("POST", "/api/studio/conflict") => {
            let pick = match crate::studio::parse_conflict_pick_body(body) {
                Ok(pick) => pick,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            if state.conflict.is_none() {
                respond_text(
                    stream,
                    "409 Conflict",
                    "studio refused: no external change is pending; disk and Studio agree. Nothing changed.",
                );
                return;
            }
            if !crate::studio::project_reachable(&state.folder) {
                // REC-003: an unreachable project stays read-only and
                // keeps every staged edit; nothing applies.
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            // Clone the staged edits: the queue clears only after the
            // whole pick (apply plus, for disk, the step back) applies,
            // so a failing pick keeps both versions.
            let staged = state.save_queue.staged.clone();
            let composition = state.session.composition_name().to_string();
            match crate::studio::apply_conflict_pick(
                &state.folder,
                Some(&composition),
                pick.pick,
                &pick.actor,
                &staged,
            ) {
                Ok(resolution) => {
                    state.save_queue.staged.clear();
                    let report = state.session.reload_keep_playhead(&mut state.transport);
                    state.kdl_disk = crate::studio::disk_version_of(&state.folder);
                    state.last_reload = Some(report.to_string());
                    state.conflict = None;
                    respond_json(
                        stream,
                        "200 OK",
                        &crate::studio::conflict_resolved_json(&resolution),
                    );
                }
                Err(message) => respond_text(stream, "409 Conflict", &message),
            }
        }
        ("GET", "/api/studio/timeline") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "404 Not Found",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let want = query_get(query, "composition");
            let found = match &want {
                None => Some(state.session.composition().clone()),
                Some(name) => state
                    .session
                    .all_compositions()
                    .iter()
                    .find(|c| &c.name == name)
                    .cloned(),
            };
            match found {
                Some(comp) => {
                    let shown = readout_composition(state, &comp, want.as_deref());
                    respond_json(stream, "200 OK", &crate::studio::timeline_rows_json(&shown))
                }
                None => {
                    let available: Vec<String> = state
                        .session
                        .all_compositions()
                        .iter()
                        .map(|c| c.name.clone())
                        .collect();
                    respond_text(
                        stream,
                        "404 Not Found",
                        &format!(
                            "studio: the deep link names unknown composition {:?}: the project holds {}; showing the project root {:?} instead.",
                            want.unwrap_or_default(),
                            available
                                .iter()
                                .map(|n| format!("{n:?}"))
                                .collect::<Vec<_>>()
                                .join(", "),
                            state.session.composition_name()
                        ),
                    );
                }
            }
        }
        // Timeline tools (Sprint 16, T004 + T006 / CAP-004, CAP-006,
        // INT-001, REC-001): select moves and trims, razor splits at the
        // click (`all-tracks` splits every track), select-before and
        // select-after pick clip sets, split at playhead divides the
        // clip under the playhead, and T006 adds clip gestures with
        // ripple and links (multi-select, multi-drag, marquee, delete,
        // close-gap, close-all-gaps, link, unlink, detach-audio,
        // merge-back, set-ripple) — every applied tool lands as a named
        // history turn through the SDK patch machinery with a receipt.
        // A refused tool stages nothing and names its cause. The
        // session reloads after an applied tool so rows and frames show
        // the new look with the playhead kept.
        ("POST", "/api/studio/tool") => {
            if state.conflict.is_some() {
                // STATE-04: editing pauses until the maker's pick lands.
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            let call = match crate::studio_tools::parse_tool_body(body) {
                Ok(call) => call,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            let composition = state.session.composition_name().to_string();
            let fps = state.session.fps().value();
            let playhead_secs = if fps > 0.0 {
                state.transport.position() as f64 / fps
            } else {
                0.0
            };
            match crate::studio_tools::apply_studio_tool(
                &state.folder,
                Some(&composition),
                &call,
                playhead_secs,
            ) {
                Ok(receipt) => {
                    let report = state.session.reload_keep_playhead(&mut state.transport);
                    state.kdl_disk = crate::studio::disk_version_of(&state.folder);
                    state.last_reload = Some(report.to_string());
                    let rows = crate::studio::timeline_rows_json(state.session.composition());
                    let out = format!(
                        "{{\"applied\":true,\"receipt\":{},\"rows\":{},\"readout\":{{{}}},\"reload\":\"{}\"}}",
                        crate::studio_tools::tool_receipt_json(&receipt),
                        rows,
                        readout_json(state),
                        json_escape(&report.message)
                    );
                    respond_json(stream, "200 OK", &out);
                }
                Err(message) => respond_text(stream, "409 Conflict", &message),
            }
        }
        // Work area (Sprint 16, T004): the in/out bounds playback seeks
        // and export ranges. Saved per project in the hidden history
        // directory, next to the layout file.
        ("GET", "/api/studio/workarea") => {
            let load = crate::studio_tools::load_work_area(&state.folder);
            respond_json(
                stream,
                "200 OK",
                &crate::studio_tools::work_area_json(&load),
            );
        }
        ("POST", "/api/studio/workarea") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            match crate::studio_tools::set_work_area(
                &state.folder,
                body,
                state.session.duration_secs(),
            ) {
                Ok(area) => {
                    let load = crate::studio_tools::WorkAreaLoad {
                        area,
                        restored: true,
                        notice: None,
                    };
                    respond_json(
                        stream,
                        "200 OK",
                        &crate::studio_tools::work_area_json(&load),
                    );
                }
                Err(message) => respond_text(stream, "400 Bad Request", &message),
            }
        }
        ("DELETE", "/api/studio/workarea") => {
            crate::studio_tools::clear_work_area(&state.folder);
            let load = crate::studio_tools::load_work_area(&state.folder);
            respond_json(
                stream,
                "200 OK",
                &crate::studio_tools::work_area_json(&load),
            );
        }
        // Studio seek (Sprint 16, T004): the ruler parks the playhead,
        // clamped into the work area when one is set (reported, never
        // silent) and refused past the timeline end naming the frame.
        ("POST", "/api/studio/seek") => {
            let index = match query_get(query, "frame").map(|v| v.parse::<u64>()) {
                Some(Ok(i)) => i,
                _ => {
                    respond_text(
                        stream,
                        "400 Bad Request",
                        "preview refused: /api/studio/seek needs `?frame=N` (a frame index); pass the frame to show",
                    );
                    return;
                }
            };
            if let Err(e) = state.transport.scrub_to(index) {
                respond_text(stream, "409 Conflict", &e.to_string());
                return;
            }
            let area = crate::studio_tools::load_work_area(&state.folder);
            let (frame, clamped) = crate::studio_tools::clamp_frame_to_work_area(
                index,
                &area.area,
                state.session.fps().value(),
                state.session.frame_count(),
            );
            if clamped {
                let _ = state.transport.scrub_to(frame);
            }
            let out = format!(
                "{{\"frame\":{},\"clamped\":{},\"work_area\":{},\"readout\":{{{}}}}}",
                frame,
                clamped,
                crate::studio_tools::work_area_json(&area),
                readout_json(state)
            );
            respond_json(stream, "200 OK", &out);
        }
        // Gap highlights (Sprint 16, T006 / CAP-006): empty intervals
        // per track from the effective composition (the SDK delta
        // replayed), with the ripple switch the closes honor.
        ("GET", "/api/studio/gaps") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "404 Not Found",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let want = query_get(query, "composition");
            match find_studio_composition(state, want.as_deref()) {
                Some(comp) => {
                    let shown = crate::studio::effective_composition_for(
                        &state.folder,
                        &comp,
                        want.as_deref(),
                    );
                    let session_rows = sdk::timeline::rows_of(&shown);
                    let load = crate::studio_clips::load_clip_links(&state.folder);
                    respond_json(
                        stream,
                        "200 OK",
                        &crate::studio_clips::gaps_json(
                            &crate::studio_clips::gaps_of(&session_rows),
                            load.store.ripple,
                        ),
                    )
                }
                None => respond_text(stream, "404 Not Found", &unknown_composition(state, want)),
            }
        }
        // Link groups (Sprint 16, T006 / CAP-006): members with detached
        // flags, the in-sync verdict, and the `out-of-sync` badge text
        // exactly while offsets drifted. Reads the effective composition
        // so applied moves show at once.
        ("GET", "/api/studio/links") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "404 Not Found",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let want = query_get(query, "composition");
            match find_studio_composition(state, want.as_deref()) {
                Some(comp) => {
                    let shown = crate::studio::effective_composition_for(
                        &state.folder,
                        &comp,
                        want.as_deref(),
                    );
                    let session_rows = sdk::timeline::rows_of(&shown);
                    let load = crate::studio_clips::load_clip_links(&state.folder);
                    respond_json(
                        stream,
                        "200 OK",
                        &crate::studio_clips::links_json(
                            &load.store,
                            &shown.name,
                            &session_rows,
                            load.notice.as_deref(),
                        ),
                    )
                }
                None => respond_text(stream, "404 Not Found", &unknown_composition(state, want)),
            }
        }
        ("POST", "/api/reload") => {
            if state.conflict.is_some() {
                // STATE-04: a manual reload would hand the disk version
                // a silent win over staged work; the pick owns it.
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            let report = state.session.reload_keep_playhead(&mut state.transport);
            state.kdl_disk = crate::studio::disk_version_of(&state.folder);
            state.last_reload = Some(report.to_string());
            let body = format!(
                "{{\"applied\":{},\"elapsed_ms\":{},\"message\":\"{}\",\"playhead\":{}}}",
                report.applied,
                report.elapsed_ms(),
                json_escape(&report.message),
                match report.playhead {
                    Some(p) => p.to_string(),
                    None => "null".to_string(),
                }
            );
            respond_json(stream, "200 OK", &body);
        }
        ("POST", "/api/shutdown") => {
            state.shutdown.store(true, Ordering::SeqCst);
            respond_text(stream, "200 OK", "preview shutting down");
        }
        // Keyframe lanes (Sprint 16, T007 / CAP-007): one property
        // group each with draggable diamonds and the diamond menu,
        // read from the effective composition (the SDK delta replayed),
        // so applied retimes show at once.
        ("GET", "/api/studio/lanes") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "404 Not Found",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let want = query_get(query, "composition");
            match find_studio_composition(state, want.as_deref()) {
                Some(comp) => {
                    let shown = readout_composition(state, &comp, want.as_deref());
                    respond_json(
                        stream,
                        "200 OK",
                        &crate::studio::keyframe_lanes_json(&shown),
                    )
                }
                None => respond_text(stream, "404 Not Found", &unknown_composition(state, want)),
            }
        }
        // Diamond menu edits (Sprint 16, T007 / CAP-007): retime (with
        // segment clamp), edit ease, copy, and delete apply through the
        // SDK patch and history machinery as one validated turn.
        ("POST", "/api/studio/keyframe") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            if state.conflict.is_some() {
                // STATE-04: editing pauses until the maker's pick lands.
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            let req = match crate::studio::parse_keyframe_body(body) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            let composition = req
                .composition
                .clone()
                .or_else(|| query_get(query, "composition"))
                .unwrap_or_else(|| state.session.composition_name().to_string());
            if find_studio_composition(state, Some(&composition)).is_none() {
                respond_text(
                    stream,
                    "404 Not Found",
                    &unknown_composition(state, Some(composition)),
                );
                return;
            }
            let base =
                find_studio_composition(state, Some(&composition)).expect("composition exists");
            let shown =
                crate::studio::effective_composition_for(&state.folder, &base, Some(&composition));
            let (op, clamped, applied_at) = match crate::studio::keyframe_op_for(&shown, &req) {
                Ok(built) => built,
                Err(message) => {
                    respond_text(stream, "409 Conflict", &message);
                    return;
                }
            };
            let edit = crate::studio::StudioEdit {
                actor: req.actor.clone(),
                source: crate::studio::SaveSource::Visual,
                ops: vec![op],
            };
            match crate::studio::apply_studio_edit(&state.folder, Some(&composition), &edit) {
                Ok(receipt) => respond_json(
                    stream,
                    "200 OK",
                    &crate::studio::keyframe_receipt_json(&receipt, clamped, applied_at),
                ),
                Err(message) => respond_text(stream, "409 Conflict", &message),
            }
        }
        // Composition cards with live hover preview, drag-or-add
        // placement, and render-one (Sprint 18, T001 / CAP-001, J-01,
        // J-07, REC-001): cards read project truth with a generation,
        // placements land as named history turns, render-one queues
        // exactly one composition, and stale generations refresh with
        // fresh cards instead of acting.
        ("GET", "/api/studio/compositions") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "404 Not Found",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let load = crate::studio_compositions::list_composition_cards(&state.folder);
            respond_json(
                stream,
                "200 OK",
                &crate::studio_compositions::cards_json(&load),
            );
        }
        ("GET", "/api/studio/compositions/preview") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "404 Not Found",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let name = match query_get(query, "composition") {
                Some(n) if !n.trim().is_empty() => n,
                _ => {
                    respond_text(
                        stream,
                        "400 Bad Request",
                        "studio refused: the hover preview needs `?composition=NAME` (a composition name); pass the card to preview. Nothing was drawn.",
                    );
                    return;
                }
            };
            if let Some(raw) = query_get(query, "generation") {
                match raw.parse::<u64>() {
                    Ok(expected) => {
                        let now = crate::studio_compositions::generation_of(&state.folder);
                        if expected != now {
                            respond_text(
                                stream,
                                "409 Conflict",
                                "studio refused: the hover preview went stale (the project moved on); reload the cards and hover again. Nothing was drawn.",
                            );
                            return;
                        }
                    }
                    Err(_) => {
                        respond_text(
                            stream,
                            "400 Bad Request",
                            "studio refused: the hover preview `generation` is not an integer; pass the card generation, or omit it. Nothing was drawn.",
                        );
                        return;
                    }
                }
            }
            match crate::studio_compositions::composition_preview_png(&state.folder, &name) {
                Ok(png) => respond(stream, "200 OK", "image/png", &png),
                Err(message) => {
                    let status = if message.contains("unknown composition") {
                        "404 Not Found"
                    } else {
                        "409 Conflict"
                    };
                    respond_text(stream, status, &message);
                }
            }
        }
        ("POST", "/api/studio/compositions/place") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            if state.conflict.is_some() {
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            if state.save_queue.paused {
                respond_text(
                    stream,
                    "409 Conflict",
                    "studio: the save queue is paused; resume it before compositions apply. Nothing was applied or recorded.",
                );
                return;
            }
            let req = match crate::studio_compositions::parse_place_body(body) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            match crate::studio_compositions::place_composition(&state.folder, &req) {
                Ok(receipt) => respond_json(
                    stream,
                    "200 OK",
                    &crate::studio_compositions::place_receipt_json(&receipt),
                ),
                Err(crate::studio_compositions::PlaceError::Stale { message, fresh }) => {
                    respond_json(
                        stream,
                        "409 Conflict",
                        &crate::studio_compositions::stale_json(&message, &fresh),
                    );
                }
                Err(crate::studio_compositions::PlaceError::Refused(message)) => {
                    let status = if message.contains("unknown composition")
                        || message.contains("unknown timeline")
                    {
                        "404 Not Found"
                    } else {
                        "409 Conflict"
                    };
                    respond_text(stream, status, &message);
                }
            }
        }
        ("POST", "/api/studio/compositions/render-one") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            if state.conflict.is_some() {
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            let req = match crate::studio_compositions::parse_render_one_body(body) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            match crate::studio_compositions::queue_render_one(&state.folder, &req) {
                Ok(receipt) => respond_json(
                    stream,
                    "200 OK",
                    &crate::studio_compositions::render_one_receipt_json(&receipt),
                ),
                Err(crate::studio_compositions::RenderOneError::Stale { message, fresh }) => {
                    respond_json(
                        stream,
                        "409 Conflict",
                        &crate::studio_compositions::stale_json(&message, &fresh),
                    );
                }
                Err(crate::studio_compositions::RenderOneError::Refused(message)) => {
                    let status = if message.contains("unknown composition") {
                        "404 Not Found"
                    } else {
                        "409 Conflict"
                    };
                    respond_text(stream, status, &message);
                }
            }
        }
        ("GET", "/api/studio/compositions/queue") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "404 Not Found",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            match crate::studio_compositions::list_render_queue(&state.folder) {
                Ok(jobs) => respond_json(
                    stream,
                    "200 OK",
                    &crate::studio_compositions::render_queue_json(&jobs),
                ),
                Err(message) => respond_text(stream, "409 Conflict", &message),
            }
        }
        // Clip visuals (Sprint 16, T007 / CAP-008, REC-005): filmstrips,
        // waveforms with peak marks, text content, badges, and fade
        // handles from composition data, with named placeholders where
        // media cannot show.
        ("GET", "/api/studio/visuals") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "404 Not Found",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let want = query_get(query, "composition");
            match find_studio_composition(state, want.as_deref()) {
                Some(comp) => {
                    let shown = readout_composition(state, &comp, want.as_deref());
                    let available: Vec<String> = state
                        .session
                        .all_compositions()
                        .iter()
                        .map(|c| c.name.clone())
                        .collect();
                    let fps_value = state.session.fps().value();
                    let frame_count = state.session.frame_count();
                    // The mix borrow ends before the reply is written:
                    // the readout builds its JSON while the borrow is
                    // alive, then the socket write runs borrow-free.
                    match state.session.mix_pcm() {
                        Ok(mix) => {
                            let body = crate::studio::clip_visuals_json(
                                &shown,
                                &available,
                                &state.folder,
                                mix,
                                None,
                                fps_value,
                                frame_count,
                            );
                            respond_json(stream, "200 OK", &body);
                        }
                        Err(e) => {
                            let why = e.to_string();
                            let body = crate::studio::clip_visuals_json(
                                &shown,
                                &available,
                                &state.folder,
                                None,
                                Some(&why),
                                fps_value,
                                frame_count,
                            );
                            respond_json(stream, "200 OK", &body);
                        }
                    }
                }
                None => respond_text(stream, "404 Not Found", &unknown_composition(state, want)),
            }
        }
        // Beat strip reads (Sprint 16, T007 / CAP-007): the beats with
        // the beat-only undo depth.
        ("GET", "/api/studio/beats") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "404 Not Found",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let want = query_get(query, "composition");
            match find_studio_composition(state, want.as_deref()) {
                Some(comp) => {
                    let load = crate::studio::load_beats(&state.folder);
                    respond_json(
                        stream,
                        "200 OK",
                        &crate::studio::beats_json(&load.store, &comp.name, load.notice.as_deref()),
                    )
                }
                None => respond_text(stream, "404 Not Found", &unknown_composition(state, want)),
            }
        }
        // Beat strip edits (Sprint 16, T007 / CAP-007): add, move, and
        // delete beats with beat-only undo, persisted in the hidden
        // history directory. Beats are Studio rhythm data, never
        // composition data, so clips never retime.
        ("POST", "/api/studio/beats") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let req = match crate::studio::parse_beats_body(body) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            let composition = req
                .composition
                .clone()
                .or_else(|| query_get(query, "composition"))
                .unwrap_or_else(|| state.session.composition_name().to_string());
            if find_studio_composition(state, Some(&composition)).is_none() {
                respond_text(
                    stream,
                    "404 Not Found",
                    &unknown_composition(state, Some(composition)),
                );
                return;
            }
            let base =
                find_studio_composition(state, Some(&composition)).expect("composition exists");
            let duration = base.duration;
            let at = match req.action {
                crate::studio::BeatsAction::Add | crate::studio::BeatsAction::Move => {
                    match req.at {
                        Some(t) => t,
                        None => {
                            respond_text(
                            stream,
                            "400 Bad Request",
                            "studio refused: the beats edit names no `at` instant; pass the beat time in composition seconds. Nothing was staged.",
                        );
                            return;
                        }
                    }
                }
                _ => 0.0,
            };
            let mut load = crate::studio::load_beats(&state.folder);
            let outcome: Result<(String, String), String> = match req.action {
                crate::studio::BeatsAction::Add => {
                    let name = req.name.clone().unwrap_or_default();
                    crate::studio::add_beat(&mut load.store, &composition, &name, at, duration)
                        .map(|(applied, clamped)| {
                            let detail = if clamped {
                                format!(
                                    "added beat {name:?} at {applied:.3}s (clamped to the composition end)"
                                )
                            } else {
                                format!("added beat {name:?} at {applied:.3}s")
                            };
                            ("add".to_string(), detail)
                        })
                }
                crate::studio::BeatsAction::Move => {
                    let name = req.name.clone().unwrap_or_default();
                    crate::studio::move_beat(&mut load.store, &composition, &name, at, duration)
                        .map(|(applied, clamped)| {
                            let detail = if clamped {
                                format!(
                                    "moved beat {name:?} to {applied:.3}s (clamped to the composition end)"
                                )
                            } else {
                                format!("moved beat {name:?} to {applied:.3}s")
                            };
                            ("move".to_string(), detail)
                        })
                }
                crate::studio::BeatsAction::Delete => {
                    let name = req.name.clone().unwrap_or_default();
                    crate::studio::delete_beat(&mut load.store, &composition, &name)
                        .map(|()| ("delete".to_string(), format!("deleted beat {name:?}")))
                }
                crate::studio::BeatsAction::Undo => {
                    crate::studio::undo_beats(&mut load.store, &composition).map(|count| {
                        (
                            "undo".to_string(),
                            format!("beat undo restored {count} beat(s)"),
                        )
                    })
                }
            };
            match outcome {
                Err(message) => respond_text(stream, "409 Conflict", &message),
                Ok((action, detail)) => {
                    match crate::studio::persist_beats(&state.folder, &load.store) {
                        Err(message) => respond_text(stream, "409 Conflict", &message),
                        Ok(()) => respond_json(
                            stream,
                            "200 OK",
                            &crate::studio::beats_receipt_json(
                                &load.store,
                                &composition,
                                &action,
                                &detail,
                            ),
                        ),
                    }
                }
            }
        }
        // Inspector rows (Sprint 17, T007 / CAP-007, J-04, INT-002,
        // REC-001): live rows of the selected clip across the ten
        // groups with per-row reset defaults, render-deferred rows
        // marked unavailable with their milestone, and row edits plus
        // resets recorded as validated turns through the SDK patch
        // and history machinery. A refused value names its rule and
        // location and stages nothing.
        ("GET", "/api/studio/inspector") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "404 Not Found",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let want = query_get(query, "composition");
            let base = match &want {
                None => Some(state.session.composition().clone()),
                Some(name) => state
                    .session
                    .all_compositions()
                    .iter()
                    .find(|c| &c.name == name)
                    .cloned(),
            };
            match base {
                None => respond_text(stream, "404 Not Found", &unknown_composition(state, want)),
                Some(comp) => {
                    let shown = readout_composition(state, &comp, want.as_deref());
                    let target_raw = query_get(query, "target").unwrap_or_default();
                    let selection_raw = query_get(query, "selection").unwrap_or_default();
                    let selection: Vec<String> = selection_raw
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect();
                    let target = if target_raw.trim().is_empty() {
                        match selection.first() {
                            Some(first) => crate::studio_inspector::parse_inspector_target(first),
                            None => {
                                respond_text(
                                    stream,
                                    "400 Bad Request",
                                    "studio refused: the inspector needs `?target=<id-or-index>` (or `?selection=<keys>` taking the first key); rows address one clip. Nothing was staged.",
                                );
                                return;
                            }
                        }
                    } else {
                        crate::studio_inspector::parse_inspector_target(&target_raw)
                    };
                    match crate::studio_inspector::inspector_json(&shown, &target, &selection) {
                        Ok(out) => respond_json(stream, "200 OK", &out),
                        Err(message) => respond_text(stream, "404 Not Found", &message),
                    }
                }
            }
        }
        ("POST", "/api/studio/inspector") => {
            let edit = match crate::studio_inspector::parse_inspector_body(body) {
                Ok(edit) => edit,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            if state.conflict.is_some() {
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            if state.save_queue.paused {
                respond_text(
                    stream,
                    "409 Conflict",
                    "studio: the save queue is paused; resume it before inspector rows apply. Nothing was applied or recorded.",
                );
                return;
            }
            let composition = state.session.composition_name().to_string();
            match crate::studio_inspector::apply_inspector_edit(
                &state.folder,
                Some(&composition),
                &edit,
            ) {
                Ok(receipt) => respond_json(
                    stream,
                    "200 OK",
                    &crate::studio_inspector::inspector_receipt_json(&receipt),
                ),
                Err(message) => respond_text(stream, "409 Conflict", &message),
            }
        }
        // Variables panel (Sprint 18, T005 / CAP-005, J-02,
        // STATE-02, INT-001, REC-001): declared variables with
        // defaults, bound state, and warnings; declare, edit, and
        // remove validate like the SDK and record guarded history
        // turns; binding writes the reference onto the selected
        // element as a validated turn; copies place exact values
        // text and a runnable render command. A refused action
        // names its cause and stages nothing.
        ("GET", "/api/studio/variables") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "404 Not Found",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let want = query_get(query, "composition");
            let view = query_get(query, "view").unwrap_or_default();
            if view == "values" || view == "command" {
                let raw = query_get(query, "overrides").unwrap_or_else(|| "{}".to_string());
                let delta = match validate::validate::parse_variable_overrides(&raw) {
                    Ok(delta) => delta,
                    Err(e) => {
                        respond_text(
                            stream,
                            "400 Bad Request",
                            &format!(
                                "studio refused: the variables copy `overrides` are not a variables map ({e}). Pass a JSON object of string keys to scalar values. Nothing was staged."
                            ),
                        );
                        return;
                    }
                };
                if view == "values" {
                    respond_json(
                        stream,
                        "200 OK",
                        &crate::studio_variables::copy_variable_values(&delta),
                    );
                } else {
                    let command = crate::studio_variables::copy_render_command(
                        &state.folder,
                        want.as_deref(),
                        &delta,
                        None,
                    );
                    respond_json(
                        stream,
                        "200 OK",
                        &format!(
                            "{{\"command\":\"{}\"}}",
                            crate::studio::json_escape(&command)
                        ),
                    );
                }
                return;
            }
            match crate::studio_variables::variables_json(&state.folder, want.as_deref()) {
                Ok(out) => respond_json(stream, "200 OK", &out),
                Err(message) => respond_text(stream, "404 Not Found", &message),
            }
        }
        ("POST", "/api/studio/variables") => {
            let action = match crate::studio_variables::parse_variables_action(body) {
                Ok(action) => action,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let preview_only =
                matches!(action, crate::studio_variables::VariablesAction::Preview { .. });
            if !preview_only {
                if state.conflict.is_some() {
                    respond_text(
                        stream,
                        "409 Conflict",
                        &conflict_blocker_message(state.save_queue.staged.len()),
                    );
                    return;
                }
                if state.save_queue.paused {
                    respond_text(
                        stream,
                        "409 Conflict",
                        "studio: the save queue is paused; resume it before variables edits apply. Nothing was applied or recorded.",
                    );
                    return;
                }
            }
            let composition = state.session.composition_name().to_string();
            let composition = Some(composition.as_str());
            match action {
                crate::studio_variables::VariablesAction::Declare { actor, input } => {
                    match crate::studio_variables::declare_panel_variable(
                        &state.folder,
                        composition,
                        &actor,
                        input,
                    ) {
                        Ok(receipt) => respond_json(
                            stream,
                            "200 OK",
                            &crate::studio_variables::declared_receipt_json(&receipt),
                        ),
                        Err(message) => respond_text(stream, "409 Conflict", &message),
                    }
                }
                crate::studio_variables::VariablesAction::Edit { actor, id, input } => {
                    match crate::studio_variables::edit_panel_variable(
                        &state.folder,
                        composition,
                        &actor,
                        &id,
                        input,
                    ) {
                        Ok(receipt) => respond_json(
                            stream,
                            "200 OK",
                            &crate::studio_variables::declared_receipt_json(&receipt),
                        ),
                        Err(message) => respond_text(stream, "409 Conflict", &message),
                    }
                }
                crate::studio_variables::VariablesAction::Remove { actor, id } => {
                    match crate::studio_variables::remove_panel_variable(
                        &state.folder,
                        composition,
                        &actor,
                        &id,
                    ) {
                        Ok(receipt) => respond_json(
                            stream,
                            "200 OK",
                            &crate::studio_variables::removed_receipt_json(&receipt),
                        ),
                        Err(message) => respond_text(stream, "409 Conflict", &message),
                    }
                }
                crate::studio_variables::VariablesAction::Bind(bind) => {
                    match crate::studio_variables::apply_variable_bind(
                        &state.folder,
                        composition,
                        &bind,
                    ) {
                        Ok(receipt) => respond_json(
                            stream,
                            "200 OK",
                            &crate::studio_variables::bind_receipt_json(&receipt),
                        ),
                        Err(message) => respond_text(stream, "409 Conflict", &message),
                    }
                }
                crate::studio_variables::VariablesAction::Preview { delta } => {
                    match crate::studio_variables::preview_variable_overrides(
                        &state.folder,
                        composition,
                        delta,
                    ) {
                        Ok(preview) => respond_json(
                            stream,
                            "200 OK",
                            &crate::studio_variables::preview_json(&preview),
                        ),
                        Err(message) => respond_text(stream, "409 Conflict", &message),
                    }
                }
            }
        }
        // Animation catalog (Sprint 17, T008 / CAP-008, CAP-009,
        // J-05): card kinds, engine-supported ease families with
        // presets, and unavailable families with the supported
        // spelling — plus the echoed canvas/timeline selection sync.
        ("GET", "/api/studio/animation") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "404 Not Found",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let want = query_get(query, "composition");
            let base = match &want {
                None => Some(state.session.composition().clone()),
                Some(name) => state
                    .session
                    .all_compositions()
                    .iter()
                    .find(|c| &c.name == name)
                    .cloned(),
            };
            match base {
                None => respond_text(stream, "404 Not Found", &unknown_composition(state, want)),
                Some(comp) => {
                    let shown = readout_composition(state, &comp, want.as_deref());
                    let selection_raw = query_get(query, "selection").unwrap_or_default();
                    let selection: Vec<String> = selection_raw
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect();
                    respond_json(
                        stream,
                        "200 OK",
                        &crate::studio_animate::animation_catalog_json(&shown, &selection),
                    );
                }
            }
        }
        // Tween cards (Sprint 17, T008 / CAP-008, J-05, INT-001):
        // `to`, `from`, `from-to`, and `set` cards with start,
        // length, and speed write real tweens and sets as one named
        // turn. Parse failures answer 400; apply failures 409 with
        // the rule and location while nothing stages.
        ("POST", "/api/studio/card") => {
            let req = match crate::studio_animate::parse_card_body(body) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            if state.conflict.is_some() {
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            if state.save_queue.paused {
                respond_text(
                    stream,
                    "409 Conflict",
                    "studio: the save queue is paused; resume it before tween cards apply. Nothing was applied or recorded.",
                );
                return;
            }
            let composition = state.session.composition_name().to_string();
            match crate::studio_animate::apply_card_edit(&state.folder, Some(&composition), &req) {
                Ok(receipt) => respond_json(
                    stream,
                    "200 OK",
                    &crate::studio_animate::card_receipt_json(&receipt),
                ),
                Err(message) => respond_text(stream, "409 Conflict", &message),
            }
        }
        // Bezier editor (Sprint 17, T008 / CAP-008, J-05, INT-001):
        // engine easings write tween ease, per-segment key ease, or
        // every segment at once as one named turn. Unsupported and
        // unknown families refuse at parse time, never faked.
        ("POST", "/api/studio/bezier") => {
            let req = match crate::studio_animate::parse_bezier_body(body) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            if state.conflict.is_some() {
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            if state.save_queue.paused {
                respond_text(
                    stream,
                    "409 Conflict",
                    "studio: the save queue is paused; resume it before curves apply. Nothing was applied or recorded.",
                );
                return;
            }
            let composition = state.session.composition_name().to_string();
            match crate::studio_animate::apply_bezier_edit(&state.folder, Some(&composition), &req)
            {
                Ok(receipt) => respond_json(
                    stream,
                    "200 OK",
                    &crate::studio_animate::bezier_receipt_json(&receipt),
                ),
                Err(message) => respond_text(stream, "409 Conflict", &message),
            }
        }
        // Unrolled computed values (Sprint 17, T008 / CAP-008, J-05):
        // sampled tween windows, key segments, expression results,
        // and live path positions for editing. Unknown directives
        // answer 404; bad counts 400; nothing stages either way.
        ("GET", "/api/studio/unroll") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "404 Not Found",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let req = match crate::studio_animate::parse_unroll_params(
                query_get(query, "tween").as_deref(),
                query_get(query, "key").as_deref(),
                query_get(query, "samples").as_deref(),
            ) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            let want = query_get(query, "composition");
            let base = match &want {
                None => Some(state.session.composition().clone()),
                Some(name) => state
                    .session
                    .all_compositions()
                    .iter()
                    .find(|c| &c.name == name)
                    .cloned(),
            };
            match base {
                None => respond_text(stream, "404 Not Found", &unknown_composition(state, want)),
                Some(comp) => {
                    let shown = readout_composition(state, &comp, want.as_deref());
                    match crate::studio_animate::unroll_json(&shown, &req) {
                        Ok(out) => respond_json(stream, "200 OK", &out),
                        Err(message) => {
                            let status = if message.contains("names nothing") {
                                "404 Not Found"
                            } else {
                                "400 Bad Request"
                            };
                            respond_text(stream, status, &message);
                        }
                    }
                }
            }
        }
        // Live motion-path preview (Sprint 17, T008 / CAP-009, J-05):
        // positions along the same arc-length table the render
        // travels. An unfollowable path answers 400 with its cause.
        ("GET", "/api/studio/path-preview") => {
            let data = query_get(query, "d").unwrap_or_default();
            let samples = query_get(query, "samples")
                .as_deref()
                .unwrap_or("25")
                .trim()
                .parse::<usize>()
                .unwrap_or(25);
            match crate::studio_animate::preview_path(&data, samples) {
                Ok(out) => respond_json(stream, "200 OK", &out),
                Err(message) => respond_text(stream, "400 Bad Request", &message),
            }
        }
        // Motion paths (Sprint 17, T008 / CAP-009, J-05, INT-001):
        // `add` appends a path tween, `retarget` commits edited
        // points to a path tween — always as one named turn. An
        // unfollowable path or a mismatched morph refuses with its
        // reason while nothing stages.
        ("POST", "/api/studio/path") => {
            let req = match crate::studio_animate::parse_path_body(body) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            if state.conflict.is_some() {
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            if state.save_queue.paused {
                respond_text(
                    stream,
                    "409 Conflict",
                    "studio: the save queue is paused; resume it before motion paths apply. Nothing was applied or recorded.",
                );
                return;
            }
            let composition = state.session.composition_name().to_string();
            match crate::studio_animate::apply_path_edit(&state.folder, Some(&composition), &req) {
                Ok(receipt) => respond_json(
                    stream,
                    "200 OK",
                    &crate::studio_animate::path_receipt_json(&receipt),
                ),
                Err(message) => respond_text(stream, "409 Conflict", &message),
            }
        }
        // Gesture capture (Sprint 17, T008 / CAP-009, J-05, INT-001,
        // REC-004): recorded samples land as `x`/`y` keyframes in one
        // named turn. An empty capture discards itself with a 200
        // note and no turn.
        ("POST", "/api/studio/capture") => {
            let req = match crate::studio_animate::parse_capture_body(body) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            if state.conflict.is_some() {
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            if state.save_queue.paused {
                respond_text(
                    stream,
                    "409 Conflict",
                    "studio: the save queue is paused; resume it before captures apply. Nothing was applied or recorded.",
                );
                return;
            }
            let composition = state.session.composition_name().to_string();
            match crate::studio_animate::apply_capture(&state.folder, Some(&composition), &req) {
                Ok(outcome) => respond_json(
                    stream,
                    "200 OK",
                    &crate::studio_animate::capture_outcome_json(&outcome),
                ),
                Err(message) => respond_text(stream, "409 Conflict", &message),
            }
        }
        // Snap settings (Sprint 16, T005 / CAP-005): the visible
        // project setting behind drag magnets — the on/off switch
        // plus the threshold in CSS pixels. Refusals save nothing.
        ("GET", "/api/studio/snap") => {
            let load = crate::studio_snap_zoom::load_snap(&state.folder);
            respond_json(
                stream,
                "200 OK",
                &crate::studio_snap_zoom::snap_json(&load),
            );
        }
        ("POST", "/api/studio/snap") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            match crate::studio_snap_zoom::save_snap(&state.folder, body) {
                Ok(settings) => {
                    let load = crate::studio_snap_zoom::SnapLoad {
                        settings,
                        restored: true,
                        notice: None,
                    };
                    respond_json(
                        stream,
                        "200 OK",
                        &crate::studio_snap_zoom::snap_json(&load),
                    );
                }
                Err(message) => respond_text(stream, "400 Bad Request", &message),
            }
        }
        // Snap resolution (Sprint 16, T005 / CAP-005): one drag
        // position against playhead, clip edges, beats, and the frame
        // grid within the visible threshold, with the guide line the
        // panel paints. Pure view math: no patch stages, no history
        // records, so the delta and turns stay exactly as they were.
        ("POST", "/api/studio/snap-resolve") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let req = match crate::studio_snap_zoom::parse_snap_resolve_body(body) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            let composition = req
                .composition
                .clone()
                .or_else(|| query_get(query, "composition"))
                .unwrap_or_else(|| state.session.composition_name().to_string());
            if find_studio_composition(state, Some(&composition)).is_none() {
                respond_text(
                    stream,
                    "404 Not Found",
                    &unknown_composition(state, Some(composition)),
                );
                return;
            }
            let base =
                find_studio_composition(state, Some(&composition)).expect("composition exists");
            let shown = readout_composition(state, &base, Some(&composition));
            let fps_value = state.session.fps().value();
            let frame_count = state.session.frame_count();
            let playhead_secs = match req.playhead {
                Some(t) => t,
                None => {
                    if fps_value > 0.0 {
                        state.transport.position() as f64 / fps_value
                    } else {
                        0.0
                    }
                }
            };
            let beats = crate::studio::load_beats(&state.folder);
            let targets = crate::studio_snap_zoom::build_snap_candidates(
                &shown,
                &beats.store.beats_for(&composition),
                Some(playhead_secs),
                fps_value,
                frame_count,
            );
            let load = crate::studio_snap_zoom::load_snap(&state.folder);
            let outcome = crate::studio_snap_zoom::resolve_snap(
                req.want,
                &load.settings,
                req.px_per_sec,
                &targets,
            );
            respond_json(
                stream,
                "200 OK",
                &crate::studio_snap_zoom::snap_resolve_json(&load.settings, &outcome, req.want),
            );
        }
        // Canvas snap (Sprint 17, T003 / CAP-003): one film point
        // against element edges and centres, equidistance, frame edges
        // and centre, and grid within the visible threshold, with the
        // guide lines and ruler positions the panel paints. Pure view
        // math: no patch stages, no history records, so the delta and
        // turns stay exactly as they were.
        ("POST", "/api/studio/canvas-snap") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let req = match crate::studio_canvas_snap::parse_canvas_snap_body(body) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            let composition = req
                .composition
                .clone()
                .or_else(|| query_get(query, "composition"))
                .unwrap_or_else(|| state.session.composition_name().to_string());
            match find_studio_composition(state, Some(&composition)) {
                None => respond_text(
                    stream,
                    "404 Not Found",
                    &unknown_composition(state, Some(composition)),
                ),
                Some(base) => {
                    let shown = readout_composition(state, &base, Some(&composition));
                    let rows = crate::studio::studio_rows_of(&shown);
                    let (kept, dropped) =
                        crate::studio_canvas::sync_canvas_selection(&req.selection, &rows);
                    let (width, height) = state.session.canvas();
                    let settings = crate::studio_snap_zoom::load_snap(&state.folder).settings;
                    let candidates = crate::studio_canvas_snap::build_canvas_snap_candidates(
                        &shown,
                        width as f64,
                        height as f64,
                        req.grid,
                        &req.exclude,
                    );
                    let outcome = crate::studio_canvas_snap::resolve_canvas_snap(
                        req.want,
                        &settings,
                        req.scale,
                        &candidates,
                    );
                    respond_json(
                        stream,
                        "200 OK",
                        &crate::studio_canvas_snap::canvas_snap_json(
                            &settings, &outcome, req.want, req.grid, req.scale, &kept, &dropped,
                        ),
                    );
                }
            }
        }
        // Zoom (Sprint 16, T005 / CAP-005): fit fills the viewport,
        // manual holds a percent of fit from 10 percent to frame
        // level anchored at the cursor or playhead, selection frames
        // the selected clips. Out-of-bounds scales clamp and show the
        // clamped value. View state only: the preference persists in
        // the hidden history directory; no patch stages.
        ("GET", "/api/studio/zoom") => {
            let load = crate::studio_snap_zoom::load_zoom(&state.folder);
            respond_json(stream, "200 OK", &crate::studio_snap_zoom::zoom_json(&load));
        }
        ("POST", "/api/studio/zoom") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let req = match crate::studio_snap_zoom::parse_zoom_body(body) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            let composition = req
                .composition
                .clone()
                .or_else(|| query_get(query, "composition"))
                .unwrap_or_else(|| state.session.composition_name().to_string());
            match find_studio_composition(state, Some(&composition)) {
                None => respond_text(
                    stream,
                    "404 Not Found",
                    &unknown_composition(state, Some(composition)),
                ),
                Some(base) => {
                    let shown = readout_composition(state, &base, Some(&composition));
                    let rows = crate::studio::studio_rows_of(&shown);
                    let fps_value = state.session.fps().value();
                    let playhead_secs = if fps_value > 0.0 {
                        state.transport.position() as f64 / fps_value
                    } else {
                        0.0
                    };
                    match crate::studio_snap_zoom::apply_zoom(
                        &state.folder,
                        &req,
                        &rows,
                        shown.duration,
                        fps_value,
                        playhead_secs,
                    ) {
                        Err(message) => respond_text(stream, "409 Conflict", &message),
                        Ok((outcome, pref)) => respond_json(
                            stream,
                            "200 OK",
                            &crate::studio_snap_zoom::zoom_result_json(&outcome, &pref),
                        ),
                    }
                }
            }
        }
        // Canvas view (Sprint 17, T001 / CAP-001): zoom 25 to 400
        // percent anchored at the cursor or centre, free pan, fit,
        // fullscreen, rulers, minimap, and 90/70 percent safe margins
        // on demand. View state only: the view persists in the hidden
        // history directory; no patch stages, no history records, and
        // a missing film shows the empty canvas with its reason.
        ("GET", "/api/studio/canvas") => {
            let composition = query_get(query, "composition")
                .unwrap_or_else(|| state.session.composition_name().to_string());
            match find_studio_composition(state, Some(&composition)) {
                None => respond_text(
                    stream,
                    "404 Not Found",
                    &unknown_composition(state, Some(composition)),
                ),
                Some(base) => {
                    let shown = readout_composition(state, &base, Some(&composition));
                    let selection: Vec<String> = query_get(query, "selection")
                        .map(|raw| {
                            raw.split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect()
                        })
                        .unwrap_or_default();
                    let rows = crate::studio::studio_rows_of(&shown);
                    let (kept, dropped) =
                        crate::studio_canvas::sync_canvas_selection(&selection, &rows);
                    let (width, height) = state.session.canvas();
                    let film = crate::studio_canvas::CanvasFilm {
                        width,
                        height,
                        frame_count: state.session.frame_count(),
                        font_families: crate::studio_canvas::canvas_font_lists_of(&shown),
                    };
                    let load = crate::studio_canvas::load_canvas(&state.folder);
                    let detail = crate::studio_canvas::canvas_status_line(&load.view);
                    respond_json(
                        stream,
                        "200 OK",
                        &crate::studio_canvas::canvas_state_json(
                            &load,
                            &detail,
                            &film,
                            state.transport.position(),
                            &kept,
                            &dropped,
                        ),
                    );
                }
            }
        }
        ("POST", "/api/studio/canvas") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let current = crate::studio_canvas::load_canvas(&state.folder).view;
            let update = match crate::studio_canvas::parse_canvas_body(body, &current) {
                Ok(update) => update,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            let composition = update
                .composition
                .clone()
                .or_else(|| query_get(query, "composition"))
                .unwrap_or_else(|| state.session.composition_name().to_string());
            match find_studio_composition(state, Some(&composition)) {
                None => respond_text(
                    stream,
                    "404 Not Found",
                    &unknown_composition(state, Some(composition)),
                ),
                Some(base) => {
                    let shown = readout_composition(state, &base, Some(&composition));
                    let echo = update.selection.clone().unwrap_or_default();
                    let rows = crate::studio::studio_rows_of(&shown);
                    let (kept, dropped) = crate::studio_canvas::sync_canvas_selection(&echo, &rows);
                    let (width, height) = state.session.canvas();
                    let film = crate::studio_canvas::CanvasFilm {
                        width,
                        height,
                        frame_count: state.session.frame_count(),
                        font_families: crate::studio_canvas::canvas_font_lists_of(&shown),
                    };
                    // View state only: persist the hidden canvas file
                    // directly — never a patch, never a turn.
                    match crate::studio_canvas::save_canvas(&state.folder, body) {
                        Err(message) => respond_text(stream, "400 Bad Request", &message),
                        Ok((view, outcome)) => {
                            let load = crate::studio_canvas::CanvasLoad {
                                view,
                                restored: true,
                                notice: None,
                            };
                            respond_json(
                                stream,
                                "200 OK",
                                &crate::studio_canvas::canvas_state_json(
                                    &load,
                                    &outcome.detail,
                                    &film,
                                    state.transport.position(),
                                    &kept,
                                    &dropped,
                                ),
                            );
                        }
                    }
                }
            }
        }
        // Canvas selection and layers (Sprint 17, T002 / CAP-002,
        // INT-003): click, Shift-toggle/add, marquee, and clear
        // resolve against authored element boxes, and the minimal
        // layers readout lists elements by z-order with kind icons,
        // groups, scope marks, boxes, off-canvas indicators, and the
        // synced selection echo (Sprint 18, T004 adds icons, groups,
        // and `?scope=`). Selection is view state: it writes nothing,
        // stages no patch, and records no turn.
        ("GET", "/api/studio/layers") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "404 Not Found",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let composition = query_get(query, "composition")
                .unwrap_or_else(|| state.session.composition_name().to_string());
            match find_studio_composition(state, Some(&composition)) {
                None => respond_text(
                    stream,
                    "404 Not Found",
                    &unknown_composition(state, Some(composition)),
                ),
                Some(base) => {
                    let shown = readout_composition(state, &base, Some(&composition));
                    let scope_raw = query_get(query, "scope").unwrap_or_default();
                    let scope = match crate::studio_select::resolve_group_scope(
                        &shown, &scope_raw,
                    ) {
                        Ok(scope) => scope,
                        Err(message) => {
                            respond_text(stream, "404 Not Found", &message);
                            return;
                        }
                    };
                    let selection: Vec<String> = query_get(query, "selection")
                        .map(|raw| {
                            raw.split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect()
                        })
                        .unwrap_or_default();
                    let rows = crate::studio::studio_rows_of(&shown);
                    let (kept, dropped) =
                        crate::studio_canvas::sync_canvas_selection(&selection, &rows);
                    let (width, height) = state.session.canvas();
                    respond_json(
                        stream,
                        "200 OK",
                        &crate::studio_select::layers_json(
                            &shown,
                            width as f64,
                            height as f64,
                            &kept,
                            &dropped,
                            scope.as_deref(),
                        ),
                    );
                }
            }
        }
        // Full Layers panel (Sprint 18, T004 / CAP-004, J-04, INT-001,
        // INT-002): drag reorder, group, and ungroup commit through
        // the shared patch and history machinery as one named turn
        // each; enter and exit scope selection to a group as view
        // state with nothing staged. Every receipt echoes the synced
        // selection so Layers, canvas, and timeline show the same
        // selection at once.
        ("POST", "/api/studio/layers") => {
            let req = match crate::studio_select::parse_layers_body(body) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            if state.conflict.is_some() {
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            let commits = match req.action.as_str() {
                "reorder" | "ungroup" => true,
                "group" => !req.keys.is_empty(),
                _ => false,
            };
            if commits && state.save_queue.paused {
                respond_text(
                    stream,
                    "409 Conflict",
                    "studio: the save queue is paused; resume it before Layers reorder, group, or ungroup apply. Nothing was applied or recorded.",
                );
                return;
            }
            let composition = query_get(query, "composition")
                .unwrap_or_else(|| state.session.composition_name().to_string());
            match find_studio_composition(state, Some(&composition)) {
                None => respond_text(
                    stream,
                    "404 Not Found",
                    &unknown_composition(state, Some(composition)),
                ),
                Some(base) => {
                    let shown = readout_composition(state, &base, Some(&composition));
                    let rows = crate::studio::studio_rows_of(&shown);
                    let (kept, dropped) =
                        crate::studio_canvas::sync_canvas_selection(&req.selection, &rows);
                    match crate::studio_select::apply_layers(
                        &state.folder,
                        Some(&composition),
                        &req,
                        &shown,
                    ) {
                        Err(message) => {
                            let status = if message.contains("names no clip")
                                || message.contains("names nothing")
                                || message.contains("names no layer group")
                                || message.contains("names no grouped members")
                                || message.contains("past the open composition")
                            {
                                "404 Not Found"
                            } else if message.contains("not JSON")
                                || message.contains("names no `")
                                || message.contains("not a document index")
                                || message.contains("is not a clip reference")
                                || message.contains("unknown layers action")
                            {
                                "400 Bad Request"
                            } else {
                                "409 Conflict"
                            };
                            respond_text(stream, status, &message);
                        }
                        Ok(receipt) => respond_json(
                            stream,
                            "200 OK",
                            &crate::studio_select::layers_receipt_json(&receipt, &kept, &dropped),
                        ),
                    }
                }
            }
        }
        ("POST", "/api/studio/select") => {
            let req = match crate::studio_select::parse_select_body(body) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            if state.conflict.is_some() {
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            let composition = query_get(query, "composition")
                .unwrap_or_else(|| state.session.composition_name().to_string());
            match find_studio_composition(state, Some(&composition)) {
                None => respond_text(
                    stream,
                    "404 Not Found",
                    &unknown_composition(state, Some(composition)),
                ),
                Some(base) => {
                    let shown = readout_composition(state, &base, Some(&composition));
                    let (width, height) = state.session.canvas();
                    match crate::studio_select::resolve_selection(
                        &shown,
                        &req,
                        width as f64,
                        height as f64,
                    ) {
                        Err(message) => {
                            let status = if message.contains("names nothing") {
                                "404 Not Found"
                            } else {
                                "409 Conflict"
                            };
                            respond_text(stream, status, &message);
                        }
                        Ok(resolved) => {
                            let echo = if req.selection.is_empty() {
                                resolved.clone()
                            } else {
                                req.selection.clone()
                            };
                            let rows = crate::studio::studio_rows_of(&shown);
                            let (kept, dropped) =
                                crate::studio_canvas::sync_canvas_selection(&echo, &rows);
                            respond_json(
                                stream,
                                "200 OK",
                                &crate::studio_select::select_json(
                                    &shown,
                                    &req,
                                    &resolved,
                                    &kept,
                                    &dropped,
                                    width as f64,
                                    height as f64,
                                ),
                            );
                        }
                    }
                }
            }
        }
        // Canvas transforms (Sprint 17, T002 / CAP-002, J-01, INT-001,
        // REC-001..003): move, resize, nudge, and rotate gestures.
        // Transforms preview by default and stage nothing until the
        // maker commits; `cancelled` discards the preview. Commits
        // apply through the SDK patch and history machinery as one
        // named turn. A refused gesture names its rule and location
        // and stages nothing; a clamped resize names the bound and
        // the turn records the clamped result.
        ("POST", "/api/studio/transform") => {
            let req = match crate::studio_select::parse_transform_body(body) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let composition = query_get(query, "composition")
                .or_else(|| {
                    serde_json::from_str::<serde_json::Value>(body)
                        .ok()
                        .and_then(|v| {
                            v.get("composition")
                                .and_then(|c| c.as_str())
                                .map(str::to_string)
                        })
                })
                .unwrap_or_else(|| state.session.composition_name().to_string());
            match find_studio_composition(state, Some(&composition)) {
                None => respond_text(
                    stream,
                    "404 Not Found",
                    &unknown_composition(state, Some(composition)),
                ),
                Some(base) => {
                    let shown = readout_composition(state, &base, Some(&composition));
                    let rows = crate::studio::studio_rows_of(&shown);
                    let (kept, dropped) =
                        crate::studio_canvas::sync_canvas_selection(&req.selection, &rows);
                    if req.cancelled {
                        respond_json(
                            stream,
                            "200 OK",
                            &crate::studio_select::transform_cancelled_json(&req, &kept, &dropped),
                        );
                        return;
                    }
                    let (width, height) = state.session.canvas();
                    let film_w = width as f64;
                    let film_h = height as f64;
                    if req.action == "rotate" {
                        let (angle, snapped) = crate::studio_select::preview_rotate(
                            req.degrees.expect("rotate carries degrees"),
                            req.shift,
                        );
                        if !req.commit {
                            respond_json(
                                stream,
                                "200 OK",
                                &crate::studio_select::transform_preview_json(
                                    &req,
                                    &[],
                                    false,
                                    &[],
                                    Some((angle, snapped)),
                                    &kept,
                                    &dropped,
                                ),
                            );
                            return;
                        }
                        respond_text(
                            stream,
                            "409 Conflict",
                            &crate::studio_select::rotate_commit_refusal(
                                &shown, &req, angle, snapped,
                            ),
                        );
                        return;
                    }
                    if state.conflict.is_some() {
                        respond_text(
                            stream,
                            "409 Conflict",
                            &conflict_blocker_message(state.save_queue.staged.len()),
                        );
                        return;
                    }
                    if req.commit && state.save_queue.paused {
                        respond_text(
                            stream,
                            "409 Conflict",
                            "studio: the save queue is paused; resume it before canvas transforms apply. Nothing was applied or recorded.",
                        );
                        return;
                    }
                    match crate::studio_select::propose_transform(&shown, &req, film_w, film_h) {
                        Err(message) => {
                            let status = if message.contains("names nothing")
                                || message.contains("past the open composition")
                            {
                                "404 Not Found"
                            } else if message.contains("not JSON")
                                || message.contains("not a number")
                                || message.contains("not finite")
                                || message.contains("names no ")
                                || message.contains("unknown canvas")
                            {
                                "400 Bad Request"
                            } else {
                                "409 Conflict"
                            };
                            respond_text(stream, status, &message);
                        }
                        Ok((proposed, clamped, notes)) => {
                            if !req.commit {
                                respond_json(
                                    stream,
                                    "200 OK",
                                    &crate::studio_select::transform_preview_json(
                                        &req, &proposed, clamped, &notes, None, &kept, &dropped,
                                    ),
                                );
                                return;
                            }
                            match crate::studio_select::apply_transform(
                                &state.folder,
                                Some(&composition),
                                &req,
                                &proposed,
                            ) {
                                Ok(receipt) => respond_json(
                                    stream,
                                    "200 OK",
                                    &crate::studio_select::transform_receipt_json(
                                        &req, &receipt, &proposed, clamped, &notes, &kept, &dropped,
                                    ),
                                ),
                                Err(message) => respond_text(stream, "409 Conflict", &message),
                            }
                        }
                    }
                }
            }
        }
        // Crop mode (Sprint 17, T004 / CAP-004, J-02, INT-001,
        // INT-003, REC-001..003): Free, 16:9, 9:16, 1:1, and 4:5
        // presets constrain the crop frame with live preview.
        // Previews and cancels stage nothing; commits apply through
        // the SDK patch and history machinery as one named turn. A
        // refused crop names its rule and location and stages
        // nothing; an un-fittable frame clamps and shows the clamped
        // frame.
        ("POST", "/api/studio/crop") => {
            let req = match crate::studio_crop::parse_crop_body(body) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let composition = req
                .composition
                .clone()
                .or_else(|| query_get(query, "composition"))
                .unwrap_or_else(|| state.session.composition_name().to_string());
            match find_studio_composition(state, Some(&composition)) {
                None => respond_text(
                    stream,
                    "404 Not Found",
                    &unknown_composition(state, Some(composition)),
                ),
                Some(base) => {
                    let shown = readout_composition(state, &base, Some(&composition));
                    let rows = crate::studio::studio_rows_of(&shown);
                    let (kept, dropped) =
                        crate::studio_canvas::sync_canvas_selection(&req.selection, &rows);
                    if req.cancelled {
                        respond_json(
                            stream,
                            "200 OK",
                            &crate::studio_crop::crop_cancelled_json(&req, &kept, &dropped),
                        );
                        return;
                    }
                    if state.conflict.is_some() {
                        respond_text(
                            stream,
                            "409 Conflict",
                            &conflict_blocker_message(state.save_queue.staged.len()),
                        );
                        return;
                    }
                    if req.commit && state.save_queue.paused {
                        respond_text(
                            stream,
                            "409 Conflict",
                            "studio: the save queue is paused; resume it before crops apply. Nothing was applied or recorded.",
                        );
                        return;
                    }
                    let (width, height) = state.session.canvas();
                    let film_w = width as f64;
                    let film_h = height as f64;
                    match crate::studio_crop::propose_crop(&shown, &req, film_w, film_h) {
                        Err(message) => {
                            let status = if message.contains("names nothing")
                                || message.contains("past the open composition")
                            {
                                "404 Not Found"
                            } else {
                                "409 Conflict"
                            };
                            respond_text(stream, status, &message);
                        }
                        Ok((proposed, clamped, notes)) => {
                            if !req.commit {
                                respond_json(
                                    stream,
                                    "200 OK",
                                    &crate::studio_crop::crop_preview_json(
                                        &req, &proposed, clamped, &notes, &kept, &dropped,
                                    ),
                                );
                                return;
                            }
                            match crate::studio_crop::apply_crop(
                                &state.folder,
                                Some(&composition),
                                &req,
                                &proposed,
                            ) {
                                Ok(receipt) => respond_json(
                                    stream,
                                    "200 OK",
                                    &crate::studio_crop::crop_receipt_json(
                                        &req, &receipt, &proposed, clamped, &notes, &kept, &dropped,
                                    ),
                                ),
                                Err(message) => respond_text(stream, "409 Conflict", &message),
                            }
                        }
                    }
                }
            }
        }
        // Inline text editing sessions (Sprint 17, T006 / CAP-006,
        // J-03, STATE-03, REC-001..002): double-click opens the live
        // session with bold, italic, underline, and colour; typing
        // previews without staging; Enter commits as one patch turn
        // and Escape cancels with nothing staged. An invalid value
        // (for example an unknown font family) refuses with its
        // reason keeping the prior text; offered fonts always
        // resolve.
        ("GET", "/api/studio/text") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let composition = query_get(query, "composition")
                .unwrap_or_else(|| state.session.composition_name().to_string());
            match find_studio_composition(state, Some(&composition)) {
                None => respond_text(
                    stream,
                    "404 Not Found",
                    &unknown_composition(state, Some(composition)),
                ),
                Some(base) => {
                    let shown = readout_composition(state, &base, Some(&composition));
                    let target_raw = query_get(query, "target").unwrap_or_default();
                    let selection: Vec<String> = query_get(query, "selection")
                        .map(|raw| {
                            raw.split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect()
                        })
                        .unwrap_or_default();
                    let target = if target_raw.trim().is_empty() {
                        match selection.first() {
                            Some(first) => crate::studio_inspector::parse_inspector_target(first),
                            None => {
                                respond_text(
                                    stream,
                                    "400 Bad Request",
                                    "studio refused: the inline text session needs `?target=<id-or-index>` (or `?selection=<keys>` taking the first key); sessions open on one text clip. Nothing was staged.",
                                );
                                return;
                            }
                        }
                    } else {
                        crate::studio_inspector::parse_inspector_target(&target_raw)
                    };
                    let req = crate::studio_text::TextRequest {
                        action: "open".to_string(),
                        target,
                        value: None,
                        bold: None,
                        italic: None,
                        underline: None,
                        color: None,
                        font_family: None,
                        actor: String::new(),
                        selection: selection.clone(),
                    };
                    let rows = crate::studio::studio_rows_of(&shown);
                    let (kept, dropped) =
                        crate::studio_canvas::sync_canvas_selection(&req.selection, &rows);
                    match crate::studio_text::open_session(
                        &state.folder,
                        &shown,
                        &req,
                        &kept,
                        &dropped,
                    ) {
                        Ok(out) => respond_json(stream, "200 OK", &out),
                        Err(message) => {
                            let status = if message.contains("names nothing")
                                || message.contains("past the open composition")
                            {
                                "404 Not Found"
                            } else {
                                "409 Conflict"
                            };
                            respond_text(stream, status, &message);
                        }
                    }
                }
            }
        }
        ("POST", "/api/studio/text") => {
            let req = match crate::studio_text::parse_text_body(body) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let composition = query_get(query, "composition")
                .or_else(|| {
                    serde_json::from_str::<serde_json::Value>(body)
                        .ok()
                        .and_then(|v| {
                            v.get("composition")
                                .and_then(|c| c.as_str())
                                .map(str::to_string)
                        })
                })
                .unwrap_or_else(|| state.session.composition_name().to_string());
            match find_studio_composition(state, Some(&composition)) {
                None => respond_text(
                    stream,
                    "404 Not Found",
                    &unknown_composition(state, Some(composition)),
                ),
                Some(base) => {
                    let shown = readout_composition(state, &base, Some(&composition));
                    let rows = crate::studio::studio_rows_of(&shown);
                    let (kept, dropped) =
                        crate::studio_canvas::sync_canvas_selection(&req.selection, &rows);
                    if req.action == "cancel" {
                        respond_json(
                            stream,
                            "200 OK",
                            &crate::studio_text::text_cancelled_json(&req, &kept, &dropped),
                        );
                        return;
                    }
                    if req.action == "open" {
                        match crate::studio_text::open_session(
                            &state.folder,
                            &shown,
                            &req,
                            &kept,
                            &dropped,
                        ) {
                            Ok(out) => respond_json(stream, "200 OK", &out),
                            Err(message) => {
                                let status = if message.contains("names nothing")
                                    || message.contains("past the open composition")
                                {
                                    "404 Not Found"
                                } else {
                                    "409 Conflict"
                                };
                                respond_text(stream, status, &message);
                            }
                        }
                        return;
                    }
                    if req.action == "preview" {
                        match crate::studio_text::preview_session(
                            &state.folder,
                            &shown,
                            &req,
                            &kept,
                            &dropped,
                        ) {
                            Ok(out) => respond_json(stream, "200 OK", &out),
                            Err(message) => {
                                let status = if message.contains("names nothing")
                                    || message.contains("past the open composition")
                                {
                                    "404 Not Found"
                                } else if message.contains("not JSON")
                                    || message.contains("names no ")
                                    || message.contains("unknown inline text")
                                    || message.contains("not a switch")
                                    || message.contains("not text")
                                {
                                    "400 Bad Request"
                                } else {
                                    "409 Conflict"
                                };
                                respond_text(stream, status, &message);
                            }
                        }
                        return;
                    }
                    if state.conflict.is_some() {
                        respond_text(
                            stream,
                            "409 Conflict",
                            &conflict_blocker_message(state.save_queue.staged.len()),
                        );
                        return;
                    }
                    if state.save_queue.paused {
                        respond_text(
                            stream,
                            "409 Conflict",
                            "studio: the save queue is paused; resume it before inline text commits apply. Nothing was applied or recorded.",
                        );
                        return;
                    }
                    match crate::studio_text::apply_text_edit(
                        &state.folder,
                        Some(&composition),
                        &req,
                    ) {
                        Ok(receipt) => respond_json(
                            stream,
                            "200 OK",
                            &crate::studio_text::text_receipt_json(&receipt, &kept, &dropped),
                        ),
                        Err(message) => {
                            let status = if message.contains("names nothing")
                                || message.contains("past the open composition")
                            {
                                "404 Not Found"
                            } else if message.contains("names no ")
                                || message.contains("unknown inline text")
                            {
                                "400 Bad Request"
                            } else {
                                "409 Conflict"
                            };
                            respond_text(stream, status, &message);
                        }
                    }
                }
            }
        }
        // Canvas transport extras (Sprint 17, T005 / CAP-005): J, K,
        // L shuttle at 1x, 2x, and 4x with K pausing, one-frame arrow
        // steps, the 0.25x to 2x speed menu, the seek bar under the
        // canvas, and playback health reporting the clock followed and
        // the still-picture duration. A film short of data holds its
        // last frame with the hold reported, never looping a tail
        // snippet. Transport moves no composition data: every action
        // drives the serving-session playhead (the Sprint 15 player
        // clock), persists only the menu speed in the hidden transport
        // file, and stages no patch and records no turn.
        ("GET", "/api/studio/transport") => {
            let composition = query_get(query, "composition")
                .unwrap_or_else(|| state.session.composition_name().to_string());
            match find_studio_composition(state, Some(&composition)) {
                None => respond_text(
                    stream,
                    "404 Not Found",
                    &unknown_composition(state, Some(composition)),
                ),
                Some(base) => {
                    let shown = readout_composition(state, &base, Some(&composition));
                    let selection: Vec<String> = query_get(query, "selection")
                        .map(|raw| {
                            raw.split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect()
                        })
                        .unwrap_or_default();
                    let body = render_transport_state(state, &shown, &selection, None);
                    respond_json(stream, "200 OK", &body);
                }
            }
        }
        ("POST", "/api/studio/transport") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let parsed = match crate::studio_transport::parse_transport_body(body) {
                Ok(parsed) => parsed,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            let request = parsed.request;
            let selection = parsed.selection;
            let composition = parsed.composition;
            let composition = composition
                .or_else(|| query_get(query, "composition"))
                .unwrap_or_else(|| state.session.composition_name().to_string());
            match find_studio_composition(state, Some(&composition)) {
                None => respond_text(
                    stream,
                    "404 Not Found",
                    &unknown_composition(state, Some(composition)),
                ),
                Some(base) => {
                    let shown = readout_composition(state, &base, Some(&composition));
                    let echo = selection.clone().unwrap_or_default();
                    match run_studio_transport(state, request) {
                        Err((status, message)) => respond_text(stream, &status, &message),
                        Ok(detail) => {
                            let body = render_transport_state(state, &shown, &echo, Some(&detail));
                            respond_json(stream, "200 OK", &body);
                        }
                    }
                }
            }
        }
        // Remappable shortcuts (Sprint 16, T008 / CAP-009, J-04,
        // INT-003, REC-004): the full default map drives playback, the
        // work area, tools, edit, and panels; a project-level remap
        // overrides single keys and chords alike, takes effect
        // immediately, and leaves the default map visible underneath; a
        // describe-a-change action copies a ready agent request from the
        // selection. An unparseable remap falls back to defaults naming
        // the bad entry; a colliding chord keeps the old binding and
        // reports the collision.
        ("GET", "/api/studio/shortcuts") => {
            let load = crate::studio_shortcuts::load_shortcuts(&state.folder);
            respond_json(
                stream,
                "200 OK",
                &crate::studio_shortcuts::shortcuts_json(&load),
            );
        }
        ("POST", "/api/studio/shortcuts") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            match crate::studio_shortcuts::save_remap(&state.folder, body) {
                Ok(load) => respond_json(
                    stream,
                    "200 OK",
                    &crate::studio_shortcuts::shortcuts_json(&load),
                ),
                Err(message) => {
                    let status =
                        if message.contains("collision") || message.contains("already drives it") {
                            "409 Conflict"
                        } else {
                            "400 Bad Request"
                        };
                    respond_text(stream, status, &message);
                }
            }
        }
        // Shortcut dispatch (Sprint 16, T008): one pressed chord or
        // named action drives its area against the serving session —
        // transport, work area, SDK-backed tools and undo, the agent
        // handoff, or panel state. A refused dispatch stages nothing
        // and names its cause.
        ("POST", "/api/studio/shortcut") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let load = crate::studio_shortcuts::load_shortcuts(&state.folder);
            let req = match crate::studio_shortcuts::parse_dispatch_body(body, &load.bindings) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            match run_shortcut_dispatch(state, &req) {
                Ok(out) => respond_json(stream, "200 OK", &out),
                Err((status, message)) => respond_text(stream, &status, &message),
            }
        }
        // Describe-a-change handoff (Sprint 16, T008 / INT-003): copy a
        // ready agent request from the current selection, naming the
        // composition, playhead, and selected windows.
        ("GET", "/api/studio/describe") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "404 Not Found",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let selection: Vec<String> = query_get(query, "selection")
                .map(|raw| {
                    raw.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            match describe_selection(state, &selection) {
                Ok(out) => respond_json(stream, "200 OK", &out),
                Err((status, message)) => respond_text(stream, &status, &message),
            }
        }
        // Code panel (Sprint 18, T003 / CAP-003, J-03, REC-003):
        // the file tree, the notation editor, and jump to the
        // selected element's source line. Reads never write and
        // never block; mutations travel validated turns (tracked
        // `*.kdl` sources) or direct writes with restore-on-failure,
        // and refuse while a conflict pends or the save queue pauses
        // — like every other composition-affecting Studio gesture.
        ("GET", "/api/studio/code/tree") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            match crate::studio_code::list_tree(&state.folder) {
                Ok(entries) => respond_json(
                    stream,
                    "200 OK",
                    &crate::studio_code::tree_json(&entries),
                ),
                Err(message) => respond_text(stream, "409 Conflict", &message),
            }
        }
        ("GET", "/api/studio/code/file") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let rel = match query_get(query, "path") {
                Some(p) if !p.trim().is_empty() => p,
                _ => {
                    respond_text(
                        stream,
                        "400 Bad Request",
                        "studio refused: the code file read needs `?path=<project-relative-file>`; pass the file to open. Nothing changed.",
                    );
                    return;
                }
            };
            match crate::studio_code::read_code_file(&state.folder, &rel) {
                Ok(file) => respond_json(
                    stream,
                    "200 OK",
                    &crate::studio_code::code_file_json(&file),
                ),
                Err(message) => respond_text(stream, "409 Conflict", &message),
            }
        }
        ("POST", "/api/studio/code/tree") => {
            let action = match crate::studio_code::parse_tree_action_body(body) {
                Ok(action) => action,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            if state.conflict.is_some() {
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            if state.save_queue.paused {
                respond_text(
                    stream,
                    "409 Conflict",
                    "studio: the save queue is paused; resume it before tree changes apply. Nothing was applied or recorded.",
                );
                return;
            }
            match crate::studio_code::apply_tree_action(&state.folder, &action) {
                Ok(receipt) => {
                    if receipt.path == validate::validate::COMPOSITION_FILE {
                        state.kdl_disk = crate::studio::disk_version_of(&state.folder);
                    }
                    respond_json(
                        stream,
                        "200 OK",
                        &crate::studio_code::tree_receipt_json(&receipt),
                    )
                }
                Err(message) => respond_text(stream, "409 Conflict", &message),
            }
        }
        ("POST", "/api/studio/code/save") => {
            let save = match crate::studio_code::parse_save_body(body) {
                Ok(save) => save,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            if state.conflict.is_some() {
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            if state.save_queue.paused {
                respond_text(
                    stream,
                    "409 Conflict",
                    "studio: the save queue is paused; resume it before code saves apply. Nothing was applied or recorded.",
                );
                return;
            }
            match crate::studio_code::save_code_file(&state.folder, &save) {
                Ok(receipt) => {
                    if receipt.path == validate::validate::COMPOSITION_FILE {
                        state.kdl_disk = crate::studio::disk_version_of(&state.folder);
                    }
                    respond_json(
                        stream,
                        "200 OK",
                        &crate::studio_code::save_receipt_json(&receipt),
                    )
                }
                Err(message) => respond_text(stream, "409 Conflict", &message),
            }
        }
        ("GET", "/api/studio/code/jump") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let key = match query_get(query, "selection") {
                Some(k) if !k.trim().is_empty() => k,
                _ => {
                    respond_text(
                        stream,
                        "400 Bad Request",
                        "studio refused: the code jump needs `?selection=<id-or-#index>` from the canvas or timeline; pass the selected element. Nothing changed.",
                    );
                    return;
                }
            };
            let composition = query_get(query, "composition");
            match crate::studio_code::jump_to_selection(
                &state.folder,
                composition.as_deref(),
                &key,
            ) {
                Ok(jump) => respond_json(stream, "200 OK", &crate::studio_code::jump_json(&jump)),
                Err(message) => respond_text(stream, "404 Not Found", &message),
            }
        }
        ("GET", "/api/studio/assets") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "404 Not Found",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let parsed = match crate::studio_assets::parse_assets_query(query) {
                Ok(parsed) => parsed,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            let comps = state.session.all_compositions().to_vec();
            if parsed.scope == crate::studio_assets::AssetScope::All {
                let projects =
                    crate::studio_assets::discover_projects(&state.folder, &comps);
                respond_json(
                    stream,
                    "200 OK",
                    &crate::studio_assets::all_projects_json(&projects, &parsed),
                );
                return;
            }
            let listed = crate::studio_assets::list_assets(&state.folder, &comps);
            let kept = crate::studio_assets::filter_assets(&listed, &parsed);
            respond_json(
                stream,
                "200 OK",
                &crate::studio_assets::assets_json(
                    &crate::studio_assets::served_project_name(&state.folder),
                    &kept,
                ),
            );
        }
        ("POST", "/api/studio/assets/import") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            if state.conflict.is_some() {
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            if state.save_queue.paused {
                respond_text(
                    stream,
                    "409 Conflict",
                    "studio: the save queue is paused; resume it before assets import. Nothing was copied.",
                );
                return;
            }
            let req = match crate::studio_assets::parse_import_body(body) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            match crate::studio_assets::import_asset(&state.folder, &req) {
                Ok(receipt) => respond_json(
                    stream,
                    "200 OK",
                    &crate::studio_assets::import_receipt_json(&receipt),
                ),
                Err(message) => {
                    let status = if message.contains("does not exist") {
                        "404 Not Found"
                    } else {
                        "409 Conflict"
                    };
                    respond_text(stream, status, &message);
                }
            }
        }
        ("POST", "/api/studio/assets") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            if state.conflict.is_some() {
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            if state.save_queue.paused {
                respond_text(
                    stream,
                    "409 Conflict",
                    "studio: the save queue is paused; resume it before assets act. Nothing was staged.",
                );
                return;
            }
            let served = crate::studio_assets::served_project_name(&state.folder);
            if let Some(want) = query_get(query, "project") {
                if want.trim() != served {
                    respond_text(
                        stream,
                        "409 Conflict",
                        &format!(
                            "studio refused: project {want:?} is read-only in this view; only {served:?} accepts asset actions. Nothing was staged."
                        ),
                    );
                    return;
                }
            }
            let mut req = match crate::studio_assets::parse_action_body(body) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            // An insert without a named composition targets the
            // served composition, never an arbitrary first project
            // entry.
            if req.action == "insert" && req.composition.is_none() {
                req.composition = Some(state.session.composition_name().to_string());
            }
            let comps = state.session.all_compositions().to_vec();
            let outcome = match req.action.as_str() {
                "reveal" => crate::studio_assets::reveal_asset(&state.folder, &req.asset),
                "rename" => crate::studio_assets::rename_asset(
                    &state.folder,
                    &comps,
                    &req.asset,
                    req.name.as_deref(),
                ),
                "remove" => {
                    crate::studio_assets::remove_asset(&state.folder, &comps, &req.asset)
                }
                _ => crate::studio_assets::insert_asset(&state.folder, &comps, &req),
            };
            match outcome {
                Ok(receipt) => respond_json(stream, "200 OK", &receipt),
                Err(message) => {
                    let status = if message.contains("is not in the project")
                        || message.contains("unknown composition")
                    {
                        "404 Not Found"
                    } else {
                        "409 Conflict"
                    };
                    respond_text(stream, status, &message);
                }
            }
        }
        // Clipboard and drop panel (Sprint 18, T008 / CAP-008, J-06,
        // STATE-02, REC-001, REC-004): copy returns a self-contained
        // payload, paste lands it at the playhead with renamed ids,
        // cut removes only after its copy confirms, duplicate pastes
        // in place, and drops place files, assets, and compositions
        // at the drop point. Incompatible payloads and unsupported
        // drops refuse by name staging nothing.
        ("POST", "/api/studio/clipboard/copy") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            if state.conflict.is_some() {
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            let req = match crate::studio_clipboard::parse_selection_body(body, true) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            match crate::studio_clipboard::copy_clips(
                &state.folder,
                req.composition.as_deref(),
                &req.targets,
            ) {
                Ok(payload) => respond_json(
                    stream,
                    "200 OK",
                    &crate::studio_clipboard::clipboard_json(&payload),
                ),
                Err(message) => {
                    let status = if message.contains("unknown clip")
                        || message.contains("unknown composition")
                    {
                        "404 Not Found"
                    } else {
                        "409 Conflict"
                    };
                    respond_text(stream, status, &message);
                }
            }
        }
        ("POST", "/api/studio/clipboard/paste") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            if state.conflict.is_some() {
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            if state.save_queue.paused {
                respond_text(
                    stream,
                    "409 Conflict",
                    "studio: the save queue is paused; resume it before pasting. Nothing was staged.",
                );
                return;
            }
            let req = match crate::studio_clipboard::parse_paste_body(body) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            match crate::studio_clipboard::paste_clips(
                &state.folder,
                req.composition.as_deref(),
                &req.payload,
                req.playhead,
                &req.actor,
                None,
            ) {
                Ok(receipt) => respond_json(
                    stream,
                    "200 OK",
                    &crate::studio_clipboard::paste_receipt_json(&receipt),
                ),
                Err(message) => {
                    let status = if message.contains("unknown composition")
                        || message.contains("missing from the target project")
                        || message.contains("does not hold")
                    {
                        "404 Not Found"
                    } else {
                        "409 Conflict"
                    };
                    respond_text(stream, status, &message);
                }
            }
        }
        ("POST", "/api/studio/clipboard/cut") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            if state.conflict.is_some() {
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            if state.save_queue.paused {
                respond_text(
                    stream,
                    "409 Conflict",
                    "studio: the save queue is paused; resume it before cutting. Nothing was staged.",
                );
                return;
            }
            let req = match crate::studio_clipboard::parse_selection_body(body, false) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            match crate::studio_clipboard::cut_clips(
                &state.folder,
                req.composition.as_deref(),
                &req.targets,
                &req.actor,
            ) {
                Ok(receipt) => respond_json(
                    stream,
                    "200 OK",
                    &crate::studio_clipboard::cut_receipt_json(&receipt),
                ),
                Err(message) => {
                    let status = if message.contains("unknown clip")
                        || message.contains("unknown composition")
                    {
                        "404 Not Found"
                    } else {
                        "409 Conflict"
                    };
                    respond_text(stream, status, &message);
                }
            }
        }
        ("POST", "/api/studio/clipboard/duplicate") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            if state.conflict.is_some() {
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            if state.save_queue.paused {
                respond_text(
                    stream,
                    "409 Conflict",
                    "studio: the save queue is paused; resume it before duplicating. Nothing was staged.",
                );
                return;
            }
            let req = match crate::studio_clipboard::parse_selection_body(body, false) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            match crate::studio_clipboard::duplicate_clips(
                &state.folder,
                req.composition.as_deref(),
                &req.targets,
                &req.actor,
            ) {
                Ok(receipt) => respond_json(
                    stream,
                    "200 OK",
                    &crate::studio_clipboard::duplicate_receipt_json(&receipt),
                ),
                Err(message) => {
                    let status = if message.contains("unknown clip")
                        || message.contains("unknown composition")
                    {
                        "404 Not Found"
                    } else {
                        "409 Conflict"
                    };
                    respond_text(stream, status, &message);
                }
            }
        }
        ("POST", "/api/studio/clipboard/drop") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            if state.conflict.is_some() {
                respond_text(
                    stream,
                    "409 Conflict",
                    &conflict_blocker_message(state.save_queue.staged.len()),
                );
                return;
            }
            if state.save_queue.paused {
                respond_text(
                    stream,
                    "409 Conflict",
                    "studio: the save queue is paused; resume it before dropping. Nothing was staged.",
                );
                return;
            }
            let req = match crate::studio_clipboard::parse_drop_body(body) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            match crate::studio_clipboard::drop_onto_timeline(&state.folder, &req) {
                Ok(receipt) => respond_json(
                    stream,
                    "200 OK",
                    &crate::studio_clipboard::drop_receipt_json(&receipt),
                ),
                Err(message) => {
                    let status = if message.contains("unknown composition")
                        || message.contains("is not in the project")
                        || message.contains("does not hold")
                    {
                        "404 Not Found"
                    } else {
                        "409 Conflict"
                    };
                    respond_text(stream, status, &message);
                }
            }
        }
        // Renders panel and queue (Sprint 18, T006 / CAP-006, CAP-009,
        // J-05, STATE-03, STATE-04, REC-002, REC-003, REC-005): jobs
        // queue with settings, run in order with measured progress
        // and an ETA estimate, cancel with the stage reached,
        // download byte-matches, delete refuses running jobs, and
        // hide-finished clears completed rows only.
        ("GET", "/api/studio/renders") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "404 Not Found",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let (available, detail) = crate::studio_renders::ffmpeg_status(None);
            match crate::studio_compositions::list_render_queue(&state.folder) {
                Ok(jobs) => respond_json(
                    stream,
                    "200 OK",
                    &crate::studio_renders::renders_queue_json(
                        &jobs,
                        available,
                        &detail,
                        crate::studio_renders::now_ms(),
                    ),
                ),
                Err(message) => respond_text(stream, "409 Conflict", &message),
            }
        }
        ("POST", "/api/studio/renders") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let req = match crate::studio_renders::parse_enqueue_body(body) {
                Ok(req) => req,
                Err(message) => {
                    respond_text(stream, "400 Bad Request", &message);
                    return;
                }
            };
            match crate::studio_renders::enqueue_job(&state.folder, &req, None) {
                Ok(receipt) => respond_json(
                    stream,
                    "200 OK",
                    &crate::studio_renders::enqueue_receipt_json(&receipt),
                ),
                Err(message) => {
                    let status = if message.contains("unknown composition") {
                        "404 Not Found"
                    } else {
                        "409 Conflict"
                    };
                    respond_text(stream, status, &message);
                }
            }
        }
        ("POST", "/api/studio/renders/hide-finished") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            match crate::studio_renders::hide_finished(&state.folder) {
                Ok(report) => respond_text(stream, "200 OK", &report),
                Err(message) => respond_text(stream, "409 Conflict", &message),
            }
        }
        ("POST", p) if p.starts_with("/api/studio/renders/") && p.ends_with("/cancel") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let seq = match p
                .strip_prefix("/api/studio/renders/")
                .and_then(|s| s.strip_suffix("/cancel"))
                .and_then(|s| s.parse::<u64>().ok())
            {
                Some(seq) => seq,
                None => {
                    respond_text(
                        stream,
                        "400 Bad Request",
                        "studio refused: cancel needs `/api/studio/renders/{seq}/cancel` (a job sequence); pass the row to cancel. Nothing changed.",
                    );
                    return;
                }
            };
            match crate::studio_renders::cancel_job(&state.folder, seq) {
                Ok(report) => respond_text(stream, "200 OK", &report),
                Err(message) => {
                    let status = if message.contains("unknown job") {
                        "404 Not Found"
                    } else {
                        "409 Conflict"
                    };
                    respond_text(stream, status, &message);
                }
            }
        }
        ("DELETE", p) if p.starts_with("/api/studio/renders/") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "409 Conflict",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let seq = match p
                .strip_prefix("/api/studio/renders/")
                .and_then(|s| s.parse::<u64>().ok())
            {
                Some(seq) => seq,
                None => {
                    respond_text(
                        stream,
                        "400 Bad Request",
                        "studio refused: delete needs `/api/studio/renders/{seq}` (a job sequence); pass the row to delete. Nothing was deleted.",
                    );
                    return;
                }
            };
            match crate::studio_renders::delete_job(&state.folder, seq) {
                Ok(report) => respond_text(stream, "200 OK", &report),
                Err(message) => {
                    let status = if message.contains("unknown job") {
                        "404 Not Found"
                    } else {
                        "409 Conflict"
                    };
                    respond_text(stream, status, &message);
                }
            }
        }
        ("GET", p) if p.starts_with("/api/studio/renders/") && p.ends_with("/download") => {
            if !crate::studio::project_reachable(&state.folder) {
                respond_text(
                    stream,
                    "404 Not Found",
                    &crate::studio::unreachable_notice(&state.folder),
                );
                return;
            }
            let seq = match p
                .strip_prefix("/api/studio/renders/")
                .and_then(|s| s.strip_suffix("/download"))
                .and_then(|s| s.parse::<u64>().ok())
            {
                Some(seq) => seq,
                None => {
                    respond_text(
                        stream,
                        "400 Bad Request",
                        "studio refused: download needs `/api/studio/renders/{seq}/download` (a job sequence); pass the row to download.",
                    );
                    return;
                }
            };
            match crate::studio_renders::download_job(&state.folder, seq) {
                Ok((output, bytes)) => respond(
                    stream,
                    "200 OK",
                    crate::studio_renders::download_content_type(&output),
                    &bytes,
                ),
                Err(message) => {
                    let status = if message.contains("unknown job") {
                        "404 Not Found"
                    } else {
                        "409 Conflict"
                    };
                    respond_text(stream, status, &message);
                }
            }
        }
        _ => respond_text(
            stream,
            "404 Not Found",
            &format!("preview refused: unknown route {method} {path}; open / for the review page"),
        ),
    }
}

/// Drive one canvas transport action against the serving session
/// (Sprint 17, T005 / CAP-005): shuttle, steps, the speed menu, seeks,
/// and player-clock ticks. Every action moves only the
/// serving-session playhead — no patch stages and no history turn
/// records — and only a validated menu speed persists in the hidden
/// transport file. Returns the strip detail line; a refused action
/// stages nothing and reports its status with its cause.
fn run_studio_transport(
    state: &mut ServerState,
    request: crate::studio_transport::TransportRequest,
) -> Result<String, (String, String)> {
    use crate::studio_transport::{
        advance_position, hold_first_message, hold_last_message, load_transport, save_transport,
        TransportRequest as Req, SHUTTLE_RATES,
    };
    let now = std::time::Instant::now();
    let bad = |message: String| (String::from("409 Conflict"), message);
    let saved = load_transport(&state.folder);
    match request {
        Req::Play => {
            state
                .transport
                .set_speed(saved.settings.speed)
                .map_err(|e| bad(e.to_string()))?;
            state
                .session
                .play_with_audio(&mut state.transport)
                .map_err(|e| bad(e.to_string()))?;
            let live = &mut state.studio_transport_live;
            live.shuttle.play_plain();
            live.note_frame(state.transport.position(), now);
        }
        Req::Pause | Req::ShuttleK => {
            state.transport.pause();
            let live = &mut state.studio_transport_live;
            if matches!(request, Req::ShuttleK) {
                live.shuttle.press_k();
            } else {
                live.shuttle.pause_plain();
            }
            live.note_frame(state.transport.position(), now);
        }
        Req::ShuttleL | Req::ShuttleJ => {
            let forward = matches!(request, Req::ShuttleL);
            {
                let live = &mut state.studio_transport_live;
                if forward {
                    live.shuttle.press_l();
                } else {
                    live.shuttle.press_j();
                }
            }
            let rate = state
                .studio_transport_live
                .shuttle
                .shuttle_rate
                .unwrap_or(SHUTTLE_RATES[0]);
            state
                .transport
                .set_speed(rate)
                .map_err(|e| bad(e.to_string()))?;
            state
                .session
                .play_with_audio(&mut state.transport)
                .map_err(|e| bad(e.to_string()))?;
            let pos = state.transport.position();
            let live = &mut state.studio_transport_live;
            live.note_frame(pos, now);
        }
        Req::StepForward => {
            state
                .transport
                .step_forward()
                .map_err(|e| bad(e.to_string()))?;
            let pos = state.transport.position();
            state.studio_transport_live.note_frame(pos, now);
        }
        Req::StepBack => {
            state
                .transport
                .step_back()
                .map_err(|e| bad(e.to_string()))?;
            let pos = state.transport.position();
            state.studio_transport_live.note_frame(pos, now);
        }
        Req::Seek(index) => {
            state
                .transport
                .scrub_to(index)
                .map_err(|e| bad(e.to_string()))?;
            let pos = state.transport.position();
            state.studio_transport_live.note_frame(pos, now);
        }
        Req::SetSpeed(speed) => {
            state
                .transport
                .set_speed(speed)
                .map_err(|e| bad(e.to_string()))?;
            let settings = crate::studio_transport::TransportSettings {
                speed,
                loop_enabled: saved.settings.loop_enabled,
            };
            save_transport(&state.folder, &settings)
                .map_err(|message| (String::from("500 Internal Server Error"), message))?;
            let live = &mut state.studio_transport_live;
            live.shuttle.shuttle_rate = None;
            live.note_frame(state.transport.position(), now);
        }
        Req::Tick(elapsed_secs) => {
            if state.transport.readout().playing {
                let frames = state.session.frame_count();
                let fps = state.session.fps().value();
                let pos = state.transport.position();
                let shuttle = state.studio_transport_live.shuttle;
                let rate = shuttle.effective_rate(saved.settings.speed);
                let reverse =
                    shuttle.direction == crate::studio_transport::ShuttleDirection::Reverse;
                let (next, carry, parked) = advance_position(
                    pos,
                    frames,
                    fps,
                    elapsed_secs,
                    rate,
                    reverse,
                    state.studio_transport_live.carry,
                );
                if next != pos {
                    state
                        .transport
                        .scrub_to(next)
                        .map_err(|e| bad(e.to_string()))?;
                }
                let live = &mut state.studio_transport_live;
                live.note_frame(next, now);
                live.carry = carry;
                if parked {
                    state.transport.pause();
                    let live = &mut state.studio_transport_live;
                    live.shuttle.playing = false;
                    live.held = Some(if reverse {
                        hold_first_message(frames)
                    } else {
                        hold_last_message(frames.saturating_sub(1), frames)
                    });
                }
            }
        }
    }
    let live = &state.studio_transport_live;
    let readout = state.transport.readout();
    Ok(crate::studio_transport::transport_detail(
        &live.shuttle,
        saved.settings.speed,
        readout.frame,
        readout.frame_count,
        readout.playing,
    ))
}

/// Render one canvas transport state (Sprint 17, T005): the playhead
/// with its readout, the shuttle, the 0.25x to 2x speed menu, the
/// seek bar, playback health, the synced selection, and the
/// `font-family` pass-through. A moved playhead restarts the
/// still-picture watch here too, so health stays honest however the
/// playhead moved.
fn render_transport_state(
    state: &mut ServerState,
    shown: &notation::model::Composition,
    selection: &[String],
    detail: Option<&str>,
) -> String {
    use crate::studio_transport::{load_transport, transport_state_json};
    let now = std::time::Instant::now();
    let saved = load_transport(&state.folder);
    let pos = state.transport.position();
    {
        let live = &mut state.studio_transport_live;
        if pos != live.last_frame {
            live.note_frame(pos, now);
        }
    }
    let live = &state.studio_transport_live;
    let readout = state.transport.readout();
    // The loop switch lives on the serving-session playhead (shared
    // with the review page); echo its live value, not the file copy.
    let mut effective = saved.settings;
    effective.loop_enabled = readout.loop_enabled;
    let rows = crate::studio::studio_rows_of(shown);
    let (kept, dropped) = crate::studio_canvas::sync_canvas_selection(selection, &rows);
    let empty_reason = state.transport.disabled_reason();
    let has_audio = state.session.mix_info().ok().flatten().is_some();
    let detail = detail.map(str::to_string).unwrap_or_else(|| {
        crate::studio_transport::transport_detail(
            &live.shuttle,
            saved.settings.speed,
            readout.frame,
            readout.frame_count,
            readout.playing,
        )
    });
    transport_state_json(
        &effective,
        saved.restored,
        saved.notice.as_deref(),
        &live.shuttle,
        readout.frame,
        readout.frame_count,
        readout.time_secs,
        readout.duration_secs,
        readout.fps.value(),
        readout.playing,
        readout.enabled,
        empty_reason.as_deref(),
        has_audio,
        live.still_secs(now),
        live.held.as_deref(),
        &detail,
        pos,
        &kept,
        &dropped,
        &crate::studio_canvas::canvas_font_lists_of(shown),
    )
}

/// Drive one dispatched shortcut against the serving session (Sprint
/// 16, T008 / CAP-009, INT-003): transport, work area, SDK-backed
/// tools and undo, the agent handoff, or panel state. Tool and undo
/// actions apply through the same SDK patch/history, work-area, theme,
/// and layout machinery as their buttons, then reload the session so
/// rows and frames show the new look with the playhead kept. Returns
/// the full response body; a refused dispatch stages nothing and
/// reports its status with its cause.
fn run_shortcut_dispatch(
    state: &mut ServerState,
    req: &crate::studio_shortcuts::DispatchRequest,
) -> Result<String, (String, String)> {
    use crate::studio_shortcuts::ShortcutAction;
    let bad = |message: String| (String::from("409 Conflict"), message);
    match req.action {
        ShortcutAction::SplitAtPlayhead
        | ShortcutAction::SelectBefore
        | ShortcutAction::SelectAfter
        | ShortcutAction::Undo
            if state.conflict.is_some() =>
        {
            // STATE-04: editing pauses until the maker's pick lands, so
            // a dispatched tool or undo never merges over the disk
            // version without an explicit choice.
            return Err((
                String::from("409 Conflict"),
                conflict_blocker_message(state.save_queue.staged.len()),
            ));
        }
        _ => {}
    }
    let composition = state.session.composition_name().to_string();
    let fps = state.session.fps().value();
    let frame_count = state.session.frame_count();
    let playhead_secs = if fps > 0.0 {
        state.transport.position() as f64 / fps
    } else {
        0.0
    };
    let duration = state.session.duration_secs();
    let effect = match req.action {
        ShortcutAction::PlayToggle => {
            if state.transport.readout().playing {
                state.transport.pause();
            } else if let Err(e) = state.session.play_with_audio(&mut state.transport) {
                return Err((String::from("409 Conflict"), e.to_string()));
            }
            let playing = state.transport.readout().playing;
            format!("{{\"playing\":{playing}}}")
        }
        ShortcutAction::StepForward => {
            state
                .transport
                .step_forward()
                .map_err(|e| bad(e.to_string()))?;
            String::new()
        }
        ShortcutAction::StepBack => {
            state
                .transport
                .step_back()
                .map_err(|e| bad(e.to_string()))?;
            String::new()
        }
        ShortcutAction::GoToStart => {
            state
                .transport
                .scrub_to(0)
                .map_err(|e| bad(e.to_string()))?;
            String::new()
        }
        ShortcutAction::GoToEnd => {
            let last = frame_count.saturating_sub(1);
            state
                .transport
                .scrub_to(last)
                .map_err(|e| bad(e.to_string()))?;
            String::new()
        }
        ShortcutAction::ToggleLoop => {
            let on = !state.transport.readout().loop_enabled;
            state.transport.set_loop(on);
            format!("{{\"loop\":{on}}}")
        }
        ShortcutAction::SpeedUp => {
            let speed = state.transport.readout().speed + 0.25;
            state
                .transport
                .set_speed(speed)
                .map_err(|e| bad(e.to_string()))?;
            format!("{{\"speed\":{speed}}}")
        }
        ShortcutAction::SpeedDown => {
            let speed = (state.transport.readout().speed - 0.25).max(0.25);
            state
                .transport
                .set_speed(speed)
                .map_err(|e| bad(e.to_string()))?;
            format!("{{\"speed\":{speed}}}")
        }
        ShortcutAction::SetWorkIn | ShortcutAction::SetWorkOut => {
            let area = crate::studio_tools::load_work_area(&state.folder);
            let (inp, outp) = match req.action {
                ShortcutAction::SetWorkIn => {
                    (playhead_secs, area.area.out_point.unwrap_or(duration))
                }
                _ => (area.area.in_point.unwrap_or(0.0), playhead_secs),
            };
            let area_body = format!("{{\"in\":{inp},\"out\":{outp}}}");
            crate::studio_tools::set_work_area(&state.folder, &area_body, duration).map_err(bad)?;
            let fresh = crate::studio_tools::load_work_area(&state.folder);
            format!(
                "{{\"work_area\":{}}}",
                crate::studio_tools::work_area_json(&fresh)
            )
        }
        ShortcutAction::ClearWorkArea => {
            crate::studio_tools::clear_work_area(&state.folder);
            let fresh = crate::studio_tools::load_work_area(&state.folder);
            format!(
                "{{\"work_area\":{}}}",
                crate::studio_tools::work_area_json(&fresh)
            )
        }
        ShortcutAction::ToolSelect => "{\"panel_tool\":\"select\"}".to_string(),
        ShortcutAction::ToolRazor => "{\"panel_tool\":\"razor\"}".to_string(),
        ShortcutAction::SplitAtPlayhead
        | ShortcutAction::SelectBefore
        | ShortcutAction::SelectAfter => {
            let tool_body = format!("{{\"tool\":\"{}\"}}", req.action.as_str());
            let call = crate::studio_tools::parse_tool_body(&tool_body)
                .map_err(|message| (String::from("400 Bad Request"), message))?;
            let receipt = crate::studio_tools::apply_studio_tool(
                &state.folder,
                Some(&composition),
                &call,
                playhead_secs,
            )
            .map_err(bad)?;
            let report = state.session.reload_keep_playhead(&mut state.transport);
            state.kdl_disk = crate::studio::disk_version_of(&state.folder);
            state.last_reload = Some(report.to_string());
            let rows = crate::studio::timeline_rows_json(state.session.composition());
            let turn = match receipt.turn {
                Some(t) => t.to_string(),
                None => "null".to_string(),
            };
            let selection = receipt
                .selection
                .iter()
                .map(|s| format!("\"{}\"", crate::studio::json_escape(s)))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{{\"summary\":\"{}\",\"turn_summary\":\"{}\",\"applied\":{},\"turn\":{turn},\"selection\":[{selection}],\"rows\":{rows}}}",
                crate::studio::json_escape(&receipt.summary),
                crate::studio::json_escape(&receipt.turn_summary),
                receipt.applied
            )
        }
        ShortcutAction::Undo => {
            let listing =
                sdk::history::list_turns(&state.folder).map_err(|e| bad(e.to_string()))?;
            let target = listing
                .turns
                .iter()
                .filter(|t| !t.undone)
                .max_by_key(|t| t.seq)
                .map(|t| t.seq);
            match target {
                None => {
                    return Err(bad(format!(
                        "studio refused: undo names no turn still in effect ({} recorded, all undone or none); edit first, then undo. Nothing was reversed.",
                        listing.turns.len()
                    )));
                }
                Some(seq) => {
                    let receipt = sdk::history::undo_turn(&state.folder, seq)
                        .map_err(|e| bad(e.to_string()))?;
                    let report = state.session.reload_keep_playhead(&mut state.transport);
                    state.kdl_disk = crate::studio::disk_version_of(&state.folder);
                    state.last_reload = Some(report.to_string());
                    let rows = crate::studio::timeline_rows_json(state.session.composition());
                    let files = receipt
                        .restored_files
                        .iter()
                        .map(|f| format!("\"{}\"", crate::studio::json_escape(f)))
                        .collect::<Vec<_>>()
                        .join(",");
                    format!(
                        "{{\"undone_seq\":{},\"summary\":\"{}\",\"restored_files\":[{files}],\"restored_state\":\"{}\",\"rows\":{rows}}}",
                        receipt.seq,
                        crate::studio::json_escape(&receipt.summary),
                        crate::studio::json_escape(&receipt.restored_state)
                    )
                }
            }
        }
        ShortcutAction::DescribeChange => {
            let out = describe_selection(state, &req.selection)?;
            let inner = out
                .trim_start_matches('{')
                .trim_end_matches('}')
                .to_string();
            format!("{{{inner}}}")
        }
        ShortcutAction::ResetLayout => {
            crate::studio::reset_layout(&state.folder);
            let load = crate::studio::load_layout(&state.folder);
            format!(
                "{{\"reset\":true,\"layout_state\":{}}}",
                crate::studio::layout_json(&load)
            )
        }
        ShortcutAction::ToggleTheme => {
            let load = crate::studio::load_theme(&state.folder);
            let next = load.theme.other();
            let saved = crate::studio::save_theme(
                &state.folder,
                &format!("{{\"theme\":\"{}\"}}", next.as_str()),
            )
            .map_err(|message| (String::from("409 Conflict"), message))?;
            format!("{{\"theme\":\"{}\"}}", saved.as_str())
        }
        ShortcutAction::FocusPreview => "{\"focus\":\"preview\"}".to_string(),
        ShortcutAction::FocusTimeline => "{\"focus\":\"timeline\"}".to_string(),
    };
    let inner = if effect.is_empty() {
        "\"ok\":true".to_string()
    } else {
        let no_open = effect.strip_prefix('{').unwrap_or(&effect);
        no_open.strip_suffix('}').unwrap_or(no_open).to_string()
    };
    let head = format!(
        "{{\"action\":\"{}\",\"area\":\"{}\",\"chord\":\"{}\",\"actor\":\"{}\",\"effect\":{{{}}}",
        req.action.as_str(),
        req.action.area().as_str(),
        crate::studio::json_escape(&req.chord),
        crate::studio::json_escape(&req.actor),
        inner,
    );
    Ok(format!("{head},{}", readout_json(state)) + "}")
}

/// Copy the ready agent request from one selection (Sprint 16, T008 /
/// INT-003): shared by the describe route and the describe-change
/// dispatch, so both name the same composition, playhead, and windows.
fn describe_selection(
    state: &mut ServerState,
    selection: &[String],
) -> Result<String, (String, String)> {
    if selection.is_empty() {
        return Err((
            String::from("400 Bad Request"),
            "studio refused: the describe-a-change handoff names no selection; pick one or more timeline clips first. Nothing was staged or recorded.".to_string(),
        ));
    }
    let composition = state.session.composition_name().to_string();
    let base = match find_studio_composition(state, Some(&composition)) {
        Some(comp) => comp,
        None => {
            return Err((
                String::from("404 Not Found"),
                unknown_composition(state, Some(composition)),
            ));
        }
    };
    let shown = readout_composition(state, &base, Some(&composition));
    let rows = crate::studio::studio_rows_of(&shown);
    let clips = crate::studio_shortcuts::resolve_handoff_selection(&rows, selection)
        .map_err(|message| (String::from("409 Conflict"), message))?;
    let request = crate::studio_shortcuts::describe_request_text(
        &composition,
        state.transport.position(),
        state.session.fps().value(),
        state.session.duration_secs(),
        &clips,
    );
    Ok(format!(
        "{{\"composition\":\"{}\",\"request\":\"{}\"}}",
        crate::studio::json_escape(&composition),
        crate::studio::json_escape(&request)
    ))
}

/// `"readout": {...}` fragment shared by the control replies.
fn readout_json(state: &ServerState) -> String {
    let r = state.transport.readout();
    format!(
        "\"readout\":{{\"frame\":{},\"frame_count\":{},\"time_secs\":{:.3},\"duration_secs\":{:.3},\"fps\":{},\"playing\":{},\"loop\":{},\"speed\":{},\"enabled\":{}}}",
        r.frame,
        r.frame_count,
        r.time_secs,
        r.duration_secs,
        r.fps.value(),
        r.playing,
        r.loop_enabled,
        r.speed,
        r.enabled
    )
}

/// `/api/status` body: project, composition, port, readout, measured
/// latency medians, and the last reload report.
fn render_status(state: &mut ServerState) -> String {
    let report = state.session.latency_report();
    let mut actions = String::new();
    for row in &report.actions {
        if !actions.is_empty() {
            actions.push(',');
        }
        actions.push_str(&format!(
            "{{\"action\":\"{}\",\"bar_ms\":{},\"warm_median_ms\":{},\"warm_samples\":{},\"cold_median_ms\":{},\"cold_samples\":{},\"meets_bar\":{}}}",
            row.action,
            row.bar_ms,
            fmt_opt(row.warm_median_ms),
            row.warm_samples,
            fmt_opt(row.cold_median_ms),
            row.cold_samples,
            match row.meets_bar {
                Some(true) => "true",
                Some(false) => "false",
                None => "null",
            }
        ));
    }
    format!(
        "{{\"project\":\"{}\",\"composition\":\"{}\",\"port\":{},\"url\":\"{}\",\"frame_count\":{},\"canvas\":[{{\"width\":{},\"height\":{}}}],\"fps\":{},\"duration_secs\":{:.3},\"all_bars_met\":{},\"latency\":[{}],\"reload\":\"{}\",{}}}",
        json_escape(&state.dir_key),
        json_escape(state.session.composition_name()),
        state.port,
        daemon::url_for_port(state.port),
        state.session.frame_count(),
        state.session.canvas().0,
        state.session.canvas().1,
        state.session.fps().value(),
        state.session.duration_secs(),
        match report.all_bars_met() {
            Some(true) => "true",
            Some(false) => "false",
            None => "null",
        },
        actions,
        json_escape(state.last_reload.as_deref().unwrap_or("no reload yet")),
        readout_json(state)
    )
}

fn fmt_opt(v: Option<f64>) -> String {
    match v {
        Some(m) => format!("{m:.3}"),
        None => "null".to_string(),
    }
}

/// The review page: true frames, transport with readout, the mix as
/// the clock, measured medians, and the last reload report.
fn render_page(state: &mut ServerState) -> String {
    let r = state.transport.readout();
    let silent = state.session.mix_info().ok().flatten().is_none();
    let report = state.session.latency_report();
    let mut medians = String::new();
    for row in &report.actions {
        medians.push_str(&format!("<li>{row}</li>"));
    }
    let reload = state
        .last_reload
        .clone()
        .unwrap_or_else(|| "no reload yet".to_string());
    let audio_block = if silent {
        "<p id=\"audio-note\">silent project: video plays with no audio track.</p>".to_string()
    } else {
        "<audio id=\"mix\" src=\"/api/mix.wav\" preload=\"auto\"></audio>\n<p id=\"audio-note\">press play: the mix is the clock.</p>".to_string()
    };
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>Framesmith preview — {project} — {composition}</title>\n\
        <style>body{{background:#111;color:#eee;font-family:sans-serif;max-width:900px;margin:2em auto}}img{{max-width:100%;background:#000}}button,input{{margin:.2em}}#readout{{font-variant-numeric:tabular-nums}}</style>\n\
        </head>\n<body>\n<h1>preview: {project} — {composition}</h1>\n\
        <p id=\"readout\">{readout}</p>\n\
        <img id=\"frame\" src=\"/api/frame?i={frame}&action=step\" alt=\"preview frame\">\n\
        <div>\n<button onclick=\"step(-1)\">step back</button>\n<button onclick=\"step(1)\">step forward</button>\n\
        <button onclick=\"play()\">play</button>\n<button onclick=\"pause()\">pause</button>\n\
        <label>scrub <input id=\"scrub\" type=\"range\" min=\"0\" max=\"{max}\" value=\"{frame}\" oninput=\"scrub(this.value)\"></label>\n\
        <label>loop <input id=\"loop\" type=\"checkbox\" onchange=\"setLoop(this.checked)\"></label>\n\
        <label>speed <input id=\"speed\" type=\"number\" value=\"1\" step=\"0.25\" min=\"0.1\" onchange=\"setSpeed(this.value)\"></label>\n\
        </div>\n{audio}\n<h2>measured transport medians</h2>\n<ul>{medians}</ul>\n<h2>reload</h2>\n<p id=\"reload\">{reload}</p>\n\
        <script>\nlet cur = {frame};\nasync function show(i, action) {{\n  const r = await fetch('/api/frame?i=' + i + '&action=' + (action || 'step'));\n  if (!r.ok) {{ const t = await r.text(); document.getElementById('readout').textContent = t; return; }}\n  const b = await r.blob();\n  document.getElementById('frame').src = URL.createObjectURL(b);\n  cur = i;\n  const s = await (await fetch('/api/status')).json();\n  document.getElementById('readout').textContent = readoutText(s.readout);\n  document.getElementById('reload').textContent = s.reload;\n}}\nfunction readoutText(r) {{ return 'frame ' + r.frame + '/' + r.frame_count + ' ' + r.time_secs.toFixed(3) + 's/' + r.duration_secs.toFixed(3) + 's ' + (r.playing ? 'playing' : 'paused') + ' loop ' + (r.loop ? 'on' : 'off') + ' ' + r.speed + 'x'; }}\nasync function step(d) {{\n  const r = await (await fetch('/api/step?d=' + d, {{method: 'POST'}})).json();\n  show(r.readout.frame, 'step');\n}}\nasync function scrub(i) {{ const r = await (await fetch('/api/scrub?i=' + i, {{method: 'POST'}})).json(); show(r.readout.frame, 'scrub'); }}\nasync function setLoop(on) {{ await fetch('/api/loop?on=' + (on ? '1' : '0'), {{method: 'POST'}}); }}\nasync function setSpeed(v) {{ const r = await fetch('/api/speed?v=' + v, {{method: 'POST'}}); if (!r.ok) alert(await r.text()); }}\nasync function play() {{\n  const r = await fetch('/api/play', {{method: 'POST'}});\n  if (!r.ok) {{ alert(await r.text()); return; }}\n  const j = await r.json();\n  const mix = document.getElementById('mix');\n  if (mix && j.audio === 'sound') {{\n    mix.playbackRate = j.readout.speed;\n    await mix.play();\n    mix.ontimeupdate = async () => {{\n      const s = await (await fetch('/api/sync?time=' + mix.currentTime, {{method: 'POST'}})).json();\n      if (s.frame !== cur) show(s.frame, 'step');\n    }};\n    mix.onended = () => pause();\n  }} else {{\n    const fps = j.readout.fps || 30;\n    window._t = setInterval(async () => {{\n      const s = await (await fetch('/api/status')).json();\n      let n = s.readout.frame + 1;\n      if (n >= s.readout.frame_count) {{ if (s.readout.loop) n = 0; else {{ pause(); return; }} }}\n      const t = await (await fetch('/api/scrub?i=' + n, {{method: 'POST'}})).json();\n      show(t.readout.frame, 'step');\n    }}, 1000 / fps / j.readout.speed);\n  }}\n}}\nasync function pause() {{\n  const mix = document.getElementById('mix');\n  if (mix) mix.pause();\n  if (window._t) {{ clearInterval(window._t); window._t = null; }}\n  await fetch('/api/pause', {{method: 'POST'}});\n  const s = await (await fetch('/api/status')).json();\n  document.getElementById('readout').textContent = readoutText(s.readout);\n}}\n</script>\n</body>\n</html>\n",
        project = state.dir_key,
        composition = state.session.composition_name(),
        readout = r,
        frame = r.frame,
        max = state.session.frame_count().saturating_sub(1),
        audio = audio_block,
        medians = medians,
        reload = reload,
    )
}

/// The Studio shell page (Sprint 16, T002 / CAP-002, REC-003): the
/// saved per-project arrangement (or the default with a banner when it
/// cannot restore), the saved per-project theme, deep-link resolution
/// over the serving session (a missing composition lands on the
/// project root with a banner), the timeline rows of the shown
/// composition with the persisted edit delta replayed, and the live
/// save-queue state. An unreachable project opens the shell read-only
/// with a banner and every staged edit kept.
fn render_studio(state: &mut ServerState, query: &str) -> String {
    let reachable = crate::studio::project_reachable(&state.folder);
    let available: Vec<String> = state
        .session
        .all_compositions()
        .iter()
        .map(|c| c.name.clone())
        .collect();
    let session_comp = state.session.composition().clone();
    let shown = if reachable {
        readout_composition(state, &session_comp, None)
    } else {
        session_comp
    };
    // The conflict banner (Sprint 16, T003): disk versus unsaved with
    // the maker's two picks, naming both versions.
    let conflict_notice = state.conflict.as_ref().map(|pending| {
        crate::studio::conflict_notice(&state.folder, &pending.disk, &state.save_queue.summaries())
    });
    let keys = crate::studio::selection_keys_of(&shown);
    let req = crate::studio::parse_deep_link(query);
    let resolved = crate::studio::resolve_deep_link(
        &req,
        &state.dir_key,
        state.session.composition_name(),
        &available,
        state.session.frame_count(),
        state.session.fps().value(),
        state.session.duration_secs(),
        &keys,
    );
    let load = crate::studio::load_layout(&state.folder);
    let theme = crate::studio::load_theme(&state.folder);
    let mut notices = Vec::new();
    if let Some(notice) = load.notice.clone() {
        notices.push(notice);
    }
    if let Some(notice) = theme.notice.clone() {
        notices.push(notice);
    }
    notices.extend(resolved.notices.clone());
    if !reachable {
        notices.push(crate::studio::unreachable_notice(&state.folder));
    } else if state.save_queue.paused {
        notices.push(format!(
            "studio: the save queue is paused with {} staged edit(s); resume to apply. Nothing is lost.",
            state.save_queue.staged.len()
        ));
    }
    let (width, height) = state.session.canvas();
    crate::studio::studio_page_html(&crate::studio::StudioPageModel {
        project: state.dir_key.clone(),
        composition: resolved.composition.clone(),
        all_compositions: available,
        frame: resolved.frame,
        frame_count: state.session.frame_count(),
        duration_secs: state.session.duration_secs(),
        fps_value: state.session.fps().value(),
        width,
        height,
        selection: resolved.selection.clone(),
        layout: load.layout.clone(),
        layout_restored: load.restored,
        notices,
        rows_json: crate::studio::timeline_rows_json(&shown),
        conflict_notice,
        theme: theme.theme.as_str().to_string(),
        read_only: !reachable,
        save_paused: state.save_queue.paused,
        staged: state.save_queue.staged.len(),
        staged_summaries: state.save_queue.summaries(),
    })
}

/// The embeddable player page (Sprint 15, T005): the same session's
/// frames and mix behind the player method/event surface. The film
/// model comes straight from the session, so shown bytes equal the
/// pipeline's bytes exactly and nothing is written into the project.
fn render_player(state: &mut ServerState, attrs: &crate::player::PlayerAttributes) -> String {
    let r = state.transport.readout();
    let silent = state.session.mix_info().ok().flatten().is_none();
    let background = state.session.background_css();
    crate::player::player_page_html_with_background(
        &crate::player::PlayerPageModel {
            project: state.dir_key.clone(),
            composition: state.session.composition_name().to_string(),
            frame: r.frame,
            frame_count: state.session.frame_count(),
            duration_secs: state.session.duration_secs(),
            fps_value: state.session.fps().value(),
            width: state.session.canvas().0,
            height: state.session.canvas().1,
            has_audio: !silent,
            attrs: attrs.clone(),
        },
        &background,
    )
}
