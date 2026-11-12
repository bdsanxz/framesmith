//! Assets panel regressions (Sprint 18, T002 / CAP-002, J-02,
//! STATE-01, STATE-04, REC-003): image, video, audio, font, and LUT
//! cube assets list with truthful used state; import and drop copy
//! media into the project safely reporting the stored name; search,
//! type, and used filters narrow correctly; project and all-projects
//! views switch with other projects read-only; context actions act
//! or refuse with reasons; removing a referenced asset refuses
//! naming its referrers; unsupported types and duplicate imports
//! report by name. Only validated turns (plus the import, rename,
//! and remove file receipts) write the project; hidden studio state
//! files stay hidden; queue outputs land outside the project sources
//! like CLI renders; default renders stay byte-identical.

use preview::daemon;
use preview::server::{run_serve, ServeOptions};
use preview::studio_assets;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-studio-assets-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(folder: &Path, name: &str, content: &[u8]) {
    if let Some(parent) = Path::new(name).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(folder.join(parent)).unwrap();
        }
    }
    std::fs::write(folder.join(name), content).unwrap();
}

/// A project with one used image, video, audio, and LUT plus
/// unused leftovers: `media/card.png` (image clip), `media/clip.mp4`
/// (video clip), `media/bed.wav` (audio clip), and `look.cube`
/// (clip grade LUT) are referenced; `media/extra.png`,
/// `media/idle.mp3`, and `fonts/Body.ttf` sit unused.
/// Repository media roots the fixtures copy (read-only): a real
/// PNG, a real MP4, and a real WAV, so preview sessions open and
/// decode exactly like production projects.
fn repo_media(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../cli/examples")
        .join(name)
}

fn asset_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        b"composition name=\"a\" width=320 height=180 duration=4 fps=10 background=\"#112233\" version=1 {\n    image id=\"hero\" path=\"media/card.png\" start=0 duration=4\n    video id=\"clip\" path=\"media/clip.mp4\" start=0 duration=4\n    audio id=\"bed\" path=\"media/bed.wav\" start=0 duration=4\n    box id=\"card\" x=10 y=20 width=100 height=50 fill=\"#ffffff\" start=0 duration=4 {\n        grade {\n            lut src=\"look.cube\" amount=1\n        }\n    }\n    text id=\"title\" value=\"Hi\" start=0 duration=4 x=10 y=150 size=24 color=\"#ffffff\"\n}\n",
    );
    // Real decodable bytes come from the repository examples; the
    // preview session opens (and decodes) referenced media, so faked
    // bytes would refuse the open exactly like production.
    std::fs::create_dir_all(folder.join("media")).unwrap();
    for (dest, src) in [
        ("media/card.png", "reference-1-title-card/media/card.png"),
        ("media/clip.mp4", "text-on-video/clip.mp4"),
        ("media/bed.wav", "sound-reference/media/vox.wav"),
    ] {
        std::fs::copy(repo_media(src), folder.join(dest)).unwrap();
    }
    write(folder, "media/extra.png", b"\x89PNG\r\n\x1a\nunused-png");
    write(folder, "media/idle.mp3", b"unused-mp3-bytes");
    write(folder, "fonts/Body.ttf", b"unused-font-bytes");
    let mut cube = String::from("TITLE \"look\"\nLUT_3D_SIZE 2\n");
    for b in 0..2 {
        for g in 0..2 {
            for r in 0..2 {
                cube.push_str(&format!("{b} {g} {r}\n"));
            }
        }
    }
    write(folder, "look.cube", cube.as_bytes());
}

fn valid_comps(folder: &Path) -> Vec<notation::model::Composition> {
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
    verdict.compositions
}

