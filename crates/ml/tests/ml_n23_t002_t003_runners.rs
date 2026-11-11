//! Sprint 23, T002 + T003: stub transcript/TTS determinism and gates.
//!
//! Covers: stub transcript JSON identity through the Sprint 22
//! importer (zero drift by construction), SRT/VTT conversion
//! determinism and re-import identity, TTS byte-identical reruns,
//! voice-shaped output, unknown-voice refusal, missing-model
//! refusals with the install fix, and WAV round-trips.

use ml::config::MlConfig;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-ml-n23-t23-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn wav_bytes(samples: &[f32], rate: u32) -> Vec<u8> {
    ml::wav::write_wav_mono16(samples, rate)
}

#[test]
fn stub_transcript_imports_with_zero_drift() {
    // The stub emits through the same formatter captions import, so
    // decode must return every cue and word time exactly.
    let cues = ml::stub::stub_transcript_cues(4.0);
    assert!(!cues.is_empty());
    let json = ml::stub::stub_transcript_json(4.0);
    let decoded = notation::captions::parse_transcript_json(&json).expect("stub JSON parses");
    assert_eq!(decoded.cues.len(), cues.len());
    for (a, b) in decoded.cues.iter().zip(cues.iter()) {
        assert_eq!((a.start, a.end), (b.start, b.end));
        assert_eq!(a.words.len(), b.words.len());
        for (wa, wb) in a.words.iter().zip(b.words.iter()) {
            assert_eq!(wa.text, wb.text);
            assert_eq!((wa.start, wa.end), (wb.start, wb.end));
        }
    }
}

#[test]
fn srt_vtt_convert_deterministically_and_reimport_cleanly() {
    let cues = ml::stub::stub_transcript_cues(6.0);
    let srt_a = notation::captions::format_srt(&cues);
    let srt_b = notation::captions::format_srt(&cues);
    assert_eq!(srt_a, srt_b);
    let vtt_a = notation::captions::format_vtt(&cues);
    let vtt_b = notation::captions::format_vtt(&cues);
    assert_eq!(vtt_a, vtt_b);
    assert!(vtt_a.starts_with("WEBVTT"));
    // Re-import drifts by zero: cue count and times survive the trip.
    let back = notation::captions::decode_named("out.srt", &srt_a).expect("srt re-imports");
    assert_eq!(back.cues.len(), cues.len());
    for (a, b) in back.cues.iter().zip(cues.iter()) {
        assert_eq!((a.start, a.end), (b.start, b.end));
        let ta: String = a
            .words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let tb: String = b
            .words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(ta, tb);
    }
    let back_vtt = notation::captions::decode_named("out.vtt", &vtt_a).expect("vtt re-imports");
    assert_eq!(back_vtt.cues.len(), cues.len());
}

#[test]
fn stub_tts_reruns_are_byte_identical_and_voice_shaped() {
    let a = ml::stub::stub_tts_samples("hello framesmith", "aria", 1.0);
    let b = ml::stub::stub_tts_samples("hello framesmith", "aria", 1.0);
    assert_eq!(a, b);
    assert!(!a.is_empty());
    // Voice and speed shape the output.
    let other_voice = ml::stub::stub_tts_samples("hello framesmith", "kai", 1.0);
    assert_ne!(a, other_voice);
    let faster = ml::stub::stub_tts_samples("hello framesmith", "aria", 2.0);
    assert!(faster.len() < a.len());
    // The bytes are a valid WAV that decodes back.
    let root = tmp("tts-wav");
    let path = root.join("voice.wav");
    ml::wav::write_wav_file(&path, &a, 48_000).unwrap();
    let back = ml::wav::read_wav(&path).expect("tts wav decodes");
    assert_eq!(back.rate, 48_000);
    assert_eq!(back.samples.len(), a.len());
}

