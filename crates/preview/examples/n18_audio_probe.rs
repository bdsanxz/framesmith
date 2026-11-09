//! n18 proof probe (Sprint 18, T009): drives the two panel surfaces
//! the HTTP proof script cannot reach from shell.
//!
//! - `audio <folder> <ffmpeg> <base-turn>`: the audio rack gate on the
//!   staged reference project. Auditions `level-up` on `music` without
//!   writing, applies `fade-ends` as one patch turn, rewrites the
//!   `volume` lane, round-trips a lane copy/paste, and checks the named
//!   refusals. Receipt turns must equal `base-turn`, `base-turn + 1`,
//!   `base-turn + 2`.
//! - `paste <folder> <sibling> <at>`: the cross-project paste style
//!   gate. Reads the staged `main` composition and the sibling `b`
//!   composition and checks the pasted clips landed renamed with
//!   styles and motion carried.
//!
//! Both modes print a JSON report to the output path and exit 0. Any
//! single failed check prints to stderr and exits 1.

use preview::studio_audio::{
    apply_preset, audition_preset, copy_lane, paste_lane, read_master_meter,
    E_RACK_EMPTY_CLIPBOARD, E_RACK_EMPTY_RANGE, E_RACK_UNKNOWN_PRESET,
};
use sdk::{ClipRef, LaneEdit, OpenOptions, SdkSession};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn fail(message: &str) -> ! {
    eprintln!("probe FAIL: {message}");
    std::process::exit(1);
}

fn tree_bytes(folder: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| fail(&format!("read_dir {}: {e}", dir.display())))
            .flatten()
            .collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(key) = path.strip_prefix(folder) {
                let rel = key.to_string_lossy().to_string();
                // The serving preview server appends run logs under
                // `logs/` while the proof drives it; those are server
                // exhaust, not project truth, so the no-write oracle
                // ignores them. Everything else (sources, media,
                // hidden studio state) must stay byte-identical.
                if rel == "logs" || rel.starts_with("logs/") {
                    continue;
                }
                let bytes = std::fs::read(&path).unwrap_or_default();
                out.insert(rel, bytes);
            }
        }
    }
    out
}

fn open_read(folder: &Path, composition: &str) -> SdkSession {
    SdkSession::open(
        folder,
        OpenOptions {
            persist: false,
            history: false,
            composition: Some(composition.to_string()),
            ..OpenOptions::default()
        },
    )
    .unwrap_or_else(|e| fail(&format!("open {composition}: {e}")))
}

fn music_lane(folder: &Path) -> Vec<(f64, f64)> {
    let session = open_read(folder, "main");
    let comp = session.effective_composition();
    let idx = comp
        .clips
        .iter()
        .position(|c| c.id() == Some("music"))
        .unwrap_or_else(|| fail("music clip missing from main"));
    let mut stops: Vec<(f64, f64)> = comp
        .keys
        .iter()
        .filter(|k| k.target == idx && k.property == "volume")
        .flat_map(|k| k.stops.iter().map(|s| (s.at, s.value)))
        .collect();
    stops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    stops
}

fn turns_len(folder: &Path) -> usize {
    sdk::history::list_turns(folder)
        .unwrap_or_else(|e| fail(&format!("history lists: {e}")))
        .turns
        .len()
}

