//! Clipboard and drop regressions (Sprint 18, T008 / CAP-008, J-06,
//! STATE-02, REC-001, REC-004): copy, cut, paste, and duplicate move
//! clips across compositions and projects; paste lands at the
//! playhead keeping offsets and tracks with ids renamed and styles
//! and motion carried; cut removes only after its copy confirms;
//! drops of files, assets, and compositions place media at the drop
//! point; incompatible payloads and unsupported types refuse by name
//! staging nothing. Only validated turns write the project (plus
//! hidden studio state files); queue outputs land outside sources
//! like CLI renders; default renders stay byte-identical because
//! sources never rewrite.

use preview::daemon;
use preview::server::{run_serve, ServeOptions};
use preview::studio_clipboard;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-studio-clipboard-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(folder: &Path, name: &str, content: &str) {
    if let Some(parent) = Path::new(name).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(folder.join(parent)).unwrap();
        }
    }
    std::fs::write(folder.join(name), content).unwrap();
}

fn write_bytes(folder: &Path, name: &str, content: &[u8]) {
    if let Some(parent) = Path::new(name).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(folder.join(parent)).unwrap();
        }
    }
    std::fs::write(folder.join(name), content).unwrap();
}

/// Repository media roots the fixtures copy (read-only): a real PNG
/// and a real WAV, so validation and sessions treat referenced media
/// exactly like production projects.
fn repo_media(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../cli/examples")
        .join(name)
}

/// Two timelines: `a` (the root) holds a styled text (`hero`, with a
/// tween and a key) plus two boxes (`card` with a set, `plain`);
/// `b` starts empty. Windows: hero 1..5, card 2..5, plain 0..1.
fn two_comp_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"a\" root=true width=320 height=180 duration=8 fps=10 background=\"#112233\" version=1 {\n    text id=\"hero\" value=\"Hi\" start=1 duration=4 x=10 y=40 size=24 color=\"#ffffff\" letter-spacing=2 font-family=\"Alpha, Beta\"\n    box id=\"card\" x=10 y=20 width=100 height=50 fill=\"#ffffff\" start=2 duration=3\n    box id=\"plain\" x=0 y=0 width=10 height=10 fill=\"#000000\" start=0 duration=1\n    tween target=0 property=\"opacity\" kind=\"to\" start=1 duration=2 to=0.5 ease=\"linear\"\n    key target=0 property=\"opacity\" {\n        frame at=1 value=0\n        frame at=3 value=1\n    }\n    set target=1 property=\"x\" start=2 duration=1 value=15\n}\ncomposition name=\"b\" width=320 height=180 duration=8 fps=10 background=\"#223344\" version=1 {\n}\n",
    );
}

/// Server timelines: like [`two_comp_project`] but with render-safe
/// fonts only — the serving preview session fail-closes unknown
/// `font-family` lists at boot, exactly like production.
fn server_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"a\" root=true width=320 height=180 duration=8 fps=10 background=\"#112233\" version=1 {\n    text id=\"hero\" value=\"Hi\" start=1 duration=4 x=10 y=40 size=24 color=\"#ffffff\" letter-spacing=2\n    box id=\"card\" x=10 y=20 width=100 height=50 fill=\"#ffffff\" start=2 duration=3\n    box id=\"plain\" x=0 y=0 width=10 height=10 fill=\"#000000\" start=0 duration=1\n}\ncomposition name=\"b\" width=320 height=180 duration=8 fps=10 background=\"#223344\" version=1 {\n}\n",
    );
}

/// One empty root timeline named `b`: the cross-project paste target.
fn empty_b_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"b\" root=true width=320 height=180 duration=8 fps=10 background=\"#223344\" version=1 {\n}\n",
    );
}

/// A root timeline with one image clip (`pic`) plus real media bytes
/// and an unused text; the `.txt` leftover proves unsupported drops
/// refuse by name.
fn media_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"a\" root=true width=320 height=180 duration=8 fps=10 background=\"#112233\" version=1 {\n    image id=\"pic\" path=\"media/card.png\" start=0 duration=4\n    text id=\"title\" value=\"Hi\" start=0 duration=4 x=10 y=150 size=24 color=\"#ffffff\"\n}\n",
    );
    write_bytes(
        folder,
        "media/card.png",
        &std::fs::read(repo_media("reference-1-title-card/media/card.png")).unwrap(),
    );
    write_bytes(
        folder,
        "media/bed.wav",
        &std::fs::read(repo_media("sound-reference/media/vox.wav")).unwrap(),
    );
    write(folder, "notes.txt", "not droppable\n");
}

