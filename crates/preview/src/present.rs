//! Slideshow present service (Sprint 24, T002 / CAP-002-005).
//!
//! One navigation state machine drives both views (presenter and
//! audience) in sync: arrow keys, P, Home/End, hotspot clicks,
//! branches resolve first, fragments hold, autoplay ticks. Frames
//! come from the picker cache — navigation never re-renders the film.
//! A missing manifest refuses; a vanished target holds the last good
//! slide and reports the hold.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notation::model::{Composition, SlideshowManifest};

/// One navigation hold: a vanished target or deck-end hold.
/// The session holds the last good slide and reports the hold.
#[derive(Debug, Clone, PartialEq)]
pub struct NavHold {
    pub reason: String,
}

impl NavHold {
    pub fn new(reason: impl Into<String>) -> Self {
        Self { reason: reason.into() }
    }
    pub fn message(&self) -> &str {
        &self.reason
    }
}

/// Single navigation state machine (CAP-003): every input advances
/// or retreats the same machine; branches resolve first, fragments
/// hold before advancing when advanced explicitly; autoplay advances
/// on duration; navigation at deck ends holds and reports, never
/// wraps silently.
#[derive(Debug, Clone)]
pub struct Navigation {
    deck: SlideshowManifest,
    current: usize,
    fragment: usize,
    timer_start: Instant,
    hold: Option<NavHold>,
    last_autoplay: Option<Instant>,
}

impl Navigation {
    pub fn new(deck: SlideshowManifest) -> Self {
        Self {
            current: 0,
            fragment: 0,
            timer_start: Instant::now(),
            hold: None,
            last_autoplay: None,
            deck,
        }
    }

    pub fn deck(&self) -> &SlideshowManifest {
        &self.deck
    }

    pub fn current_index(&self) -> usize {
        self.current
    }

    pub fn current_slide(&self) -> Option<&notation::model::SlideshowSlide> {
        self.deck.slides.get(self.current)
    }

    pub fn next_slide(&self) -> Option<&notation::model::SlideshowSlide> {
        self.deck.slides.get(self.current + 1)
    }

    pub fn fragment_index(&self) -> usize {
        self.fragment
    }

    pub fn hold(&self) -> Option<&NavHold> {
        self.hold.as_ref()
    }

    pub fn elapsed(&self) -> Duration {
        self.timer_start.elapsed()
    }

    /// Advance; branches resolve first, then fragment holds, then
    /// sequential next. Returns the hold when navigation cannot move.
    pub fn next(&mut self) -> Option<NavHold> {
        let Some(slide) = self.deck.slides.get(self.current).cloned() else {
            let h = NavHold::new("present: no slide beyond the deck end; holding current slide");
            self.hold = Some(h.clone());
            return Some(h);
        };
        // Branches resolve first.
        if let Some(branch) = slide.branches.first() {
            if let Some(idx) = self.deck.index_of(&branch.to) {
                self.current = idx;
                self.fragment = 0;
                self.hold = None;
                return None;
            } else {
                let h = NavHold::new(format!(
                    "present: branch target {:?} vanished since validation; holding slide {:?}",
                    branch.to, slide.id
                ));
                self.hold = Some(h.clone());
                return Some(h);
            }
        }
        // Fragments hold before advancing when advanced explicitly.
        if self.fragment < slide.fragments.len() {
            self.fragment += 1;
            if self.fragment < slide.fragments.len() {
                let h = NavHold::new(format!(
                    "present: fragment hold {}/{} on slide {:?}",
                    self.fragment,
                    slide.fragments.len(),
                    slide.id
                ));
                // Not a vanished target — fragment holds are expected.
                // Still report as hold for sync viewers but keep slide visible.
                self.hold = Some(h.clone());
                return Some(h);
            } else {
                // Last fragment revealed; hold shows fragment consumed.
                let h = NavHold::new(format!(
                    "present: fragment {}/{} consumed on slide {:?}",
                    self.fragment,
                    slide.fragments.len(),
                    slide.id
                ));
                self.hold = Some(h.clone());
                return Some(h);
            }
        }
        // Sequential next.
        if self.current + 1 < self.deck.slides.len() {
            self.current += 1;
            self.fragment = 0;
            self.hold = None;
            None
        } else {
            let h = NavHold::new("present: at the last slide; holding position");
            self.hold = Some(h.clone());
            Some(h)
        }
    }

