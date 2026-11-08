//! Timeline tools regressions (Sprint 16, T004 / CAP-004, J-02, J-06,
//! STATE-03, INT-001, REC-001): select moves and trims, razor splits at
//! the click (the all-tracks modifier splits every track), select-before
//! and select-after pick clip sets, split at playhead divides the clip
//! under the playhead, ruler clicks seek, and the in/out work area
//! bounds playback. Every applied tool lands as a named history turn
//! through the SDK with a receipt; a refused tool stages nothing and
//! names its cause.

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
        "framesmith-studio-tools-{}-{}-{}",
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

/// Two text clips on the main track: `title` (0..2s) and `bye` (2..4s),
/// 10 fps, 40 frames.
fn text_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=4 fps=10 background=\"#112233\" version=1 {\n    text id=\"title\" value=\"Hi\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n    text id=\"bye\" value=\"Bye\" start=2 duration=2 x=10 y=80 size=24 color=\"#ffffff\"\n}\n",
    );
}

/// Overlapping clips on two tracks for the all-tracks razor: `main`
/// text (0..4s) on the graphics lane plus a `pip` text (1..3s) on the
/// caption lane.
fn two_track_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=4 fps=10 background=\"#112233\" version=1 {\n    text id=\"main\" value=\"Main\" start=0 duration=4 x=10 y=40 size=24 color=\"#ffffff\"\n    text id=\"pip\" value=\"Pip\" start=1 duration=2 x=10 y=80 size=24 color=\"#ffffff\" track=\"caption\"\n}\n",
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

#[test]
fn studio_select_moves_a_clip_with_a_named_turn() {
    with_server("tool-move", text_project, |folder, _reg, _entry, port| {
        let before_delta = delta_bytes(folder);
        let before_turns = turn_count(folder);
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/tool",
            Some("{\"tool\":\"select\",\"clip\":\"title\",\"to\":1.0}"),
        );
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("\"applied\":true"), "{out}");
        assert!(out.contains("select moved"), "{out}");
        assert!(out.contains("\"turn\":"), "{out}");
        assert!(out.contains("studio"), "{out}");
        // The rows moved with the clip.
        assert!(out.contains("[1.000,3.000]"), "{out}");
        // The delta and history recorded the validated turn.
        assert_ne!(delta_bytes(folder), before_delta, "a turn persisted");
        let listing = sdk::history::list_turns(folder).expect("history lists");
        assert_eq!(listing.turns.len(), before_turns + 1, "one turn recorded");
        let last = listing.turns.last().expect("a turn");
        assert_eq!(last.actor, "studio", "the turn is named");
        assert!(last.summary.contains("retime"), "{:?}", last.summary);
    });
}

#[test]
fn studio_select_trims_a_clip_edge_with_a_named_turn() {
    with_server("tool-trim", text_project, |folder, _reg, _entry, port| {
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/tool",
            Some("{\"tool\":\"select\",\"clip\":\"bye\",\"start\":2.5}"),
        );
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("select trimmed"), "{out}");
        assert!(out.contains("[2.500,4.000]"), "{out}");
        let listing = sdk::history::list_turns(folder).expect("history lists");
        assert_eq!(listing.turns.len(), 1);
        assert_eq!(listing.turns[0].actor, "studio");
    });
}

#[test]
fn studio_razor_splits_at_the_click_and_all_tracks_splits_every_track() {
    with_server("tool-razor", text_project, |_folder, _reg, _entry, port| {
        // Single-track razor: one clip becomes two.
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/tool",
            Some("{\"tool\":\"razor\",\"at\":1.0}"),
        );
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("razor split"), "{out}");
        let (code, rows) = body_str(port, "GET", "/api/studio/timeline", None);
        assert_eq!(code, 200, "{rows}");
        assert_eq!(rows.matches("\"kind\":\"text\"").count(), 3, "{rows}");
    });
    with_server(
        "tool-razor-all",
        two_track_project,
        |folder, _reg, _entry, port| {
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"razor\",\"at\":2.0,\"all-tracks\":true}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("2 clip(s)"), "{out}");
            let (code, rows) = body_str(port, "GET", "/api/studio/timeline", None);
            assert_eq!(code, 200, "{rows}");
            assert_eq!(rows.matches("\"kind\":\"text\"").count(), 4, "{rows}");
            // Both halves show on the timeline: the split divided each track.
            let listing = sdk::history::list_turns(folder).expect("history lists");
            assert_eq!(listing.turns.len(), 1);
            assert!(
                listing.turns[0].summary.contains("split"),
                "{:?}",
                listing.turns[0].summary
            );
        },
    );
}

#[test]
fn studio_select_before_and_after_pick_clip_sets() {
    with_server("tool-pick", text_project, |folder, _reg, _entry, port| {
        // `title` ends at 2.0s; `bye` opens at 2.0s.
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/tool",
            Some("{\"tool\":\"select-before\",\"at\":2.0}"),
        );
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("select-before"), "{out}",);
        assert!(out.contains("title"), "{out}");
        assert!(!out.contains("\"selection\":[\"bye\"]"), "{out}");
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/tool",
            Some("{\"tool\":\"select-after\",\"at\":2.0}"),
        );
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("bye"), "{out}");
        // Picks record turns too: every applied action lands in history.
        let listing = sdk::history::list_turns(folder).expect("history lists");
        assert_eq!(listing.turns.len(), 2, "each pick records a turn");
    });
}