fn run_audio(folder: &Path, ffmpeg: &Path, base: u64) -> String {
    let before = tree_bytes(folder);
    let turns_before = turns_len(folder);
    let music = ClipRef::by_id("music");

    let meter_before = read_master_meter(folder, Some("main"), ffmpeg)
        .unwrap_or_else(|e| fail(&format!("meter reads: {}", e.message)));
    if meter_before.silent || meter_before.peak == 0 {
        fail("meter reads silence on the sounding reference");
    }

    let audition = audition_preset(folder, Some("main"), "level-up", &music, ffmpeg)
        .unwrap_or_else(|e| fail(&format!("audition runs: {}", e.message)));
    if audition.preset != "level-up" || !audition.changed {
        fail("audition neither names the preset nor changes the mix");
    }
    if audition.after.peak <= audition.before.peak {
        fail("a +3 dB lift must raise the master peak");
    }
    if tree_bytes(folder) != before {
        fail("audition wrote to the project");
    }

    let applied = apply_preset(folder, Some("main"), "fade-ends", &music, "maker")
        .unwrap_or_else(|e| fail(&format!("apply lands: {}", e.message)));
    if applied.applied != 2 || applied.turn != base {
        fail(&format!(
            "fade-ends must apply 2 stages as turn {base}, got {:?}",
            (applied.applied, applied.turn)
        ));
    }

    let clipboard = copy_lane(folder, Some("main"), &music, "volume", 0.0, 2.0)
        .unwrap_or_else(|e| fail(&format!("copy reads: {}", e.message)));
    let ats: Vec<f64> = clipboard.stops.iter().map(|s| s.at).collect();
    if ats != vec![0.0, 1.0, 2.0] {
        fail(&format!(
            "lane copy must carry the exact stops, got {ats:?}"
        ));
    }

    let rewrite = preview::studio_audio::rewrite_lane(
        folder,
        Some("main"),
        &music,
        "volume",
        0.0,
        2.0,
        &LaneEdit::Ramp {
            from: None,
            to: None,
        },
        "maker",
    )
    .unwrap_or_else(|e| fail(&format!("rewrite lands: {}", e.message)));
    if rewrite.turn != base + 1 {
        fail(&format!(
            "rewrite must land as turn {}, got {}",
            base + 1,
            rewrite.turn
        ));
    }

    let pasted = paste_lane(folder, Some("main"), &clipboard, &music, "maker")
        .unwrap_or_else(|e| fail(&format!("paste lands: {}", e.message)));
    if pasted.turn != base + 2 {
        fail(&format!(
            "lane paste must land as turn {}, got {}",
            base + 2,
            pasted.turn
        ));
    }
    if music_lane(folder) != vec![(0.0, 1.0), (1.0, 0.8), (2.0, 0.6)] {
        fail(&format!(
            "round trip must restore the exact stops: {:?}",
            music_lane(folder)
        ));
    }

    let empty =
        copy_lane(folder, Some("main"), &music, "pan", 0.0, 2.0).expect_err("empty copy refuses");
    if empty.code != E_RACK_EMPTY_RANGE || !empty.message.contains("0..2") {
        fail(&format!("empty range must refuse by name: {empty:?}"));
    }
    let unknown = apply_preset(folder, Some("main"), "nope", &music, "maker")
        .expect_err("unknown preset refuses");
    if unknown.code != E_RACK_UNKNOWN_PRESET || !unknown.message.contains("catalog") {
        fail(&format!(
            "unknown preset must list the catalog: {unknown:?}"
        ));
    }
    let silent = paste_lane(
        folder,
        Some("main"),
        &preview::studio_audio::LaneClipboard::default(),
        &music,
        "maker",
    )
    .expect_err("empty paste refuses");
    if silent.code != E_RACK_EMPTY_CLIPBOARD {
        fail(&format!("empty paste must refuse: {silent:?}"));
    }

    if turns_len(folder) != turns_before + 3 {
        fail("audio gate must record exactly three turns");
    }

    format!(
        "{{\"meter_peak\":{},\"audition_changed\":true,\"audition_after_peak\":{},\"apply_turn\":{},\"rewrite_turn\":{},\"paste_turn\":{},\"turns_before\":{},\"turns_after\":{}}}",
        meter_before.peak,
        audition.after.peak,
        applied.turn,
        rewrite.turn,
        pasted.turn,
        turns_before,
        turns_len(folder)
    )
}

