//! Sprint 21 T004: offline map viewports through the public
//! [`validate_folder`] API — the same verdict `check` reports.
//!
//! Covers every acceptance criterion of
//! `T004-render-maps-offline.md` at the validation gate: packed
//! viewports with markers, labels, polylines, and polygons resolve;
//! missing packs, uncovered viewports, undecodable tiles, and
//! attribution-less packs refuse naming file (and zoom); a second
//! viewport refuses the one-viewport boundary; partial coverage stays
//! valid (the named placeholder covers the gaps at render time).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Diagnostic, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n21maps-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_comp(dir: &Path, body: &str) {
    std::fs::write(dir.join("composition.kdl"), body).unwrap();
}

const OPEN: &str =
    "composition name=\"main\" width=640 height=480 duration=2 background=\"#000\" version=1 {\n";

fn map_block(extra: &str, children: &str) -> String {
    format!("{OPEN}    map pack=\"tiles\" center=\"0,0\" zoom=1 width=256 height=256 start=0 duration=2{extra} {{\n{children}    }}\n}}\n")
}

fn diags(body: &str) -> Vec<Diagnostic> {
    let dir = tmp("case");
    write_comp(&dir, body);
    validate_folder(&dir).diagnostics
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

// --- fixture tiles (hand-encoded solid PNGs, like the render tests) --

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
    // Minimal zlib wrapper around stored blocks: CMF/FLG header,
    // then the blocks, then Adler-32 over the raw scanlines.
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

/// A tile pack covering zoom 1 fully (all four tiles) with a manifest.
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

fn first_map(dir: &Path) -> notation::model::MapClip {
    let v = validate_folder(dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match v.composition.unwrap().clips.into_iter().next().unwrap() {
        notation::model::Clip::Map(m) => m,
        other => panic!("expected a map, got {other:?}"),
    }
}

#[test]
fn packed_viewport_with_overlays_is_valid() {
    let dir = tmp("valid");
    full_pack(&dir);
    let body = map_block(
        "",
        "        marker lat=1 lon=1 label=\"hub\" color=\"#ff0000\"\n        label lat=2 lon=2 value=\"north\" size=14 color=\"#000000\"\n        polyline points=\"-10,-10 10,10\" width=3 color=\"#0000ff\"\n        polygon points=\"-20,20 -20,30 -10,30\" fill=\"#3388ff\" border-width=2 border-color=\"#0000ff\"\n",
    );
    write_comp(&dir, &body);
    let m = first_map(&dir);
    assert_eq!(m.pack, "tiles");
    assert_eq!((m.center_lat, m.center_lon), (0.0, 0.0));
    assert_eq!(m.zoom, 1);
    assert_eq!(m.attribution, "© fixtures");
    assert_eq!(m.markers.len(), 1);
    assert_eq!(m.markers[0].label.as_deref(), Some("hub"));
    assert_eq!(m.labels.len(), 1);
    assert_eq!(m.polylines.len(), 1);
    assert_eq!(m.polylines[0].points.len(), 2);
    assert_eq!(m.polygons.len(), 1);
}

#[test]
fn default_viewport_covers_the_canvas() {
    let dir = tmp("canvas");
    full_pack(&dir);
    // No `width`/`height`: the 640x480 canvas at zoom 1 needs all
    // four tiles, which the pack covers.
    let body = format!(
        "{OPEN}    map pack=\"tiles\" center=\"0,0\" zoom=1 start=0 duration=2 {{
    }}
}}
"
    );
    write_comp(&dir, &body);
    let m = first_map(&dir);
    assert!(m.width.is_none() && m.height.is_none());
}

#[test]
fn clip_attribution_overrides_the_pack() {
    let dir = tmp("override");
    std::fs::create_dir_all(dir.join("tiles/1/0")).unwrap();
    std::fs::create_dir_all(dir.join("tiles/1/1")).unwrap();
    for x in 0..2 {
        for y in 0..2 {
            std::fs::write(
                dir.join(format!("tiles/1/{x}/{y}.png")),
                solid_png(256, (1, 2, 3)),
            )
            .unwrap();
        }
    }
    // No manifest at all: the clip override satisfies the gate.
    let body = map_block(" attribution=\"© maker\"", "");
    write_comp(&dir, &body);
    assert_eq!(first_map(&dir).attribution, "© maker");
}