    pub fn prev(&mut self) -> Option<NavHold> {
        if self.current == 0 {
            let h = NavHold::new("present: at the first slide; holding position");
            self.hold = Some(h.clone());
            return Some(h);
        }
        self.current -= 1;
        self.fragment = 0;
        self.hold = None;
        None
    }

    pub fn home(&mut self) -> Option<NavHold> {
        self.current = 0;
        self.fragment = 0;
        self.hold = None;
        None
    }

    pub fn end(&mut self) -> Option<NavHold> {
        if self.deck.slides.is_empty() {
            let h = NavHold::new("present: empty deck; holding");
            self.hold = Some(h.clone());
            return Some(h);
        }
        self.current = self.deck.slides.len() - 1;
        self.fragment = 0;
        self.hold = None;
        None
    }

    pub fn goto(&mut self, slide_id: &str) -> Option<NavHold> {
        if let Some(idx) = self.deck.index_of(slide_id) {
            self.current = idx;
            self.fragment = 0;
            self.hold = None;
            None
        } else {
            let h = NavHold::new(format!(
                "present: slide {:?} vanished since validation; holding slide {:?}",
                slide_id,
                self.current_slide().map(|s| s.id.as_str()).unwrap_or("?")
            ));
            self.hold = Some(h.clone());
            Some(h)
        }
    }

    /// Hotspot click: navigate to the hotspot's target. Vanished
    /// targets hold the last good slide and report.
    pub fn hotspot(&mut self, hotspot_id: &str) -> Option<NavHold> {
        let Some(slide) = self.deck.slides.get(self.current).cloned() else {
            let h = NavHold::new(format!("present: hotspot {hotspot_id:?} has no current slide"));
            self.hold = Some(h.clone());
            return Some(h);
        };
        match slide.hotspots.iter().find(|h| h.id == hotspot_id) {
            None => {
                let h = NavHold::new(format!(
                    "present: hotspot {hotspot_id:?} not on slide {:?}; holding",
                    slide.id
                ));
                self.hold = Some(h.clone());
                Some(h)
            }
            Some(hot) => {
                if let Some(idx) = self.deck.index_of(&hot.target) {
                    self.current = idx;
                    self.fragment = 0;
                    self.hold = None;
                    None
                } else {
                    let h = NavHold::new(format!(
                        "present: hotspot {:?} target {:?} vanished; holding slide {:?}",
                        hotspot_id, hot.target, slide.id
                    ));
                    self.hold = Some(h.clone());
                    Some(h)
                }
            }
        }
    }

    /// Autoplay tick: advances on duration, skipping fragments by
    /// duration. Returns true when a slide advanced.
    pub fn tick_autoplay(&mut self, now: Instant) -> bool {
        let Some(dur) = self.deck.autoplay else {
            return false;
        };
        let interval = Duration::from_secs_f64(dur);
        let base = self.last_autoplay.unwrap_or(self.timer_start);
        if now.duration_since(base) >= interval {
            self.last_autoplay = Some(now);
            // Autoplay skips fragments by duration.
            if self.current + 1 < self.deck.slides.len() {
                self.current += 1;
                self.fragment = 0;
                self.hold = None;
                true
            } else {
                let h = NavHold::new("present: autoplay at last slide; holding");
                self.hold = Some(h.clone());
                false
            }
        } else {
            false
        }
    }

    /// Test helper: advance autoplay deterministically.
    pub fn tick_autoplay_at(&mut self, offset_secs: f64) -> bool {
        let now = Instant::now() + Duration::from_secs_f64(offset_secs);
        self.tick_autoplay(now)
    }

    pub fn clear_hold(&mut self) {
        self.hold = None;
    }
}

/// Present session (CAP-002): holds the deck plus a per-composition
/// picker cache. Both presenter and audience views read the same
/// [`Navigation`] so they stay in sync. Frames are served from
/// [`crate::PreviewSession`] caches; navigation alone never triggers
/// a render.
pub struct PresentSession {
    deck: SlideshowManifest,
    compositions: HashMap<String, Composition>,
    previews: HashMap<String, crate::PreviewSession>,
    nav: Arc<Mutex<Navigation>>,
    folder: std::path::PathBuf,
}