#[test]
fn studio_split_at_playhead_divides_the_clip_under_the_playhead() {
    with_server(
        "tool-split-ph",
        text_project,
        |folder, _reg, _entry, port| {
            // Park the playhead inside `title` (0..2s): frame 5 at 10 fps.
            let (code, _) = body_str(port, "POST", "/api/studio/seek?frame=5", None);
            assert_eq!(code, 200);
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"split-at-playhead\"}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("at playhead 0.500s"), "{out}");
            let (code, rows) = body_str(port, "GET", "/api/studio/timeline", None);
            assert_eq!(code, 200, "{rows}");
            assert_eq!(rows.matches("\"kind\":\"text\"").count(), 3, "{rows}");
            let listing = sdk::history::list_turns(folder).expect("history lists");
            assert_eq!(listing.turns.len(), 1);
        },
    );
}

#[test]
fn studio_ruler_seek_parks_and_the_work_area_bounds_playback() {
    with_server("tool-seek", text_project, |_folder, _reg, _entry, port| {
        // No work area: the seek parks exactly.
        let (code, out) = body_str(port, "POST", "/api/studio/seek?frame=30", None);
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("\"frame\":30"), "{out}");
        assert!(out.contains("\"clamped\":false"), "{out}");
        // Past the end refuses naming the frame.
        let (code, refused) = body_str(port, "POST", "/api/studio/seek?frame=40", None);
        assert_eq!(code, 409, "{refused}");
        assert!(refused.contains("frame 40"), "{refused}");
        // A missing index refuses naming the parameter.
        let (code, refused) = body_str(port, "POST", "/api/studio/seek", None);
        assert_eq!(code, 400, "{refused}");
        assert!(refused.contains("?frame=N"), "{refused}");
        // Set the work area: seeks outside clamp into it and say so.
        let (code, area) = body_str(
            port,
            "POST",
            "/api/studio/workarea",
            Some("{\"in\":1.0,\"out\":3.0}"),
        );
        assert_eq!(code, 200, "{area}");
        assert!(area.contains("\"in\":1.000"), "{area}");
        let (code, out) = body_str(port, "POST", "/api/studio/seek?frame=35", None);
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("\"frame\":30"), "{out}");
        assert!(out.contains("\"clamped\":true"), "{out}");
        let (code, out) = body_str(port, "POST", "/api/studio/seek?frame=5", None);
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("\"frame\":10"), "{out}");
        assert!(out.contains("\"clamped\":true"), "{out}");
        // Inside the area seeks park exactly.
        let (code, out) = body_str(port, "POST", "/api/studio/seek?frame=20", None);
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("\"clamped\":false"), "{out}");
        // Clearing restores the full timeline.
        let (code, cleared) = body_str(port, "DELETE", "/api/studio/workarea", None);
        assert_eq!(code, 200, "{cleared}");
        assert!(cleared.contains("\"restored\":false"), "{cleared}");
    });
}

#[test]
fn studio_work_area_refusals_name_the_cause_and_save_nothing() {
    with_server(
        "tool-area-no",
        text_project,
        |folder, _reg, _entry, port| {
            // Duration is 4s: negative, unopened, and past-the-end areas refuse.
            for (label, bad) in [
                ("empty", ""),
                ("garbage", "{ nope"),
                ("negative", "{\"in\":-1.0,\"out\":2.0}"),
                ("unopened", "{\"in\":2.0,\"out\":2.0}"),
                ("past-end", "{\"in\":3.0,\"out\":9.0}"),
            ] {
                let (code, refused) = body_str(port, "POST", "/api/studio/workarea", Some(bad));
                assert_eq!(code, 400, "{label}: {refused}");
                assert!(refused.contains("studio refused"), "{label}: {refused}");
            }
            assert!(
                !preview::studio_tools::work_area_path_for(folder).exists(),
                "refused areas write nothing"
            );
            // A corrupt work-area file falls back to the full timeline with a banner.
            let path = preview::studio_tools::work_area_path_for(folder);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "{ not a work area").unwrap();
            let (code, area) = body_str(port, "GET", "/api/studio/workarea", None);
            assert_eq!(code, 200, "{area}");
            assert!(area.contains("full timeline"), "{area}");
            assert!(area.contains("\"restored\":false"), "{area}");
        },
    );
}