fn names(entries: &[studio_assets::AssetEntry]) -> Vec<String> {
    entries.iter().map(|e| e.name.clone()).collect()
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
        serde_json::from_str(&text).unwrap_or_else(|_| panic!("not JSON: {text}"));
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
fn listing_reports_truthful_used_state() {
    let root = tmp("listing");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    asset_project(&folder);
    let comps = valid_comps(&folder);
    let listed = studio_assets::list_assets(&folder, &comps);
    assert_eq!(
        names(&listed),
        vec![
            "fonts/Body.ttf",
            "look.cube",
            "media/bed.wav",
            "media/card.png",
            "media/clip.mp4",
            "media/extra.png",
            "media/idle.mp3",
        ]
    );
    let by_name = |name: &str| listed.iter().find(|e| e.name == name).unwrap().clone();
    // Used assets name their referrers.
    let card = by_name("media/card.png");
    assert_eq!(card.kind, studio_assets::AssetKind::Image);
    assert!(card.used);
    assert_eq!(
        card.used_by,
        vec!["composition \"a\" image clip id \"hero\""]
    );
    let clip = by_name("media/clip.mp4");
    assert_eq!(clip.kind, studio_assets::AssetKind::Video);
    assert!(clip.used);
    assert_eq!(
        clip.used_by,
        vec!["composition \"a\" video clip id \"clip\""]
    );
    let bed = by_name("media/bed.wav");
    assert_eq!(bed.kind, studio_assets::AssetKind::Audio);
    assert!(bed.used);
    let cube = by_name("look.cube");
    assert_eq!(cube.kind, studio_assets::AssetKind::Lut);
    assert!(cube.used);
    assert_eq!(
        cube.used_by,
        vec!["composition \"a\" box clip id \"card\" grade lut"]
    );
    // Unused assets list with empty referrers.
    for name in ["media/extra.png", "media/idle.mp3", "fonts/Body.ttf"] {
        let entry = by_name(name);
        assert!(!entry.used, "{name} lists unused");
        assert!(entry.used_by.is_empty(), "{name} names no referrers");
    }
    let font = by_name("fonts/Body.ttf");
    assert_eq!(font.kind, studio_assets::AssetKind::Font);
    // Hidden studio state never lists.
    assert!(listed.iter().all(|e| !e.name.starts_with('.')));
}

#[test]
fn reference_map_names_every_reference_kind() {
    let root = tmp("refs");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    asset_project(&folder);
    let mut comps = valid_comps(&folder);
    // A font declaration, a lane grade LUT, and an image variable
    // default join the map without touching the validator.
    comps[0].fonts.push(notation::model::FontDecl {
        path: "fonts/Body.ttf".to_string(),
        line: 0,
        column: 0,
    });
    let mut lane_grade = notation::model::Grade::empty();
    lane_grade.lut.src = Some("media/lane.cube".to_string());
    comps[0].lane_grades.push(notation::model::LaneGrade {
        track: notation::model::TrackKind::Video,
        grade: lane_grade,
    });
    comps[0].variables.push(notation::model::VariableDecl {
        id: "poster".to_string(),
        var_type: notation::model::VariableType::Image,
        default: Some(notation::model::VariableDefault::Image(
            "media/poster.png".to_string(),
        )),
        label: None,
        options: None,
        line: 0,
        column: 0,
    });
    let map = studio_assets::reference_map(&comps);
    assert_eq!(
        map.get("fonts/Body.ttf").unwrap(),
        &vec!["composition \"a\" font declaration".to_string()]
    );
    assert_eq!(
        map.get("media/lane.cube").unwrap(),
        &vec!["composition \"a\" lane 0 grade lut".to_string()]
    );
    assert_eq!(
        map.get("media/poster.png").unwrap(),
        &vec!["composition \"a\" variable \"poster\" default".to_string()]
    );
    assert!(map.get("media/card.png").unwrap()[0].contains("hero"));
}

#[test]
fn filters_narrow_by_search_type_and_used() {
    let root = tmp("filters");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    asset_project(&folder);
    let comps = valid_comps(&folder);
    let listed = studio_assets::list_assets(&folder, &comps);
    let run = |query: &str| {
        let parsed = studio_assets::parse_assets_query(query).expect("query parses");
        studio_assets::filter_assets(&listed, &parsed)
            .iter()
            .map(|e| e.name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(run(""), names(&listed));
    assert_eq!(run("search=card"), vec!["media/card.png"]);
    assert_eq!(run("type=image"), vec!["media/card.png", "media/extra.png"]);
    assert_eq!(run("type=font"), vec!["fonts/Body.ttf"]);
    assert_eq!(
        run("used=unused"),
        vec!["fonts/Body.ttf", "media/extra.png", "media/idle.mp3"]
    );
    assert_eq!(run("type=video&used=used"), vec!["media/clip.mp4"]);
    assert_eq!(run("search=CARD"), vec!["media/card.png"]);
    let bad_type = studio_assets::parse_assets_query("type=nope");
    assert!(bad_type.is_err());
    assert!(bad_type.unwrap_err().contains("unknown asset `type`"));
    let bad_used = studio_assets::parse_assets_query("used=sometimes");
    assert!(bad_used.is_err());
    assert!(bad_used
        .unwrap_err()
        .contains("unknown asset `used` filter"));
    let bad_scope = studio_assets::parse_assets_query("scope=everywhere");
    assert!(bad_scope.is_err());
    assert!(bad_scope.unwrap_err().contains("unknown asset `scope`"));
}

#[test]
fn import_stores_each_type_and_reports_its_stored_name() {
    let root = tmp("import");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    asset_project(&folder);
    let drop = root.join("drop");
    std::fs::create_dir_all(&drop).unwrap();
    let cases = [
        (
            "shot.png",
            b"png-bytes".as_slice(),
            "media/shot.png",
            "image",
        ),
        (
            "clip.mov",
            b"mov-bytes".as_slice(),
            "media/clip.mov",
            "video",
        ),
        (
            "sting.mp3",
            b"mp3-bytes".as_slice(),
            "media/sting.mp3",
            "audio",
        ),
        (
            "Body.ttf",
            b"ttf-bytes".as_slice(),
            "fonts/Body-2.ttf",
            "font",
        ),
        ("look.cube", b"cube-bytes".as_slice(), "look-2.cube", "lut"),
    ];
    for (file, bytes, stored, kind) in cases {
        let source = drop.join(file);
        std::fs::write(&source, bytes).unwrap();
        let req = studio_assets::parse_import_body(&format!(
            "{{\"source\":{}}}",
            serde_json::to_string(source.to_str().unwrap()).unwrap()
        ))
        .expect("import parses");
        let receipt = studio_assets::import_asset(&folder, &req).expect("import applies");
        // `fonts/Body.ttf` and `look.cube` already exist in the
        // fixture with different bytes, so they gain a suffix.
        assert_eq!(receipt.stored, stored, "{file} stores safely");
        assert_eq!(receipt.kind.name(), kind);
        assert!(!receipt.duplicate);
        assert!(folder.join(&receipt.stored).is_file());
        assert_eq!(std::fs::read(folder.join(&receipt.stored)).unwrap(), bytes);
        let rendered = studio_assets::import_receipt_json(&receipt);
        assert!(rendered.contains("\"stored-name\":\""));
        assert!(rendered.contains(stored));
    }
}

#[test]
fn import_unsupported_type_refuses_naming_accepted_types() {
    let root = tmp("import-bad");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    asset_project(&folder);
    let source = root.join("run.exe");
    std::fs::write(&source, b"nope").unwrap();
    let req = studio_assets::parse_import_body(&format!(
        "{{\"source\":{}}}",
        serde_json::to_string(source.to_str().unwrap()).unwrap()
    ))
    .unwrap();
    let err = studio_assets::import_asset(&folder, &req).expect_err("unsupported refuses");
    assert!(err.contains("names no accepted asset type"), "{err}");
    for kind in ["image", "video", "audio", "font", "lut"] {
        assert!(err.contains(kind), "{err} names {kind}");
    }
    assert!(err.contains(".png") && err.contains(".cube"), "{err}");
    // A missing source refuses naming itself.
    let missing = studio_assets::ImportRequest {
        source: root.join("gone.png").to_string_lossy().to_string(),
        name: None,
    };
    let err = studio_assets::import_asset(&folder, &missing).expect_err("missing refuses");
    assert!(err.contains("does not exist"), "{err}");
}

#[test]
fn duplicate_import_keeps_the_existing_file() {
    let root = tmp("dup");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    asset_project(&folder);
    // Same bytes under the stored name: kept, reported duplicate.
    let same = root.join("same.png");
    std::fs::write(&same, b"\x89PNG\r\n\x1a\nunused-png").unwrap();
    let req = studio_assets::ImportRequest {
        source: same.to_string_lossy().to_string(),
        name: Some("extra.png".to_string()),
    };
    let receipt = studio_assets::import_asset(&folder, &req).expect("duplicate applies");
    assert_eq!(receipt.stored, "media/extra.png");
    assert!(receipt.duplicate);
    // Same name, different bytes: suffixed, never overwritten.
    let other = root.join("other.png");
    std::fs::write(&other, b"\x89PNG\r\n\x1a\ndifferent").unwrap();
    let req = studio_assets::ImportRequest {
        source: other.to_string_lossy().to_string(),
        name: Some("extra.png".to_string()),
    };
    let receipt = studio_assets::import_asset(&folder, &req).expect("suffixed applies");
    assert_eq!(receipt.stored, "media/extra-2.png");
    assert!(!receipt.duplicate);
    assert_eq!(
        std::fs::read(folder.join("media/extra.png")).unwrap(),
        b"\x89PNG\r\n\x1a\nunused-png"
    );
    // A traversal name cannot escape the project.
    let evil = root.join("evil.mp4");
    std::fs::write(&evil, b"evil").unwrap();
    let req = studio_assets::ImportRequest {
        source: evil.to_string_lossy().to_string(),
        name: Some("../../evil.mp4".to_string()),
    };
    let receipt = studio_assets::import_asset(&folder, &req).expect("traversal stores safely");
    assert_eq!(receipt.stored, "media/evil.mp4");
}

#[test]
fn reveal_reports_the_stored_path() {
    let root = tmp("reveal");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    asset_project(&folder);
    let out = studio_assets::reveal_asset(&folder, "media/card.png").expect("reveal applies");
    assert!(out.contains("\"revealed\":true"));
    assert!(out.contains("media/card.png"));
    let err = studio_assets::reveal_asset(&folder, "media/gone.png").expect_err("missing refuses");
    assert!(err.contains("not in the project"), "{err}");
}

#[test]
fn remove_and_rename_refuse_while_referenced() {
    let root = tmp("blocked");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    asset_project(&folder);
    let comps = valid_comps(&folder);
    // Remove of a referenced asset refuses naming its referrers, and
    // the file stays.
    let err = studio_assets::remove_asset(&folder, &comps, "media/card.png")
        .expect_err("referenced remove refuses");
    assert!(err.contains("still referenced"), "{err}");
    assert!(err.contains("hero"), "{err} names the referrer");
    assert!(folder.join("media/card.png").is_file());
    // Rename of a referenced asset refuses the same way.
    let err = studio_assets::rename_asset(&folder, &comps, "media/bed.wav", Some("nap.wav"))
        .expect_err("referenced rename refuses");
    assert!(err.contains("still referenced"), "{err}");
    assert!(err.contains("bed"), "{err} names the referrer");
    assert!(folder.join("media/bed.wav").is_file());
    // Unreferenced assets rename and remove cleanly.
    let out = studio_assets::rename_asset(&folder, &comps, "media/extra.png", Some("spare.png"))
        .expect("rename applies");
    assert!(out.contains("\"renamed\":true"));
    assert!(out.contains("media/spare.png"));
    assert!(!folder.join("media/extra.png").exists());
    let out =
        studio_assets::remove_asset(&folder, &comps, "media/spare.png").expect("remove applies");
    assert!(out.contains("\"removed\":true"));
    assert!(!folder.join("media/spare.png").exists());
    // Rename onto an existing name refuses; an unsupported target
    // extension refuses naming the accepted types.
    let err = studio_assets::rename_asset(&folder, &comps, "media/idle.mp3", Some("bed.wav"))
        .expect_err("taken name refuses");
    assert!(err.contains("already exists"), "{err}");
    let err = studio_assets::rename_asset(&folder, &comps, "media/idle.mp3", Some("notes.txt"))
        .expect_err("bad extension refuses");
    assert!(err.contains("names no accepted asset type"), "{err}");
}

#[test]
fn insert_applies_as_a_history_turn() {
    let root = tmp("insert");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    asset_project(&folder);
    let kdl_before = std::fs::read(folder.join("composition.kdl")).unwrap();
    let comps = valid_comps(&folder);
    let req = studio_assets::parse_action_body(
        "{\"action\":\"insert\",\"asset\":\"media/extra.png\",\"actor\":\"maker\"}",
    )
    .expect("action parses");
    let out = studio_assets::insert_asset(&folder, &comps, &req).expect("insert applies");
    assert!(out.contains("\"inserted\":true"));
    assert!(out.contains("\"turn\":0"), "{out}");
    assert!(out.contains("image element"), "{out}");
    // The composition sources stay byte-identical: the turn lands in
    // the hidden delta and history, never beside the sources.
    assert_eq!(
        std::fs::read(folder.join("composition.kdl")).unwrap(),
        kdl_before
    );
    // The effective composition carries the new image clip.
    let shown =
        preview::studio::effective_composition_for_session(&folder, None).expect("effective opens");
    assert!(shown.clips.iter().any(|c| match c {
        notation::model::Clip::Image(img) => img.path == "media/extra.png",
        _ => false,
    }));
    // The video and audio kinds insert through the same machinery.
    let comps = valid_comps(&folder);
    for (asset, check) in [
        ("media/clip.mp4", "video element"),
        ("media/bed.wav", "audio element"),
    ] {
        let req = studio_assets::parse_action_body(&format!(
            "{{\"action\":\"insert\",\"asset\":\"{asset}\",\"actor\":\"maker\"}}"
        ))
        .unwrap();
        let out = studio_assets::insert_asset(&folder, &comps, &req).expect("insert applies");
        assert!(out.contains(check), "{out}");
    }
}

#[test]
fn insert_refuses_with_reasons() {
    let root = tmp("insert-bad");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    asset_project(&folder);
    let comps = valid_comps(&folder);
    // Fonts join through declarations, LUTs through grade sources.
    for (asset, why) in [
        ("fonts/Body.ttf", "declarations in the notation"),
        ("look.cube", "grade `lut` source"),
    ] {
        let req = studio_assets::parse_action_body(&format!(
            "{{\"action\":\"insert\",\"asset\":\"{asset}\",\"actor\":\"maker\"}}"
        ))
        .unwrap();
        let err = studio_assets::insert_asset(&folder, &comps, &req).expect_err("refuses");
        assert!(err.contains(why), "{err}");
    }
    // Missing assets, unknown compositions, and bad windows refuse.
    let req =
        studio_assets::parse_action_body("{\"action\":\"insert\",\"asset\":\"media/gone.png\"}")
            .unwrap();
    let err = studio_assets::insert_asset(&folder, &comps, &req).expect_err("missing refuses");
    assert!(err.contains("not in the project"), "{err}");
    let req = studio_assets::parse_action_body(
        "{\"action\":\"insert\",\"asset\":\"media/extra.png\",\"composition\":\"nope\"}",
    )
    .unwrap();
    let err = studio_assets::insert_asset(&folder, &comps, &req).expect_err("unknown refuses");
    assert!(err.contains("unknown composition"), "{err}");
    let req = studio_assets::parse_action_body(
        "{\"action\":\"insert\",\"asset\":\"media/extra.png\",\"start\":-1}",
    )
    .unwrap();
    let err = studio_assets::insert_asset(&folder, &comps, &req).expect_err("bad start refuses");
    assert!(err.contains("before the composition"), "{err}");
    let bad = studio_assets::parse_action_body("{\"action\":\"explode\",\"asset\":\"x\"}");
    assert!(bad.is_err());
    assert!(bad.unwrap_err().contains("unknown asset action"));
}

#[test]
fn all_projects_view_lists_siblings_and_stays_read_only() {
    let root = tmp("allproj");
    let first = root.join("first");
    let second = root.join("second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    asset_project(&first);
    write(
        &second,
        "composition.kdl",
        b"composition name=\"b\" width=320 height=180 duration=2 fps=10 background=\"#000000\" version=1 {\n    image id=\"solo\" path=\"media/solo.png\" start=0 duration=2\n}\n",
    );
    write(&second, "media/solo.png", b"\x89PNG\r\n\x1a\nsolo");
    let comps = valid_comps(&first);
    let projects = studio_assets::discover_projects(&first, &comps);
    assert_eq!(projects.len(), 2);
    assert_eq!(projects[0].project, "first");
    assert!(!projects[0].read_only);
    assert_eq!(projects[1].project, "second");
    assert!(projects[1].read_only);
    let solo = projects[1]
        .assets
        .iter()
        .find(|e| e.name == "media/solo.png")
        .expect("sibling asset lists");
    assert!(solo.used);
    // Filters apply per project in the all view.
    let query = studio_assets::parse_assets_query("scope=all-projects&used=unused").unwrap();
    let rendered = studio_assets::all_projects_json(&projects, &query);
    assert!(rendered.contains("\"scope\":\"all-projects\""));
    assert!(rendered.contains("\"read-only\":true"));
    assert!(!rendered.contains("media/solo.png"));
}

#[test]
fn http_lists_filters_and_refusals() {
    with_server("http-list", asset_project, |_folder, _reg, _entry, port| {
        let (code, value) = json(port, "GET", "/api/studio/assets", None);
        assert_eq!(code, 200);
        assert_eq!(value["scope"], "project");
        let assets = value["assets"].as_array().expect("assets array");
        assert_eq!(assets.len(), 7);
        let card = assets
            .iter()
            .find(|a| a["name"] == "media/card.png")
            .expect("card lists");
        assert_eq!(card["type"], "image");
        assert_eq!(card["used"], true);
        assert!(card["used-by"].as_array().unwrap()[0]
            .as_str()
            .unwrap()
            .contains("hero"));
        // Type, used, and search filters narrow the rows.
        let (code, value) = json(port, "GET", "/api/studio/assets?type=image", None);
        assert_eq!(code, 200);
        assert_eq!(value["assets"].as_array().unwrap().len(), 2);
        let (code, value) = json(port, "GET", "/api/studio/assets?used=unused", None);
        assert_eq!(code, 200);
        let got: Vec<String> = value["assets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            got,
            vec!["fonts/Body.ttf", "media/extra.png", "media/idle.mp3"]
        );
        let (code, value) = json(
            port,
            "GET",
            "/api/studio/assets?search=card&type=image",
            None,
        );
        assert_eq!(code, 200);
        assert_eq!(value["assets"].as_array().unwrap().len(), 1);
        // Unknown filter values refuse with reasons.
        let (code, text) = body_str(port, "GET", "/api/studio/assets?type=nope", None);
        assert_eq!(code, 400);
        assert!(text.contains("unknown asset `type`"), "{text}");
        // The all-projects view lists every sibling project.
        let (code, value) = json(port, "GET", "/api/studio/assets?scope=all-projects", None);
        assert_eq!(code, 200);
        assert_eq!(value["scope"], "all-projects");
        assert_eq!(value["projects"].as_array().unwrap().len(), 1);
        assert_eq!(value["projects"][0]["read-only"], false);
    });
}

#[test]
fn http_import_reports_stored_name_and_duplicate() {
    with_server(
        "http-import",
        asset_project,
        |folder, _reg, _entry, port| {
            let drop = folder.parent().unwrap().join("drop");
            std::fs::create_dir_all(&drop).unwrap();
            let source = drop.join("fresh.png");
            std::fs::write(&source, b"\x89PNG\r\n\x1a\nfresh").unwrap();
            let body = format!(
                "{{\"source\":{}}}",
                serde_json::to_string(source.to_str().unwrap()).unwrap()
            );
            let (code, value) = json(port, "POST", "/api/studio/assets/import", Some(&body));
            assert_eq!(code, 200);
            assert_eq!(value["imported"], true);
            assert_eq!(value["stored-name"], "media/fresh.png");
            assert_eq!(value["duplicate"], false);
            assert!(folder.join("media/fresh.png").is_file());
            // Importing the same bytes again keeps the existing file.
            let (code, value) = json(port, "POST", "/api/studio/assets/import", Some(&body));
            assert_eq!(code, 200);
            assert_eq!(value["duplicate"], true);
            assert_eq!(value["stored-name"], "media/fresh.png");
            // Unsupported types refuse naming the accepted types.
            let bad = drop.join("run.exe");
            std::fs::write(&bad, b"nope").unwrap();
            let body = format!(
                "{{\"source\":{}}}",
                serde_json::to_string(bad.to_str().unwrap()).unwrap()
            );
            let (code, text) = body_str(port, "POST", "/api/studio/assets/import", Some(&body));
            assert_eq!(code, 409);
            assert!(text.contains("accepted types are"), "{text}");
            assert!(text.contains("image") && text.contains("lut"), "{text}");
        },
    );
}

#[test]
fn http_actions_apply_and_refuse() {
    with_server(
        "http-actions",
        asset_project,
        |folder, _reg, _entry, port| {
            // Insert lands as a history turn through the patch machinery.
            let (code, value) = json(
                port,
                "POST",
                "/api/studio/assets",
                Some("{\"action\":\"insert\",\"asset\":\"media/extra.png\",\"actor\":\"maker\"}"),
            );
            assert_eq!(code, 200);
            assert_eq!(value["inserted"], true);
            assert_eq!(value["turn"], 0);
            // Reveal reports the stored path.
            let (code, value) = json(
                port,
                "POST",
                "/api/studio/assets",
                Some("{\"action\":\"reveal\",\"asset\":\"media/card.png\"}"),
            );
            assert_eq!(code, 200);
            assert_eq!(value["path"], "media/card.png");
            // Removing a referenced asset refuses naming its referrers.
            let (code, text) = body_str(
                port,
                "POST",
                "/api/studio/assets",
                Some("{\"action\":\"remove\",\"asset\":\"media/card.png\"}"),
            );
            assert_eq!(code, 409);
            assert!(text.contains("still referenced"), "{text}");
            assert!(text.contains("hero"), "{text}");
            assert!(folder.join("media/card.png").is_file());
            // Removing an unused asset applies.
            let (code, value) = json(
                port,
                "POST",
                "/api/studio/assets",
                Some("{\"action\":\"remove\",\"asset\":\"media/idle.mp3\"}"),
            );
            assert_eq!(code, 200);
            assert_eq!(value["removed"], true);
            assert!(!folder.join("media/idle.mp3").exists());
            // Renaming an unused asset applies.
            let (code, value) = json(
                port,
                "POST",
                "/api/studio/assets",
                Some(
                    "{\"action\":\"rename\",\"asset\":\"media/extra.png\",\"name\":\"spare.png\"}",
                ),
            );
            assert_eq!(code, 200);
            assert_eq!(value["to"], "media/spare.png");
            // Actions naming another project refuse read-only.
            let (code, text) = body_str(
                port,
                "POST",
                "/api/studio/assets?project=other",
                Some("{\"action\":\"remove\",\"asset\":\"media/card.png\"}"),
            );
            assert_eq!(code, 409);
            assert!(text.contains("read-only"), "{text}");
            // Inserting a font refuses naming how fonts join.
            let (code, text) = body_str(
                port,
                "POST",
                "/api/studio/assets",
                Some("{\"action\":\"insert\",\"asset\":\"fonts/Body.ttf\"}"),
            );
            assert_eq!(code, 409, "{text}");
            assert!(text.contains("declarations in the notation"), "{text}");
        },
    );
}