/// A root timeline with media but no text: the target missing the
/// image file still validates because nothing references it.
fn media_free_target(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"b\" root=true width=320 height=180 duration=8 fps=10 background=\"#223344\" version=1 {\n}\n",
    );
}

fn valid_names(folder: &Path) -> Vec<String> {
    let verdict = validate::validate::validate_folder(folder);
    assert!(
        verdict.diagnostics.is_empty(),
        "fixture validates clean: {:?}",
        verdict
            .diagnostics
            .iter()
            .map(|d| format!("{} {}", d.code, d.message))
            .collect::<Vec<_>>()
    );
    verdict
        .compositions
        .iter()
        .map(|c| c.name.clone())
        .collect()
}

fn effective(folder: &Path, composition: &str) -> notation::model::Composition {
    let session = sdk::SdkSession::open(
        folder,
        sdk::OpenOptions {
            persist: false,
            history: false,
            composition: Some(composition.to_string()),
            ..sdk::OpenOptions::default()
        },
    )
    .expect("probe opens");
    session.effective_composition().clone()
}

fn targets(ids: &[&str]) -> Vec<studio_clipboard::ClipTarget> {
    ids.iter()
        .map(|id| studio_clipboard::ClipTarget {
            id: Some(id.to_string()),
            index: None,
        })
        .collect()
}

fn kdl_bytes(folder: &Path) -> Vec<u8> {
    std::fs::read(folder.join("composition.kdl")).expect("sources stay readable")
}

/// Every file under the project folder with its bytes, so a refused
/// action can prove it staged nothing anywhere.
fn snapshot(folder: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                stack.push(path);
            } else {
                let bytes = std::fs::read(&path).unwrap();
                out.push((path.strip_prefix(folder).unwrap().to_path_buf(), bytes));
            }
        }
    }
    out.sort();
    out
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

