//! Sprint 21 T006: cross-adapter time seek through prepare plus the
//! CPU blitter — the same frames `render` writes.
//!
//! Covers `T006-seek-every-adapter-by-time.md`: seeking a mixed film
//! to any time shows every adapter's time-correct frame (looped
//! Lottie wraps instead of freezing, interpolated 3D cameras resolve
//! at the clip-local instant, static charts and maps ignore the
//! clock, custom sources gate their declared range); seeking never
//! re-decodes what readiness resolved (prepared bytes survive deleted
//! sources); unsatisfiable seeks hold the last good frame with a
//! report; still-loading sources wait briefly then proceed or name
//! the source. Refusal paths name the file or source and the cause.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use render::custom_source::CustomSourceRegistry;
use render::seek::{
    custom_instant, film_local_t, lottie_instant, scene3d_instant, wait_for_ready, AdapterKind,
    DecodeLedger, SeekCache, WaitBudget,
};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-render-n21seek-{}-{}-{}",
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

/// The shared slide fixture: a red rect sliding 10→50px over 2s at
/// 30fps (see `crates/lottie/src/lib.rs`).
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

/// One mixed film: four adapter kinds side by side on a 256x64
/// canvas (lottie | chart | scene3d | map), six seconds at 30fps.
fn mixed_project(dir: &Path, lottie_extra: &str) {
    std::fs::write(dir.join("slide.json"), slide_json()).unwrap();
    std::fs::write(dir.join("sales.csv"), "Q1,12\nQ2,19\n").unwrap();
    full_pack(dir);
    let comp = format!(
        "composition name=\"main\" width=256 height=64 duration=6 fps=30 background=\"#000000\" version=1 {{\n\
         \x20   lottie path=\"slide.json\" start=0 duration=6 width=64 height=64{lottie_extra}\n\
         \x20   chart kind=\"bar\" data=\"sales.csv\" start=0 duration=6 x=64 y=0 width=64 height=64\n\
         \x20   scene3d start=0 duration=6 x=128 y=0 width=64 height=64 {{\n\
         \x20       object name=\"cube\" kind=\"box\" size=\"2,2,2\" color=\"#c02020\"\n\
         \x20       camera mode=\"fixed\" position=\"0,1,6\" target=\"0,0,0\"\n\
         \x20   }}\n\
         \x20   map pack=\"tiles\" center=\"0,0\" zoom=1 start=0 duration=6 x=192 y=0 width=64 height=64 {{\n\
         \x20       marker lat=1 lon=1 label=\"hub\" color=\"#ff0000\"\n\
         \x20   }}\n\
         }}\n"
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
        .expect("adapter-only preparation needs no decoder binary")
}

fn pixel(frame: &[u8], canvas_w: u32, x: u32, y: u32) -> (u8, u8, u8) {
    let o = (y * canvas_w + x) as usize * 3;
    (frame[o], frame[o + 1], frame[o + 2])
}

fn region_ink(frame: &[u8], canvas_w: u32, x0: u32, x1: u32, h: u32) -> usize {
    let mut ink = 0;
    for y in 0..h {
        for x in x0..x1 {
            let (r, g, b) = pixel(frame, canvas_w, x, y);
            if r != 0 || g != 0 || b != 0 {
                ink += 1;
            }
        }
    }
    ink
}

#[test]
fn all_five_kinds_show_time_correct_frames_at_one_seek() {
    let dir = tmp("mixed");
    mixed_project(&dir, " loop-mode=\"loop\"");
    let mut prep = prepared(&dir);
    // One seek to t=1.0s (frame 30): the slide sits at x=50.
    let frame = prep.cpu_frame(30).expect("seek draws");
    // Lottie region (x 0..64): red rect centered at x=50.
    assert_eq!(pixel(&frame, 256, 50, 32), (255, 0, 0));
    // Chart region (x 64..128): axes, bars, and labels paint ink.
    assert!(
        region_ink(&frame, 256, 64, 128, 64) > 20,
        "the chart paints at the seek"
    );
    // Scene3d region (x 128..192): the lit cube paints ink.
    assert!(
        region_ink(&frame, 256, 128, 192, 64) > 20,
        "the scene paints at the seek"
    );
    // Map region (x 192..256): tiles and the marker paint ink.
    assert!(
        region_ink(&frame, 256, 192, 256, 64) > 20,
        "the map paints at the seek"
    );
    // Custom sources seek through the same coordinator at the same
    // film time: a ready source renders deterministically.
    let mut registry = CustomSourceRegistry::new();
    registry
        .register("fx", 0.0, 6.0, 16, 16, vec![3u8; 8])
        .unwrap();
    registry.mark_ready("fx").unwrap();
    let t = custom_instant("fx", 1.0, 0.0, 0.0, 6.0).expect("in range");
    let a = registry.render_at("fx", t).unwrap();
    let again = registry.render_at("fx", t).unwrap();
    assert_eq!(a.pixels, again.pixels, "same film time, byte-identical");
}

#[test]
fn seeks_across_five_times_agree_with_the_loop_wrap() {
    let dir = tmp("times");
    mixed_project(&dir, " loop-mode=\"loop\"");
    let mut prep = prepared(&dir);
    // Five seeks across the moving second plus the wrap: t=2.5s
    // repeats t=0.5s exactly (the 2s loop turns the corner; every
    // other kind is static, so whole frames match). The slide
    // fixture animates 10→50px over the first second only.
    let early = prep.cpu_frame(7).expect("t=0.23 draws");
    let base = prep.cpu_frame(15).expect("t=0.5 draws");
    let mid = prep.cpu_frame(22).expect("t=0.73 draws");
    let edge = prep.cpu_frame(30).expect("t=1.0 draws");
    let wrapped = prep.cpu_frame(75).expect("t=2.5 draws");
    assert_ne!(early, base, "the slide moves across seeks");
    assert_ne!(base, mid, "the slide keeps moving");
    assert_ne!(mid, edge, "the slide reaches the key edge");
    assert_eq!(base, wrapped, "the loop wrap lands on the base instant");
    // The coordinator agrees: the wrap maps to the base instant and
    // reports the turn.
    let at = lottie_instant(2.0, notation::model::LoopMode::Loop, 2.5, 0.0).expect("finite");
    assert!((at.mapped - 0.5).abs() < 1e-9);
    assert!(at.wrapped);
}

#[test]
fn interpolated_cameras_resolve_at_the_clip_local_instant() {
    let dir = tmp("orbit");
    std::fs::write(dir.join("composition.kdl"), "composition name=\"main\" width=64 height=64 duration=4 fps=30 background=\"#000000\" version=1 {\n    scene3d start=0 duration=4 width=64 height=64 {\n    object name=\"cube\" kind=\"box\"\n    camera mode=\"orbit\" distance=6 {\n        key at=0 azimuth=0\n        key at=2 azimuth=90\n    }\n}\n}\n").unwrap();
    let mut prep = prepared(&dir);
    let early = prep.cpu_frame(30).expect("t=1 draws");
    let late = prep.cpu_frame(90).expect("t=3 draws");
    assert_ne!(early, late, "the orbit moves across the seek");
    // Reruns are byte-identical.
    let mut prep2 = prepared(&dir);
    assert_eq!(prep2.cpu_frame(30).expect("rerun"), early);
    assert_eq!(prep2.cpu_frame(90).expect("rerun"), late);
    // The key lerp is exact: the midpoint camera sits halfway
    // between the bracketing key snapshots.
    let v = validate::validate::validate_folder(&dir);
    let comp = v.composition.unwrap();
    let scene = comp
        .clips
        .iter()
        .find_map(|c| match c {
            notation::model::Clip::Scene3d(sc) => Some(sc),
            _ => None,
        })
        .expect("one scene");
    let c0 = render::scene3d::resolve_camera(&scene.camera, 0.0);
    let c1 = render::scene3d::resolve_camera(&scene.camera, 2.0);
    let mid = render::scene3d::resolve_camera(&scene.camera, 1.0);
    for i in 0..3 {
        let want = (c0.position[i] + c1.position[i]) / 2.0;
        assert!(
            (mid.position[i] - want).abs() < 1e-9,
            "keys lerp at the midpoint"
        );
    }
    assert_eq!(scene3d_instant(3.0, 1.0), Some(2.0), "clip-local clock");
}

#[test]
fn seeking_never_re_decodes_resolved_sources() {
    let dir = tmp("nodecode");
    mixed_project(&dir, " loop-mode=\"loop\"");
    let mut prep = prepared(&dir);
    let before = prep.cpu_frame(30).expect("seek draws");
    // Delete every resolved file source: lottie JSON, chart CSV, and
    // the whole tile pack. Prepared bytes survive; seeks never
    // re-read them.
    std::fs::remove_file(dir.join("slide.json")).unwrap();
    std::fs::remove_file(dir.join("sales.csv")).unwrap();
    std::fs::remove_dir_all(dir.join("tiles")).unwrap();
    let after = prep.cpu_frame(30).expect("seek still draws");
    assert_eq!(before, after, "seeks read prepared bytes, never the disk");
    let wrapped = prep.cpu_frame(75).expect("wrap still draws");
    let base = prep.cpu_frame(15).expect("base still draws");
    assert_eq!(base, wrapped);
    // The counting contract: twenty coordinator seeks decode once.
    let mut ledger = DecodeLedger::new();
    let mut prepared_bytes: std::collections::HashMap<String, Vec<u8>> = Default::default();
    let mut decodes = 0usize;
    for _ in 0..20 {
        ledger.resolve_cached(&mut prepared_bytes, "slide.json", || {
            decodes += 1;
            vec![1u8; 4]
        });
    }
    assert_eq!(decodes, 1);
    assert_eq!(ledger.decode_count("slide.json"), 1);
}

#[test]
fn unsatisfiable_seeks_hold_last_good_with_a_report() {
    // A non-looping animation holds its edge frame past the window.
    let dir = tmp("hold");
    mixed_project(&dir, "");
    let mut prep = prepared(&dir);
    let end = prep.cpu_frame(60).expect("window edge draws");
    let past = prep.cpu_frame(90).expect("past-end draws");
    assert_eq!(end, past, "past-end holds the edge frame");
    // A custom seek past the declared range holds the last good
    // frame through the coordinator and reports the hold.
    let mut registry = CustomSourceRegistry::new();
    registry
        .register("fx", 0.0, 2.0, 8, 8, vec![1u8; 4])
        .unwrap();
    registry.mark_ready("fx").unwrap();
    let mut cache = SeekCache::new();
    let good = registry.render_at("fx", 0.5).unwrap();
    cache.store("fx", good.pixels.clone());
    let cause = custom_instant("fx", 9.0, 0.0, 0.0, 2.0)
        .expect_err("past-end refuses")
        .to_string();
    let (held, report) = cache
        .hold(AdapterKind::Custom, "fx", &cause)
        .expect("holds");
    assert_eq!(held, good.pixels);
    assert!(
        report.contains("\"fx\""),
        "report names the source: {report}"
    );
    assert!(
        report.contains("out of range"),
        "report names the cause: {report}"
    );
    // With no last good frame, the hold refuses naming the source.
    let empty = SeekCache::new();
    let err = empty
        .hold(
            AdapterKind::Lottie,
            "slide",
            "the animation instant is not finite",
        )
        .expect_err("nothing to hold");
    assert!(err.contains("\"slide\""), "refusal names the source: {err}");
    // A non-finite film time is unsatisfiable for every kind.
    assert_eq!(film_local_t(f64::NAN, 0.0), None);
}

#[test]
fn still_loading_sources_wait_briefly_then_proceed_or_name() {
    use render::custom_source::Readiness;
    use std::sync::{Arc, Mutex};
    // A source that resolves mid-wait proceeds.
    let state = Arc::new(Mutex::new(Readiness::Loading));
    let flip = Arc::clone(&state);
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(20));
        *flip.lock().unwrap() = Readiness::Ready;
    });
    let (out, elapsed) =
        render::seek::timed_wait("fx", WaitBudget::brief(), || state.lock().unwrap().clone());
    assert!(out.is_ok(), "a resolving source proceeds: {out:?}");
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "the wait stays brief: {elapsed:?}"
    );
    // A failed source names the source and the cause.
    let err = wait_for_ready("fx", WaitBudget::brief(), || Readiness::Failed {
        cause: "pack is corrupt".to_string(),
    })
    .expect_err("failed refuses");
    assert!(err.contains("\"fx\""), "names the source: {err}");
    assert!(err.contains("pack is corrupt"), "names the cause: {err}");
    // A stuck source fails naming the still-loading source.
    let quick = WaitBudget {
        attempts: 10,
        interval: std::time::Duration::from_millis(1),
    };
    let err = wait_for_ready("fx", quick, || Readiness::Loading).expect_err("stuck fails");
    assert!(err.contains("\"fx\""), "names the source: {err}");
    assert!(err.contains("still loading"), "names the state: {err}");
}
