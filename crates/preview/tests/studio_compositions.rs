//! Compositions panel regressions (Sprint 18, T001 / CAP-001, J-01,
//! J-07, STATE-01, REC-001): cards show a live hover preview, the root
//! badge, and the lint count; dragging or adding places the
//! composition on the timeline as one named history turn; render-one
//! queues exactly that composition; stale cards refresh instead of
//! acting; a lint-failing composition renders nothing with the count
//! named; a drag onto a full timeline refuses with the reason. Every
//! refusal stages, queues, records, and renders nothing.

use preview::studio_compositions;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-studio-compositions-{}-{}-{}",
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

fn moving_text(value: &str, y: u32, duration: u32) -> String {
    format!(
        "    text value=\"{value}\" start=0 duration={duration} x=10 y={y} size=24 color=\"#ffffff\"\n    tween target=0 property=\"x\" kind=\"to\" start=0 duration={duration} to=200\n"
    )
}

/// Two lint-clean compositions: motion covers the frozen-timeline
/// rule, single texts dodge overlap, positions dodge off-canvas and
/// the caption band, white-on-dark passes contrast.
fn moving_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        &format!(
            "composition name=\"main\" root=true width=320 height=180 duration=2 fps=10 background=\"#112233\" version=1 {{\n{} }}\ncomposition name=\"b-roll\" width=320 height=180 duration=3 fps=10 background=\"#223344\" version=1 {{\n{} }}\n",
            moving_text("Hi", 40, 2),
            moving_text("Yo", 100, 3),
        ),
    );
}

/// `main` carries two overlapping opaque boxes (overlap plus a frozen
/// span); `b-roll` stays lint-clean.
fn overlap_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        &format!(
            "composition name=\"main\" root=true width=320 height=180 duration=2 fps=10 background=\"#112233\" version=1 {{\n    box start=0 duration=2 x=10 y=10 width=100 height=100 fill=\"#ff0000\"\n    box start=0 duration=2 x=50 y=50 width=100 height=100 fill=\"#00ff00\"\n }}\ncomposition name=\"b-roll\" width=320 height=180 duration=3 fps=10 background=\"#223344\" version=1 {{\n{} }}\n",
            moving_text("Yo", 100, 3),
        ),
    );
}

fn cycle_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        &format!(
            "composition name=\"main\" root=true width=320 height=180 duration=2 fps=10 background=\"#112233\" version=1 {{\n{}    nest nest=\"mid\" start=0 duration=1\n }}\ncomposition name=\"mid\" width=320 height=180 duration=2 fps=10 background=\"#223344\" version=1 {{\n{}    nest nest=\"leaf\" start=0 duration=1\n }}\ncomposition name=\"leaf\" width=320 height=180 duration=2 fps=10 background=\"#334455\" version=1 {{\n{}}}\n",
            moving_text("Hi", 40, 2),
            moving_text("Yo", 100, 2),
            moving_text("End", 60, 2),
        ),
    );
}

/// Ten compositions: `c0` nests `c1` … `c7` nests `c8` (depth 8,
/// valid), plus a plain `extra`. Root sits on `c0`.
fn chain_project(folder: &Path) {
    let mut out = String::new();
    for i in 0..9u32 {
        let root = if i == 0 { " root=true" } else { "" };
        out.push_str(&format!(
            "composition name=\"c{i}\"{root} width=320 height=180 duration=2 fps=10 background=\"#112233\" version=1 {{\n{}",
            moving_text("Moving", 40, 2),
        ));
        if i < 8 {
            out.push_str(&format!(
                "    nest nest=\"c{}\" start=0 duration=1\n",
                i + 1
            ));
        }
        out.push_str(" }\n");
    }
    out.push_str(&format!(
        "composition name=\"extra\" width=320 height=180 duration=2 fps=10 background=\"#223344\" version=1 {{\n{}}}\n",
        moving_text("Extra", 100, 2),
    ));
    write(folder, "composition.kdl", &out);
}