fn json(port: u16, method: &str, target: &str, body: Option<&str>) -> (u16, serde_json::Value) {
    let (code, text) = body_str(port, method, target, body);
    let value: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("not JSON ({e}): {text}"));
    (code, value)
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
fn copy_paste_across_compositions_keeps_offsets_tracks_styles_and_motion() {
    let folder = tmp("paste-offsets");
    two_comp_project(&folder);
    assert_eq!(valid_names(&folder), vec!["a".to_string(), "b".to_string()]);

    // Copy `hero` (1..5) and `card` (2..5): the payload carries both
    // clips with the tween, key (both on hero), and set (on card).
    let payload = studio_clipboard::copy_clips(&folder, Some("a"), &targets(&["hero", "card"]))
        .expect("copy resolves");
    assert_eq!(payload.clips.len(), 2);
    assert_eq!(payload.tweens.len(), 1);
    assert_eq!(payload.keys.len(), 1);
    assert_eq!(payload.sets.len(), 1);
    assert_eq!(payload.base_start, 1.0);

    // Paste at playhead 3: hero opens at 3, card keeps its 1s offset.
    let receipt = studio_clipboard::paste_clips(&folder, Some("b"), &payload, 3.0, "maker", None)
        .expect("paste lands");
    assert_eq!(receipt.pasted, 2);
    assert_eq!(receipt.at, 3.0);
    assert_eq!(
        receipt.renamed,
        vec!["hero-paste".to_string(), "card-paste".to_string()]
    );
    // History sequences start at 0: the first recorded turn is turn 0.
    assert_eq!(receipt.turn, 0);

    let landed = effective(&folder, "b");
    assert_eq!(landed.clips.len(), 2);
    let hero = landed
        .clips
        .iter()
        .find(|c| c.id() == Some("hero-paste"))
        .expect("renamed hero lands");
    let card = landed
        .clips
        .iter()
        .find(|c| c.id() == Some("card-paste"))
        .expect("renamed card lands");
    // Playhead anchor plus kept offsets.
    assert_eq!(hero.start(), 3.0);
    assert_eq!(card.start(), 4.0);
    assert_eq!(hero.duration(), 4.0);
    assert_eq!(card.duration(), 3.0);
    // Tracks stay as copied.
    let source = effective(&folder, "a");
    let source_hero = source
        .clips
        .iter()
        .find(|c| c.id() == Some("hero"))
        .unwrap();
    let source_card = source
        .clips
        .iter()
        .find(|c| c.id() == Some("card"))
        .unwrap();
    assert_eq!(hero.track(), source_hero.track());
    assert_eq!(card.track(), source_card.track());
    // Styles ride along untouched, including the fail-closed
    // font-family try-order list.
    let notation::model::Clip::Text(pasted) = hero else {
        panic!("hero pastes as a text clip");
    };
    let notation::model::Clip::Text(orig) = source_hero else {
        panic!("hero copies from a text clip");
    };
    assert_eq!(pasted.letter_spacing, orig.letter_spacing);
    assert_eq!(pasted.size, orig.size);
    assert_eq!(pasted.color, orig.color);
    assert_eq!(pasted.font_family, orig.font_family);
    assert_eq!(
        pasted.font_family,
        Some(vec!["Alpha".to_string(), "Beta".to_string()])
    );
    // Motion remaps onto the landed clips with shifted windows.
    assert_eq!(landed.tweens.len(), 1);
    let hero_idx = landed
        .clips
        .iter()
        .position(|c| c.id() == Some("hero-paste"))
        .unwrap();
    let card_idx = landed
        .clips
        .iter()
        .position(|c| c.id() == Some("card-paste"))
        .unwrap();
    assert_eq!(landed.tweens[0].target, hero_idx);
    assert_eq!(landed.tweens[0].start, 3.0);
    assert_eq!(landed.keys.len(), 1);
    assert_eq!(landed.keys[0].target, hero_idx);
    assert_eq!(
        landed.keys[0]
            .stops
            .iter()
            .map(|s| s.at)
            .collect::<Vec<_>>(),
        vec![3.0, 5.0]
    );
    assert_eq!(landed.sets.len(), 1);
    assert_eq!(landed.sets[0].target, card_idx);
    assert_eq!(landed.sets[0].start, 4.0);
    // One named history turn records the paste.
    let turns = sdk::history::list_turns(&folder).expect("history lists");
    assert_eq!(turns.turns.len(), 1);
}

#[test]
fn copy_paste_across_projects_renames_ids_through_the_wire_format() {
    let dir_a = tmp("paste-project-a");
    let dir_b = tmp("paste-project-b");
    two_comp_project(&dir_a);
    empty_b_project(&dir_b);
    let sources_a = kdl_bytes(&dir_a);

    let payload = studio_clipboard::copy_clips(&dir_a, Some("a"), &targets(&["plain"]))
        .expect("copy resolves");
    // The payload survives the system clipboard as JSON text.
    let wire = studio_clipboard::clipboard_json(&payload);
    let back = studio_clipboard::parse_clipboard_body(&wire).expect("wire parses");
    assert_eq!(back.clips.len(), 1);

    let receipt = studio_clipboard::paste_clips(&dir_b, Some("b"), &back, 2.0, "maker", None)
        .expect("cross-project paste lands");
    assert_eq!(receipt.renamed, vec!["plain-paste".to_string()]);
    let landed = effective(&dir_b, "b");
    assert_eq!(landed.clips.len(), 1);
    assert_eq!(landed.clips[0].start(), 2.0);
    // The source project records nothing and its sources never
    // rewrite, so its default renders stay byte-identical; the
    // target sources stay byte-identical too (turns live in hidden
    // studio state files, never beside the sources).
    assert_eq!(
        sdk::history::list_turns(&dir_a)
            .expect("history lists")
            .turns
            .len(),
        0
    );
    assert_eq!(kdl_bytes(&dir_a), sources_a);
    assert_eq!(effective(&dir_b, "b").clips[0].id(), Some("plain-paste"));
}

