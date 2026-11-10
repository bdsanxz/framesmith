//! Sprint 22 T004: drift-free export round-trips on the captions
//! reference bytes.
//!
//! The one-command proof (`tools/n22-captions-proof.sh`) stages the
//! reference to a workdir copy and points this test at it with
//! `FRAMESMITH_N22_PROOF_REF`, so the export gate runs on exactly the
//! staged fixture bytes. Without the variable the test falls back to
//! the checkout reference, so `cargo test` alone still proves the
//! same property on the same files.

use std::path::PathBuf;

fn reference_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("FRAMESMITH_N22_PROOF_REF") {
        PathBuf::from(dir)
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/captions-reference")
    }
}

/// Timing and words without decode provenance, so the round-trip
/// comparison holds the track content only.
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

#[test]
fn proof_reference_exports_reimport_with_zero_drift() {
    let dir = reference_dir();
    assert!(
        dir.join("talk.srt").is_file(),
        "reference SRT missing in {}",
        dir.display()
    );
    let srt = std::fs::read_to_string(dir.join("talk.srt")).unwrap();
    let cues = notation::captions::parse_srt(&srt).unwrap().cues;
    assert_eq!(cues.len(), 3, "the reference carries three cues");
    let want = semantically(&cues);

    // Every exporter writes exactly what the track holds; each
    // re-import must match the track with zero drift.
    let srt_out = notation::captions::format_srt(&cues);
    let vtt_out = notation::captions::format_vtt(&cues);
    let json_out = notation::captions::format_transcript_json(&cues);
    assert_eq!(
        semantically(&notation::captions::parse_srt(&srt_out).unwrap().cues),
        want,
        "SRT export round-trips with zero drift"
    );
    assert_eq!(
        semantically(&notation::captions::parse_vtt(&vtt_out).unwrap().cues),
        want,
        "VTT export round-trips with zero drift"
    );
    assert_eq!(
        semantically(
            &notation::captions::parse_transcript_json(&json_out)
                .unwrap()
                .cues
        ),
        want,
        "JSON export round-trips with zero drift"
    );

    // Re-exporting an export is byte-stable: the text form holds no
    // hidden state.
    let cues2 = notation::captions::parse_srt(&srt_out).unwrap().cues;
    assert_eq!(
        notation::captions::format_srt(&cues2),
        srt_out,
        "SRT re-export is byte-stable"
    );
    let cues3 = notation::captions::parse_vtt(&vtt_out).unwrap().cues;
    assert_eq!(
        notation::captions::format_vtt(&cues3),
        vtt_out,
        "VTT re-export is byte-stable"
    );
    let cues4 = notation::captions::parse_transcript_json(&json_out)
        .unwrap()
        .cues;
    assert_eq!(
        notation::captions::format_transcript_json(&cues4),
        json_out,
        "JSON re-export is byte-stable"
    );
}