impl PresentSession {
    pub fn from_verdict(
        folder: std::path::PathBuf,
        verdict: validate::validate::Verdict,
        _options: crate::PreviewOptions,
    ) -> Result<Self, String> {
        let deck = verdict.slideshow.clone().ok_or_else(|| {
            "present refused: no slideshow manifest in the project; declare a `slideshow { slide ... }` node in composition.kdl".to_string()
        })?;
        // Validate verdict is valid (should be; present gates it).
        if verdict.status != validate::validate::Status::Valid {
            return Err(format!(
                "present refused: {} is invalid; fix the problems `check` reports and re-run. No present was started.",
                folder.display()
            ));
        }
        let mut compositions: HashMap<String, Composition> = HashMap::new();
        for c in &verdict.compositions {
            compositions.insert(c.name.clone(), c.clone());
            if let Some(id) = &c.id {
                compositions.insert(id.clone(), c.clone());
            }
        }
        let mut previews: HashMap<String, crate::PreviewSession> = HashMap::new();
        for slide in &deck.slides {
            let comp_name = &slide.composition;
            if previews.contains_key(comp_name) {
                continue;
            }
            // Opening a preview session per distinct composition
            // shares the same picker cache path; a vanished
            // composition would have failed validation already, but
            // a mid-session vanish is detected at navigation time.
            let session = crate::PreviewSession::open(
                &folder,
                crate::PreviewOptions {
                    composition: Some(comp_name.clone()),
                    ..crate::PreviewOptions::default()
                },
            )
            .map_err(|e| format!("present refused: cannot preview slide composition {comp_name:?}: {e}"))?;
            previews.insert(comp_name.clone(), session);
        }
        let nav = Arc::new(Mutex::new(Navigation::new(deck.clone())));
        Ok(Self {
            deck,
            compositions,
            previews,
            nav,
            folder,
        })
    }

    pub fn deck(&self) -> &SlideshowManifest {
        &self.deck
    }

    pub fn nav(&self) -> Arc<Mutex<Navigation>> {
        Arc::clone(&self.nav)
    }

    pub fn folder(&self) -> &std::path::Path {
        &self.folder
    }

    /// Frame bytes for the current slide from the picker cache
    /// (never re-rendering on navigation — repeats hit the cache).
    /// A vanished target holds the last good frame.
    pub fn frame_for_current(
        &mut self,
        nav_guard: &Navigation,
        frame_index: u64,
    ) -> Result<crate::PreviewFrame, crate::PreviewFrameError> {
        let Some(slide) = nav_guard.current_slide() else {
            return Err(crate::PreviewFrameError {
                kind: crate::FrameErrorKind::OutOfRange,
                message: "present: no current slide".to_string(),
            });
        };
        let session = self.previews.get_mut(&slide.composition).ok_or_else(|| {
            crate::PreviewFrameError {
                kind: crate::FrameErrorKind::RenderFailed,
                message: format!(
                    "present: slide {:?} composition {:?} vanished since validation; holding last good slide",
                    slide.id, slide.composition
                ),
            }
        })?;
        session.frame_png(frame_index)
    }
}

/// Present server options (Sprint 24, T002).
#[derive(Debug, Clone)]
pub struct PresentServeOptions {
    pub folder: std::path::PathBuf,
    pub port: u16,
}

