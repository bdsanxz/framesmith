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
use std::time::{Duration, SystemTime};

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
    kdl_mtime: Option<SystemTime>,
    last_reload: Option<String>,
    shutdown: Arc<AtomicBool>,
}

impl ServerState {
    fn new(session: PreviewSession, folder: PathBuf, port: u16) -> Self {
        let transport = session.transport();
        let dir_key = session.project_folder().to_string_lossy().into_owned();
        let kdl_mtime = composition_mtime(&folder);
        Self {
            session,
            transport,
            folder,
            dir_key,
            port,
            kdl_mtime,
            last_reload: None,
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Re-prepare when the composition file moved since the last
    /// check (Sprint 14, T005): valid edits apply with the playhead
    /// kept, broken edits list diagnostics while last good frames
    /// keep showing. Runs before frames and status so edits appear
    /// hot without a manual reload call.
    fn maybe_reload(&mut self) {
        let now = composition_mtime(&self.folder);
        if now == self.kdl_mtime {
            return;
        }
        self.kdl_mtime = now;
        let report = self.session.reload_keep_playhead(&mut self.transport);
        self.last_reload = Some(report.to_string());
    }
}

fn composition_file(folder: &Path) -> PathBuf {
    folder.join(validate::validate::COMPOSITION_FILE)
}

fn composition_mtime(folder: &Path) -> Option<SystemTime> {
    std::fs::metadata(composition_file(folder))
        .and_then(|m| m.modified())
        .ok()
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

/// One HTTP exchange: read the head, route, answer, close.
fn handle_one(stream: TcpStream, state: &mut ServerState) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let Some((method, target)) = read_request(&stream) else {
        return;
    };
    // Hot paths watch the file; the shutdown route must answer even
    // mid-edit, so it skips the reload check.
    if !(method == "POST" && strip_query(&target).0 == "/api/shutdown") {
        state.maybe_reload();
    }
    route(stream, state, &method, &target);
}

fn strip_query(target: &str) -> (&str, &str) {
    match target.split_once('?') {
        Some((p, q)) => (p, q),
        None => (target, ""),
    }
}

/// Read the request head (`METHOD TARGET HTTP/1.x` plus headers).
fn read_request(stream: &TcpStream) -> Option<(String, String)> {
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
    let head = std::str::from_utf8(&buf).ok()?;
    let line = head.lines().next()?;
    let mut parts = line.split_whitespace();
    Some((parts.next()?.to_string(), parts.next()?.to_string()))
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

fn query_get(query: &str, key: &str) -> Option<String> {
    query_pairs(query)
        .into_iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
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

fn route(stream: TcpStream, state: &mut ServerState, method: &str, target: &str) {
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
        ("POST", "/api/reload") => {
            let report = state.session.reload_keep_playhead(&mut state.transport);
            state.kdl_mtime = composition_mtime(&state.folder);
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
        _ => respond_text(
            stream,
            "404 Not Found",
            &format!("preview refused: unknown route {method} {path}; open / for the review page"),
        ),
    }
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
