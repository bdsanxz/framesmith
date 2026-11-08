//! Clip editing with ripple and links (Sprint 16, T006 / CAP-006, J-02,
//! J-06, STATE-03, INT-001, REC-001): move, edge trim, multi-select,
//! multi-drag with offsets kept, marquee, and delete land exactly;
//! ripple stays on by default so deleting a main-track clip closes the
//! gap with the receipt naming what moved; gaps highlight with working
//! close-gap and close-all-gaps actions; linked clips move together with
//! link, unlink, detach-audio, merge-back, and the out-of-sync badge; a
//! sync-breaking edit is refused with the reason and moves nothing.

use preview::daemon;
use preview::server::{run_serve, ServeOptions};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-studio-clips-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

/// Two text clips on the main lane: `title` (0..2s) and `bye`
/// (2..4s), 10 fps, 40 frames, no gaps.
fn text_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=4 fps=10 background=\"#112233\" version=1 {\n    text id=\"title\" value=\"Hi\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n    text id=\"bye\" value=\"Bye\" start=2 duration=2 x=10 y=80 size=24 color=\"#ffffff\"\n}\n",
    );
}

/// Three contiguous text clips: `a` (0..1s), `b` (1..2s), `c`
/// (2..4s). Deleting `b` opens a same-lane gap for ripple to close.
fn three_clip_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=4 fps=10 background=\"#112233\" version=1 {\n    text id=\"a\" value=\"A\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n    text id=\"b\" value=\"B\" start=1 duration=1 x=10 y=60 size=24 color=\"#ffffff\"\n    text id=\"c\" value=\"C\" start=2 duration=2 x=10 y=80 size=24 color=\"#ffffff\"\n}\n",
    );
}

/// Two text clips with an authored gap: `a` (0..1s) and `c`
/// (2..4s), leaving a 1..2s highlight on the lane.
fn gap_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=4 fps=10 background=\"#112233\" version=1 {\n    text id=\"a\" value=\"A\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n    text id=\"c\" value=\"C\" start=2 duration=2 x=10 y=80 size=24 color=\"#ffffff\"\n}\n",
    );
}

/// Minimal PCM WAV (48 kHz stereo s16) of `secs` seconds, written by
/// hand so audio tests need no encoder.
fn write_wav(dir: &Path, name: &str, secs: f64) {
    let rate: u32 = 48000;
    let frames = (secs * rate as f64).round() as u32;
    let data_len = frames * 2 * 2;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames * 2 {
        let s =
            ((i as f64 * 440.0 * 2.0 * std::f64::consts::PI / rate as f64).sin() * 10000.0) as i16;
        out.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(dir.join(name), out).unwrap();
}

/// A picture clip plus a sounding clip: `pic` text (0..2s) on the
/// graphics lane with `snd` audio (0..2s) on the audio lane.
fn av_project(folder: &Path) {
    write_wav(folder, "s.wav", 2.0);
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=4 fps=10 background=\"#112233\" version=1 {\n    text id=\"pic\" value=\"Pic\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n    audio id=\"snd\" path=\"s.wav\" start=0 duration=2 volume=-6\n}\n",
    );
}

/// Two text clips for link guards without an audio lane: `p1`
/// (0..2s) and `p2` (2..4s).
fn two_text_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=4 fps=10 background=\"#112233\" version=1 {\n    text id=\"p1\" value=\"One\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n    text id=\"p2\" value=\"Two\" start=2 duration=2 x=10 y=80 size=24 color=\"#ffffff\"\n}\n",
    );
}

/// Minimal HTTP exchange over loopback with an optional body.
/// Returns (status code, body).
fn http(port: u16, method: &str, target: &str, body: Option<&str>) -> (u16, Vec<u8>) {
    let addr = format!("127.0.0.1:{port}");
    let mut stream = TcpStream::connect_timeout(&addr.parse().unwrap(), Duration::from_secs(5))
        .expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let payload = body.unwrap_or("");
    let request = format!(
        "{method} {target} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    stream.write_all(request.as_bytes()).unwrap();
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
        Some(i) => raw[i + 4..].to_vec(),
        None => Vec::new(),
    };
    (code, body)
}

fn body_str(port: u16, method: &str, target: &str, body: Option<&str>) -> (u16, String) {
    let (code, raw) = http(port, method, target, body);
    (code, String::from_utf8_lossy(&raw).into_owned())
}

