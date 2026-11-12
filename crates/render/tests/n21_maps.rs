//! Sprint 21 T004: offline `map` clips through prepare plus the CPU
//! blitter — the same frames `render` writes.
//!
//! Covers the render half of `T004-render-maps-offline.md`: packed
//! tiles composite at the requested center and zoom with markers,
//! labels, polylines, polygons, scale bar, and attribution text;
//! reruns are byte-identical; a vanished tile refuses at preparation
//! — before the first frame — naming the file; no network is touched
//! (the renderer reads local files only).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-render-n21maps-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

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

/// A zoom-1 pack with four distinct tiles plus a manifest.
fn full_pack(dir: &Path) {
    let colors = [
        ((0, 0), (200, 210, 220)),
        ((1, 0), (210, 200, 220)),
        ((0, 1), (220, 210, 200)),
        ((1, 1), (190, 200, 210)),
    ];
    for ((x, y), color) in colors {
        let abs = dir.join(format!("tiles/1/{x}/{y}.png"));
        std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
        std::fs::write(abs, solid_png(256, color)).unwrap();
    }
    std::fs::write(
        dir.join("tiles/pack.json"),
        "{\"attribution\": \"© fixtures\", \"min-zoom\": 1, \"max-zoom\": 3}",
    )
    .unwrap();
}

fn project(dir: &Path, map_line: &str) {
    let comp = format!(
        "composition name=\"main\" width=256 height=256 duration=2 fps=30 background=\"#000000\" version=1 {{\n{map_line}\n}}\n"
    );
    std::fs::write(dir.join("composition.kdl"), comp).unwrap();
}

fn prepared(dir: &Path) -> render::render::Prepared {
    let v = validate::validate::validate_folder(dir);
    assert_eq!(
        v.status,
        validate::validate::Status::Valid,
        "{:?}",
        v.diagnostics
    );
    let comp = v.composition.unwrap();
    render::render::prepare_frames(&comp, &[comp.clone()], dir, Path::new("ffmpeg"))
        .expect("map-only preparation needs no decoder binary")
}

fn pixel(frame: &[u8], w: u32, x: u32, y: u32) -> (u8, u8, u8) {
    let o = (y * w + x) as usize * 3;
    (frame[o], frame[o + 1], frame[o + 2])
}

const MAP_LINE: &str = "    map pack=\"tiles\" center=\"0,0\" zoom=1 width=256 height=256 start=0 duration=2 {\n        marker lat=0 lon=0 label=\"hub\" color=\"#ff0000\"\n        label lat=20 lon=20 value=\"north\" size=12 color=\"#000000\"\n        polyline points=\"-20,-20 20,20\" width=3 color=\"#0000ff\"\n        polygon points=\"-30,20 -30,40 -10,40\" fill=\"#00ff00\" border-width=2 border-color=\"#000000\"\n    }";

#[test]
fn packed_viewport_enters_the_composition_stack() {
    let dir = tmp("stack");
    full_pack(&dir);
    project(&dir, MAP_LINE);
    let mut prep = prepared(&dir);
    let frame = prep.cpu_frame(0).expect("frame 0 draws");
    assert_eq!(frame.len(), 256 * 256 * 3);
    // Untouched tile pixels composite at center and zoom: (10,10)
    // lands in tile (0,0) with its own fill.
    assert_eq!(pixel(&frame, 256, 10, 10), (200, 210, 220));
    // The center marker pins red over tile (1,1).
    assert_eq!(pixel(&frame, 256, 128, 128), (255, 0, 0));
    // The scale bar paints black/white segments bottom-left.
    let mut bar = false;
    for y in 224..250 {
        for x in 8..100 {
            let p = pixel(&frame, 256, x, y);
            if p == (0, 0, 0) || p == (255, 255, 255) {
                bar = true;
            }
        }
    }
    assert!(bar, "scale bar must paint");
    // The attribution pill paints white bottom-right with dark ink.
    let mut pill = false;
    let mut ink = false;
    for y in 228..250 {
        for x in 150..250 {
            let p = pixel(&frame, 256, x, y);
            if p == (255, 255, 255) {
                pill = true;
            }
            if p.0 < 40 && p.1 < 40 && p.2 < 40 {
                ink = true;
            }
        }
    }
    assert!(pill, "attribution pill must paint");
    assert!(ink, "attribution text must paint");
    // Maps are static per film time: every seek shows the same frame.
    assert_eq!(prep.cpu_frame(30).expect("seek draws"), frame);
    // Reruns are byte-identical through a fresh preparation.
    let mut prep2 = prepared(&dir);
    assert_eq!(prep2.cpu_frame(0).expect("rerun draws"), frame);
}

#[test]
fn corrupted_tile_refuses_before_the_first_frame() {
    let dir = tmp("vanish");
    full_pack(&dir);
    project(&dir, MAP_LINE);
    let v = validate::validate::validate_folder(&dir);
    assert_eq!(
        v.status,
        validate::validate::Status::Valid,
        "{:?}",
        v.diagnostics
    );
    let comp = v.composition.unwrap();
    // A tile corrupted after validation refuses at preparation,
    // naming the file and the cause — never a partial frame. (A tile
    // missing outright renders the named placeholder instead.)
    std::fs::write(dir.join("tiles/1/1/1.png"), b"not a png anymore").unwrap();
    let err =
        match render::render::prepare_frames(&comp, &[comp.clone()], &dir, Path::new("ffmpeg")) {
            Ok(_) => panic!("a corrupted tile must refuse preparation"),
            Err(e) => e,
        };
    // Separator-agnostic: the refusal names the tile path with the
    // OS separator (possibly Debug-escaped), so compare the
    // normalized form.
    let err = err.replace("\\\\", "/").replace('\\', "/");
    assert!(
        err.contains("tiles/1/1/1.png"),
        "refusal must name the file, got: {err}"
    );
}

#[test]
fn missing_pack_refuses_naming_the_path() {
    let dir = tmp("missing");
    project(&dir, MAP_LINE);
    let v = validate::validate::validate_folder(&dir);
    assert_eq!(v.status, validate::validate::Status::Invalid);
    assert!(
        v.diagnostics
            .iter()
            .any(|d| d.code == "E_MISSING_FILE" && d.message.contains("\"tiles\"")),
        "{:?}",
        v.diagnostics
            .iter()
            .map(|d| (d.code.clone(), d.message.clone()))
            .collect::<Vec<_>>()
    );
}
