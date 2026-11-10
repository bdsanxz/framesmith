//! Sprint 21 T007: per-source adapter readiness through the public
//! [`validate_folder`] API — the same verdict `check` (and `lint`)
//! reports.
//!
//! Covers the readiness half of `T007-report-readiness-and-lint.md`:
//! every adapter source reports `ready`, `loading`, or `failed` with
//! cause, per source and never per frame; failed sources stay listed
//! with the file and cause validation refused with; lint runs the
//! same validation, so its refusals name the same file and cause.
//! Loading is the live state of registry-backed custom sources (see
//! `render::custom_source` and `render::seek::wait_for_ready`); file
//! kinds resolve synchronously, so they report `ready` or `failed`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::readiness::{AdapterReadiness, AdapterState};
use validate::validate::{validate_folder, Diagnostic, Status, Verdict};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n21readiness-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(folder: &Path, name: &str, content: &[u8]) {
    std::fs::write(folder.join(name), content).unwrap();
}

/// Smallest time-varying animation: a red rect sliding left to right
/// over 2s at 30fps (see `n21_lottie.rs`).
fn slide_json() -> Vec<u8> {
    serde_json::json!({
        "v": "5.7.4", "fr": 30.0, "ip": 0.0, "op": 60.0, "w": 64.0, "h": 64.0,
        "nm": "slide",
        "markers": [{"tm": 0.0, "cm": "intro", "dr": 30.0}],
        "layers": [{
            "ddd": 0, "ind": 1, "ty": 4, "nm": "box", "sr": 1.0,
            "ks": {
                "o": {"a": 0, "k": 100.0},
                "r": {"a": 0, "k": 0.0},
                "p": {"a": 1, "k": [
                    {"t": 0.0, "s": [10.0, 32.0, 0.0], "e": [50.0, 32.0, 0.0]},
                    {"t": 30.0, "s": [50.0, 32.0, 0.0]}
                ]},
                "a": {"a": 0, "k": [0.0, 0.0, 0.0]},
                "s": {"a": 0, "k": [100.0, 100.0, 100.0]}
            },
            "shapes": [{
                "ty": "gr", "it": [
                    {"ty": "rc", "p": {"a": 0, "k": [0.0, 0.0]}, "s": {"a": 0, "k": [20.0, 20.0]}, "r": {"a": 0, "k": 0.0}},
                    {"ty": "fl", "c": {"a": 0, "k": [1.0, 0.0, 0.0, 1.0]}, "o": {"a": 0, "k": 100.0}},
                    {"ty": "tr", "p": {"a": 0, "k": [0.0, 0.0]}, "a": {"a": 0, "k": [0.0, 0.0]},
                     "s": {"a": 0, "k": [100.0, 100.0]}, "r": {"a": 0, "k": 0.0}, "o": {"a": 0, "k": 100.0}}
                ]
            }]
        }]
    })
    .to_string()
    .into_bytes()
}