#[test]
fn studio_refused_tools_stage_nothing_and_name_the_cause() {
    with_server("tool-refuse", text_project, |folder, _reg, _entry, port| {
        let before_delta = delta_bytes(folder);
        let before_turns = turn_count(folder);
        // Malformed bodies refuse before anything stages.
        for (label, bad) in [
            ("empty", ""),
            ("garbage", "{ nope"),
            ("unknown-tool", "{\"tool\":\"smear\"}"),
            ("select-no-clip", "{\"tool\":\"select\",\"to\":1.0}"),
            (
                "select-no-gesture",
                "{\"tool\":\"select\",\"clip\":\"title\"}",
            ),
            (
                "select-both",
                "{\"tool\":\"select\",\"clip\":\"title\",\"to\":1.0,\"start\":0.5}",
            ),
        ] {
            let (code, refused) = body_str(port, "POST", "/api/studio/tool", Some(bad));
            assert_eq!(code, 400, "{label}: {refused}");
            assert!(refused.contains("studio refused"), "{label}: {refused}");
        }
        // Unknown clips, targetless splits, and out-of-range picks
        // refuse with the cause.
        for (label, bad, needle) in [
            (
                "unknown-clip",
                "{\"tool\":\"select\",\"clip\":\"nope\",\"to\":1.0}",
                "names no clip",
            ),
            (
                "razor-gap",
                "{\"tool\":\"razor\",\"at\":99.0}",
                "meets no clip",
            ),
            (
                "before-empty",
                "{\"tool\":\"select-before\",\"at\":0.0}",
                "matches no clip",
            ),
            (
                "after-empty",
                "{\"tool\":\"select-after\",\"at\":4.0}",
                "matches no clip",
            ),
        ] {
            let (code, refused) = body_str(port, "POST", "/api/studio/tool", Some(bad));
            assert_eq!(code, 409, "{label}: {refused}");
            assert!(refused.contains("studio refused"), "{label}: {refused}");
            assert!(refused.contains(needle), "{label}: {refused}");
        }
        // Split at a gap playhead refuses too: park at frame 39 (3.9s)
        // is inside `bye`, so move it past the end is impossible —
        // instead split where no clip sits by seeking a one-clip project
        // gap: frame 25 sits in `bye` (2..4s); use razor at a gap time
        // on this project has no gap, so force the SDK edge: split
        // exactly at a window edge refuses strictly-inside.
        let (code, refused) = body_str(
            port,
            "POST",
            "/api/studio/tool",
            Some("{\"tool\":\"razor\",\"clip\":\"title\",\"at\":2.0}"),
        );
        assert_eq!(code, 409, "{refused}");
        assert!(refused.contains("strictly inside"), "{refused}");
        // Every refusal left the delta and history exactly as they were.
        assert_eq!(delta_bytes(folder), before_delta, "refusals stage nothing");
        assert_eq!(turn_count(folder), before_turns, "refusals record nothing");
    });
}

#[test]
fn studio_split_at_gap_playhead_stages_nothing() {
    with_server("tool-gap", text_project, |folder, _reg, _entry, port| {
        // `bye` spans 2..4s: shrink it so 3.5s sits in no window.
        let (code, _) = body_str(
            port,
            "POST",
            "/api/studio/tool",
            Some("{\"tool\":\"select\",\"clip\":\"bye\",\"end\":3.0}"),
        );
        assert_eq!(code, 200);
        let before_delta = delta_bytes(folder);
        let before_turns = turn_count(folder);
        // Playhead at 3.5s (frame 35) now sits in a gap.
        let (code, _) = body_str(port, "POST", "/api/studio/seek?frame=35", None);
        assert_eq!(code, 200);
        let (code, refused) = body_str(
            port,
            "POST",
            "/api/studio/tool",
            Some("{\"tool\":\"split-at-playhead\"}"),
        );
        assert_eq!(code, 409, "{refused}");
        assert!(refused.contains("meets no clip window"), "{refused}");
        assert_eq!(delta_bytes(folder), before_delta, "refusals stage nothing");
        assert_eq!(turn_count(folder), before_turns, "refusals record nothing");
    });
}

#[test]
fn studio_tool_round_trip_keeps_default_renders_byte_identical() {
    with_server(
        "tool-roundtrip",
        text_project,
        |folder, _reg, _entry, port| {
            // Capture the default frame before any tool runs.
            let (code, before) = http(port, "GET", "/api/frame?i=5&action=step", None);
            assert_eq!(code, 200);
            // Move `title` away and back home: the frames must differ, then
            // match the default bytes exactly after the SDK undo replays.
            let (code, _) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"select\",\"clip\":\"title\",\"to\":1.0}"),
            );
            assert_eq!(code, 200);
            let (code, moved) = http(port, "GET", "/api/frame?i=5&action=step", None);
            assert_eq!(code, 200);
            assert_ne!(moved, before, "the moved clip shows on the canvas");
            // Undo the recorded turn through the shared history, then reload.
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
            // The composition sources never changed: only hidden files did.
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

#[test]
fn studio_shell_serves_the_tool_surface() {
    with_server("tool-page", text_project, |_folder, _reg, _entry, port| {
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200, "{page}");
        assert!(page.contains("id=\"tool-bar\""), "{page}");
        assert!(page.contains("id=\"tool-status\""), "{page}");
        assert!(page.contains("split at playhead"), "{page}");
        assert!(page.contains("select before"), "{page}");
        assert!(page.contains("select after"), "{page}");
        assert!(page.contains("work area"), "{page}");
        assert!(page.contains("/api/studio/tool"), "{page}");
        assert!(page.contains("/api/studio/seek"), "{page}");
    });
}
