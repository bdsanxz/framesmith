//! Sprint 23, T004: background removal pixels and beat analysis.
//!
//! Covers: stub subject-alpha coverage (opaque center, transparent
//! corners), inverse-plate identity where the matte is transparent,
//! matte refusal on non-PNG input, BPM on a synthetic click track
//! with snapped times and silence marks, silence-only no-grid with
//! the reason named, steady-drone no-grid, beat-file round-trip and
//! determinism, and quality/model gates.

use ml::config::MlConfig;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-ml-n23-t004-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Write a 64x48 RGBA PNG: solid red background, solid blue center
/// box (the "subject").
fn subject_png(path: &PathBuf) {
    let (w, h) = (64u32, 48u32);
    let mut px = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let subject = x >= 20 && x < 44 && y >= 12 && y < 36;
            if subject {
                px[i] = 0;
                px[i + 1] = 0;
                px[i + 2] = 255;
            } else {
                px[i] = 255;
                px[i + 1] = 0;
                px[i + 2] = 0;
            }
            px[i + 3] = 255;
        }
    }
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(file, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&px).unwrap();
}

/// Decode any still PNG to 8-bit RGBA (mirrors the matte bar plus
/// color): RGB inputs gain an opaque alpha channel.
fn decode_rgba(path: &str) -> (u32, u32, Vec<u8>) {
    let file = std::fs::File::open(path).unwrap();
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().unwrap();
    assert!(!reader.info().is_animated());
    let (w, h) = (reader.info().width, reader.info().height);
    let has_alpha = matches!(
        reader.info().color_type,
        png::ColorType::Rgba | png::ColorType::GrayscaleAlpha
    );
    let mut buf = vec![0u8; reader.output_buffer_size()];
    reader.next_frame(&mut buf).unwrap();
    if has_alpha {
        assert_eq!(buf.len(), (w * h * 4) as usize);
        (w, h, buf)
    } else {
        assert_eq!(buf.len(), (w * h * 3) as usize);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for (i, px) in buf.chunks_exact(3).enumerate() {
            rgba[i * 4] = px[0];
            rgba[i * 4 + 1] = px[1];
            rgba[i * 4 + 2] = px[2];
            rgba[i * 4 + 3] = 255;
        }
        (w, h, rgba)
    }
}

/// A 4 s 120 BPM click track at 48 kHz: unit impulses every 0.5 s
/// with exponential decay, plus 0.4 s of leading/trailing silence.
fn click_track() -> Vec<f32> {
    const RATE: usize = 48_000;
    let mut s = vec![0.0f32; RATE * 4];
    for beat in 0..7 {
        let at = RATE / 2 + beat * (RATE / 2);
        for n in 0..240 {
            if at + n < s.len() {
                s[at + n] += (-(n as f32) / 40.0).exp() * 0.9;
            }
        }
    }
    s
}

#[test]
fn alpha_covers_subject_and_plate_matches_background() {
    let root = tmp("matte");
    let input = root.join("shot.png");
    subject_png(&input);
    let alpha_out = root.join("subject-alpha.png");
    let plate_out = root.join("background-plate.png");
    ml::matting::stub_segment_files(
        &input.display().to_string(),
        &alpha_out.display().to_string(),
        &plate_out.display().to_string(),
    )
    .expect("stub segments");
    let (w, h, alpha) = decode_rgba(&alpha_out.display().to_string());
    assert_eq!((w, h), (64, 48));
    let at = |x: u32, y: u32| alpha[((y * w + x) * 4 + 3) as usize];
    // Ellipse center is fully opaque; frame corners are transparent.
    assert_eq!(at(32, 24), 255);
    assert_eq!(at(0, 0), 0);
    assert_eq!(at(63, 0), 0);
    assert_eq!(at(0, 47), 0);
    assert_eq!(at(63, 47), 0);
    // Plate matches the background exactly where the matte is
    // transparent (red corners), and carries no subject blue.
    let (_, _, plate) = decode_rgba(&plate_out.display().to_string());
    let rgb = |buf: &[u8], x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        (buf[i], buf[i + 1], buf[i + 2])
    };
    assert_eq!(rgb(&plate, 0, 0), (255, 0, 0));
    assert_eq!(rgb(&plate, 63, 47), (255, 0, 0));
    // Both outputs decode under the matte bar the Sprint 22 slot
    // applies (readable still PNGs).
    assert!(ml::matting::decode_still_png(&alpha_out.display().to_string()).is_ok());
    assert!(ml::matting::decode_still_png(&plate_out.display().to_string()).is_ok());
}