// --- minimal tile-pack fixture (hand-encoded solid PNGs) ---

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for b in data {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn chunk(kind: &[u8; 4], data: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut c = Vec::new();
    c.extend_from_slice(kind);
    c.extend_from_slice(data);
    out.extend_from_slice(&crc32(&c).to_be_bytes());
}

fn solid_png(edge: u32, color: (u8, u8, u8)) -> Vec<u8> {
    let mut raw = Vec::new();
    for _ in 0..edge {
        raw.push(0);
        for _ in 0..edge {
            raw.extend_from_slice(&[color.0, color.1, color.2]);
        }
    }
    let mut deflate = vec![0x78, 0x01];
    let mut rest = raw.as_slice();
    while !rest.is_empty() {
        let take = rest.len().min(65535);
        let (head, tail) = rest.split_at(take);
        rest = tail;
        deflate.push(u8::from(rest.is_empty()));
        deflate.extend_from_slice(&(take as u16).to_le_bytes());
        deflate.extend_from_slice(&(!(take as u16)).to_le_bytes());
        deflate.extend_from_slice(head);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for byte in &raw {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    deflate.extend_from_slice(&((b << 16 | a).to_be_bytes()));
    let mut png = vec![137, 80, 78, 71, 13, 10, 26, 10];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&edge.to_be_bytes());
    ihdr.extend_from_slice(&edge.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &ihdr, &mut png);
    chunk(b"IDAT", &deflate, &mut png);
    chunk(b"IEND", &[], &mut png);
    png
}

/// A tile pack covering zoom 1 fully with a manifest attribution.
fn full_pack(dir: &Path) {
    for x in 0..2 {
        for y in 0..2 {
            let rel = format!("tiles/1/{x}/{y}.png");
            let abs = dir.join(&rel);
            std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
            std::fs::write(abs, solid_png(256, (200, 210, 220))).unwrap();
        }
    }
    std::fs::write(
        dir.join("tiles/pack.json"),
        "{\"attribution\": \"© fixtures\", \"min-zoom\": 1, \"max-zoom\": 3}",
    )
    .unwrap();
}

const OPEN: &str =
    "composition name=\"main\" width=640 height=480 duration=4 background=\"#000\" version=1 {\n";
const CLOSE: &str = "}\n";

fn comp_with(body: &str) -> String {
    format!("{OPEN}{body}{CLOSE}")
}

fn states(v: &Verdict) -> Vec<(&'static str, AdapterState)> {
    v.readiness
        .iter()
        .map(|e| (e.kind, e.state.clone()))
        .collect()
}

fn codes(ds: &[Diagnostic]) -> Vec<String> {
    ds.iter().map(|d| d.code.to_string()).collect()
}

fn assert_refusal(ds: &[Diagnostic], code: &str, needles: &[&str]) {
    assert!(!ds.is_empty(), "expected a refusal, got none");
    assert!(
        ds.iter().any(|d| d.code == code),
        "expected code {code} in {:?}",
        codes(ds)
    );
    let messages: Vec<String> = ds.iter().map(|d| d.message.clone()).collect();
    for needle in needles {
        assert!(
            messages.iter().any(|m| m.contains(needle)),
            "expected {needle:?} in {messages:?}"
        );
    }
}

#[test]
fn mixed_project_lists_every_source_ready_once() {
    let dir = tmp("mixed");
    write(&dir, "slide.json", &slide_json());
    full_pack(&dir);
    let body = comp_with(
        "    lottie id=\"hero\" path=\"slide.json\" start=0 width=64 height=64\n    chart kind=\"bar\" start=0 duration=2 x=10 y=10 width=300 height=200 {\n        row label=\"Q1\" value=12\n    }\n    scene3d start=0 duration=2 width=320 height=240 {\n    object name=\"cube\" kind=\"box\"\n}\n    map pack=\"tiles\" center=\"0,0\" zoom=1 width=256 height=256 start=0 duration=2 {\n    }\n",
    );
    write(&dir, "composition.kdl", body.as_bytes());
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    // One entry per source, never per frame: four clips, four entries.
    assert_eq!(v.readiness.len(), 4, "{:?}", v.readiness);
    assert_eq!(
        states(&v),
        [
            ("lottie", AdapterState::Ready),
            ("chart", AdapterState::Ready),
            ("scene3d", AdapterState::Ready),
            ("map", AdapterState::Ready),
        ]
    );
    // Sources name their files: the inspector line is reviewable.
    let sources: Vec<&str> = v.readiness.iter().map(|e| e.source.as_str()).collect();
    assert_eq!(
        sources,
        ["slide.json", "inline rows", "authored scene", "tiles"]
    );
    // The written id names the entry; anonymous clips fall back to
    // the `<kind clip N>` label.
    assert_eq!(v.readiness[0].display_name(), "lottie \"hero\"");
    assert_eq!(v.readiness[1].display_name(), "<chart clip 2>");
    assert!(v.readiness.iter().all(|e| e.cause.is_none()));
}

#[test]
fn two_clips_sharing_one_file_list_two_sources() {
    let dir = tmp("shared");
    write(&dir, "slide.json", &slide_json());
    let body = comp_with(
        "    lottie path=\"slide.json\" start=0 width=64 height=64\n    lottie path=\"slide.json\" start=2 width=64 height=64\n",
    );
    write(&dir, "composition.kdl", body.as_bytes());
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    // Per source, never per frame — but two clips are two sources.
    assert_eq!(v.readiness.len(), 2);
    assert!(v.readiness.iter().all(|e| e.state.is_ready()));
    assert_eq!(v.readiness[0].source, "slide.json");
    assert_eq!(v.readiness[1].source, "slide.json");
}

#[test]
fn corrupt_lottie_lists_failed_with_file_and_cause() {
    let dir = tmp("corrupt");
    write(&dir, "bad.json", b"{oops");
    let body = comp_with("    lottie path=\"bad.json\" start=0 duration=1 width=64 height=64\n");
    write(&dir, "composition.kdl", body.as_bytes());
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_LOTTIE", &["bad.json", "undecodable"]);
    // The failed source stays listed with the same file and cause.
    assert_eq!(v.readiness.len(), 1);
    let entry = &v.readiness[0];
    assert_eq!(
        (entry.kind, entry.state.clone()),
        ("lottie", AdapterState::Failed)
    );
    assert_eq!(entry.source, "bad.json");
    let cause = entry.cause.as_deref().unwrap_or("");
    assert!(cause.contains("bad.json"), "cause names file: {cause}");
    assert!(cause.contains("undecodable"), "cause names cause: {cause}");
}

#[test]
fn missing_chart_data_file_lists_failed_naming_the_file() {
    let dir = tmp("chart-file");
    let body = comp_with("    chart kind=\"bar\" data=\"gone.csv\" start=0 duration=2\n");
    write(&dir, "composition.kdl", body.as_bytes());
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_MISSING_FILE", &["gone.csv"]);
    assert_eq!(v.readiness.len(), 1);
    let entry = &v.readiness[0];
    assert_eq!(
        (entry.kind, entry.state.clone()),
        ("chart", AdapterState::Failed)
    );
    assert_eq!(entry.source, "gone.csv");
    let cause = entry.cause.as_deref().unwrap_or("");
    assert!(cause.contains("gone.csv"), "cause names file: {cause}");
}

#[test]
fn malformed_chart_row_lists_failed_naming_row_and_field() {
    let dir = tmp("chart-row");
    let body = comp_with(
        "    chart kind=\"bar\" start=0 duration=2 {\n        row label=\"Q1\" value=12\n        row label=\"Q2\" value=\"lots\"\n    }\n",
    );
    write(&dir, "composition.kdl", body.as_bytes());
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_CHART", &["row 2", "value"]);
    // Inline rows have no file: the source names the rows.
    assert_eq!(v.readiness.len(), 1);
    let entry = &v.readiness[0];
    assert_eq!(entry.state, AdapterState::Failed);
    assert_eq!(entry.source, "inline rows");
    let cause = entry.cause.as_deref().unwrap_or("");
    assert!(cause.contains("row 2"), "cause names row: {cause}");
}

#[test]
fn unbuildable_scene_lists_failed_naming_object_and_cause() {
    let dir = tmp("scene");
    let body = comp_with(
        "    scene3d start=0 duration=2 width=320 height=240 {\n    object name=\"wheel\" kind=\"torus\"\n}\n",
    );
    write(&dir, "composition.kdl", body.as_bytes());
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_SCENE3D", &["wheel", "torus"]);
    assert_eq!(v.readiness.len(), 1);
    let entry = &v.readiness[0];
    assert_eq!(
        (entry.kind, entry.state.clone()),
        ("scene3d", AdapterState::Failed)
    );
    let cause = entry.cause.as_deref().unwrap_or("");
    assert!(cause.contains("wheel"), "cause names object: {cause}");
}

#[test]
fn attribution_less_map_pack_lists_failed_naming_the_pack() {
    let dir = tmp("nopack");
    std::fs::create_dir_all(dir.join("tiles/1/0")).unwrap();
    std::fs::write(dir.join("tiles/1/0/0.png"), solid_png(256, (1, 2, 3))).unwrap();
    let body = comp_with(
        "    map pack=\"tiles\" center=\"0,0\" zoom=1 width=256 height=256 start=0 duration=2 {\n    }\n",
    );
    write(&dir, "composition.kdl", body.as_bytes());
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_MAP", &["\"tiles\"", "attribution"]);
    assert_eq!(v.readiness.len(), 1);
    let entry = &v.readiness[0];
    assert_eq!(
        (entry.kind, entry.state.clone()),
        ("map", AdapterState::Failed)
    );
    assert_eq!(entry.source, "tiles");
    let cause = entry.cause.as_deref().unwrap_or("");
    assert!(cause.contains("tiles"), "cause names pack: {cause}");
}