#[test]
fn paste_within_one_composition_still_renames_ids() {
    let folder = tmp("paste-same-comp");
    two_comp_project(&folder);
    let payload = studio_clipboard::copy_clips(&folder, Some("a"), &targets(&["hero"]))
        .expect("copy resolves");
    let receipt = studio_clipboard::paste_clips(&folder, Some("a"), &payload, 5.0, "maker", None)
        .expect("same-composition paste lands");
    assert_eq!(receipt.renamed, vec!["hero-paste".to_string()]);
    let landed = effective(&folder, "a");
    let ids: Vec<_> = landed.clips.iter().filter_map(|c| c.id()).collect();
    assert_eq!(ids.iter().filter(|id| **id == "hero").count(), 1);
    assert_eq!(ids.iter().filter(|id| **id == "hero-paste").count(), 1);
    let pasted = landed
        .clips
        .iter()
        .find(|c| c.id() == Some("hero-paste"))
        .unwrap();
    assert_eq!(pasted.start(), 5.0);
}

#[test]
fn cut_confirmed_copy_removes_and_carries_the_payload() {
    let folder = tmp("cut-ok");
    two_comp_project(&folder);
    let receipt = studio_clipboard::cut_clips(&folder, Some("a"), &targets(&["card"]), "maker")
        .expect("cut applies");
    assert_eq!(receipt.removed, 1);
    assert_eq!(receipt.payload.clips.len(), 1);
    assert_eq!(receipt.payload.clips[0].id(), Some("card"));
    let landed = effective(&folder, "a");
    assert!(landed.clips.iter().all(|c| c.id() != Some("card")));
    assert_eq!(landed.clips.len(), 2);
    // The carried payload pastes elsewhere next.
    let repaste =
        studio_clipboard::paste_clips(&folder, Some("b"), &receipt.payload, 0.0, "maker", None)
            .expect("cut payload repastes");
    assert_eq!(repaste.renamed, vec!["card-paste".to_string()]);
}

#[test]
fn cut_failed_copy_removes_nothing() {
    let folder = tmp("cut-refused");
    two_comp_project(&folder);
    let before = snapshot(&folder);
    let err = studio_clipboard::cut_clips(&folder, Some("a"), &targets(&["ghost"]), "maker")
        .expect_err("unknown clip refuses");
    assert!(err.contains("\"ghost\""), "refusal names the clip: {err}");
    assert_eq!(effective(&folder, "a").clips.len(), 3);
    assert_eq!(snapshot(&folder), before, "failed cut stages nothing");
    assert_eq!(
        sdk::history::list_turns(&folder)
            .expect("history lists")
            .turns
            .len(),
        0
    );
}

#[test]
fn duplicate_in_place_keeps_window_and_renames_id() {
    let folder = tmp("duplicate");
    two_comp_project(&folder);
    let receipt =
        studio_clipboard::duplicate_clips(&folder, Some("a"), &targets(&["hero"]), "maker")
            .expect("duplicate applies");
    assert_eq!(receipt.duplicated, 1);
    assert_eq!(receipt.renamed, vec!["hero-paste".to_string()]);
    let landed = effective(&folder, "a");
    assert_eq!(landed.clips.len(), 4);
    let orig = landed
        .clips
        .iter()
        .find(|c| c.id() == Some("hero"))
        .unwrap();
    let dup = landed
        .clips
        .iter()
        .find(|c| c.id() == Some("hero-paste"))
        .unwrap();
    assert_eq!(dup.start(), orig.start());
    assert_eq!(dup.duration(), orig.duration());
    assert_eq!(dup.track(), orig.track());
    // One duplicate is one undo unit.
    assert_eq!(
        sdk::history::list_turns(&folder)
            .expect("history lists")
            .turns
            .len(),
        1
    );
}

#[test]
fn drop_asset_places_media_at_the_drop_point() {
    let folder = tmp("drop-asset");
    media_project(&folder);
    let sources = kdl_bytes(&folder);
    let req = studio_clipboard::parse_drop_body(
        "{\"kind\":\"asset\",\"asset\":\"media/card.png\",\"drop-at\":1.5,\"actor\":\"maker\"}",
    )
    .expect("drop parses");
    let receipt = studio_clipboard::drop_onto_timeline(&folder, &req).expect("drop lands");
    assert_eq!(receipt.kind, "asset");
    assert_eq!(receipt.landed, "media/card.png");
    assert_eq!(receipt.at, 1.5);
    let landed = effective(&folder, "a");
    assert_eq!(landed.clips.len(), 3);
    let placed = &landed.clips[2];
    assert_eq!(placed.start(), 1.5);
    let notation::model::Clip::Image(image) = placed else {
        panic!("asset drops as an image clip");
    };
    assert_eq!(image.path, "media/card.png");
    // Sources never rewrite, so default renders stay byte-identical.
    assert_eq!(kdl_bytes(&folder), sources);
}