/// Sorted map of every file under `folder` to its bytes: the
/// refusal oracle (sources, history, delta, and queue unchanged) and
/// the write-boundary oracle (only hidden studio state added).
fn tree_bytes(folder: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let key = path
                    .strip_prefix(folder)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                out.insert(key, std::fs::read(&path).unwrap());
            }
        }
    }
    out
}

fn has_render_output(folder: &Path) -> bool {
    tree_bytes(folder).keys().any(|k| {
        k.ends_with(".mp4") || k.ends_with(".png") || k.ends_with(".wav") || k.ends_with(".gif")
    })
}

fn place_req(
    source: &str,
    target: Option<&str>,
    at: Option<f64>,
) -> studio_compositions::PlaceRequest {
    studio_compositions::PlaceRequest {
        source: source.to_string(),
        target: target.map(str::to_string),
        at,
        actor: "maker".to_string(),
        expected_generation: None,
        max_clips: None,
    }
}

fn render_req(composition: &str) -> studio_compositions::RenderOneRequest {
    studio_compositions::RenderOneRequest {
        composition: composition.to_string(),
        actor: "maker".to_string(),
        expected_generation: None,
    }
}

const PNG_SIG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

/// Minimal HTTP exchange over loopback with an optional body.
/// Returns (status code, body bytes).
fn http(port: u16, method: &str, target: &str, body: Option<&str>) -> (u16, Vec<u8>) {
    use std::io::{Read, Write};
    let addr = format!("127.0.0.1:{port}");
    let mut stream = std::net::TcpStream::connect_timeout(
        &addr.parse().unwrap(),
        std::time::Duration::from_secs(5),
    )
    .expect("connect");
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(15)))
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
fn with_server(tag: &str, setup: impl FnOnce(&Path), f: impl FnOnce(&Path, u16)) {
    use std::time::{Duration, Instant};
    let root = tmp(tag);
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    setup(&folder);
    let reg = root.join("registry.json");
    let dir = preview::daemon::canonical_dir(&folder).unwrap();
    let opts = preview::server::ServeOptions {
        folder: folder.clone(),
        port: 0,
        registry: reg.clone(),
        composition: None,
    };
    let handle = std::thread::spawn(move || preview::server::run_serve(&opts));
    let start = Instant::now();
    let entry = loop {
        let entries = preview::daemon::load_registry(&reg).unwrap_or_default();
        if let Some(e) = entries
            .into_iter()
            .find(|e| e.dir == dir && preview::daemon::is_live(e.port))
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
    f(&folder, entry.port);
    assert!(
        preview::daemon::request_shutdown(entry.port),
        "shutdown answers"
    );
    assert_eq!(handle.join().expect("server thread"), 0);
    assert!(
        !preview::daemon::is_live(entry.port),
        "port is quiet after shutdown"
    );
}

// --- cards ---------------------------------------------------------------

#[test]
fn cards_list_every_composition_with_root_badge_and_zero_lint() {
    let folder = tmp("cards");
    moving_project(&folder);
    let load = studio_compositions::list_composition_cards(&folder);
    assert!(
        load.notice.is_none(),
        "clean project lists, got {:?}",
        load.notice
    );
    assert_eq!(load.cards.len(), 2);
    assert_eq!(load.cards[0].name, "main");
    assert!(load.cards[0].is_root, "the render root carries the badge");
    assert!(!load.cards[1].is_root, "one root badge only");
    assert_eq!(load.cards[1].name, "b-roll");
    for card in &load.cards {
        assert_eq!(card.lint_count, 0, "lint-clean card shows zero");
    }
    assert_eq!(load.cards[0].clip_count, 1);
    assert_eq!(load.cards[0].frame_count, 20);
    assert_eq!(load.cards[1].frame_count, 30);
    assert_eq!(load.unattributed_lint, 0);
}

#[test]
fn cards_name_lint_per_composition() {
    let folder = tmp("lint-count");
    overlap_project(&folder);
    let load = studio_compositions::list_composition_cards(&folder);
    assert!(
        load.notice.is_none(),
        "overlapping still validates, got {:?}",
        load.notice
    );
    let main = load
        .cards
        .iter()
        .find(|c| c.name == "main")
        .expect("main card");
    let broll = load
        .cards
        .iter()
        .find(|c| c.name == "b-roll")
        .expect("b-roll card");
    // Overlap plus the frozen still span, both inside main's span.
    assert_eq!(main.lint_count, 2, "main names its two findings");
    assert_eq!(broll.lint_count, 0, "b-roll stays clean");
}

#[test]
fn cards_for_broken_project_are_empty_with_reason() {
    let folder = tmp("broken");
    write(
        &folder,
        "composition.kdl",
        "composition name=\"main\" {\n    text value=\"Hi\"\n",
    );
    let load = studio_compositions::list_composition_cards(&folder);
    assert!(load.cards.is_empty(), "no guessed cards");
    let notice = load.notice.expect("a reason replaces blank cards");
    assert!(notice.contains("cannot list compositions"), "{notice}");
}

#[test]
fn cards_for_missing_project_are_empty_with_reason() {
    let folder = tmp("missing").join("no-such-project");
    let load = studio_compositions::list_composition_cards(&folder);
    assert!(load.cards.is_empty());
    assert!(load.notice.is_some(), "a reason replaces blank cards");
}

#[test]
fn cards_json_carries_generation_badges_and_counts() {
    let folder = tmp("cards-json");
    moving_project(&folder);
    let load = studio_compositions::list_composition_cards(&folder);
    let raw = studio_compositions::cards_json(&load);
    let value: serde_json::Value = serde_json::from_str(&raw).expect("cards render as JSON");
    assert_eq!(
        value
            .get("cards")
            .and_then(|c| c.as_array())
            .map(|a| a.len()),
        Some(2)
    );
    let main = &value["cards"][0];
    assert_eq!(main["name"], serde_json::Value::String("main".to_string()));
    assert_eq!(main["root"], serde_json::Value::Bool(true));
    assert_eq!(main["lint"], serde_json::Value::Number(0.into()));
    assert!(value.get("generation").and_then(|g| g.as_u64()).is_some());
}

// --- hover preview --------------------------------------------------------

#[test]
fn hover_preview_draws_png_and_tracks_edits() {
    let folder = tmp("preview");
    moving_project(&folder);
    let first =
        studio_compositions::composition_preview_png(&folder, "main").expect("hover preview draws");
    assert!(first.starts_with(&PNG_SIG), "the preview is PNG bytes");
    // Live means current: an edited composition redraws, never served stale.
    write(
        &folder,
        "composition.kdl",
        &std::fs::read_to_string(folder.join("composition.kdl"))
            .unwrap()
            .replacen("Hi", "Bye", 1),
    );
    let second = studio_compositions::composition_preview_png(&folder, "main")
        .expect("edited preview draws");
    assert!(second.starts_with(&PNG_SIG));
    assert_ne!(first, second, "the thumbnail tracks the edit");
    assert!(
        !has_render_output(&folder),
        "browsing previews writes no outputs into the project"
    );
}

#[test]
fn hover_preview_unknown_composition_refuses() {
    let folder = tmp("preview-unknown");
    moving_project(&folder);
    let before = tree_bytes(&folder);
    let err = studio_compositions::composition_preview_png(&folder, "nope")
        .expect_err("unknown composition refuses");
    assert!(err.contains("unknown composition"), "{err}");
    assert!(
        err.contains("\"main\"") && err.contains("\"b-roll\""),
        "{err}"
    );
    assert_eq!(
        tree_bytes(&folder),
        before,
        "a refused preview writes nothing"
    );
}

// --- placement -------------------------------------------------------------

#[test]
fn drag_places_at_the_drop_instant_as_a_named_history_turn() {
    let folder = tmp("drag");
    moving_project(&folder);
    let receipt = studio_compositions::place_composition(
        &folder,
        &place_req("b-roll", Some("main"), Some(1.0)),
    )
    .expect("the drop lands");
    assert_eq!(receipt.source, "b-roll");
    assert_eq!(receipt.target, "main");
    assert_eq!(receipt.at, 1.0);
    assert_eq!(
        receipt.summaries,
        vec!["add nest element at end".to_string()]
    );
    // The nest resolves: the effective timeline plays b-roll (3s) from 1.0.
    let session = sdk::SdkSession::open(
        &folder,
        sdk::OpenOptions {
            composition: Some("main".to_string()),
            ..sdk::OpenOptions::default()
        },
    )
    .expect("timeline reopens");
    let tail = session
        .effective_composition()
        .clips
        .last()
        .cloned()
        .expect("a clip landed");
    let notation::model::Clip::Nest(nest) = tail else {
        panic!("the landed clip is a nest");
    };
    assert_eq!(nest.target, "b-roll");
    assert_eq!((nest.start, nest.duration), (1.0, 3.0));
    // One undo system: the turn lists under the actor and reverses whole.
    let listing = sdk::history::list_turns(&folder).expect("history lists");
    let turn = listing
        .turns
        .iter()
        .find(|t| t.seq == receipt.turn)
        .expect("the turn lists");
    assert_eq!(turn.actor, "maker");
    assert!(
        turn.summary.contains("nest"),
        "the turn names the nest: {}",
        turn.summary
    );
    // The placed timeline still draws with the nest resolved.
    let png = studio_compositions::composition_preview_png(&folder, "main")
        .expect("the nested timeline draws");
    assert!(png.starts_with(&PNG_SIG));
    let raw = studio_compositions::place_receipt_json(&receipt);
    let value: serde_json::Value = serde_json::from_str(&raw).expect("receipt renders as JSON");
    assert_eq!(value["placed"], serde_json::Value::Bool(true));
    assert_eq!(
        value["source"],
        serde_json::Value::String("b-roll".to_string())
    );
    assert_eq!(
        value["target"],
        serde_json::Value::String("main".to_string())
    );
}

// --- panel wiring ----------------------------------------------------------

#[test]
fn panel_slots_carry_hyphenated_styles_and_the_four_endpoints() {
    let css = studio_compositions::compositions_css();
    assert!(css.contains("background-color"), "CSS stays hyphenated");
    assert!(css.contains("border-radius"), "CSS stays hyphenated");
    assert!(css.contains("object-fit"), "CSS stays hyphenated");
    assert!(
        !css.contains("fontFamily") && !css.contains("font-family"),
        "no font resolution here"
    );
    let html = studio_compositions::compositions_html();
    assert!(
        html.contains("panel-compositions"),
        "the panel section renders"
    );
    assert!(html.contains("composition-list"), "cards have a home");
    assert!(html.contains("composition-status"), "receipts have a home");
    let js = studio_compositions::compositions_js();
    for endpoint in [
        "/api/studio/compositions",
        "/api/studio/compositions/preview",
        "/api/studio/compositions/place",
        "/api/studio/compositions/render-one",
    ] {
        assert!(js.contains(endpoint), "the panel drives {endpoint}");
    }
    assert!(
        js.contains("text/studio-composition"),
        "cards drag by composition mime"
    );
    assert!(
        js.contains("paintCompositions"),
        "cards repaint on load and on stale"
    );
}

#[test]
fn add_appends_at_the_timeline_end() {
    let folder = tmp("add");
    moving_project(&folder);
    let receipt = studio_compositions::place_composition(&folder, &place_req("b-roll", None, None))
        .expect("add lands on the root");
    assert_eq!(receipt.target, "main", "add targets the render root");
    assert_eq!(receipt.at, 2.0, "add appends at the timeline end");
}

#[test]
fn undo_reverses_a_placement_through_shared_history() {
    let folder = tmp("undo");
    moving_project(&folder);
    let receipt = studio_compositions::place_composition(
        &folder,
        &place_req("b-roll", Some("main"), Some(0.5)),
    )
    .expect("the drop lands");
    sdk::history::undo_turn(&folder, receipt.turn).expect("the turn reverses");
    let session = sdk::SdkSession::open(
        &folder,
        sdk::OpenOptions {
            composition: Some("main".to_string()),
            ..sdk::OpenOptions::default()
        },
    )
    .expect("timeline reopens");
    assert!(
        session
            .effective_composition()
            .clips
            .iter()
            .all(|c| !matches!(c, notation::model::Clip::Nest(_))),
        "no nest remains after undo"
    );
}

#[test]
fn place_unknown_source_refuses_naming_the_holdings() {
    let folder = tmp("unknown-source");
    moving_project(&folder);
    let before = tree_bytes(&folder);
    let err = studio_compositions::place_composition(
        &folder,
        &place_req("nope", Some("main"), Some(0.0)),
    )
    .expect_err("unknown source refuses");
    let message = err.to_string();
    assert!(message.contains("unknown composition"), "{message}");
    assert!(message.contains("\"main\", \"b-roll\""), "{message}");
    assert_eq!(
        tree_bytes(&folder),
        before,
        "nothing staged, queued, or recorded"
    );
}

#[test]
fn place_into_itself_refuses() {
    let folder = tmp("self-nest");
    moving_project(&folder);
    let before = tree_bytes(&folder);
    let err = studio_compositions::place_composition(
        &folder,
        &place_req("main", Some("main"), Some(0.0)),
    )
    .expect_err("self-nest refuses");
    let message = err.to_string();
    assert!(message.contains("cannot nest into itself"), "{message}");
    assert_eq!(
        tree_bytes(&folder),
        before,
        "nothing staged, queued, or recorded"
    );
}

#[test]
fn place_closing_a_cycle_refuses_naming_the_chain() {
    let folder = tmp("cycle");
    cycle_project(&folder);
    let before = tree_bytes(&folder);
    // main already nests mid: dropping main onto mid would close the loop.
    let err =
        studio_compositions::place_composition(&folder, &place_req("main", Some("mid"), Some(0.0)))
            .expect_err("the cycle refuses");
    let message = err.to_string();
    assert!(message.contains("cycle"), "{message}");
    assert!(message.contains("\"main\""), "{message}");
    assert!(message.contains("\"mid\""), "{message}");
    assert_eq!(
        tree_bytes(&folder),
        before,
        "nothing staged, queued, or recorded"
    );
}

#[test]
fn place_past_the_depth_limit_refuses() {
    let folder = tmp("depth");
    chain_project(&folder);
    let before = tree_bytes(&folder);
    // c0 nests … nests c8 at depth 8: hanging extra under c8 nests 9 deep.
    let err =
        studio_compositions::place_composition(&folder, &place_req("extra", Some("c8"), Some(0.0)))
            .expect_err("over-deep nesting refuses");
    let message = err.to_string();
    assert!(message.contains("8-level limit"), "{message}");
    assert_eq!(
        tree_bytes(&folder),
        before,
        "nothing staged, queued, or recorded"
    );
}

#[test]
fn stale_drop_refreshes_with_fresh_cards_instead_of_acting() {
    let folder = tmp("stale");
    moving_project(&folder);
    let load = studio_compositions::list_composition_cards(&folder);
    // An external edit moves the generation under the maker.
    write(
        &folder,
        "composition.kdl",
        &std::fs::read_to_string(folder.join("composition.kdl"))
            .unwrap()
            .replacen("Yo", "Yo-ho", 1),
    );
    let before = tree_bytes(&folder);
    let mut req = place_req("b-roll", Some("main"), Some(0.0));
    req.expected_generation = Some(load.generation);
    let err =
        studio_compositions::place_composition(&folder, &req).expect_err("the stale drop refuses");
    let (message, fresh) = match err {
        studio_compositions::PlaceError::Stale { message, fresh } => (message, fresh),
        other => panic!("a stale drop refreshes, got {other}"),
    };
    assert!(message.contains("changed under this drop"), "{message}");
    assert_ne!(
        fresh.generation, load.generation,
        "fresh cards carry the new truth"
    );
    assert_eq!(fresh.cards.len(), 2, "fresh cards still list everything");
    assert_eq!(tree_bytes(&folder), before, "the stale drop stages nothing");
    // Retrying on the fresh generation lands.
    req.expected_generation = Some(fresh.generation);
    let receipt = studio_compositions::place_composition(&folder, &req).expect("retry lands");
    assert_eq!(receipt.source, "b-roll");
}

#[test]
fn drop_onto_a_full_timeline_refuses_with_the_reason() {
    let folder = tmp("full");
    moving_project(&folder);
    let before = tree_bytes(&folder);
    // main seats one clip: a cap of one proves the refusal exactly.
    let mut req = place_req("b-roll", Some("main"), Some(0.0));
    req.max_clips = Some(1);
    let err = studio_compositions::place_composition(&folder, &req)
        .expect_err("the full timeline refuses");
    let message = err.to_string();
    assert!(message.contains("is full"), "{message}");
    assert!(message.contains("1 clip(s)"), "{message}");
    assert!(message.contains("1-clip seating cap"), "{message}");
    assert_eq!(
        tree_bytes(&folder),
        before,
        "a refused drop writes nothing anywhere"
    );
}

// --- render-one ------------------------------------------------------------

#[test]
fn render_one_queues_exactly_that_composition() {
    let folder = tmp("queue");
    moving_project(&folder);
    let first = studio_compositions::queue_render_one(&folder, &render_req("b-roll"))
        .expect("render-one queues");
    assert_eq!(first.seq, 1);
    assert_eq!(first.composition, "b-roll");
    assert!(
        first.output.contains(".framesmith"),
        "outputs plan outside the project sources, got {}",
        first.output
    );
    assert!(
        first.output.ends_with("b-roll-1.mp4"),
        "got {}",
        first.output
    );
    let second = studio_compositions::queue_render_one(&folder, &render_req("main"))
        .expect("a second render-one queues behind");
    assert_eq!(second.seq, 2);
    assert_eq!(second.composition, "main");
    let jobs = studio_compositions::list_render_queue(&folder).expect("the queue lists");
    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs[0].composition, "b-roll");
    assert_eq!(jobs[1].composition, "main");
    assert!(jobs.iter().all(|j| j.status == "queued"));
    assert!(
        !has_render_output(&folder),
        "queueing writes plans, never pixels"
    );
    let raw = studio_compositions::render_one_receipt_json(&first);
    let value: serde_json::Value = serde_json::from_str(&raw).expect("receipt renders as JSON");
    assert_eq!(value["seq"], serde_json::Value::Number(1.into()));
    assert_eq!(
        value["composition"],
        serde_json::Value::String("b-roll".to_string())
    );
    let queue_raw = studio_compositions::render_queue_json(&jobs);
    let queue: serde_json::Value = serde_json::from_str(&queue_raw).expect("queue renders as JSON");
    assert_eq!(queue["jobs"].as_array().map(|a| a.len()), Some(2));
}

