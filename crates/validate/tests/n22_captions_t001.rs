//! Sprint 22 T001: caption track with word timing through the
//! validation gate — the same verdict `check` reports.
//!
//! Covers the T001 acceptance criteria at the gate: one captions
//! track per composition; SRT, VTT, and transcript JSON decoding to
//! identical cue and word times; export round-trips with zero timing
//! drift; and refusals naming file, cue, and cause for unreadable
//! files, overlapping cues, unparseable times, unknown formats, and a
//! second captions track.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Diagnostic, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n22captions-{}-{}-{}",
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
    "composition name=\"main\" width=640 height=480 duration=6 background=\"#000\" version=1 {\n";

fn captions_block(extra: &str) -> String {
    format!("{OPEN}    captions {extra} start=0 duration=6\n}}\n")
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

/// Timing and words without decode provenance (file lines are not
/// track content, so drift checks compare these only).
fn semantically(cues: &[notation::model::CaptionCue]) -> Vec<(f64, f64, Vec<(String, f64, f64)>)> {
    cues.iter()
        .map(|c| {
            (
                c.start,
                c.end,
                c.words
                    .iter()
                    .map(|w| (w.text.clone(), w.start, w.end))
                    .collect(),
            )
        })
        .collect()
}

fn valid(body: &str) -> validate::validate::Verdict {
    let dir = tmp("valid");
    write_comp(&dir, body);
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    v
}

const SRT: &str = "1\n00:00:00,000 --> 00:00:01,500\nhello world\n\n2\n00:00:02,000 --> 00:00:03,500\ngoodbye moon\n\n";

const VTT: &str =
    "WEBVTT\n\n00:00.000 --> 00:01.500\nhello world\n\n00:02.000 --> 00:03.500\ngoodbye moon\n\n";

const JSON: &str = "{\"cues\":[{\"start\":0.0,\"end\":1.5,\"words\":[{\"text\":\"hello\",\"start\":0.0,\"end\":0.75},{\"text\":\"world\",\"start\":0.75,\"end\":1.5}]},{\"start\":2.0,\"end\":3.5,\"words\":[{\"text\":\"goodbye\",\"start\":2.0,\"end\":2.75},{\"text\":\"moon\",\"start\":2.75,\"end\":3.5}]}]}";

#[test]
fn srt_vtt_and_json_decode_to_identical_cue_and_word_times() {
    let dir = tmp("decode");
    std::fs::write(dir.join("a.srt"), SRT).unwrap();
    std::fs::write(dir.join("a.vtt"), VTT).unwrap();
    std::fs::write(dir.join("a.json"), JSON).unwrap();
    let tracks: Vec<notation::model::CaptionClip> = ["a.srt", "a.vtt", "a.json"]
        .iter()
        .map(|name| {
            let body = captions_block(&format!("src=\"{name}\""));
            write_comp(&dir, &body);
            let v = validate_folder(&dir);
            assert_eq!(v.status, Status::Valid, "{name}: {:?}", v.diagnostics);
            let comp = v.composition.expect("valid verdict holds the composition");
            assert_eq!(comp.clips.len(), 1, "{name}");
            match &comp.clips[0] {
                notation::model::Clip::Caption(cc) => cc.clone(),
                other => panic!("{name}: expected a captions clip, got {}", other.kind()),
            }
        })
        .collect();
    // Two cues each, two words each, identical windows everywhere.
    for (i, track) in tracks.iter().enumerate() {
        assert_eq!(track.cues.len(), 2, "track {i}");
        assert_eq!(track.cues[0].words.len(), 2, "track {i} cue 0");
        assert_eq!(track.cues[1].words.len(), 2, "track {i} cue 1");
    }
    assert_eq!(
        semantically(&tracks[0].cues),
        semantically(&tracks[2].cues),
        "srt matches framesmith json"
    );
    for (i, cue) in tracks[0].cues.iter().enumerate() {
        let vtt = &tracks[1].cues[i];
        assert!(
            (cue.start - vtt.start).abs() < 1e-9 && (cue.end - vtt.end).abs() < 1e-9,
            "cue {i}: srt {cue:?} vs vtt {vtt:?}"
        );
        assert_eq!(
            cue.words.iter().map(|w| w.text.clone()).collect::<Vec<_>>(),
            vtt.words.iter().map(|w| w.text.clone()).collect::<Vec<_>>(),
            "cue {i} words"
        );
        for (a, b) in cue.words.iter().zip(vtt.words.iter()) {
            assert!(
                (a.start - b.start).abs() < 1e-9 && (a.end - b.end).abs() < 1e-9,
                "cue {i} word timing: {a:?} vs {b:?}"
            );
        }
    }
}

#[test]
fn exports_reimport_with_zero_timing_drift() {
    // SRT, VTT, and JSON exports re-parse to the identical track.
    let decoded = notation::captions::parse_srt(SRT).expect("fixture parses");
    for (tag, text) in [
        ("srt", notation::captions::format_srt(&decoded.cues)),
        ("vtt", notation::captions::format_vtt(&decoded.cues)),
        (
            "json",
            notation::captions::format_transcript_json(&decoded.cues),
        ),
    ] {
        let again = notation::captions::decode_named(&format!("x.{tag}"), &text)
            .unwrap_or_else(|e| panic!("{tag} export re-parses: {e}"));
        assert_eq!(
            semantically(&again.cues),
            semantically(&decoded.cues),
            "{tag} round-trip drifts: {text:?}"
        );
        assert!(
            again.notes.is_empty(),
            "{tag} round-trip notes: {:?}",
            again.notes
        );
    }
    // Whisper/OpenAI-shaped JSON imports map words onto cues.
    let whisper = "{\"segments\":[{\"start\":0.5,\"end\":1.5,\"text\":\"hi there\",\"words\":[{\"word\":\"hi\",\"start\":0.5,\"end\":1.0},{\"word\":\"there\",\"start\":1.0,\"end\":1.5}]}]}";
    let track = notation::captions::parse_transcript_json(whisper).expect("whisper parses");
    assert_eq!(track.cues.len(), 1);
    assert_eq!(track.cues[0].words.len(), 2);
    assert_eq!(track.cues[0].words[0].text, "hi");
    assert_eq!(track.cues[0].words[1].text, "there");
    // ... and its export round-trips with zero drift too.
    let exported = notation::captions::format_transcript_json(&track.cues);
    let again = notation::captions::parse_transcript_json(&exported).expect("re-parses");
    assert_eq!(again.cues, track.cues, "whisper export round-trip drifts");
}

#[test]
fn unreadable_and_unknown_sources_refuse_naming_file_and_cause() {
    // Missing file names the file.
    assert_refusal(
        &diags(&captions_block("src=\"gone.srt\"")),
        "E_MISSING_FILE",
        &["gone.srt"],
    );
    // Unknown extension names the file (never a guessed format).
    let dir = tmp("ext");
    std::fs::write(dir.join("talk.txt"), "hello\n").unwrap();
    write_comp(&dir, &captions_block("src=\"talk.txt\""));
    assert_refusal(
        &validate_folder(&dir).diagnostics,
        "E_BAD_CAPTIONS",
        &["talk.txt", "no caption format"],
    );
    // Undecodable body names the file and the cause.
    let dir = tmp("bad");
    std::fs::write(dir.join("bad.srt"), "1\nno timing here\n\n").unwrap();
    write_comp(&dir, &captions_block("src=\"bad.srt\""));
    assert_refusal(
        &validate_folder(&dir).diagnostics,
        "E_BAD_CAPTIONS",
        &["bad.srt", "undecodable"],
    );
    // Unparseable timestamps name the line.
    let dir = tmp("ts");
    std::fs::write(dir.join("ts.srt"), "1\nsoon --> later\nhello\n\n").unwrap();
    write_comp(&dir, &captions_block("src=\"ts.srt\""));
    assert_refusal(
        &validate_folder(&dir).diagnostics,
        "E_BAD_CAPTIONS",
        &["ts.srt", "line 2"],
    );
}

#[test]
fn overlapping_cues_refuse_naming_both_cue_indexes() {
    let dir = tmp("overlap");
    std::fs::write(
        dir.join("o.srt"),
        "1\n00:00:00,000 --> 00:00:02,000\nfirst\n\n2\n00:00:01,000 --> 00:00:03,000\nsecond\n\n",
    )
    .unwrap();
    write_comp(&dir, &captions_block("src=\"o.srt\""));
    assert_refusal(
        &validate_folder(&dir).diagnostics,
        "E_BAD_CAPTIONS",
        &["cue 1", "cue 2"],
    );
}

#[test]
fn inline_cues_carry_explicit_word_times() {
    let body = format!(
        "{OPEN}    captions start=0 duration=6 {{\n        cue start=0.5 end=2.5 value=\"hi there\"\n        cue start=3 end=4 {{\n            word value=\"a\" start=3 end=3.4\n            word value=\"b\" start=3.4 end=4\n        }}\n    }}\n}}\n"
    );
    let v = valid(&body);
    let comp = v.composition.unwrap();
    match &comp.clips[0] {
        notation::model::Clip::Caption(cc) => {
            assert_eq!(cc.cues.len(), 2);
            assert_eq!(cc.cues[0].words.len(), 2);
            assert_eq!(cc.cues[1].words[0].text, "a");
            assert_eq!(cc.cues[1].words[1].text, "b");
            assert!((cc.cues[1].words[0].end - 3.4).abs() < 1e-9);
        }
        other => panic!("expected a captions clip, got {}", other.kind()),
    }
}

#[test]
fn word_outside_its_cue_refuses_naming_cue_and_word() {
    let body = format!(
        "{OPEN}    captions start=0 duration=6 {{\n        cue start=1 end=2 {{\n            word value=\"far\" start=5 end=6\n        }}\n    }}\n}}\n"
    );
    assert_refusal(&diags(&body), "E_BAD_CAPTIONS", &["cue 1", "word 1"]);
}

#[test]
fn no_source_at_all_refuses_naming_the_missing_field() {
    let body = format!("{OPEN}    captions start=0 duration=6\n}}\n");
    assert_refusal(
        &diags(&body),
        "E_MISSING_FIELD",
        &["neither `src`", "`cue`"],
    );
}

#[test]
fn two_caption_tracks_refuse_the_one_track_boundary() {
    let body = format!(
        "{OPEN}    captions start=0 duration=6 {{\n        cue start=0 end=1 value=\"one\"\n    }}\n    captions start=0 duration=6 {{\n        cue start=0 end=1 value=\"two\"\n    }}\n}}\n"
    );
    assert_refusal(
        &diags(&body),
        "E_BAD_CAPTIONS",
        &["two `captions` clips", "one track"],
    );
}

#[test]
fn dangling_overrides_drop_with_a_named_note() {
    // The override addresses a word the track no longer has:
    // validation stays valid, and lint names the drop.
    let dir = tmp("dangle");
    std::fs::write(dir.join("d.srt"), SRT).unwrap();
    write_comp(
        &dir,
        &format!(
            "{OPEN}    captions src=\"d.srt\" start=0 duration=6 {{\n        override cue=0 word=0 color=\"#ffd60a\"\n        override cue=9 word=0 color=\"#ff0000\"\n    }}\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid verdict holds the composition");
    match &comp.clips[0] {
        notation::model::Clip::Caption(cc) => {
            assert_eq!(cc.overrides.len(), 1, "the live override rides the clip");
            assert_eq!((cc.overrides[0].cue, cc.overrides[0].word), (0, 0));
            assert!(
                cc.notes
                    .iter()
                    .any(|n| n.contains("cue 9") && n.contains("dangle")),
                "the dangling override is noted: {:?}",
                cc.notes
            );
        }
        other => panic!("expected a captions clip, got {}", other.kind()),
    }
    let notes: Vec<String> = v
        .lint_findings
        .iter()
        .filter(|f| f.rule == "caption-notes")
        .map(|f| f.message.clone())
        .collect();
    assert!(
        notes
            .iter()
            .any(|m| m.contains("cue 9") && m.contains("dangle")),
        "lint names the drop: {notes:?}"
    );
}

#[test]
fn cues_past_the_film_range_note_instead_of_refusing() {
    let dir = tmp("range");
    write_comp(
        &dir,
        &format!(
            "{OPEN}    captions start=0 duration=6 {{\n        cue start=0 end=1 value=\"heard\"\n        cue start=9 end=10 value=\"unheard\"\n    }}\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let notes: Vec<String> = v
        .lint_findings
        .iter()
        .filter(|f| f.rule == "caption-notes")
        .map(|f| f.message.clone())
        .collect();
    assert!(
        notes
            .iter()
            .any(|m| m.contains("cue 2") && m.contains("never shows")),
        "lint names the out-of-range cue: {notes:?}"
    );
}

#[test]
fn empty_track_exports_valid_empty_files() {
    // An empty track writes valid empty bodies that re-import to the
    // same empty track (emptiness is not corruption): each export
    // parses back to zero cues with a named note, never an error.
    let empty: Vec<notation::model::CaptionCue> = Vec::new();
    for (tag, text) in [
        ("srt", notation::captions::format_srt(&empty)),
        ("vtt", notation::captions::format_vtt(&empty)),
        ("json", notation::captions::format_transcript_json(&empty)),
    ] {
        let again = notation::captions::decode_named(&format!("x.{tag}"), &text)
            .unwrap_or_else(|e| panic!("{tag} empty export re-parses: {e}"));
        assert!(again.cues.is_empty(), "{tag}");
        assert!(
            again.notes.iter().any(|n| n.contains("no cues")),
            "{tag} notes the emptiness: {:?}",
            again.notes
        );
    }
    // And a cue-less source file validates to a track that renders
    // nothing (covered by the gap-frame equality in render tests).
    let dir = tmp("empty");
    std::fs::write(dir.join("empty.srt"), "").unwrap();
    write_comp(&dir, &captions_block("src=\"empty.srt\""));
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.expect("valid").clips[0] {
        notation::model::Clip::Caption(cc) => {
            assert!(cc.cues.is_empty());
            assert!(
                cc.notes.iter().any(|n| n.contains("no cues")),
                "{:?}",
                cc.notes
            );
        }
        other => panic!("expected a captions clip, got {}", other.kind()),
    }
}