/// Run one real server for this test: `setup` writes the project,
/// `f` drives it, then the server shuts down and the thread joins.
fn with_server(
    tag: &str,
    setup: impl FnOnce(&Path),
    f: impl FnOnce(&Path, &Path, daemon::ServerEntry, u16),
) {
    let root = tmp(tag);
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    setup(&folder);
    let reg = root.join("registry.json");
    let dir = daemon::canonical_dir(&folder).unwrap();
    let opts = ServeOptions {
        folder: folder.clone(),
        port: 0,
        registry: reg.clone(),
        composition: None,
    };
    let handle = std::thread::spawn(move || run_serve(&opts));
    let start = Instant::now();
    let entry = loop {
        let entries = daemon::load_registry(&reg).unwrap_or_default();
        if let Some(e) = entries
            .into_iter()
            .find(|e| e.dir == dir && daemon::is_live(e.port))
        {
            break e;
        }
        if handle.is_finished() {
            panic!("server exited before serving");
        }
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "server never became ready"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    f(&folder, &reg, entry.clone(), entry.port);
    assert!(daemon::request_shutdown(entry.port), "shutdown answers");
    assert_eq!(handle.join().expect("server thread"), 0);
    assert!(!daemon::is_live(entry.port), "port is quiet after shutdown");
}

fn delta_bytes(folder: &Path) -> Option<Vec<u8>> {
    std::fs::read(folder.join(".framesmith").join("sdk-overrides.json")).ok()
}

fn turn_count(folder: &Path) -> usize {
    sdk::history::list_turns(folder)
        .map(|l| l.turns.len())
        .unwrap_or(usize::MAX)
}

fn timeline_rows(port: u16) -> String {
    let (code, rows) = body_str(port, "GET", "/api/studio/timeline", None);
    assert_eq!(code, 200, "{rows}");
    rows
}

#[test]
fn studio_clips_move_and_both_trim_edges_land_exactly() {
    with_server(
        "clips-move-trim",
        text_project,
        |folder, _reg, _entry, port| {
            // Move `title` to 1.0s: the window and a named turn follow.
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"select\",\"clip\":\"title\",\"to\":1.0}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("select moved"), "{out}");
            assert!(out.contains("[1.000,3.000]"), "{out}");
            // Trim the opening edge of `bye`.
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"select\",\"clip\":\"bye\",\"start\":2.5}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("select trimmed"), "{out}");
            assert!(out.contains("[2.500,4.000]"), "{out}");
            // Trim the closing edge of `title`.
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"select\",\"clip\":\"title\",\"end\":1.5}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("select trimmed"), "{out}");
            assert!(out.contains("[1.000,1.500]"), "{out}");
            let listing = sdk::history::list_turns(folder).expect("history lists");
            assert_eq!(listing.turns.len(), 3, "each gesture records a turn");
            assert!(
                listing.turns.iter().all(|t| t.actor == "studio"),
                "turns are named"
            );
        },
    );
}

#[test]
fn studio_clips_multi_select_multi_drag_and_marquee_land_exactly() {
    with_server(
        "clips-multi",
        three_clip_project,
        |folder, _reg, _entry, port| {
            // Multi-select picks the named set and records a turn.
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"multi-select\",\"clips\":[\"a\",\"c\"]}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("multi-select picked"), "{out}");
            assert!(out.contains("\"selection\":[\"a\",\"c\"]"), "{out}");
            // Multi-drag shifts the set by one delta, keeping offsets:
            // `a` 0..1s and `c` 2..4s both move +0.5s.
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"multi-drag\",\"clips\":[\"a\",\"c\"],\"delta\":0.5}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("keeping offsets"), "{out}");
            assert!(out.contains("[0.500,1.500]"), "{out}");
            assert!(out.contains("[2.500,4.500]"), "{out}");
            // Marquee picks the clips its window overlaps: 0..1.6s
            // meets `a` and `b` but not `c`.
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"marquee\",\"start\":0.0,\"end\":1.6}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("marquee picked"), "{out}");
            assert!(out.contains("\"selection\":[\"a\",\"b\"]"), "{out}");
            let listing = sdk::history::list_turns(folder).expect("history lists");
            assert_eq!(listing.turns.len(), 3, "each gesture records a turn");
        },
    );
}

#[test]
fn studio_clips_delete_lands_and_ripples_by_default_naming_what_moved() {
    with_server(
        "clips-delete-ripple",
        three_clip_project,
        |folder, _reg, _entry, port| {
            // Ripple stays on by default: deleting `b` (1..2s) pulls
            // `c` (2..4s) earlier across the gap, and the receipt names
            // what moved.
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"delete\",\"clip\":\"b\"}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("delete removed"), "{out}");
            assert!(out.contains("moving c"), "{out}");
            assert!(out.contains("\"moved\":[\"c\"]"), "{out}");
            assert!(out.contains("\"ripple\":true"), "{out}");
            let rows = timeline_rows(port);
            assert!(rows.contains("[1.000,3.000]"), "{rows}");
            assert!(!rows.contains("\"id\":\"b\""), "{rows}");
            let listing = sdk::history::list_turns(folder).expect("history lists");
            assert_eq!(listing.turns.len(), 1, "the delete records one turn");
        },
    );
}

#[test]
fn studio_clips_ripple_off_leaves_gaps_open() {
    with_server(
        "clips-ripple-off",
        three_clip_project,
        |folder, _reg, _entry, port| {
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"set-ripple\",\"ripple\":false}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("ripple off"), "{out}");
            assert!(out.contains("\"ripple\":false"), "{out}");
            // Gap closes refuse while ripple is off, naming the switch.
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"close-all-gaps\"}"),
            );
            assert_eq!(code, 409, "{refused}");
            assert!(refused.contains("while ripple is off"), "{refused}");
            // With ripple off the delete leaves `c` parked at 2..4s.
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"delete\",\"clip\":\"b\"}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("gaps stay open"), "{out}");
            assert!(out.contains("\"moved\":[]"), "{out}");
            let rows = timeline_rows(port);
            assert!(rows.contains("[2.000,4.000]"), "{rows}");
            // Flipping ripple back on honors gap closes again.
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"set-ripple\",\"ripple\":true}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("\"ripple\":true"), "{out}");
            let listing = sdk::history::list_turns(folder).expect("history lists");
            assert_eq!(listing.turns.len(), 3);
        },
    );
}