#[test]
fn render_one_on_lint_failure_queues_and_renders_nothing_naming_the_count() {
    let folder = tmp("lint-refusal");
    overlap_project(&folder);
    let load = studio_compositions::list_composition_cards(&folder);
    let main = load
        .cards
        .iter()
        .find(|c| c.name == "main")
        .expect("main card");
    assert!(main.lint_count >= 1, "the fixture fails lint");
    let before = tree_bytes(&folder);
    let err = studio_compositions::queue_render_one(&folder, &render_req("main"))
        .expect_err("lint failure refuses render-one");
    let message = err.to_string();
    assert!(
        message.contains(&main.lint_count.to_string()),
        "the refusal names the failing count: {message}"
    );
    assert!(message.contains("\"main\""), "{message}");
    assert_eq!(
        tree_bytes(&folder),
        before,
        "nothing queued and nothing rendered"
    );
    // The clean card still queues exactly itself.
    let receipt = studio_compositions::queue_render_one(&folder, &render_req("b-roll"))
        .expect("the clean composition queues");
    assert_eq!(receipt.composition, "b-roll");
}

#[test]
fn stale_render_one_refreshes_with_fresh_cards() {
    let folder = tmp("stale-queue");
    moving_project(&folder);
    let load = studio_compositions::list_composition_cards(&folder);
    write(
        &folder,
        "composition.kdl",
        &std::fs::read_to_string(folder.join("composition.kdl"))
            .unwrap()
            .replacen("Yo", "Yo-ho", 1),
    );
    let before = tree_bytes(&folder);
    let mut req = render_req("b-roll");
    req.expected_generation = Some(load.generation);
    let err = studio_compositions::queue_render_one(&folder, &req)
        .expect_err("the stale render-one refuses");
    match err {
        studio_compositions::RenderOneError::Stale { fresh, .. } => {
            assert_ne!(fresh.generation, load.generation);
            assert_eq!(fresh.cards.len(), 2);
        }
        other => panic!("a stale render-one refreshes, got {other}"),
    }
    assert_eq!(
        tree_bytes(&folder),
        before,
        "the stale queue stages nothing"
    );
}

