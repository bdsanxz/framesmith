//! Sprint 22 captions through the normal layer stack: seek, paint,
//! presets, and the behind-subject matte (T001/T002/T003).
//!
//! Every assertion is comparative (this render against that render),
//! so the suite holds under any machine font set: words paint
//! through the same shaped-block path on every run, and the matte
//! composites the same bytes on every run.

use render::readiness::vanished_sources;
use render::render::prepare_frames;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-render-n22captions-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn prepare(dir: &Path) -> render::render::Prepared {
    let verdict = validate::validate::validate_folder(dir);
    assert_eq!(
        verdict.status,
        validate::validate::Status::Valid,
        "{:?}",
        verdict.diagnostics
    );
    let comp = verdict
        .composition
        .expect("valid verdict holds the composition");
    let all = vec![comp];
    prepare_frames(&all[0], &all, dir, Path::new("ffmpeg")).expect("prepares")
}

fn frame_at(dir: &Path, frame: u64) -> Vec<u8> {
    prepare(dir).cpu_frame(frame).expect("frame draws")
}

fn project(dir: &Path, clip: &str) {
    std::fs::write(
        dir.join("talk.srt"),
        "1\n00:00:00,000 --> 00:00:01,000\nhi yo\n\n2\n00:00:01,200 --> 00:00:02,000\ngo\n\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("composition.kdl"),
        format!(
            "composition name=\"main\" width=60 height=36 fps=30 duration=2 background=\"#000\" version=1 {{\n    {clip}\n}}\n"
        ),
    )
    .unwrap();
}

fn clip(clip: &str) -> String {
    format!("captions src=\"talk.srt\" start=0 duration=2 font-size=12 {clip}")
}

fn diff(a: &[u8], b: &[u8]) -> usize {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b.iter()).filter(|(x, y)| x != y).count()
}

/// Minimal straight-RGBA PNG writer (stored deflate blocks): the
/// subject-matte fixtures stay byte-exact with no encoder
/// dependency.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let low = crc & 1;
            crc >>= 1;
            if low == 1 {
                crc ^= 0xEDB8_8320;
            }
        }
    }
    !crc
}