#[test]
fn studio_clips_gap_highlights_offer_close_gap_and_close_all_gaps() {
    with_server("clips-gaps", gap_project, |folder, _reg, _entry, port| {
        // The authored 1..2s lane gap highlights with the ripple switch.
        let (code, gaps) = body_str(port, "GET", "/api/studio/gaps", None);
        assert_eq!(code, 200, "{gaps}");
        assert!(gaps.contains("\"ripple\":true"), "{gaps}");
        assert!(gaps.contains("\"track\":\"graphics\""), "{gaps}");
        assert!(gaps.contains("[1.000,2.000]"), "{gaps}");
        // Close-gap shuts the highlighted gap, pulling `c` earlier.
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/tool",
            Some("{\"tool\":\"close-gap\",\"track\":\"graphics\",\"at\":1.5}"),
        );
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("close-gap shut"), "{out}");
        assert!(out.contains("\"moved\":[\"c\"]"), "{out}");
        let rows = timeline_rows(port);
        assert!(rows.contains("[1.000,3.000]"), "{rows}");
        let (code, gaps) = body_str(port, "GET", "/api/studio/gaps", None);
        assert_eq!(code, 200, "{gaps}");
        assert!(!gaps.contains("[1.000,2.000]"), "{gaps}");
        // Open a fresh mid-lane gap, then close every gap at once:
        // `c` drags a second later, `close-all-gaps` pulls it home.
        // (A leading gap before the first clip never highlights: gaps
        // run between consecutive clips.)
        let (code, _) = body_str(
            port,
            "POST",
            "/api/studio/tool",
            Some("{\"tool\":\"multi-drag\",\"clips\":[\"c\"],\"delta\":1.0}"),
        );
        assert_eq!(code, 200);
        let (code, gaps) = body_str(port, "GET", "/api/studio/gaps", None);
        assert_eq!(code, 200, "{gaps}");
        assert!(gaps.contains("[1.000,2.000]"), "{gaps}");
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/tool",
            Some("{\"tool\":\"close-all-gaps\"}"),
        );
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("close-all-gaps shut"), "{out}");
        assert!(out.contains("\"moved\":[\"c\"]"), "{out}");
        let rows = timeline_rows(port);
        assert!(rows.contains("[1.000,3.000]"), "{rows}");
        let listing = sdk::history::list_turns(folder).expect("history lists");
        assert_eq!(listing.turns.len(), 3, "each gesture records a turn");
    });
}