#[test]
fn second_viewport_refusal_lists_second_map_failed() {
    let dir = tmp("two-maps");
    full_pack(&dir);
    let body = comp_with(
        "    map pack=\"tiles\" center=\"0,0\" zoom=1 width=256 height=256 start=0 duration=2 {\n    }\n    map pack=\"tiles\" center=\"0,0\" zoom=1 width=256 height=256 start=2 duration=2 {\n    }\n",
    );
    write(&dir, "composition.kdl", body.as_bytes());
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_MAP", &["one viewport"]);
    // The first viewport stays ready; the refused one lists failed.
    assert_eq!(v.readiness.len(), 2);
    assert_eq!(v.readiness[0].state, AdapterState::Ready);
    assert_eq!(v.readiness[1].state, AdapterState::Failed);
    let cause = v.readiness[1].cause.as_deref().unwrap_or("");
    assert!(
        cause.contains("one viewport"),
        "cause names boundary: {cause}"
    );
}

#[test]
fn replicated_lottie_template_lists_one_source() {
    let dir = tmp("replicate");
    write(&dir, "slide.json", &slide_json());
    let body = comp_with(
        "    replicate count=3 {\n        lottie path=\"slide.json\" start=0 width=64 height=64\n    }\n",
    );
    write(&dir, "composition.kdl", body.as_bytes());
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    // Three clones share one file: one entry per source, never per
    // clone and never per frame.
    assert_eq!(v.readiness.len(), 1, "{:?}", v.readiness);
    let entry = &v.readiness[0];
    assert_eq!((entry.kind, entry.state.clone()), ("lottie", AdapterState::Ready));
    assert_eq!(entry.source, "slide.json");
}

#[test]
fn readiness_state_spellings_match_the_render_gate() {
    assert_eq!(AdapterState::Ready.as_str(), "ready");
    assert_eq!(AdapterState::Loading.as_str(), "loading");
    assert_eq!(AdapterState::Failed.as_str(), "failed");
    assert_eq!(format!("{}", AdapterState::Loading), "loading");
    // The human line names kind, name, and state — or the cause when
    // failed — so `check` and the render refusal read alike.
    let ready = AdapterReadiness::ready(0, Some("hero".to_string()), "lottie", "slide.json", 5, 9);
    assert_eq!(
        format!("{ready}"),
        "readiness[lottie] lottie \"hero\": ready (slide.json)"
    );
    let failed = AdapterReadiness::failed(
        1,
        None,
        "map",
        "tiles",
        "map pack \"tiles\" names no attribution",
        7,
        5,
    );
    assert_eq!(
        format!("{failed}"),
        "readiness[map] <map clip 2>: failed (map pack \"tiles\" names no attribution)"
    );
    let loading = AdapterReadiness::loading(2, None, "chart", "sales.csv", 9, 5);
    assert_eq!(loading.state, AdapterState::Loading);
    assert!(loading.cause.is_none());
    assert!(!loading.state.is_ready());
}
