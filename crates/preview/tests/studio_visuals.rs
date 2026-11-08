//! Keyframe lanes, beat strip, and clip visuals regressions (Sprint
//! 16, T007 / CAP-007, CAP-008, J-03, REC-005): lanes group one
//! property each with draggable-diamond data and a four-action menu
//! applying through the SDK patch and history machinery; out-of-segment
//! drags clamp showing the clamped value; beats move and delete with
//! beat-only undo; every clip shows truthful visuals (filmstrips,
//! waveforms with peak marks, text content, badges, fade handles) with
//! named placeholder boxes where media cannot show. Refusals name
//! their cause and stage nothing.

use preview::daemon;
use preview::server::{run_serve, ServeOptions};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-studio-visuals-{tag}-{}-{}",
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

fn ffmpeg(args: &[&str]) {
    let out = Command::new("ffmpeg")
        .args(["-y", "-v", "error"])
        .args(args)
        .output()
        .expect("run host ffmpeg");
    assert!(out.status.success(), "host ffmpeg failed: {args:?}");
}

/// Mixed reference project: two abutting texts bridged by a fade,
/// one sounding audio clip with gain and fades, one effected video
/// clip, and one key per animated property.
fn mixed_project(folder: &Path) {
    write_wav(folder, "s.wav", 4.0);
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc=duration=2:size=64x48:rate=10",
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        &folder.join("v.mp4").display().to_string(),
    ]);
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=4 fps=10 background=\"#112233\" version=1 {\n    text id=\"a\" value=\"Hello timeline\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n    text id=\"b\" value=\"Second line\" start=2 duration=2 x=10 y=80 size=24 color=\"#ffffff\"\n    audio id=\"s\" path=\"s.wav\" start=0 duration=4 volume=-6 fade-in=0.5 fade-out=0.5\n    video id=\"v\" path=\"v.mp4\" start=1 duration=2 {\n        effect preset=\"grayscale\"\n    }\n    transition from=\"a\" to=\"b\" duration=0.5 kind=\"fade\"\n    key target=0 property=\"x\" {\n        frame at=0 value=10\n        frame at=1 value=60 ease=\"in-quad\"\n        frame at=2 value=110\n    }\n    key target=0 property=\"opacity\" {\n        frame at=0 value=0\n        frame at=2 value=1\n    }\n}\n",
    );
}

/// Minimal HTTP exchange over loopback with an optional body.
/// Returns (status code, body).
fn http(port: u16, method: &str, target: &str, body: Option<&str>) -> (u16, Vec<u8>) {
    let addr = format!("127.0.0.1:{port}");
    let mut stream = TcpStream::connect_timeout(&addr.parse().unwrap(), Duration::from_secs(5))
        .expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
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
            start.elapsed() < Duration::from_secs(60),
            "server never became ready"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    f(&folder, &reg, entry.clone(), entry.port);
    assert!(daemon::request_shutdown(entry.port), "shutdown answers");
    assert_eq!(handle.join().expect("server thread"), 0);
    assert!(!daemon::is_live(entry.port), "port is quiet after shutdown");
}

#[test]
fn lanes_group_one_property_each() {
    with_server("lanes", mixed_project, |_folder, _reg, _entry, port| {
        let (code, lanes) = body_str(port, "GET", "/api/studio/lanes", None);
        assert_eq!(code, 200, "{lanes}");
        assert!(lanes.contains("\"composition\":\"t\""), "{lanes}");
        // One lane per property, in property name order.
        let x = lanes.find("\"property\":\"x\"").expect("x lane");
        let opacity = lanes
            .find("\"property\":\"opacity\"")
            .expect("opacity lane");
        assert!(opacity < x, "lanes ride in property order: {lanes}");
        // The x key carries its three diamonds with easings and the
        // target clip segment the drags clamp into.
        assert!(lanes.contains("\"index\":0,\"target\":0"), "{lanes}");
        assert!(lanes.contains("\"at\":0.000"), "{lanes}");
        assert!(lanes.contains("\"at\":1.000"), "{lanes}");
        assert!(lanes.contains("\"at\":2.000"), "{lanes}");
        assert!(lanes.contains("\"value\":60"), "{lanes}");
        assert!(lanes.contains("\"ease\":\"in-quad\""), "{lanes}");
        assert!(lanes.contains("\"segment\":[0.000,2.000]"), "{lanes}");
        // Unknown compositions refuse naming the holdings.
        let (code, missing) = body_str(port, "GET", "/api/studio/lanes?composition=nope", None);
        assert_eq!(code, 404, "{missing}");
        assert!(missing.contains("unknown composition"), "{missing}");
    });
}