#[test]
fn drop_file_outside_the_project_refuses_naming_the_import() {
    let folder = tmp("drop-outside");
    media_project(&folder);
    let before = snapshot(&folder);
    let req = studio_clipboard::parse_drop_body(
        "{\"kind\":\"file\",\"path\":\"/tmp/card.png\",\"drop-at\":1.0,\"actor\":\"maker\"}",
    )
    .expect("drop parses");
    let err = studio_clipboard::drop_onto_timeline(&folder, &req).expect_err("outside refuses");
    assert!(
        err.contains("Assets panel"),
        "refusal names the import: {err}"
    );
    assert!(
        err.contains("/tmp/card.png"),
        "refusal names the file: {err}"
    );
    assert_eq!(snapshot(&folder), before, "outside drop stages nothing");
}

#[test]
fn drop_unsupported_type_refuses_naming_accepted_types() {
    let folder = tmp("drop-unsupported");
    media_project(&folder);
    let before = snapshot(&folder);
    // A project file of no accepted asset type refuses by name.
    let req = studio_clipboard::parse_drop_body(
        "{\"kind\":\"file\",\"path\":\"notes.txt\",\"drop-at\":1.0,\"actor\":\"maker\"}",
    )
    .expect("drop parses");
    let err = studio_clipboard::drop_onto_timeline(&folder, &req).expect_err("type refuses");
    assert!(err.contains("image"), "refusal names accepted types: {err}");
    assert!(err.contains("video"), "refusal names accepted types: {err}");
    assert!(err.contains("audio"), "refusal names accepted types: {err}");
    // An unknown drop kind refuses naming the accepted kinds.
    let bad =
        studio_clipboard::parse_drop_body("{\"kind\":\"font\"}").expect_err("unknown kind refuses");
    assert!(bad.contains("file"), "refusal names accepted kinds: {bad}");
    assert!(bad.contains("asset"), "refusal names accepted kinds: {bad}");
    assert!(
        bad.contains("composition"),
        "refusal names accepted kinds: {bad}"
    );
    assert_eq!(snapshot(&folder), before, "unsupported drops stage nothing");
}

#[test]
fn drop_composition_places_a_nest_at_the_drop_point() {
    let folder = tmp("drop-composition");
    two_comp_project(&folder);
    let req = studio_clipboard::parse_drop_body(
        "{\"kind\":\"composition\",\"source\":\"b\",\"drop-at\":2.0,\"actor\":\"maker\"}",
    )
    .expect("drop parses");
    let receipt = studio_clipboard::drop_onto_timeline(&folder, &req).expect("drop lands");
    assert_eq!(receipt.kind, "composition");
    assert_eq!(receipt.composition, "a");
    assert_eq!(receipt.landed, "b");
    assert_eq!(receipt.at, 2.0);
    let landed = effective(&folder, "a");
    let notation::model::Clip::Nest(nest) = landed.clips.last().expect("nest lands last") else {
        panic!("composition drops as a nest clip");
    };
    assert_eq!(nest.target, "b");
    assert_eq!(nest.start, 2.0);
}