#[test]
fn matte_gates_quality_model_and_input() {
    let _guard = ENV_LOCK.lock().unwrap();
    let root = tmp("matte-gates");
    let cfg = MlConfig {
        cache_dir: root.join("cache"),
        auto_download: true,
        offline: false,
        registry_dir: None,
        download_timeout_secs: 30,
    };
    assert_eq!(
        ml::matting::model_for_quality("fast").unwrap(),
        "rembg-fast"
    );
    assert_eq!(
        ml::matting::model_for_quality("balanced").unwrap(),
        "rembg-base"
    );
    assert_eq!(
        ml::matting::model_for_quality("precise").unwrap(),
        "rembg-precise"
    );
    assert!(ml::matting::model_for_quality("ultra").is_err());
    // Non-PNG input refuses naming the input.
    let jpg = root.join("shot.jpg");
    std::fs::write(&jpg, b"fake").unwrap();
    let err = ml::matting::run_remove_background(
        &cfg,
        &jpg.display().to_string(),
        "balanced",
        None,
        "stub",
        None,
        Some(&root.join("out").display().to_string()),
    )
    .expect_err("non-PNG refuses");
    assert_eq!(err.0, "E_BAD_MATTE");
    assert!(err.1.contains("still PNG"), "{}", err.1);
    // Missing model refuses with the install fix before any decode.
    let png = root.join("shot.png");
    subject_png(&png);
    let err = ml::matting::run_remove_background(
        &cfg,
        &png.display().to_string(),
        "balanced",
        None,
        "stub",
        None,
        Some(&root.join("out").display().to_string()),
    )
    .expect_err("missing model refuses");
    assert_eq!(err.0, "E_NO_MODEL");
    assert!(
        err.1.contains("framesmith models install rembg-base --yes"),
        "{}",
        err.1
    );
}

#[test]
fn beats_finds_bpm_on_click_track_with_silence_marks() {
    let track = click_track();
    let grid = ml::beats::analyze_beats(&track, 48_000, "click.wav").expect("click track grids");
    assert!((grid.bpm - 120.0).abs() < 0.11, "bpm {}", grid.bpm);
    // Seven clicks from 0.5 s: beats snap to the signal.
    assert!(
        grid.beats.len() >= 6 && grid.beats.len() <= 8,
        "{:?}",
        grid.beats
    );
    assert!((grid.beats[0] - 0.5).abs() < 0.03, "{:?}", grid.beats);
    for w in grid.beats.windows(2) {
        assert!((w[1] - w[0] - 0.5).abs() < 0.03, "{w:?}");
    }
    assert!(grid.confidence >= ml::beats::MIN_CONFIDENCE);
    // Leading and trailing silence are marked.
    assert!(!grid.silent.is_empty());
    // Beat file round-trips and is deterministic.
    let a = ml::beats::format_beat_file(&grid, "click.wav");
    let b = ml::beats::format_beat_file(&grid, "click.wav");
    assert_eq!(a, b);
    let back = ml::beats::parse_beat_file(&a).expect("beat file parses");
    assert_eq!(back.beats, grid.beats);
    assert_eq!(back.silent, grid.silent);
    assert!((back.bpm - grid.bpm).abs() < 1e-9);
    assert!(ml::beats::parse_beat_file("{\"kind\":\"nope\"}").is_err());
}

#[test]
fn beats_on_silence_yields_no_grid_with_reason() {
    let silent = vec![0.0f32; 48_000 * 2];
    let root = tmp("silence");
    let path = root.join("silence.wav");
    ml::wav::write_wav_file(&path, &silent, 48_000).unwrap();
    let cfg = MlConfig {
        cache_dir: root.join("cache"),
        auto_download: true,
        offline: false,
        registry_dir: None,
        download_timeout_secs: 30,
    };
    let err = ml::beats::run_beats(
        &cfg,
        &path.display().to_string(),
        Some(&root.join("s.beats.json").display().to_string()),
    )
    .expect_err("silence yields no grid");
    assert_eq!(err.0, "E_NO_GRID");
    assert!(
        err.1.contains("silence") || err.1.contains("onset"),
        "{}",
        err.1
    );
    assert!(
        !root.join("s.beats.json").exists(),
        "no beat file on no-grid"
    );
}

#[test]
fn beats_on_steady_drone_yields_no_grid() {
    // A pure sine has no onsets: steady-less audio invents no beats.
    let drone: Vec<f32> = (0..48_000 * 3)
        .map(|i| ((i as f32) * 440.0 * 2.0 * std::f32::consts::PI / 48_000.0).sin() * 0.5)
        .collect();
    let err =
        ml::beats::analyze_beats(&drone, 48_000, "drone.wav").expect_err("drone yields no grid");
    assert_eq!(err.0, "E_NO_GRID");
    assert!(err.1.contains("low confidence"), "{}", err.1);
}

#[test]
fn beats_refuses_undecodable_input() {
    let _guard = ENV_LOCK.lock().unwrap();
    let root = tmp("badbeats");
    let cfg = MlConfig {
        cache_dir: root.join("cache"),
        auto_download: true,
        offline: false,
        registry_dir: None,
        download_timeout_secs: 30,
    };
    let bad = root.join("bad.wav");
    std::fs::write(&bad, b"not audio").unwrap();
    let err = ml::beats::run_beats(&cfg, &bad.display().to_string(), None)
        .expect_err("bad audio refuses");
    assert_eq!(err.0, "E_BAD_BEATS");
    assert!(err.1.contains("bad.wav"), "{}", err.1);
}