#[test]
fn keyframe_menu_retimes_eases_copies_and_deletes() {
    with_server("keymenu", mixed_project, |_folder, _reg, _entry, port| {
        // Move to a valid instant inside the segment.
        let (code, receipt) = body_str(
            port,
            "POST",
            "/api/studio/keyframe",
            Some("{\"actor\":\"maker\",\"action\":\"retime\",\"key\":0,\"stop\":1,\"at\":1.5}"),
        );
        assert_eq!(code, 200, "{receipt}");
        assert!(receipt.contains("\"saved\":true"), "{receipt}");
        assert!(receipt.contains("\"turn\":0"), "{receipt}");
        assert!(receipt.contains("retime key 0 stop 1"), "{receipt}");
        assert!(receipt.contains("\"clamped\":false"), "{receipt}");
        let (_, lanes) = body_str(port, "GET", "/api/studio/lanes", None);
        assert!(lanes.contains("\"at\":1.500"), "{lanes}");

        // Past the segment edge clamps showing the clamped value.
        let (code, receipt) = body_str(
            port,
            "POST",
            "/api/studio/keyframe",
            Some("{\"actor\":\"maker\",\"action\":\"retime\",\"key\":1,\"stop\":1,\"at\":99}"),
        );
        assert_eq!(code, 200, "{receipt}");
        assert!(receipt.contains("\"clamped\":true"), "{receipt}");
        assert!(receipt.contains("\"applied_at\":2.000"), "{receipt}");
        let (_, lanes) = body_str(port, "GET", "/api/studio/lanes", None);
        assert!(lanes.contains("\"property\":\"opacity\""), "{lanes}");
        assert!(lanes.contains("\"at\":2.000"), "{lanes}");

        // Edit the stop easing, then the key fallback.
        let (code, receipt) = body_str(
            port,
            "POST",
            "/api/studio/keyframe",
            Some("{\"actor\":\"maker\",\"action\":\"set-ease\",\"key\":0,\"stop\":2,\"ease\":\"bounce\"}"),
        );
        assert_eq!(code, 200, "{receipt}");
        let (_, lanes) = body_str(port, "GET", "/api/studio/lanes", None);
        assert!(lanes.contains("\"ease\":\"bounce\""), "{lanes}");

        // Copy the key: the x lane grows a second key.
        let (code, receipt) = body_str(
            port,
            "POST",
            "/api/studio/keyframe",
            Some("{\"actor\":\"maker\",\"action\":\"copy\",\"key\":0}"),
        );
        assert_eq!(code, 200, "{receipt}");
        assert!(receipt.contains("copy key 0"), "{receipt}");
        let (_, lanes) = body_str(port, "GET", "/api/studio/lanes", None);
        assert_eq!(lanes.matches("\"target\":0").count(), 3, "{lanes}");

        // Delete one stop of the three-stop key, then delete from the
        // two-stop key, which removes the whole key.
        let (code, receipt) = body_str(
            port,
            "POST",
            "/api/studio/keyframe",
            Some("{\"actor\":\"maker\",\"action\":\"delete\",\"key\":0,\"stop\":1}"),
        );
        assert_eq!(code, 200, "{receipt}");
        let (code, receipt) = body_str(
            port,
            "POST",
            "/api/studio/keyframe",
            Some("{\"actor\":\"maker\",\"action\":\"delete\",\"key\":1,\"stop\":0}"),
        );
        assert_eq!(code, 200, "{receipt}");
        let (_, lanes) = body_str(port, "GET", "/api/studio/lanes", None);
        assert!(!lanes.contains("\"property\":\"opacity\""), "{lanes}");
    });
}