#[test]
fn tts_gates_unknown_voice_and_missing_model() {
    let _guard = ENV_LOCK.lock().unwrap();
    let root = tmp("tts-gates");
    let cfg = MlConfig {
        cache_dir: root.join("cache"),
        auto_download: true,
        offline: false,
        registry_dir: None,
        download_timeout_secs: 30,
    };
    let mut opts = ml::tts::TtsOptions::default();
    opts.voice = "no-voice".to_string();
    let err = ml::tts::run_tts(&cfg, "hello", &opts).expect_err("unknown voice refuses");
    assert_eq!(err.0, "E_BAD_TTS");
    assert!(err.1.contains("unknown voice"), "{}", err.1);
    assert!(err.1.contains("aria"), "{}", err.1);
    // Empty text refuses before any work.
    let opts = ml::tts::TtsOptions::default();
    let err = ml::tts::run_tts(&cfg, "   ", &opts).expect_err("empty text refuses");
    assert_eq!(err.0, "E_BAD_TTS");
    // Missing model refuses with the install fix (no engine spawned).
    let mut opts = ml::tts::TtsOptions::default();
    opts.engine = "stub".to_string();
    let err = ml::tts::run_tts(&cfg, "hello", &opts).expect_err("missing model refuses");
    assert_eq!(err.0, "E_NO_MODEL");
    assert!(
        err.1
            .contains("framesmith models install tts-en-aria --yes"),
        "{}",
        err.1
    );
}

#[test]
fn transcribe_gates_missing_model_before_decode() {
    let _guard = ENV_LOCK.lock().unwrap();
    let root = tmp("tr-gates");
    let wav = root.join("speech.wav");
    ml::wav::write_wav_file(&wav, &vec![0.0f32; 48_000], 48_000).unwrap();
    let cfg = MlConfig {
        cache_dir: root.join("cache"),
        auto_download: true,
        offline: false,
        registry_dir: None,
        download_timeout_secs: 30,
    };
    let mut opts = ml::transcribe::TranscribeOptions::default();
    opts.engine = "stub".to_string();
    let err = ml::transcribe::run_transcribe(&cfg, &wav.display().to_string(), &opts)
        .expect_err("missing model refuses before decode");
    assert_eq!(err.0, "E_NO_MODEL");
    assert!(
        err.1
            .contains("framesmith models install whisper-tiny-en --yes"),
        "{}",
        err.1
    );
    assert!(!root.join("speech.wav.transcript.json").exists());
    // Bad `--to` refuses naming the accepted values.
    opts.to = Some("ass".to_string());
    let err = ml::transcribe::run_transcribe(&cfg, &wav.display().to_string(), &opts)
        .expect_err("bad --to refuses");
    assert_eq!(err.0, "E_BAD_TRANSCRIBE");
    // Missing input refuses naming the input.
    let opts = ml::transcribe::TranscribeOptions::default();
    let err =
        ml::transcribe::run_transcribe(&cfg, &root.join("nope.wav").display().to_string(), &opts)
            .expect_err("missing input refuses");
    assert!(err.1.contains("cannot be read"), "{}", err.1);
}

#[test]
fn wav_roundtrip_and_refusals() {
    let samples: Vec<f32> = (0..4800).map(|i| ((i as f32) * 0.01).sin() * 0.5).collect();
    let bytes = wav_bytes(&samples, 48_000);
    let back = ml::wav::decode_wav(&bytes).expect("wav decodes");
    assert_eq!(back.rate, 48_000);
    assert_eq!(back.samples.len(), samples.len());
    for (a, b) in back.samples.iter().zip(samples.iter()) {
        assert!((a - b).abs() < 1.0 / 32768.0 + 1e-6, "{a} vs {b}");
    }
    assert!(ml::wav::decode_wav(b"too short").is_err());
    assert!(ml::wav::decode_wav(b"RIFF....NOTWAVE....").is_err());
    let mut bad = wav_bytes(&samples, 48_000);
    bad[20] = 3;
    bad[21] = 0;
    assert!(ml::wav::decode_wav(&bad).is_err());
}