#[test]
fn partial_coverage_stays_valid_for_the_placeholder() {
    let dir = tmp("partial");
    // Only one of the four needed tiles: covered tiles render, the
    // gaps show the named placeholder at render time.
    std::fs::create_dir_all(dir.join("tiles/1/0")).unwrap();
    std::fs::write(dir.join("tiles/1/0/0.png"), solid_png(256, (9, 9, 9))).unwrap();
    std::fs::write(
        dir.join("tiles/pack.json"),
        "{\"attribution\": \"© fixtures\"}",
    )
    .unwrap();
    let body = map_block("", "");
    write_comp(&dir, &body);
    assert_eq!(validate_folder(&dir).status, Status::Valid);
}

#[test]
fn missing_pack_refuses_naming_the_path() {
    let dir = tmp("missing");
    write_comp(&dir, &map_block("", ""));
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(&ds, "E_MISSING_FILE", &["\"tiles\"", "does not exist"]);
}

#[test]
fn file_pack_refuses_as_not_a_directory() {
    let dir = tmp("filepack");
    std::fs::write(dir.join("tiles"), b"not a dir").unwrap();
    write_comp(&dir, &map_block("", ""));
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(&ds, "E_BAD_MAP", &["\"tiles\"", "not a tile directory"]);
}

#[test]
fn attribution_less_pack_refuses_naming_the_pack() {
    let dir = tmp("noattr");
    full_pack(&dir);
    std::fs::remove_file(dir.join("tiles/pack.json")).unwrap();
    write_comp(&dir, &map_block("", ""));
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(&ds, "E_BAD_MAP", &["\"tiles\"", "attribution"]);
}

#[test]
fn malformed_manifest_refuses_naming_file_and_cause() {
    let dir = tmp("badjson");
    full_pack(&dir);
    std::fs::write(dir.join("tiles/pack.json"), "{oops").unwrap();
    write_comp(&dir, &map_block("", ""));
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(&ds, "E_BAD_MAP", &["\"tiles\"", "pack.json", "decoded"]);
}

#[test]
fn zoom_outside_pack_range_refuses_with_path_and_zoom() {
    let dir = tmp("zoomrange");
    full_pack(&dir);
    let body = map_block("", "").replace("zoom=1", "zoom=9");
    write_comp(&dir, &body);
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(&ds, "E_BAD_MAP", &["\"tiles\"", "zoom 9"]);
}

#[test]
fn uncovered_viewport_refuses_with_path_and_zoom() {
    let dir = tmp("uncovered");
    // Pack covers zoom 0 only; the viewport asks zoom 1.
    std::fs::create_dir_all(dir.join("tiles/0/0")).unwrap();
    std::fs::write(dir.join("tiles/0/0/0.png"), solid_png(256, (4, 5, 6))).unwrap();
    std::fs::write(
        dir.join("tiles/pack.json"),
        "{\"attribution\": \"© fixtures\"}",
    )
    .unwrap();
    write_comp(&dir, &map_block("", ""));
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(
        &ds,
        "E_BAD_MAP",
        &["\"tiles\"", "zoom 1", "covers no tiles"],
    );
}

#[test]
fn corrupt_tile_refuses_naming_file_and_cause() {
    let dir = tmp("corrupt");
    full_pack(&dir);
    std::fs::write(dir.join("tiles/1/0/0.png"), b"definitely not a png").unwrap();
    write_comp(&dir, &map_block("", ""));
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(&ds, "E_BAD_MAP", &["tiles/1/0/0.png", "decoded"]);
}

#[test]
fn wrong_size_tile_refuses() {
    let dir = tmp("size");
    full_pack(&dir);
    std::fs::write(dir.join("tiles/1/1/1.png"), solid_png(128, (7, 7, 7))).unwrap();
    write_comp(&dir, &map_block("", ""));
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(&ds, "E_BAD_MAP", &["tiles/1/1/1.png", "128x128"]);
}