#[test]
fn keyframe_refusals_name_their_cause() {
    with_server("keyrefuse", mixed_project, |_folder, _reg, _entry, port| {
        // Unknown actions and empty bodies refuse before anything stages.
        let (code, refused) = body_str(
            port,
            "POST",
            "/api/studio/keyframe",
            Some("{\"actor\":\"maker\",\"action\":\"spin\",\"key\":0}"),
        );
        assert_eq!(code, 400, "{refused}");
        assert!(refused.contains("unknown keyframe action"), "{refused}");
        let (code, refused) = body_str(port, "POST", "/api/studio/keyframe", Some(""));
        assert_eq!(code, 400, "{refused}");
        // A missing key or stop refuses naming the menu entries.
        let (code, refused) = body_str(
            port,
            "POST",
            "/api/studio/keyframe",
            Some("{\"actor\":\"maker\",\"action\":\"retime\",\"stop\":0,\"at\":1}"),
        );
        assert_eq!(code, 400, "{refused}");
        assert!(refused.contains("no `key` index"), "{refused}");
        let (code, refused) = body_str(
            port,
            "POST",
            "/api/studio/keyframe",
            Some("{\"actor\":\"maker\",\"action\":\"retime\",\"key\":0,\"at\":1}"),
        );
        assert_eq!(code, 409, "{refused}");
        assert!(refused.contains("no `stop`"), "{refused}");
        // Unknown keys, order collisions, and bad easings refuse while
        // the lanes stay exactly as they were.
        let (code, refused) = body_str(
            port,
            "POST",
            "/api/studio/keyframe",
            Some("{\"actor\":\"maker\",\"action\":\"retime\",\"key\":9,\"stop\":0,\"at\":1}"),
        );
        assert_eq!(code, 409, "{refused}");
        assert!(refused.contains("key index 9 names nothing"), "{refused}");
        let (code, refused) = body_str(
            port,
            "POST",
            "/api/studio/keyframe",
            Some("{\"actor\":\"maker\",\"action\":\"retime\",\"key\":0,\"stop\":0,\"at\":1.5}"),
        );
        assert_eq!(code, 409, "{refused}");
        assert!(
            refused.contains("never stays under stop 1 at 1"),
            "{refused}"
        );
        let (code, refused) = body_str(
            port,
            "POST",
            "/api/studio/keyframe",
            Some("{\"actor\":\"maker\",\"action\":\"set-ease\",\"key\":0,\"ease\":\"wobble\"}"),
        );
        assert_eq!(code, 409, "{refused}");
        assert!(refused.contains("\"wobble\""), "{refused}");
        // Unknown compositions refuse naming the holdings.
        let (code, refused) = body_str(
            port,
            "POST",
            "/api/studio/keyframe",
            Some("{\"actor\":\"maker\",\"action\":\"copy\",\"key\":0,\"composition\":\"nope\"}"),
        );
        assert_eq!(code, 404, "{refused}");
        assert!(refused.contains("unknown composition"), "{refused}");
        // The lanes never moved.
        let (_, lanes) = body_str(port, "GET", "/api/studio/lanes", None);
        assert!(lanes.contains("\"at\":1.000"), "{lanes}");
        assert!(lanes.contains("\"property\":\"opacity\""), "{lanes}");
    });
}