#[test]
fn incompatible_payloads_refuse_by_name_staging_nothing() {
    let folder = tmp("paste-refusals");
    two_comp_project(&folder);

    // Not JSON.
    let err = studio_clipboard::parse_clipboard_body("not json").expect_err("text refuses");
    assert!(err.contains("not JSON"), "refusal names the cause: {err}");
    // Missing envelope version.
    let err = studio_clipboard::parse_clipboard_body("{\"kind\":\"clips\",\"clips\":[]}")
        .expect_err("version refuses");
    assert!(err.contains("version"), "refusal names the version: {err}");
    // Wrong kind.
    let err = studio_clipboard::parse_clipboard_body(
        "{\"clipboard\":1,\"kind\":\"lanes\",\"clips\":[{\"x\":1}]}",
    )
    .expect_err("kind refuses");
    assert!(err.contains("\"lanes\""), "refusal names the kind: {err}");
    // No clips.
    let err = studio_clipboard::parse_clipboard_body("{\"clipboard\":1,\"kind\":\"clips\"}")
        .expect_err("missing clips refuses");
    assert!(
        err.contains("`clips`"),
        "refusal names what is missing: {err}"
    );
    // Unknown copy target.
    let err = studio_clipboard::copy_clips(&folder, Some("a"), &targets(&["ghost"]))
        .expect_err("unknown target refuses");
    assert!(err.contains("\"ghost\""), "refusal names the clip: {err}");
    // Unknown copy composition.
    let err = studio_clipboard::copy_clips(&folder, Some("ghost-comp"), &targets(&["hero"]))
        .expect_err("unknown composition refuses");
    assert!(
        err.contains("\"ghost-comp\""),
        "refusal names the composition: {err}"
    );

    // A paste needing media the target project lacks names the file.
    let target = tmp("paste-missing-media");
    media_free_target(&target);
    let media = tmp("paste-media-source");
    media_project(&media);
    let payload = studio_clipboard::copy_clips(&media, Some("a"), &targets(&["pic"]))
        .expect("media copy resolves");
    assert_eq!(payload.clips.len(), 1);
    let before = snapshot(&target);
    let err = studio_clipboard::paste_clips(&target, Some("b"), &payload, 0.0, "maker", None)
        .expect_err("missing media refuses");
    assert!(
        err.contains("media/card.png"),
        "refusal names the file: {err}"
    );
    assert_eq!(
        snapshot(&target),
        before,
        "missing-media paste stages nothing"
    );
    assert_eq!(
        sdk::history::list_turns(&target)
            .expect("history lists")
            .turns
            .len(),
        0
    );

    // A paste needing a composition the target lacks names it: the
    // source nests "inner", which the lone-"b" target does not hold.
    let with_nest = tmp("paste-nest-source");
    write(
        &with_nest,
        "composition.kdl",
        "composition name=\"a\" root=true width=320 height=180 duration=8 fps=10 background=\"#112233\" version=1 {\n    nest nest=\"inner\" start=0 duration=2\n}\ncomposition name=\"inner\" width=320 height=180 duration=8 fps=10 background=\"#223344\" version=1 {\n}\n",
    );
    let payload = studio_clipboard::copy_clips(
        &with_nest,
        Some("a"),
        &[studio_clipboard::ClipTarget {
            id: None,
            index: Some(0),
        }],
    )
    .expect("nest copy resolves");
    let lone = tmp("paste-nest-target");
    empty_b_project(&lone);
    let err = studio_clipboard::paste_clips(&lone, Some("b"), &payload, 0.0, "maker", None)
        .expect_err("missing composition refuses");
    assert!(
        err.contains("\"inner\""),
        "refusal names the composition: {err}"
    );
}

#[test]
fn unfittable_paste_refuses_staging_nothing() {
    let folder = tmp("paste-full");
    two_comp_project(&folder);
    let payload = studio_clipboard::copy_clips(&folder, Some("a"), &targets(&["hero", "card"]))
        .expect("copy resolves");
    let before = snapshot(&folder);
    // Seating override keeps the test fast: `b` holds 0 clips, so a
    // 1-clip cap cannot fit 2 pasted clips.
    let err = studio_clipboard::paste_clips(&folder, Some("b"), &payload, 0.0, "maker", Some(1))
        .expect_err("full timeline refuses");
    assert!(err.contains("seating cap"), "refusal names the cap: {err}");
    assert!(err.contains('1'), "refusal names the bound: {err}");
    // A negative playhead refuses too.
    let err = studio_clipboard::paste_clips(&folder, Some("b"), &payload, -1.0, "maker", None)
        .expect_err("negative playhead refuses");
    assert!(
        err.contains("playhead"),
        "refusal names the playhead: {err}"
    );
    assert_eq!(snapshot(&folder), before, "unfittable pastes stage nothing");
    assert_eq!(
        sdk::history::list_turns(&folder)
            .expect("history lists")
            .turns
            .len(),
        0
    );
}

#[test]
fn copy_writes_nothing_anywhere() {
    let folder = tmp("copy-clean");
    two_comp_project(&folder);
    let before = snapshot(&folder);
    let payload = studio_clipboard::copy_clips(&folder, Some("a"), &targets(&["hero", "card"]))
        .expect("copy resolves");
    assert_eq!(payload.clips.len(), 2);
    assert_eq!(snapshot(&folder), before, "copy writes nothing");
}