fn chunk(kind: &[u8; 4], data: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut keyed = kind.to_vec();
    keyed.extend_from_slice(data);
    out.extend_from_slice(&crc32(&keyed).to_be_bytes());
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn png_rgba(w: u32, h: u32, px: &[u8]) -> Vec<u8> {
    assert_eq!(px.len(), w as usize * h as usize * 4);
    let mut raw = Vec::with_capacity(h as usize * (1 + w as usize * 4));
    for row in px.chunks(w as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut zlib = vec![0x78, 0x01];
    let mut rest = &raw[..];
    while !rest.is_empty() {
        let take = rest.len().min(65535);
        let (head, tail) = rest.split_at(take);
        rest = tail;
        zlib.push(if rest.is_empty() { 0x01 } else { 0x00 });
        zlib.extend_from_slice(&(take as u16).to_le_bytes());
        zlib.extend_from_slice(&(!(take as u16)).to_le_bytes());
        zlib.extend_from_slice(head);
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut out = vec![137, 80, 78, 71, 13, 10, 26, 10];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &ihdr, &mut out);
    chunk(b"IDAT", &zlib, &mut out);
    chunk(b"IEND", &[], &mut out);
    out
}

/// Left half opaque white, right half fully transparent: the cover
/// half and the show-through half of one matte.
fn half_matte(w: u32, h: u32) -> Vec<u8> {
    let mut px = vec![0u8; w as usize * h as usize * 4];
    for y in 0..h {
        for x in 0..w {
            let o = ((y * w + x) as usize) * 4;
            if x < w / 2 {
                px[o..o + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
    }
    png_rgba(w, h, &px)
}

#[test]
fn word_time_seek_through_the_layer_stack() {
    // Cue 1 holds [0, 1), cue 2 holds [1.2, 2): frame 15 shows cue
    // 1 words, frame 33 (t=1.1) shows none, frame 45 shows cue 2.
    let dir = tmp("seek");
    project(&dir, &clip(""));
    let cue1 = frame_at(&dir, 15);
    let gap = frame_at(&dir, 33);
    let cue2 = frame_at(&dir, 45);
    assert!(diff(&cue1, &gap) > 0, "cue 1 paints words");
    assert!(diff(&cue2, &gap) > 0, "cue 2 paints words");
    assert!(diff(&cue1, &cue2) > 0, "cues paint different words");
    // The gap frame is the bare plate: a captions-free project
    // renders the identical bytes.
    let bare = tmp("bare");
    std::fs::write(
        bare.join("composition.kdl"),
        "composition name=\"main\" width=60 height=36 fps=30 duration=2 background=\"#000\" version=1 {\n}\n",
    )
    .unwrap();
    assert_eq!(gap, frame_at(&bare, 33), "no cue means the bare plate");
}

#[test]
fn seek_helpers_map_film_time_through_the_clip_start() {
    let cues = notation::captions::parse_srt(
        "1\n00:00:00,000 --> 00:00:01,000\nhi yo\n\n2\n00:00:01,200 --> 00:00:02,000\ngo\n\n",
    )
    .unwrap()
    .cues;
    assert_eq!(render::seek::caption_cue_at(&cues, 0.5, 0.0), Some(0));
    assert_eq!(render::seek::caption_cue_at(&cues, 1.1, 0.0), None);
    assert_eq!(render::seek::caption_cue_at(&cues, 1.5, 0.0), Some(1));
    // A clip starting at 10s maps film time back to cue windows.
    assert_eq!(render::seek::caption_cue_at(&cues, 10.5, 10.0), Some(0));
    assert_eq!(render::seek::caption_cue_at(&cues, 0.5, 10.0), None);
    // The spoken word rides the same map.
    assert_eq!(render::seek::caption_word_at(&cues[0], 0.25, 0.0), Some(0));
    assert_eq!(render::seek::caption_word_at(&cues[0], 0.75, 0.0), Some(1));
    assert_eq!(render::seek::caption_word_at(&cues[0], 1.5, 0.0), None);
    // Non-finite film time is unsatisfiable: hold last good (none).
    assert_eq!(render::seek::caption_cue_at(&cues, f64::NAN, 0.0), None);
    assert_eq!(
        render::seek::caption_word_at(&cues[0], f64::INFINITY, 0.0),
        None
    );
}

#[test]
fn rail_wraps_lines_and_embed_paints_at_origin() {
    // A long rail cue wraps to at most `max-lines` and still paints
    // every word (overflow stays on the last line); an embedded cue
    // paints at its origin. Both rerun byte-identically.
    let dir = tmp("wrap");
    std::fs::write(
        dir.join("composition.kdl"),
        "composition name=\"main\" width=60 height=36 fps=30 duration=2 background=\"#000\" version=1 {\n    captions start=0 duration=2 font-size=12 max-lines=2 {\n        cue start=0 end=2 value=\"alpha beta gamma delta epsilon zeta eta theta\"\n    }\n}\n",
    )
    .unwrap();
    let wrapped = frame_at(&dir, 15);
    let bare = tmp("wrapbare");
    std::fs::write(
        bare.join("composition.kdl"),
        "composition name=\"main\" width=60 height=36 fps=30 duration=2 background=\"#000\" version=1 {\n}\n",
    )
    .unwrap();
    assert!(
        diff(&wrapped, &frame_at(&bare, 15)) > 0,
        "wrapped words paint"
    );
    assert_eq!(
        frame_at(&dir, 15),
        wrapped,
        "wrapped words rerun identically"
    );
    let embed = tmp("embed");
    std::fs::write(
        embed.join("composition.kdl"),
        "composition name=\"main\" width=60 height=36 fps=30 duration=2 background=\"#000\" version=1 {\n    captions start=0 duration=2 font-size=12 position=\"embed\" x=4 y=4 {\n        cue start=0 end=2 value=\"hi\"\n    }\n}\n",
    )
    .unwrap();
    assert!(
        diff(&frame_at(&embed, 15), &frame_at(&bare, 15)) > 0,
        "embedded words paint"
    );
    assert_eq!(frame_at(&embed, 15), frame_at(&embed, 15));
}

#[test]
fn clip_window_gates_words_like_every_layer() {
    // The clip window gates first: at t=0.5 a clip starting at 1s
    // shows the bare plate, like every other layer.
    let dir = tmp("window");
    std::fs::write(
        dir.join("composition.kdl"),
        "composition name=\"main\" width=60 height=36 fps=30 duration=2 background=\"#000\" version=1 {\n    captions start=1 duration=1 font-size=12 {\n        cue start=0 end=1 value=\"late\"\n    }\n}\n",
    )
    .unwrap();
    let bare = tmp("windowbare");
    std::fs::write(
        bare.join("composition.kdl"),
        "composition name=\"main\" width=60 height=36 fps=30 duration=2 background=\"#000\" version=1 {\n}\n",
    )
    .unwrap();
    assert_eq!(
        frame_at(&dir, 15),
        frame_at(&bare, 15),
        "pre-window shows the plate"
    );
    assert!(
        diff(&frame_at(&dir, 45), &frame_at(&bare, 45)) > 0,
        "in-window shows words"
    );
}

#[test]
fn reruns_agree_byte_for_byte() {
    let dir = tmp("rerun");
    project(
        &dir,
        &clip("entrance=\"fade-up\" highlight=\"underline\" exit=\"fade-down-out\""),
    );
    let first = frame_at(&dir, 15);
    let mut prep = prepare(&dir);
    assert_eq!(prep.cpu_frame(15).expect("redraws"), first);
    assert_eq!(frame_at(&dir, 15), first, "a fresh prepare agrees");
    assert_eq!(frame_at(&dir, 45), frame_at(&dir, 45), "cue 2 agrees too");
}

#[test]
fn overrides_repaint_words_deterministically() {
    // One word override repaints the cue, and the repaint is stable.
    let dir = tmp("override");
    project(
        &dir,
        "captions src=\"talk.srt\" start=0 duration=2 font-size=12 {\n        override cue=0 word=0 color=\"#ffd60a\" scale=1.4\n    }",
    );
    let painted = frame_at(&dir, 15);
    let plain_dir = tmp("plain");
    project(&plain_dir, &clip(""));
    let plain = frame_at(&plain_dir, 15);
    assert!(diff(&painted, &plain) > 0, "the override repaints its word");
    assert_eq!(
        frame_at(&dir, 15),
        painted,
        "the override repaint is stable"
    );
}

#[test]
fn matte_covers_where_opaque_and_shows_through_where_transparent() {
    // Behind-subject with a half-opaque matte: the opaque half shows
    // the matte (caption covered), the transparent half shows the
    // caption paint unchanged.
    let dir = tmp("matte");
    project(&dir, &clip("behind-subject=true matte=\"matte.png\""));
    std::fs::write(dir.join("matte.png"), half_matte(60, 36)).unwrap();
    let covered = frame_at(&dir, 15);
    let plain_dir = tmp("matteplain");
    project(&plain_dir, &clip(""));
    let plain = frame_at(&plain_dir, 15);
    // Left half: the opaque white matte covers plate and caption.
    for y in 0..36u32 {
        for x in 0..30u32 {
            let o = ((y * 60 + x) as usize) * 3;
            assert_eq!(
                &covered[o..o + 3],
                &[255, 255, 255],
                "opaque matte covers at ({x}, {y})"
            );
        }
    }
    // Right half: transparent matte shows the caption paint through.
    for y in 0..36u32 {
        for x in 30..60u32 {
            let o = ((y * 60 + x) as usize) * 3;
            assert_eq!(
                covered[o..o + 3],
                plain[o..o + 3],
                "transparent matte shows through at ({x}, {y})"
            );
        }
    }
    // And the plain render really paints words (the cover hides some).
    let bare = tmp("mattebare");
    std::fs::write(
        bare.join("composition.kdl"),
        "composition name=\"main\" width=60 height=36 fps=30 duration=2 background=\"#000\" version=1 {\n}\n",
    )
    .unwrap();
    assert!(
        diff(&plain, &frame_at(&bare, 15)) > 0,
        "words paint without a matte"
    );
    // Byte-identical matte rerun.
    assert_eq!(frame_at(&dir, 15), covered);
}

#[test]
fn transparent_matte_renders_the_caption_paint_unchanged() {
    // A fully transparent matte shows the caption paint through
    // everywhere: byte-identical to the matte-free render.
    let dir = tmp("clear");
    project(&dir, &clip("behind-subject=true matte=\"clear.png\""));
    std::fs::write(
        dir.join("clear.png"),
        png_rgba(60, 36, &vec![0u8; 60 * 36 * 4]),
    )
    .unwrap();
    assert_eq!(frame_at(&dir, 15), {
        let plain_dir = tmp("clearplain");
        project(&plain_dir, &clip(""));
        frame_at(&plain_dir, 15)
    });
}

#[test]
fn behind_subject_without_a_matte_refuses_naming_the_matte() {
    let dir = tmp("nomatte");
    project(&dir, &clip("behind-subject=true"));
    let ds = validate::validate::validate_folder(&dir).diagnostics;
    assert!(!ds.is_empty());
    assert!(ds.iter().any(|d| d.code == "E_BAD_CAPTIONS"));
    assert!(
        ds.iter().any(|m| m.message.contains("no `matte`")),
        "{ds:?}"
    );
}

#[test]
fn unreadable_matte_refuses_naming_file_and_cause() {
    let dir = tmp("badmatte");
    project(&dir, &clip("behind-subject=true matte=\"matte.png\""));
    std::fs::write(dir.join("matte.png"), b"not a png at all").unwrap();
    let ds = validate::validate::validate_folder(&dir).diagnostics;
    assert!(!ds.is_empty());
    assert!(
        ds.iter()
            .any(|d| d.code == "E_BAD_CAPTIONS" && d.message.contains("matte.png")),
        "{ds:?}"
    );
}

#[test]
fn vanished_sources_gate_the_render_before_frame_one() {
    // The project validates; then the source vanishes: readiness
    // names it, and preparation refuses naming it.
    let dir = tmp("vanished");
    project(&dir, &clip("behind-subject=true matte=\"matte.png\""));
    std::fs::write(dir.join("matte.png"), half_matte(60, 36)).unwrap();
    let v = validate::validate::validate_folder(&dir);
    assert_eq!(v.status, validate::validate::Status::Valid);
    std::fs::remove_file(dir.join("talk.srt")).unwrap();
    let verdict = validate::validate::validate_folder(&dir);
    assert!(verdict
        .readiness
        .iter()
        .any(|r| r.kind == "captions" && r.source == "talk.srt" && r.cause.is_some()));
    // Render-side scan over a live composition names the lost file.
    let live = tmp("live");
    project(&live, &clip(""));
    let lv = validate::validate::validate_folder(&live);
    assert_eq!(lv.status, validate::validate::Status::Valid);
    let comp = lv.composition.unwrap();
    std::fs::remove_file(live.join("talk.srt")).unwrap();
    let missing = vanished_sources(&live, &[&comp]);
    assert!(
        missing
            .iter()
            .any(|m| m.kind == "captions" && m.source == "talk.srt"),
        "{missing:?}"
    );
    // And the lost matte is named too.
    let live_matte = tmp("livematte");
    project(
        &live_matte,
        &clip("behind-subject=true matte=\"matte.png\""),
    );
    std::fs::write(live_matte.join("matte.png"), half_matte(60, 36)).unwrap();
    let mv = validate::validate::validate_folder(&live_matte);
    assert_eq!(mv.status, validate::validate::Status::Valid);
    let mcomp = mv.composition.unwrap();
    std::fs::remove_file(live_matte.join("matte.png")).unwrap();
    let missing_matte = vanished_sources(&live_matte, &[&mcomp]);
    assert!(
        missing_matte
            .iter()
            .any(|m| m.kind == "captions" && m.source == "matte.png"),
        "{missing_matte:?}"
    );
}

#[test]
fn unknown_font_family_refuses_at_prepare_naming_the_family() {
    let dir = tmp("family");
    std::fs::write(dir.join("composition.kdl"), "composition name=\"main\" width=60 height=36 fps=30 duration=2 background=\"#000\" version=1 {\n    captions start=0 duration=2 font-size=12 font-family=\"No Such Family\" {\n        cue start=0 end=1 value=\"hi\"\n    }\n}\n").unwrap();
    let v = validate::validate::validate_folder(&dir);
    assert_eq!(
        v.status,
        validate::validate::Status::Valid,
        "{:?}",
        v.diagnostics
    );
    let comp = v.composition.unwrap();
    let all = vec![comp];
    let err = match prepare_frames(&all[0], &all, &dir, Path::new("ffmpeg")) {
        Ok(_) => panic!("unknown family prepares, want a refusal"),
        Err(e) => e,
    };
    assert!(err.contains("No Such Family"), "{err}");
}

#[test]
fn preset_curves_are_pure_functions_of_film_time() {
    // The same film time samples the same preset state twice (and on
    // the render path), while different instants move through the
    // curves — no wall clock, no randomness.
    use notation::captions::*;
    use notation::model::{CaptionCue, CaptionWord};
    let cue = CaptionCue {
        start: 1.0,
        end: 2.0,
        words: vec![CaptionWord {
            text: "hi".to_string(),
            start: 1.0,
            end: 1.5,
        }],
        line: 1,
        column: 1,
    };
    for name in ENTRANCE_PRESETS {
        assert_eq!(
            entrance_state(name, &cue, 1.1),
            entrance_state(name, &cue, 1.1)
        );
    }
    for name in EXIT_PRESETS {
        assert_eq!(exit_state(name, &cue, 1.9), exit_state(name, &cue, 1.9));
    }
    for name in HIGHLIGHT_PRESETS {
        assert_eq!(
            highlight_state(name, true, false),
            highlight_state(name, true, false)
        );
    }
    // The curves actually move: entrance at the cue start differs
    // from rest, exits at the cue end differ from rest, and the
    // spoken word differs from an unspoken one.
    assert_ne!(entrance_state("fade-up", &cue, 1.0), (1.0, 0.0, 0.0, 1.0));
    assert_eq!(entrance_state("fade-up", &cue, 1.5), (1.0, 0.0, 0.0, 1.0));
    assert_ne!(exit_state("fade-out", &cue, 2.0), (1.0, 0.0, 0.0, 1.0));
    assert_eq!(exit_state("fade-out", &cue, 1.5), (1.0, 0.0, 0.0, 1.0));
    assert_ne!(
        highlight_state("pop", true, false),
        highlight_state("pop", false, false)
    );
    // `none` presets always hold the resting words.
    assert_eq!(entrance_state("none", &cue, 1.0), (1.0, 0.0, 0.0, 1.0));
    assert_eq!(exit_state("none", &cue, 2.0), (1.0, 0.0, 0.0, 1.0));
    assert_eq!(
        highlight_state("none", true, false),
        (1.0, 1.0, HighlightMark::None)
    );
}

#[test]
fn backend_parity_for_caption_frames() {
    // Caption pixels agree across backends through the same painter.
    let dir = tmp("parity");
    project(
        &dir,
        &clip("entrance=\"fade-up\" highlight=\"pop\" exit=\"fade-out\""),
    );
    let mut prep = prepare(&dir);
    let cpu = prep.cpu_frame(15).expect("cpu draws");
    let gpu = render::gpu::render_frame(&mut prep, 15).expect("gpu draws");
    assert_eq!(cpu, gpu, "caption frames agree across backends");
}