#[test]
fn beats_move_delete_and_undo_beats_alone() {
    with_server("beats", mixed_project, |folder, _reg, _entry, port| {
        // The strip starts empty with no undo.
        let (code, strip) = body_str(port, "GET", "/api/studio/beats", None);
        assert_eq!(code, 200, "{strip}");
        assert!(strip.contains("\"beats\":[]"), "{strip}");
        assert!(strip.contains("\"undo_depth\":0"), "{strip}");

        // Add two beats, then move one past the composition end: the
        // move clamps to the end showing the clamped value.
        let (code, receipt) = body_str(
            port,
            "POST",
            "/api/studio/beats",
            Some("{\"action\":\"add\",\"name\":\"downbeat\",\"at\":1}"),
        );
        assert_eq!(code, 200, "{receipt}");
        assert!(receipt.contains("added beat"), "{receipt}");
        let (code, receipt) = body_str(
            port,
            "POST",
            "/api/studio/beats",
            Some("{\"action\":\"add\",\"name\":\"upbeat\",\"at\":3}"),
        );
        assert_eq!(code, 200, "{receipt}");
        let (code, receipt) = body_str(
            port,
            "POST",
            "/api/studio/beats",
            Some("{\"action\":\"move\",\"name\":\"upbeat\",\"at\":99}"),
        );
        assert_eq!(code, 200, "{receipt}");
        assert!(
            receipt.contains("clamped to the composition end"),
            "{receipt}"
        );
        let (_, strip) = body_str(port, "GET", "/api/studio/beats", None);
        assert!(
            strip.contains("\"name\":\"upbeat\",\"at\":4.000"),
            "{strip}"
        );
        assert!(strip.contains("\"undo_depth\":3"), "{strip}");

        // Delete one beat, then undo restores it while clips never move.
        let (code, receipt) = body_str(
            port,
            "POST",
            "/api/studio/beats",
            Some("{\"action\":\"delete\",\"name\":\"downbeat\"}"),
        );
        assert_eq!(code, 200, "{receipt}");
        let (code, receipt) = body_str(
            port,
            "POST",
            "/api/studio/beats",
            Some("{\"action\":\"undo\"}"),
        );
        assert_eq!(code, 200, "{receipt}");
        assert!(receipt.contains("restored 2 beat(s)"), "{receipt}");
        let (_, strip) = body_str(port, "GET", "/api/studio/beats", None);
        assert!(strip.contains("\"name\":\"downbeat\""), "{strip}");
        assert!(strip.contains("\"name\":\"upbeat\""), "{strip}");
        // The timeline rows never moved around the beat edits.
        let (_, rows) = body_str(port, "GET", "/api/studio/timeline", None);
        assert!(rows.contains("\"window\":[0.000,2.000]"), "{rows}");
        // Beats persist in the hidden history directory, never beside
        // the sources.
        assert!(
            folder
                .join(".framesmith")
                .join("studio-beats.json")
                .exists(),
            "beats persist under the hidden history dir"
        );
        assert!(
            !folder.join("studio-beats.json").exists(),
            "beats never land beside the sources"
        );
    });
}

#[test]
fn beats_refusals_name_their_cause() {
    with_server(
        "beatrefuse",
        mixed_project,
        |_folder, _reg, _entry, port| {
            // Unknown actions and empty bodies refuse.
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/beats",
                Some("{\"action\":\"tap\",\"name\":\"x\",\"at\":1}"),
            );
            assert_eq!(code, 400, "{refused}");
            assert!(refused.contains("unknown beats action"), "{refused}");
            // Missing instants refuse.
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/beats",
                Some("{\"action\":\"add\",\"name\":\"x\"}"),
            );
            assert_eq!(code, 400, "{refused}");
            assert!(refused.contains("no `at` instant"), "{refused}");
            // Duplicate names, unknown beats, and empty undo refuse while
            // the strip stays exactly as it was.
            let (code, _) = body_str(
                port,
                "POST",
                "/api/studio/beats",
                Some("{\"action\":\"add\",\"name\":\"one\",\"at\":1}"),
            );
            assert_eq!(code, 200);
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/beats",
                Some("{\"action\":\"add\",\"name\":\"one\",\"at\":2}"),
            );
            assert_eq!(code, 409, "{refused}");
            assert!(refused.contains("already marks"), "{refused}");
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/beats",
                Some("{\"action\":\"move\",\"name\":\"ghost\",\"at\":2}"),
            );
            assert_eq!(code, 409, "{refused}");
            assert!(refused.contains("names nothing"), "{refused}");
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/beats",
                Some("{\"action\":\"delete\",\"name\":\"ghost\"}"),
            );
            assert_eq!(code, 409, "{refused}");
            assert!(refused.contains("names nothing"), "{refused}");
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/beats?composition=nope",
                Some("{\"action\":\"undo\"}"),
            );
            assert_eq!(code, 404, "{refused}");
            assert!(refused.contains("unknown composition"), "{refused}");
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/beats",
                Some("{\"action\":\"move\",\"name\":\"one\",\"at\":-2}"),
            );
            assert_eq!(code, 409, "{refused}");
            let (_, strip) = body_str(port, "GET", "/api/studio/beats", None);
            assert!(strip.contains("\"name\":\"one\",\"at\":1.000"), "{strip}");
            assert!(strip.contains("\"undo_depth\":1"), "{strip}");
        },
    );
}