#[test]
fn studio_clips_linked_clips_move_together() {
    with_server(
        "clips-link-move",
        av_project,
        |folder, _reg, _entry, port| {
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"link\",\"clips\":[\"pic\",\"snd\"]}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("link"), "{out}");
            // Only hidden studio state writes: the sources never change.
            let kdl = std::fs::read(folder.join("composition.kdl")).unwrap();
            assert!(
                std::str::from_utf8(&kdl)
                    .unwrap()
                    .contains("start=0 duration=2"),
                "sources stay byte-identical"
            );
            assert!(
                folder
                    .join(".framesmith")
                    .join("studio-links.json")
                    .is_file(),
                "links persist in hidden studio state"
            );
            // The pair reads in sync with no badge.
            let (code, links) = body_str(port, "GET", "/api/studio/links", None);
            assert_eq!(code, 200, "{links}");
            assert!(links.contains("\"in_sync\":true"), "{links}");
            assert!(!links.contains("out-of-sync"), "{links}");
            // Multi-drag moves both together keeping offsets.
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"multi-drag\",\"clips\":[\"pic\",\"snd\"],\"delta\":0.5}"),
            );
            assert_eq!(code, 200, "{out}");
            let rows = timeline_rows(port);
            assert_eq!(rows.matches("[0.500,2.500]").count(), 2, "{rows}");
            // Moving one partner alone refuses naming the partner and
            // moves nothing.
            let before_delta = delta_bytes(folder);
            let before_turns = turn_count(folder);
            let before_rows = timeline_rows(port);
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"select\",\"clip\":\"pic\",\"to\":1.0}"),
            );
            assert_eq!(code, 409, "{refused}");
            assert!(refused.contains("would break sync"), "{refused}");
            assert!(refused.contains("\"snd\""), "{refused}");
            assert_eq!(delta_bytes(folder), before_delta, "refusals stage nothing");
            assert_eq!(turn_count(folder), before_turns, "refusals record nothing");
            assert_eq!(
                timeline_rows(port),
                before_rows,
                "refused moves move nothing"
            );
        },
    );
}

#[test]
fn studio_clips_unlink_detach_and_merge_back_each_work() {
    with_server(
        "clips-link-ops",
        av_project,
        |_folder, _reg, _entry, port| {
            let (code, _) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"link\",\"clips\":[\"pic\",\"snd\"]}"),
            );
            assert_eq!(code, 200);
            // Detach-audio separates the sound of the group.
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"detach-audio\",\"clip\":\"pic\"}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("detach-audio"), "{out}");
            let (code, links) = body_str(port, "GET", "/api/studio/links", None);
            assert_eq!(code, 200, "{links}");
            assert!(links.contains("\"detached\":true"), "{links}");
            // A detached member drifts freely, raising the badge.
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"select\",\"clip\":\"snd\",\"to\":0.5}"),
            );
            assert_eq!(code, 200, "{out}");
            let (code, links) = body_str(port, "GET", "/api/studio/links", None);
            assert_eq!(code, 200, "{links}");
            assert!(links.contains("\"in_sync\":false"), "{links}");
            assert!(links.contains("\"badge\":\"out-of-sync\""), "{links}");
            // Merge-back restores the offset and clears the badge.
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"merge-back\",\"clips\":[\"pic\",\"snd\"]}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("merge-back moved"), "{out}");
            let rows = timeline_rows(port);
            assert_eq!(rows.matches("[0.000,2.000]").count(), 2, "{rows}");
            let (code, links) = body_str(port, "GET", "/api/studio/links", None);
            assert_eq!(code, 200, "{links}");
            assert!(links.contains("\"in_sync\":true"), "{links}");
            assert!(!links.contains("out-of-sync"), "{links}");
            // Unlink separates the pair and prunes the spent group.
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"unlink\",\"clip\":\"pic\"}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("unlink"), "{out}");
            let (code, links) = body_str(port, "GET", "/api/studio/links", None);
            assert_eq!(code, 200, "{links}");
            assert!(links.contains("\"groups\":[]"), "{links}");
        },
    );
}