#[test]
fn render_one_unknown_composition_refuses() {
    let folder = tmp("queue-unknown");
    moving_project(&folder);
    let before = tree_bytes(&folder);
    let err = studio_compositions::queue_render_one(&folder, &render_req("nope"))
        .expect_err("unknown composition refuses");
    let message = err.to_string();
    assert!(message.contains("unknown composition"), "{message}");
    assert_eq!(tree_bytes(&folder), before, "nothing queued or rendered");
}

// --- bodies and boundaries --------------------------------------------------

#[test]
fn place_and_render_one_bodies_refuse_loudly() {
    assert!(
        studio_compositions::parse_place_body("").is_err(),
        "empty drop body refuses"
    );
    assert!(
        studio_compositions::parse_place_body("{nope").is_err(),
        "non-JSON drop refuses"
    );
    let missing_actor = studio_compositions::parse_place_body("{\"source\":\"b-roll\"}");
    assert!(missing_actor
        .expect_err("actorless drop refuses")
        .contains("no actor"));
    let missing_source = studio_compositions::parse_place_body("{\"actor\":\"maker\"}");
    assert!(missing_source
        .expect_err("sourceless drop refuses")
        .contains("no `source`"));
    let bad_at = studio_compositions::parse_place_body(
        "{\"actor\":\"maker\",\"source\":\"b-roll\",\"at\":\"soon\"}",
    );
    assert!(bad_at
        .expect_err("textual instant refuses")
        .contains("not seconds"));
    let req = studio_compositions::parse_place_body(
        "{\"actor\":\"maker\",\"source\":\"b-roll\",\"target\":\"main\",\"at\":1.5,\"generation\":7}",
    )
    .expect("a full drop body parses");
    assert_eq!(req.source, "b-roll");
    assert_eq!(req.target.as_deref(), Some("main"));
    assert_eq!(req.at, Some(1.5));
    assert_eq!(req.expected_generation, Some(7));

    assert!(
        studio_compositions::parse_render_one_body("").is_err(),
        "empty body refuses"
    );
    let missing_comp = studio_compositions::parse_render_one_body("{\"actor\":\"maker\"}");
    assert!(missing_comp
        .expect_err("compositionless body refuses")
        .contains("no `composition`"));
    let req = studio_compositions::parse_render_one_body(
        "{\"actor\":\"maker\",\"composition\":\"main\",\"generation\":9}",
    )
    .expect("a full render-one body parses");
    assert_eq!(req.composition, "main");
    assert_eq!(req.expected_generation, Some(9));
}