#[test]
fn bad_center_and_zoom_refuse() {
    let dir = tmp("badview");
    full_pack(&dir);
    write_comp(
        &dir,
        &map_block("", "").replace("center=\"0,0\"", "center=\"500,0\""),
    );
    assert_refusal(
        &validate_folder(&dir).diagnostics,
        "E_BAD_MAP",
        &["`center`"],
    );
    let dir = tmp("badzoom");
    full_pack(&dir);
    write_comp(&dir, &map_block("", "").replace("zoom=1", "zoom=99"));
    assert_refusal(&validate_folder(&dir).diagnostics, "E_BAD_MAP", &["`zoom`"]);
    let dir = tmp("nozoom");
    full_pack(&dir);
    write_comp(&dir, &map_block("", "").replace(" zoom=1", ""));
    assert_refusal(
        &validate_folder(&dir).diagnostics,
        "E_MISSING_FIELD",
        &["`zoom`"],
    );
}

#[test]
fn second_viewport_refuses_the_one_viewport_boundary() {
    let dir = tmp("two");
    full_pack(&dir);
    let body = format!(
        "{OPEN}    map pack=\"tiles\" center=\"0,0\" zoom=1 width=256 height=256 start=0 duration=2 {{
    }}
    map pack=\"tiles\" center=\"1,1\" zoom=1 width=256 height=256 start=0 duration=2 {{
    }}
}}
"
    );
    write_comp(&dir, &body);
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(&ds, "E_BAD_MAP", &["one viewport per film"]);
}

#[test]
fn overlays_validate_their_inputs() {
    let dir = tmp("ov1");
    full_pack(&dir);
    // A one-point polyline refuses.
    write_comp(
        &dir,
        &map_block(
            "",
            "        polyline points=\"1,1\" width=3 color=\"#0000ff\"\n",
        ),
    );
    assert_refusal(
        &validate_folder(&dir).diagnostics,
        "E_BAD_MAP",
        &["at least two"],
    );
    // A bad overlay color names the element and the key.
    let dir = tmp("ov2");
    full_pack(&dir);
    write_comp(
        &dir,
        &map_block("", "        marker lat=0 lon=0 color=\"red\"\n"),
    );
    assert_refusal(
        &validate_folder(&dir).diagnostics,
        "E_BAD_COLOR",
        &["marker", "`color`"],
    );
    // An out-of-range marker refuses.
    let dir = tmp("ov3");
    full_pack(&dir);
    write_comp(&dir, &map_block("", "        marker lat=90 lon=0\n"));
    assert_refusal(
        &validate_folder(&dir).diagnostics,
        "E_BAD_MAP",
        &["Web Mercator"],
    );
    // A valueless label refuses.
    let dir = tmp("ov4");
    full_pack(&dir);
    write_comp(&dir, &map_block("", "        label lat=0 lon=0\n"));
    assert_refusal(
        &validate_folder(&dir).diagnostics,
        "E_BAD_MAP",
        &["`value`"],
    );
}

#[test]
fn unknown_children_and_keys_refuse_fail_closed() {
    let dir = tmp("kid");
    full_pack(&dir);
    write_comp(&dir, &map_block("", "        traffic level=9\n"));
    assert_refusal(
        &validate_folder(&dir).diagnostics,
        "E_BAD_MAP",
        &["`traffic`"],
    );
    let dir = tmp("span");
    full_pack(&dir);
    write_comp(&dir, &map_block("", "        span value=\"x\"\n"));
    assert_refusal(
        &validate_folder(&dir).diagnostics,
        "E_BAD_TEXT",
        &["`span`"],
    );
    let dir = tmp("key");
    full_pack(&dir);
    write_comp(&dir, &map_block(" projection=\"mercator\"", ""));
    assert_refusal(
        &validate_folder(&dir).diagnostics,
        "E_UNKNOWN_KEY",
        &["\"projection\""],
    );
    let dir = tmp("fx");
    full_pack(&dir);
    write_comp(&dir, &map_block(" typewriter=true", ""));
    assert_refusal(
        &validate_folder(&dir).diagnostics,
        "E_BAD_MOTION",
        &["typewriter"],
    );
}