#[test]
fn studio_clips_sync_breaking_edits_refuse_moving_nothing() {
    with_server(
        "clips-sync-refuse",
        three_clip_project,
        |folder, _reg, _entry, port| {
            // Link `a` and `b`: every later sync-breaking gesture
            // names the cause and stages nothing.
            let (code, _) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"link\",\"clips\":[\"a\",\"b\"]}"),
            );
            assert_eq!(code, 200);
            let before_delta = delta_bytes(folder);
            let before_turns = turn_count(folder);
            let before_rows = timeline_rows(port);
            // Deleting one partner would break sync.
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"delete\",\"clip\":\"a\"}"),
            );
            assert_eq!(code, 409, "{refused}");
            assert!(refused.contains("would break sync"), "{refused}");
            // A lone trim of one partner breaks sync too.
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"select\",\"clip\":\"b\",\"end\":1.5}"),
            );
            assert_eq!(code, 409, "{refused}");
            assert!(refused.contains("would break sync"), "{refused}");
            // A lone multi-drag of one partner breaks sync as well.
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"multi-drag\",\"clips\":[\"a\"],\"delta\":0.5}"),
            );
            assert_eq!(code, 409, "{refused}");
            assert!(refused.contains("would break sync"), "{refused}");
            assert_eq!(delta_bytes(folder), before_delta, "refusals stage nothing");
            assert_eq!(turn_count(folder), before_turns, "refusals record nothing");
            assert_eq!(
                timeline_rows(port),
                before_rows,
                "refused edits move nothing"
            );
        },
    );
}

#[test]
fn studio_clips_refusal_paths_name_the_cause_and_stage_nothing() {
    with_server(
        "clips-refusals",
        text_project,
        |folder, _reg, _entry, port| {
            let before_delta = delta_bytes(folder);
            let before_turns = turn_count(folder);
            let before_rows = timeline_rows(port);
            // Unknown clip references refuse.
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"select\",\"clip\":\"nope\",\"to\":1.0}"),
            );
            assert_eq!(code, 409, "{refused}");
            assert!(refused.contains("names no clip"), "{refused}");
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"delete\",\"clip\":\"nope\"}"),
            );
            assert_eq!(code, 409, "{refused}");
            assert!(refused.contains("names no clip"), "{refused}");
            // A multi-drag parking a clip before zero refuses.
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"multi-drag\",\"clips\":[\"title\"],\"delta\":-5.0}"),
            );
            assert_eq!(code, 409, "{refused}");
            assert!(refused.contains("leaves the timeline"), "{refused}");
            // A marquee over no clip window refuses.
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"marquee\",\"start\":10.0,\"end\":11.0}"),
            );
            assert_eq!(code, 409, "{refused}");
            assert!(refused.contains("overlaps no clip"), "{refused}");
            // A gapless lane refuses close-gap and close-all-gaps.
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"close-gap\",\"track\":\"graphics\",\"at\":1.0}"),
            );
            assert_eq!(code, 409, "{refused}");
            assert!(refused.contains("finds no gap"), "{refused}");
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"close-all-gaps\"}"),
            );
            assert_eq!(code, 409, "{refused}");
            assert!(refused.contains("finds no gaps"), "{refused}");
            // Link needs two distinct declared ids on the timeline.
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"link\",\"clips\":[\"title\"]}"),
            );
            assert_eq!(code, 400, "{refused}");
            assert!(refused.contains("at least two clips"), "{refused}");
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"link\",\"clips\":[\"title\",\"nope\"]}"),
            );
            assert_eq!(code, 409, "{refused}");
            assert!(refused.contains("not a declared clip `id`"), "{refused}");
            // Unlink, detach-audio, and merge-back name unlinked clips.
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"unlink\",\"clip\":\"title\"}"),
            );
            assert_eq!(code, 409, "{refused}");
            assert!(refused.contains("sits in no link group"), "{refused}");
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"detach-audio\",\"clip\":\"title\"}"),
            );
            assert_eq!(code, 409, "{refused}");
            assert!(refused.contains("sits in no link group"), "{refused}");
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"merge-back\",\"clips\":[\"title\",\"bye\"]}"),
            );
            assert_eq!(code, 409, "{refused}");
            assert!(refused.contains("share no link baseline"), "{refused}");
            // The ripple switch needs its switch.
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"set-ripple\"}"),
            );
            assert_eq!(code, 400, "{refused}");
            assert!(refused.contains("names no switch"), "{refused}");
            assert_eq!(delta_bytes(folder), before_delta, "refusals stage nothing");
            assert_eq!(turn_count(folder), before_turns, "refusals record nothing");
            assert_eq!(
                timeline_rows(port),
                before_rows,
                "refused edits move nothing"
            );
        },
    );
}