#[test]
fn panel_writes_stay_hidden_and_sources_stay_byte_identical() {
    let folder = tmp("writes");
    moving_project(&folder);
    let sources_before = std::fs::read(folder.join("composition.kdl")).unwrap();
    studio_compositions::place_composition(&folder, &place_req("b-roll", Some("main"), Some(1.0)))
        .expect("the drop lands");
    studio_compositions::queue_render_one(&folder, &render_req("b-roll"))
        .expect("render-one queues");
    // Default renders read the on-disk sources with no delta and no
    // hidden state: byte-identical sources mean byte-identical
    // default renders.
    assert_eq!(
        std::fs::read(folder.join("composition.kdl")).unwrap(),
        sources_before,
        "composition sources stay byte-identical"
    );
    for (name, _) in tree_bytes(&folder) {
        // Separator-agnostic: the hidden prefix holds on both sides
        // of the slash/backslash divide.
        let name = name.replace('\\', "/");
        assert!(
            name.starts_with(".framesmith/") || name == "composition.kdl",
            "only validated turns plus hidden studio state files land in the project, found {name}"
        );
    }
    assert!(
        !has_render_output(&folder),
        "no render output lands anywhere in the project"
    );
}

// --- served routes ----------------------------------------------------------

#[test]
fn studio_page_serves_the_compositions_panel() {
    with_server("page", moving_project, |_folder, port| {
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200, "{page}");
        assert!(
            page.contains("panel-compositions"),
            "the panel section renders"
        );
        assert!(
            page.contains("composition-list"),
            "cards have a home: {page}"
        );
        assert!(page.contains("paintCompositions"), "the panel script boots");
    });
}