fn clip_style(clip: &notation::model::Clip) -> String {
    match clip {
        notation::model::Clip::Text(t) => format!(
            "text|{:?}|{:?}|{:?}|{:?}|{:?}",
            t.size, t.color, t.letter_spacing, t.font_family, t.value
        ),
        notation::model::Clip::Box(b) => {
            format!(
                "box|{:?}|{:?}|{:?}|{:?}|{:?}",
                b.x, b.y, b.width, b.height, b.fill
            )
        }
        other => format!("other|{}", other.id().unwrap_or("?")),
    }
}

fn run_paste(folder: &Path, sibling: &Path, at: f64) -> String {
    let source = open_read(folder, "main");
    let src = source.effective_composition().clone();
    let hero = src
        .clips
        .iter()
        .find(|c| c.id() == Some("hero"))
        .unwrap_or_else(|| fail("source hero missing"));
    let card = src
        .clips
        .iter()
        .find(|c| c.id() == Some("card"))
        .unwrap_or_else(|| fail("source card missing"));
    let hero_style = clip_style(hero);
    let card_style = clip_style(card);

    let landed_session = open_read(sibling, "b");
    let landed = landed_session.effective_composition().clone();
    if landed.clips.len() != 2 {
        fail(&format!(
            "sibling b must hold 2 pasted clips, has {}",
            landed.clips.len()
        ));
    }
    let hero_paste = landed
        .clips
        .iter()
        .find(|c| c.id() == Some("hero-paste"))
        .unwrap_or_else(|| fail("hero-paste missing from sibling"));
    let card_paste = landed
        .clips
        .iter()
        .find(|c| c.id() == Some("card-paste"))
        .unwrap_or_else(|| fail("card-paste missing from sibling"));
    if (hero_paste.start() - at).abs() > 1e-9 {
        fail(&format!(
            "hero-paste must open at the playhead {at}, opens at {}",
            hero_paste.start()
        ));
    }
    if clip_style(hero_paste) != hero_style {
        fail(&format!(
            "hero styles must ride along: {} vs {hero_style}",
            clip_style(hero_paste)
        ));
    }
    if clip_style(card_paste) != card_style {
        fail(&format!(
            "card styles must ride along: {} vs {card_style}",
            clip_style(card_paste)
        ));
    }
    let counts = (landed.tweens.len(), landed.keys.len(), landed.sets.len());
    if counts != (1, 1, 0) {
        fail(&format!(
            "motion must remap onto the landed clips, got {counts:?}"
        ));
    }
    format!(
        "{{\"renamed\":[\"hero-paste\",\"card-paste\"],\"at\":{at},\"styles_match\":true,\"motion\":{{\"tweens\":{},\"keys\":{},\"sets\":{}}}}}",
        landed.tweens.len(),
        landed.keys.len(),
        landed.sets.len()
    )
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        fail("usage: n18_audio_probe (audio <folder> <ffmpeg> <base-turn> | paste <folder> <sibling> <at>) <out.json>");
    }
    let out = args.last().cloned().unwrap();
    let report = match args[1].as_str() {
        "audio" => {
            if args.len() != 6 {
                fail("usage: n18_audio_probe audio <folder> <ffmpeg> <base-turn> <out.json>");
            }
            let base: u64 = args[4]
                .parse()
                .unwrap_or_else(|_| fail("base-turn must be a number"));
            run_audio(Path::new(&args[2]), Path::new(&args[3]), base)
        }
        "paste" => {
            if args.len() != 6 {
                fail("usage: n18_audio_probe paste <folder> <sibling> <at> <out.json>");
            }
            let at: f64 = args[4]
                .parse()
                .unwrap_or_else(|_| fail("at must be a number"));
            run_paste(Path::new(&args[2]), Path::new(&args[3]), at)
        }
        other => fail(&format!("unknown mode {other}")),
    };
    let out_path = PathBuf::from(out);
    std::fs::write(&out_path, report).unwrap_or_else(|e| fail(&format!("write report: {e}")));
}