#[test]
fn paste_undo_restores_the_timeline_through_one_history() {
    let folder = tmp("paste-undo");
    two_comp_project(&folder);
    let payload = studio_clipboard::copy_clips(&folder, Some("a"), &targets(&["hero"]))
        .expect("copy resolves");
    let receipt = studio_clipboard::paste_clips(&folder, Some("b"), &payload, 2.0, "maker", None)
        .expect("paste lands");
    assert_eq!(effective(&folder, "b").clips.len(), 1);
    sdk::history::undo_turn(&folder, receipt.turn).expect("undo reverses the paste");
    let undone = effective(&folder, "b");
    assert!(undone.clips.is_empty(), "undo restores the empty timeline");
    assert!(undone.tweens.is_empty());
    assert!(undone.keys.is_empty());
    assert!(undone.sets.is_empty());
}

#[test]
fn server_clipboard_routes_copy_cut_paste_duplicate_and_drop() {
    with_server("routes", server_project, |folder, _reg, _entry, port| {
        // Copy over HTTP returns the payload envelope.
        let (code, copied) = json(
            port,
            "POST",
            "/api/studio/clipboard/copy",
            Some("{\"composition\":\"a\",\"clips\":[{\"id\":\"hero\"}]}"),
        );
        assert_eq!(code, 200);
        assert_eq!(copied["clipboard"], 1);
        assert_eq!(copied["kind"], "clips");
        assert_eq!(copied["clips"].as_array().map(|c| c.len()), Some(1));

        // Paste at the playhead lands with a renamed id and a turn.
        let paste = format!(
            "{{\"composition\":\"b\",\"playhead\":2.0,\"actor\":\"maker\",\"clipboard\":{copied}}}",
            copied = serde_json::to_string(&copied).unwrap()
        );
        let (code, pasted) = json(port, "POST", "/api/studio/clipboard/paste", Some(&paste));
        assert_eq!(code, 200);
        assert_eq!(pasted["pasted"], true);
        assert_eq!(pasted["renamed"], serde_json::json!(["hero-paste"]));

        // Duplicate in place renames too.
        let (code, duped) = json(
            port,
            "POST",
            "/api/studio/clipboard/duplicate",
            Some("{\"composition\":\"a\",\"actor\":\"maker\",\"clips\":[{\"id\":\"card\"}]}"),
        );
        assert_eq!(code, 200);
        assert_eq!(duped["renamed"], serde_json::json!(["card-paste"]));

        // Cut removes and carries the payload for the next paste.
        let (code, cut) = json(
            port,
            "POST",
            "/api/studio/clipboard/cut",
            Some("{\"composition\":\"a\",\"actor\":\"maker\",\"clips\":[{\"id\":\"plain\"}]}"),
        );
        assert_eq!(code, 200);
        assert_eq!(cut["removed"], 1);
        assert_eq!(
            cut["clipboard"]["clips"].as_array().map(|c| c.len()),
            Some(1)
        );

        // Composition drops nest at the drop point.
        let (code, dropped) = json(
            port,
            "POST",
            "/api/studio/clipboard/drop",
            Some("{\"kind\":\"composition\",\"source\":\"b\",\"drop-at\":1.0,\"actor\":\"maker\"}"),
        );
        assert_eq!(code, 200);
        assert_eq!(dropped["dropped"], true);
        assert_eq!(dropped["landed"], "b");

        // Refusal paths: bad bodies refuse 400, unknown clips 404.
        let (code, text) = body_str(port, "POST", "/api/studio/clipboard/paste", Some("{}"));
        assert_eq!(code, 400, "{text}");
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/clipboard/copy",
            Some("{\"composition\":\"a\",\"clips\":[{\"id\":\"ghost\"}]}"),
        );
        assert_eq!(code, 404, "{text}");
        assert!(text.contains("ghost"), "{text}");
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/clipboard/drop",
            Some("{\"kind\":\"font\"}"),
        );
        assert_eq!(code, 400, "{text}");
        assert!(text.contains("composition"), "{text}");

        // The cut really removed `plain` from the served project.
        assert!(effective(folder, "a")
            .clips
            .iter()
            .all(|c| c.id() != Some("plain")));
    });
}