/// Serve present until `/api/shutdown`. Mirrors the Sprint 14
/// preview server shape but drives the present state machine.
pub fn run_present_serve(opts: &PresentServeOptions, registry: &std::path::Path) -> i32 {
    use crate::daemon;
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let folder = match daemon::canonical_dir(&opts.folder) {
        Ok(d) => std::path::PathBuf::from(d),
        Err(m) => {
            eprintln!("{m}");
            return 1;
        }
    };
    // Pre-validate and check manifest before binding.
    let verdict = validate::validate::validate_folder(&folder);
    if verdict.status == validate::validate::Status::Invalid {
        eprintln!(
            "present refused: {} is invalid. No present was started.",
            folder.display()
        );
        for d in &verdict.diagnostics {
            eprintln!("{}:{}:{} {} {} (fix: {})", d.file, d.line, d.column, d.code, d.message, d.fix);
        }
        return 1;
    }
    if verdict.slideshow.is_none() {
        eprintln!(
            "present refused: {} has no slideshow manifest (composition.kdl declares no `slideshow` node); add one `slideshow {{ slide ... }}` block to present. No present was started.",
            folder.display()
        );
        return 1;
    }
    let session = match PresentSession::from_verdict(
        folder.clone(),
        verdict.clone(),
        crate::PreviewOptions::default(),
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let listener = match crate::server::bind_exact(opts.port) {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            eprintln!(
                "present serve refused: port {} is busy",
                opts.port
            );
            return crate::server::PORT_BUSY_EXIT;
        }
        Err(e) => {
            eprintln!("present serve failed: {e}");
            return 1;
        }
    };
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(opts.port);
    let dir = folder.to_string_lossy().into_owned();
    let name = folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.clone());
    let entry = daemon::ServerEntry::new(
        dir.clone(),
        name,
        session.deck().slides.first().map(|s| s.composition.clone()).unwrap_or_default(),
        port,
        std::process::id(),
    );
    if let Err(m) = daemon::add_entry(registry, &entry) {
        eprintln!("{m}");
        return 1;
    }
    eprintln!("present serving {} at {}", entry.name, entry.url);
    let shutdown = Arc::new(AtomicBool::new(false));
    let nav = session.nav();
    let mut sess = session;
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        handle_one(stream, &mut sess, &nav, &shutdown);
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
    }
    let _ = daemon::remove_entry(registry, &dir);
    0
}

fn handle_one(
    stream: std::net::TcpStream,
    session: &mut PresentSession,
    nav: &Arc<Mutex<Navigation>>,
    shutdown: &Arc<std::sync::atomic::AtomicBool>,
) {
    use std::io::{Read, Write};
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    let Some((method, target, body)) = read_req(&stream) else {
        return;
    };
    route(stream, session, nav, &method, &target, &body, shutdown);
}