#[test]
fn composition_routes_serve_cards_preview_place_and_render_one() {
    with_server("routes", moving_project, |_folder, port| {
        let (code, raw) = body_str(port, "GET", "/api/studio/compositions", None);
        assert_eq!(code, 200, "{raw}");
        let cards: serde_json::Value = serde_json::from_str(&raw).expect("cards JSON");
        assert_eq!(cards["cards"].as_array().map(|a| a.len()), Some(2));
        let generation = cards["generation"].as_u64().expect("a generation to echo");

        let (code, png) = http(
            port,
            "GET",
            "/api/studio/compositions/preview?composition=main",
            None,
        );
        assert_eq!(code, 200, "hover preview serves");
        assert!(png.starts_with(&PNG_SIG), "the preview is PNG bytes");

        let (code, missing) = body_str(
            port,
            "GET",
            "/api/studio/compositions/preview?composition=nope",
            None,
        );
        assert_eq!(code, 404, "{missing}");

        let (code, bad) = body_str(
            port,
            "POST",
            "/api/studio/compositions/place",
            Some("{\"source\":\"b-roll\"}"),
        );
        assert_eq!(code, 400, "{bad}");

        let place = format!("{{\"actor\":\"maker\",\"source\":\"b-roll\",\"target\":\"main\",\"at\":1.0,\"generation\":{generation}}}");
        let (code, receipt) =
            body_str(port, "POST", "/api/studio/compositions/place", Some(&place));
        assert_eq!(code, 200, "{receipt}");
        assert!(receipt.contains("\"placed\":true"), "{receipt}");

        let render = format!(
            "{{\"actor\":\"maker\",\"composition\":\"b-roll\",\"generation\":{generation}}}"
        );
        let (code, queued) = body_str(
            port,
            "POST",
            "/api/studio/compositions/render-one",
            Some(&render),
        );
        // The placement above moved the generation: the panel refreshes instead.
        assert_eq!(code, 409, "{queued}");
        assert!(queued.contains("\"stale\":true"), "{queued}");
        let fresh: serde_json::Value = serde_json::from_str(&queued).expect("stale JSON");
        let next = fresh["cards"]["generation"]
            .as_u64()
            .expect("fresh generation");
        let render =
            format!("{{\"actor\":\"maker\",\"composition\":\"b-roll\",\"generation\":{next}}}");
        let (code, queued) = body_str(
            port,
            "POST",
            "/api/studio/compositions/render-one",
            Some(&render),
        );
        assert_eq!(code, 200, "{queued}");
        assert!(queued.contains("\"composition\":\"b-roll\""), "{queued}");

        let (code, queue) = body_str(port, "GET", "/api/studio/compositions/queue", None);
        assert_eq!(code, 200, "{queue}");
        assert!(queue.contains("\"composition\":\"b-roll\""), "{queue}");
    });
}