#[test]
fn visuals_show_filmstrips_waveforms_text_and_badges() {
    with_server("visuals", mixed_project, |_folder, _reg, _entry, port| {
        let (code, visuals) = body_str(port, "GET", "/api/studio/visuals", None);
        assert_eq!(code, 200, "{visuals}");
        // Video shows a filmstrip of true frame indexes.
        assert!(visuals.contains("\"kind\":\"video\""), "{visuals}");
        assert!(visuals.contains("\"kind\":\"filmstrip\""), "{visuals}");
        // The effect badge marks the graded video clip.
        assert!(visuals.contains("\"effects\":[\"grayscale\"]"), "{visuals}");
        // Audio shows a waveform with peak marks from the render mix.
        assert!(visuals.contains("\"kind\":\"waveform\""), "{visuals}");
        assert!(visuals.contains("\"buckets\":32"), "{visuals}");
        assert!(visuals.contains("\"peak_buckets\":["), "{visuals}");
        assert!(!visuals.contains("\"peak\":0"), "{visuals}");
        // Volume and fade badges mark the sounding clip with working
        // handle frames.
        assert!(visuals.contains("\"db\":-6"), "{visuals}");
        assert!(visuals.contains("\"in\":0.500,\"in_frame\":5"), "{visuals}");
        assert!(
            visuals.contains("\"out\":0.500,\"out_frame\":35"),
            "{visuals}"
        );
        // Text shows its content.
        assert!(
            visuals.contains("\"content\":\"Hello timeline\""),
            "{visuals}"
        );
        // Transitions bridge the abutting texts both ways.
        assert!(
            visuals.contains("\"side\":\"out\",\"kind\":\"fade\""),
            "{visuals}"
        );
        assert!(
            visuals.contains("\"side\":\"in\",\"kind\":\"fade\""),
            "{visuals}"
        );
        // Unknown compositions refuse naming the holdings.
        let (code, missing) = body_str(port, "GET", "/api/studio/visuals?composition=nope", None);
        assert_eq!(code, 404, "{missing}");
        assert!(missing.contains("unknown composition"), "{missing}");
    });
}

#[test]
fn visuals_show_named_placeholders_for_unrenderable_media() {
    with_server(
        "placeholders",
        mixed_project,
        |folder, _reg, _entry, port| {
            // Break both media files mid-session: validation watches the
            // composition file, so the session survives with REC-005 boxes.
            std::fs::remove_file(folder.join("s.wav")).unwrap();
            std::fs::remove_file(folder.join("v.mp4")).unwrap();
            let (code, visuals) = body_str(port, "GET", "/api/studio/visuals", None);
            assert_eq!(code, 200, "{visuals}");
            assert!(
                visuals.contains("\"kind\":\"placeholder\",\"name\":\"s\""),
                "{visuals}"
            );
            assert!(
                visuals.contains("\"kind\":\"placeholder\",\"name\":\"v\""),
                "{visuals}"
            );
            assert!(visuals.contains("is missing"), "{visuals}");
            // Text content and editing continue around the broken media.
            assert!(
                visuals.contains("\"content\":\"Hello timeline\""),
                "{visuals}"
            );
            let (code, lanes) = body_str(port, "GET", "/api/studio/lanes", None);
            assert_eq!(code, 200, "{lanes}");
            assert!(lanes.contains("\"property\":\"x\""), "{lanes}");
            let (code, receipt) = body_str(
                port,
                "POST",
                "/api/studio/beats",
                Some("{\"action\":\"add\",\"name\":\"kept\",\"at\":1}"),
            );
            assert_eq!(code, 200, "{receipt}");
        },
    );
}

#[test]
fn studio_page_hosts_lanes_beats_and_visuals() {
    with_server("page", mixed_project, |_folder, _reg, _entry, port| {
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200);
        assert!(page.contains("id=\"lanes\""), "lanes panel");
        assert!(page.contains("id=\"beat-strip\""), "beat strip");
        assert!(page.contains("id=\"clip-visuals\""), "clip visuals");
        assert!(page.contains("/api/studio/keyframe"), "diamond menu posts");
        assert!(page.contains("/api/studio/beats"), "strip posts");
        assert!(page.contains("/api/studio/visuals"), "visuals fetch");
        assert!(page.contains("/api/studio/lanes"), "lanes fetch");
        assert!(page.contains("lane-track"), "lane tracks");
        assert!(page.contains("placeholder-box"), "placeholder style");
        assert!(
            page.contains("background-color"),
            "hyphenated style attributes"
        );
    });
}