#[test]
fn studio_clips_link_guards_name_already_linked_and_audioless_groups() {
    with_server(
        "clips-link-guards",
        two_text_project,
        |folder, _reg, _entry, port| {
            let (code, _) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"link\",\"clips\":[\"p1\",\"p2\"]}"),
            );
            assert_eq!(code, 200);
            let before_delta = delta_bytes(folder);
            let before_turns = turn_count(folder);
            // Linking an already-linked clip names its partners.
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"link\",\"clips\":[\"p1\",\"p2\"]}"),
            );
            assert_eq!(code, 409, "{refused}");
            assert!(refused.contains("already linked"), "{refused}");
            // A group holding no audio clip refuses detach-audio.
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"detach-audio\",\"clip\":\"p1\"}"),
            );
            assert_eq!(code, 409, "{refused}");
            assert!(refused.contains("holds no audio clip"), "{refused}");
            assert_eq!(delta_bytes(folder), before_delta, "refusals stage nothing");
            assert_eq!(turn_count(folder), before_turns, "refusals record nothing");
        },
    );
}

#[test]
fn studio_shell_serves_the_clip_editing_surface() {
    with_server("clips-page", gap_project, |_folder, _reg, _entry, port| {
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200, "{page}");
        assert!(page.contains("id=\"clip-tools\""), "{page}");
        assert!(page.contains("id=\"gap-list\""), "{page}");
        assert!(page.contains("id=\"link-list\""), "{page}");
        assert!(page.contains("out-of-sync"), "{page}");
        assert!(page.contains("close all gaps"), "{page}");
        assert!(page.contains("multi-drag"), "{page}");
        assert!(page.contains("merge-back"), "{page}");
        assert!(page.contains("/api/studio/gaps"), "{page}");
        assert!(page.contains("/api/studio/links"), "{page}");
        assert!(page.contains("clip-ripple"), "{page}");
    });
}

#[test]
fn studio_clips_round_trip_keeps_default_renders_byte_identical() {
    with_server(
        "clips-roundtrip",
        text_project,
        |folder, _reg, _entry, port| {
            // Capture the default frame before any clip edit runs.
            let (code, before) = http(port, "GET", "/api/frame?i=5&action=step", None);
            assert_eq!(code, 200);
            // Pure link-state gestures never touch rendered frames.
            let (code, _) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"link\",\"clips\":[\"title\",\"bye\"]}"),
            );
            assert_eq!(code, 200);
            let (code, _) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"set-ripple\",\"ripple\":false}"),
            );
            assert_eq!(code, 200);
            let (code, framed) = http(port, "GET", "/api/frame?i=5&action=step", None);
            assert_eq!(code, 200);
            assert_eq!(
                framed, before,
                "link state keeps default renders byte-identical"
            );
            // A ripple delete moves frames, then the undo replays the
            // default bytes exactly. Unlink first: deleting one linked
            // partner alone would refuse as sync-breaking.
            let (code, _) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"unlink\",\"clips\":[\"title\",\"bye\"]}"),
            );
            assert_eq!(code, 200);
            let (code, _) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"set-ripple\",\"ripple\":true}"),
            );
            assert_eq!(code, 200);
            let (code, _) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"delete\",\"clip\":\"title\"}"),
            );
            assert_eq!(code, 200);
            let (code, moved) = http(port, "GET", "/api/frame?i=5&action=step", None);
            assert_eq!(code, 200);
            assert_ne!(moved, before, "the deleted clip shows on the canvas");
            let seq = sdk::history::list_turns(folder)
                .expect("history lists")
                .turns
                .last()
                .expect("a turn")
                .seq;
            sdk::history::undo_turn(folder, seq).expect("undo applies");
            let (code, _) = body_str(port, "POST", "/api/reload", None);
            assert_eq!(code, 200);
            let (code, restored) = http(port, "GET", "/api/frame?i=5&action=step", None);
            assert_eq!(code, 200);
            assert_eq!(restored, before, "undo restores byte-identical frames");
            // The composition sources never changed: only hidden
            // studio state files did.
            let kdl = std::fs::read(folder.join("composition.kdl")).unwrap();
            assert!(
                std::str::from_utf8(&kdl)
                    .unwrap()
                    .contains("start=0 duration=2"),
                "sources stay byte-identical"
            );
        },
    );
}