fn read_req(stream: &std::net::TcpStream) -> Option<(String, String, String)> {
    use std::io::Read;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
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
        if let Some(v) = header.strip_prefix("Content-Length:") {
            length = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = header.strip_prefix("content-length:") {
            length = v.trim().parse().unwrap_or(0);
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
    Some((method, target, String::from_utf8_lossy(&body).into_owned()))
}

fn strip_q(t: &str) -> (&str, &str) {
    match t.split_once('?') {
        Some((p, q)) => (p, q),
        None => (t, ""),
    }
}

fn respond(mut stream: std::net::TcpStream, status: &str, ctype: &str, body: &[u8]) {
    use std::io::Write;
    let head = format!(
        "HTTP/1.0 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn route(
    stream: std::net::TcpStream,
    session: &mut PresentSession,
    nav: &Arc<Mutex<Navigation>>,
    method: &str,
    target: &str,
    body: &str,
    shutdown: &Arc<std::sync::atomic::AtomicBool>,
) {
    let (path, query) = strip_q(target);
    match (method, path) {
        ("POST", "/api/shutdown") => {
            shutdown.store(true, std::sync::atomic::Ordering::SeqCst);
            respond(stream, "200 OK", "application/json", b"{\"ok\":true}");
        }
        ("GET", "/presenter") => {
            let html = presenter_html(session, nav, query);
            respond(stream, "200 OK", "text/html; charset=utf-8", html.as_bytes());
        }
        ("GET", "/audience") => {
            let html = audience_html(session, nav);
            respond(stream, "200 OK", "text/html; charset=utf-8", html.as_bytes());
        }
        ("GET", "/api/present/status") => {
            let nav_g = nav.lock().unwrap();
            let json = status_json(session, &nav_g);
            respond(stream, "200 OK", "application/json", json.as_bytes());
        }
        ("POST", "/api/present/navigate") => {
            // body JSON: {"action":"next"|"prev"|"home"|"end"|"goto","target":"s2"|"hotspot":"h1"}
            let mut g = nav.lock().unwrap();
            let action = extract_str(body, "action").unwrap_or_default();
            let hold: Option<NavHold> = match action.as_str() {
                "next" => g.next(),
                "prev" => g.prev(),
                "home" => g.home(),
                "end" => g.end(),
                "goto" => {
                    let t = extract_str(body, "target").unwrap_or_else(|| extract_str(body, "to").unwrap_or_default());
                    g.goto(&t)
                }
                "hotspot" => {
                    let h = extract_str(body, "hotspot").unwrap_or_else(|| extract_str(body, "id").unwrap_or_default());
                    g.hotspot(&h)
                }
                "tick" => {
                    let advanced = g.tick_autoplay(std::time::Instant::now());
                    if advanced {
                        None
                    } else {
                        g.hold().cloned()
                    }
                }
                _ => Some(NavHold::new(format!("present: unknown navigate action {action:?}"))),
            };
            let json = if let Some(h) = hold {
                format!(
                    "{{\"ok\":false,\"hold\":\"{}\",\"status\":{}}}",
                    json_escape(h.message()),
                    status_json(session, &g)
                )
            } else {
                format!("{{\"ok\":true,\"status\":{}}}", status_json(session, &g))
            };
            respond(stream, "200 OK", "application/json", json.as_bytes());
        }
        ("GET", "/api/present/frame") => {
            // Serve current slide frame 0 from picker cache.
            let nav_g = nav.lock().unwrap();
            match session.frame_for_current(&nav_g, 0) {
                Ok(frame) => respond(stream, "200 OK", "image/png", &frame.png),
                Err(e) => respond(stream, "500 Internal Server Error", "text/plain; charset=utf-8", e.message.as_bytes()),
            }
        }
        ("GET", "/api/studio/slideshow") => {
            let nav_g = nav.lock().unwrap();
            let json = slideshow_panel_json(session, &nav_g);
            respond(stream, "200 OK", "application/json", json.as_bytes());
        }
        ("GET", "/api/studio/slideshow/present") => {
            // Present launch hint for the Studio panel.
            respond(stream, "200 OK", "application/json", b"{\"cmd\":\"framesmith present\"}");
        }
        ("GET", "/") => {
            let html = presenter_html(session, nav, "");
            respond(stream, "200 OK", "text/html; charset=utf-8", html.as_bytes());
        }
        _ => {
            respond(stream, "404 Not Found", "text/plain; charset=utf-8", b"not found");
        }
    }
}

fn extract_str(body: &str, key: &str) -> Option<String> {
    // Minimal JSON string extract without external crates.
    let pat = format!("\"{key}\"");
    let idx = body.find(&pat)?;
    let after = &body[idx + pat.len()..];
    let colon = after.find(':')?;
    let rest = after[colon + 1..].trim_start();
    if let Some(stripped) = rest.strip_prefix('"') {
        let end = stripped.find('"')?;
        Some(stripped[..end].to_string())
    } else {
        None
    }
}

fn status_json(session: &PresentSession, nav: &Navigation) -> String {
    let deck = session.deck();
    let current = nav.current_slide();
    let next = nav.next_slide();
    let elapsed = nav.elapsed().as_secs_f64();
    let hold = nav.hold().map(|h| format!("\"{}\"", json_escape(h.message()))).unwrap_or_else(|| "null".to_string());
    format!(
        "{{\"currentIndex\":{},\"total\":{},\"currentId\":\"{}\",\"currentComposition\":\"{}\",\"currentNotes\":{},\"nextId\":{},\"nextComposition\":{},\"fragmentIndex\":{},\"fragments\":{},\"autoplay\":{},\"elapsedSecs\":{:.3},\"hold\":{}}}",
        nav.current_index(),
        deck.slides.len(),
        json_escape(current.map(|s| s.id.as_str()).unwrap_or("")),
        json_escape(current.map(|s| s.composition.as_str()).unwrap_or("")),
        current.and_then(|s| s.notes.as_ref()).map(|n| format!("\"{}\"", json_escape(n))).unwrap_or_else(|| "null".into()),
        next.map(|s| format!("\"{}\"", json_escape(&s.id))).unwrap_or_else(|| "null".into()),
        next.map(|s| format!("\"{}\"", json_escape(&s.composition))).unwrap_or_else(|| "null".into()),
        nav.fragment_index(),
        current.map(|s| s.fragments.len()).unwrap_or(0),
        deck.autoplay.map(|a| format!("{a}")).unwrap_or_else(|| "null".into()),
        elapsed,
        hold
    )
}

fn presenter_html(session: &PresentSession, nav: &Arc<Mutex<Navigation>>, _query: &str) -> String {
    let nav_g = nav.lock().unwrap();
    let current = nav_g.current_slide();
    let next = nav_g.next_slide();
    let deck = session.deck();
    let current_id = current.map(|s| s.id.as_str()).unwrap_or("?");
    let current_comp = current.map(|s| s.composition.as_str()).unwrap_or("?");
    let current_notes = current.and_then(|s| s.notes.as_ref()).map(|n| html_escape(n)).unwrap_or_default();
    let next_id = next.map(|s| s.id.as_str()).unwrap_or("end");
    let next_comp = next.map(|s| s.composition.as_str()).unwrap_or("?");
    let elapsed = nav_g.elapsed().as_secs_f64();
    let hold_msg = nav_g.hold().map(|h| html_escape(h.message())).unwrap_or_default();
    let slides: Vec<String> = deck.slides.iter().enumerate().map(|(i,s)| {
        let mark = if i == nav_g.current_index() { "checked" } else { "" };
        format!("<li{}>{:?} → {:?} notes {:?} fragments {} branches {} hotspots {}</li>", if mark=="checked" { " data-current=\"1\"" } else { "" }, s.id, s.composition, s.notes.as_deref().unwrap_or(""), s.fragments.len(), s.branches.len(), s.hotspots.len())
    }).collect();
    let checklist = slides.join("\n");
    let hotspots: Vec<String> = current.map(|s| s.hotspots.iter().map(|h| format!("<button type=\"button\" data-hotspot=\"{}\" style=\"background-color:#000;color:#fff;border:1px solid #fff;border-radius:4px;padding:4px 8px\">{}</button>", html_escape(&h.id), html_escape(&h.id))).collect()).unwrap_or_default();
    let hotspots_html = hotspots.join(" ");
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>Framesmith present — presenter</title>\n<style>body{{background:#111;color:#eee;font-family:sans-serif;max-width:1000px;margin:2em auto}}#frame{{max-width:100%;background:#000;display:block}}#presenter{{background-color:#000;color:#fff;border:1px solid #fff;border-radius:8px;padding:12px}}</style>\n</head>\n<body>\n<h1>presenter: current {:?} ({:?})</h1>\n<p>next: {:?} ({:?})</p>\n<p id=\"notes\">notes: {}</p>\n<p id=\"timer\" style=\"font-variant-numeric:tabular-nums\">timer: {:.1}s</p>\n<p id=\"hold\">{}</p>\n<img id=\"frame\" src=\"/api/present/frame\" alt=\"current slide\">\n<div id=\"presenter\" style=\"background-color:#000;color:#fff;border:1px solid #fff;border-radius:8px;padding:12px\">\n<button id=\"prev\" type=\"button\">prev (Left)</button>\n<button id=\"next\" type=\"button\">next (Right)</button>\n<button id=\"home\" type=\"button\">home</button>\n<button id=\"end\" type=\"button\">end</button>\n<span>hotspots: {}</span>\n<button id=\"fullscreen\" type=\"button\">toggle fullscreen (P)</button>\n</div>\n<h2>slideshow panel</h2>\n<ol>{}</ol>\n<p>autoplay: {}</p>\n<p>press <kbd>P</kbd> fullscreen, arrows navigate, Home/End jump</p>\n<script>\nlet h=null;\nfunction sync(){{fetch('/api/present/status').then(r=>r.json()).then(s=>{{document.getElementById('timer').textContent='timer: '+s.elapsedSecs.toFixed(1)+'s';document.getElementById('hold').textContent=s.hold||'';}})}}\nasync function nav(action,extra){{await fetch('/api/present/navigate',{{method:'POST',headers:{{'Content-Type':'application/json'}},body:JSON.stringify({{action,...extra}})}});sync();document.getElementById('frame').src='/api/present/frame?t='+Date.now();}}\n document.getElementById('prev').onclick=()=>nav('prev');\n document.getElementById('next').onclick=()=>nav('next');\n document.getElementById('home').onclick=()=>nav('home');\n document.getElementById('end').onclick=()=>nav('end');\n document.getElementById('fullscreen').onclick=()=>{{if(document.fullscreenElement)document.exitFullscreen();else document.documentElement.requestFullscreen();}};\n document.addEventListener('keydown',e=>{{if(e.key==='ArrowRight'||e.key===' '){{e.preventDefault();nav('next');}}else if(e.key==='ArrowLeft'){{e.preventDefault();nav('prev');}}else if(e.key==='p'||e.key==='P'){{if(document.fullscreenElement)document.exitFullscreen();else document.documentElement.requestFullscreen();}}else if(e.key==='Home'){{nav('home');}}else if(e.key==='End'){{nav('end');}}}});\n setInterval(sync,500);sync();\n</script>\n</body>\n</html>\n",
        current_id, current_comp, next_id, next_comp, current_notes, elapsed, hold_msg, hotspots_html, checklist,
        deck.autoplay.map(|a| format!("{a}s")).unwrap_or_else(|| "manual".into())
    )
}

fn audience_html(session: &PresentSession, nav: &Arc<Mutex<Navigation>>) -> String {
    let nav_g = nav.lock().unwrap();
    let current = nav_g.current_slide();
    let _de = session.deck();
    let hold_msg = nav_g.hold().map(|h| html_escape(h.message())).unwrap_or_default();
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>Framesmith present — audience</title>\n<style>body{{margin:0;background:#000}}img{{width:100vw;height:100vh;object-fit:contain;background:#000;display:block}}</style>\n</head>\n<body>\n<img id=\"frame\" src=\"/api/present/frame\" alt=\"slide\">\n<p id=\"hold\" style=\"color:#fff;position:absolute;bottom:0;left:0;padding:4px\">{}</p>\n<script>\nasync function poll(){{const r=await fetch('/api/present/status');const s=await r.json();const img=document.getElementById('frame');const cur='{current_id}';if(cur!==s.currentId){{img.src='/api/present/frame?t='+Date.now();}}document.getElementById('hold').textContent=s.hold||'';}}\nsetInterval(poll,400);poll();\n</script>\n</body>\n</html>\n",
        hold_msg,
        current_id = current.map(|s| s.id.as_str()).unwrap_or("?")
    )
}

/// Slideshow panel readout (CAP-005): checklist in order with notes,
/// fragments, branches, hotspots. The Studio panel renders this from
/// the validated manifest.
pub fn slideshow_panel_json(session: &PresentSession, nav: &Navigation) -> String {
    let deck = session.deck();
    let mut slides = String::new();
    for (i, s) in deck.slides.iter().enumerate() {
        if i > 0 { slides.push(','); }
        slides.push_str(&format!(
            "{{\"id\":\"{}\",\"composition\":\"{}\",\"notes\":{},\"fragments\":{},\"branches\":{},\"hotspots\":{},\"current\":{}}}",
            json_escape(&s.id),
            json_escape(&s.composition),
            s.notes.as_ref().map(|n| format!("\"{}\"", json_escape(n))).unwrap_or_else(|| "null".into()),
            s.fragments.len(),
            format!("[{}]", s.branches.iter().map(|b| format!("\"{}\"", json_escape(&b.to))).collect::<Vec<_>>().join(",")),
            format!("[{}]", s.hotspots.iter().map(|h| format!("\"{}\"", json_escape(&h.target))).collect::<Vec<_>>().join(",")),
            i == nav.current_index()
        ));
    }
    format!(
        "{{\"slides\":[{}],\"autoplay\":{},\"currentIndex\":{},\"hold\":{}}}",
        slides,
        deck.autoplay.map(|a| a.to_string()).unwrap_or_else(|| "null".into()),
        nav.current_index(),
        nav.hold().map(|h| format!("\"{}\"", json_escape(h.message()))).unwrap_or_else(|| "null".into())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use notation::model::{SlideshowBranch, SlideshowFragment, SlideshowHotspot, SlideshowSlide};

    fn deck_two() -> SlideshowManifest {
        SlideshowManifest {
            slides: vec![
                SlideshowSlide {
                    id: "s1".into(),
                    composition: "intro".into(),
                    notes: Some("welcome".into()),
                    fragments: vec![SlideshowFragment { id: Some("f1".into()), at: 0.5, line: 1, column: 1 }],
                    branches: vec![SlideshowBranch { to: "s2".into(), line: 1, column: 1 }],
                    hotspots: vec![SlideshowHotspot { id: "h1".into(), x: 10.0, y: 20.0, target: "s2".into(), line: 1, column: 1 }],
                    line: 1, column: 1,
                },
                SlideshowSlide {
                    id: "s2".into(),
                    composition: "agenda".into(),
                    notes: None,
                    fragments: vec![],
                    branches: vec![],
                    hotspots: vec![],
                    line: 1, column: 1,
                },
            ],
            autoplay: Some(2.0),
            line: 1, column: 1,
        }
    }

    #[test]
    fn branch_resolves_first_before_fragment() {
        let mut nav = Navigation::new(deck_two());
        assert_eq!(nav.current_index(), 0);
        assert_eq!(nav.current_slide().unwrap().id, "s1");
        // Branch to s2 resolves first, even though a fragment remains.
        let hold = nav.next();
        assert!(hold.is_none());
        assert_eq!(nav.current_index(), 1, "branch should jump to s2");
        assert_eq!(nav.fragment_index(), 0);
    }

    #[test]
    fn fragment_holds_before_advancing_when_explicit() {
        let mut deck = deck_two();
        deck.slides[0].branches.clear();
        let mut nav = Navigation::new(deck);
        let hold = nav.next();
        assert!(hold.is_some());
        assert!(hold.unwrap().message().contains("fragment"));
        assert_eq!(nav.current_index(), 0, "fragments hold on same slide");
        assert_eq!(nav.fragment_index(), 1);
        // Next advance moves to s2.
        let hold2 = nav.next();
        assert!(hold2.is_none());
        assert_eq!(nav.current_index(), 1);
    }

    #[test]
    fn hotspot_click_navigates() {
        let mut deck = deck_two();
        deck.slides[0].branches.clear();
        let mut nav = Navigation::new(deck);
        let hold = nav.hotspot("h1");
        assert!(hold.is_none());
        assert_eq!(nav.current_index(), 1);
    }

    #[test]
    fn arrows_home_end() {
        let mut nav = Navigation::new(deck_two());
        nav.end();
        assert_eq!(nav.current_index(), 1);
        nav.home();
        assert_eq!(nav.current_index(), 0);
        // Prev at first holds.
        let h = nav.prev();
        assert!(h.is_some());
        assert_eq!(nav.current_index(), 0);
    }

    #[test]
    fn autoplay_ticks() {
        let mut nav = Navigation::new(deck_two());
        // Manually move to s2 then test autoplay skipping fragments would skip to end? We'll test from s1 without branch.
        // Remove branch for pure autoplay check.
        nav.deck.slides[0].branches.clear();
        let start = Instant::now();
        nav.last_autoplay = Some(start);
        // Tick before interval does nothing.
        let now = start + Duration::from_secs_f64(1.0);
        assert!(!nav.tick_autoplay(now));
        assert_eq!(nav.current_index(), 0);
        // Tick after interval advances.
        let now2 = start + Duration::from_secs_f64(2.1);
        assert!(nav.tick_autoplay(now2));
        assert_eq!(nav.current_index(), 1);
    }

    #[test]
    fn vanished_target_holds_last_good() {
        let mut deck = deck_two();
        deck.slides[0].branches = vec![SlideshowBranch { to: "gone".into(), line: 1, column: 1 }];
        let mut nav = Navigation::new(deck);
        let hold = nav.next();
        assert!(hold.is_some());
        assert!(hold.unwrap().message().contains("vanished"));
        assert_eq!(nav.current_index(), 0, "holds last good");
        // Same for hotspot.
        let mut nav2 = Navigation::new(deck_two());
        nav2.deck.slides[0].hotspots[0].target = "gone".into();
        let h2 = nav2.hotspot("h1");
        assert!(h2.is_some());
        assert_eq!(nav2.current_index(), 0);
    }

    #[test]
    fn two_views_stay_in_sync_via_shared_state() {
        // Both views read the same Navigation; a navigate from one is
        // visible to the other (sync over HTTP is exercised in CLI tests).
        let deck = deck_two();
        let shared = Arc::new(Mutex::new(Navigation::new(deck.clone())));
        let a = Arc::clone(&shared);
        let b = Arc::clone(&shared);
        a.lock().unwrap().next();
        assert_eq!(b.lock().unwrap().current_index(), 1);
    }
}
