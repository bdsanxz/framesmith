//! Deterministic stub engines (Sprint 23, T001 / CAP-006).
//!
//! Fixed outputs prove orchestration, parsing, gates, and refusals
//! without any network or model weights. Stubs activate only behind
//! the explicit `--engine stub` override (never through `auto`), and
//! the hidden `__ml-stub` re-exec entry requires the
//! `FRAMESMITH_ML_STUB_ACTIVE=1` marker the runner sets when spawning
//! it — direct invocation without the marker refuses.

use crate::config::STUB_ACTIVE_ENV;

/// True when this process is the runner-spawned stub re-exec.
pub fn stub_marker_present() -> bool {
    std::env::var(STUB_ACTIVE_ENV)
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Spawn the deterministic stub for `runner` (`transcribe`, `tts`,
/// `segment`) as a separate process: the current executable
/// re-invoked with the hidden `__ml-stub` verb. Mirrors the
/// separate-process engine shape, so stub runs prove the same
/// orchestration real engines use.
pub fn spawn_stub(runner: &str, args: &[String]) -> Result<Vec<u8>, String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("stub engine could not find its own binary: {e}"))?;
    let mut cmd_args = vec!["__ml-stub".to_string(), runner.to_string()];
    cmd_args.extend_from_slice(args);
    let out = std::process::Command::new(&exe)
        .args(&cmd_args)
        .env(STUB_ACTIVE_ENV, "1")
        .output()
        .map_err(|e| format!("stub engine could not launch: {e}"))?;
    if !out.status.success() {
        let mut detail = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if detail.is_empty() {
            detail = format!("stub exited with status {}", out.status);
        }
        return Err(format!("stub engine failed: {detail}"));
    }
    Ok(out.stdout)
}

/// Fixed vocabulary the transcribe stub cycles through. Deterministic
/// and dependency-free.
const STUB_WORDS: &[&str] = &[
    "lights",
    "camera",
    "framesmith",
    "action",
    "rolling",
    "sound",
    "marker",
    "take",
    "scene",
    "quiet",
    "set",
    "rolling",
];

/// Build the stub transcript for `duration` seconds of audio: two
/// words per second from the fixed vocabulary, grouped four words
/// per cue. A pure function of the duration (and therefore of the
/// input bytes), so reruns are byte-identical.
pub fn stub_transcript_cues(duration: f64) -> Vec<notation::model::CaptionCue> {
    let total = (duration.max(0.5) * 2.0).round() as usize;
    let total = total.clamp(4, 200);
    let mut cues = Vec::new();
    let mut t: f64 = 0.0;
    let mut idx = 0;
    while idx < total {
        let take = (total - idx).min(4);
        let start = t;
        let mut words = Vec::new();
        for _ in 0..take {
            let text = STUB_WORDS[idx % STUB_WORDS.len()].to_string();
            let wstart = (t * 1000.0).round() / 1000.0;
            t += 0.5;
            let wend = (t * 1000.0).round() / 1000.0;
            words.push(notation::model::CaptionWord {
                text,
                start: wstart,
                end: wend,
            });
            idx += 1;
        }
        cues.push(notation::model::CaptionCue {
            start: (start * 1000.0).round() / 1000.0,
            end: (t * 1000.0).round() / 1000.0,
            words,
            line: 0,
            column: 1,
        });
    }
    cues
}

/// Render the stub transcript to the Sprint 22 words JSON shape via
/// the same formatter captions import, so import drifts by zero by
/// construction.
pub fn stub_transcript_json(duration: f64) -> String {
    notation::captions::format_transcript_json(&stub_transcript_cues(duration))
}

/// Voice base frequencies in Hz, in [`crate::catalog::KNOWN_VOICES`]
/// order. Unknown voices never reach synthesis (the runner refuses
/// naming them first).
const VOICE_BASE_HZ: &[f64] = &[220.0, 174.61, 261.63, 196.0];

/// Synthesize the stub voice: one stepped sine blip per character at
/// 48 kHz mono, pitch from the voice, pace from the speed. A pure
/// function of `(text, voice, speed)`, so reruns are byte-identical.
pub fn stub_tts_samples(text: &str, voice: &str, speed: f64) -> Vec<f32> {
    const RATE: f64 = 48_000.0;
    let voice_idx = crate::catalog::KNOWN_VOICES
        .iter()
        .position(|v| *v == voice)
        .unwrap_or(0);
    let base = VOICE_BASE_HZ[voice_idx % VOICE_BASE_HZ.len()];
    let speed = if speed.is_finite() && speed > 0.0 {
        speed.clamp(0.25, 4.0)
    } else {
        1.0
    };
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }
    let per_char = (0.09 / speed * RATE).round() as usize;
    let per_char = per_char.max(24);
    let mut out = Vec::with_capacity(chars.len() * per_char);
    for (i, ch) in chars.iter().enumerate() {
        let semis = (*ch as u32 % 12) as f64;
        let freq = base * 2f64.powf(semis / 12.0);
        for n in 0..per_char {
            let t = (i * per_char + n) as f64 / RATE;
            // Raised-cosine envelope per character: no clicks, and the
            // boundary shape is a pure function of the position.
            let phase = n as f64 / per_char as f64;
            let env = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * phase).cos();
            let v = (2.0 * std::f64::consts::PI * freq * t).sin() * env * 0.5;
            out.push(v as f32);
        }
    }
    out
}

/// Subject mask for the stub segmenter: a centered ellipse covering
/// the middle half of the frame, feathered over two pixels. A pure
/// function of `(width, height)`.
pub fn stub_alpha(width: u32, height: u32) -> Vec<u8> {
    let cx = width as f64 / 2.0;
    let cy = height as f64 / 2.0;
    let rx = width as f64 / 4.0;
    let ry = height as f64 / 4.0;
    let mut alpha = vec![0u8; (width * height) as usize];
    for y in 0..height {
        for x in 0..width {
            let dx = (x as f64 + 0.5 - cx) / rx;
            let dy = (y as f64 + 0.5 - cy) / ry;
            let d = (dx * dx + dy * dy).sqrt();
            // Feather over ~2px in normalized units.
            let feather = 2.0 / rx.min(ry).max(1.0);
            let a = if d <= 1.0 - feather {
                255.0
            } else if d >= 1.0 {
                0.0
            } else {
                255.0 * (1.0 - (d - (1.0 - feather)) / feather)
            };
            alpha[(y * width + x) as usize] = a.round().clamp(0.0, 255.0) as u8;
        }
    }
    alpha
}

/// Hidden `__ml-stub` entry point (wired by the CLI, never listed in
/// usage). Requires the runner-set marker; refuses otherwise so the
/// stub can never activate implicitly.
pub fn run_stub_verb(args: &[String]) -> i32 {
    if !stub_marker_present() {
        eprintln!(
            "framesmith __ml-stub refused: the stub engine activates only behind the explicit `--engine stub` override.\n\
             fix: run a runner (`transcribe`, `tts`, or `remove-background`) with `--engine stub`; never invoke `__ml-stub` directly."
        );
        return 1;
    }
    if args.is_empty() {
        eprintln!("framesmith __ml-stub refused: expected a runner name (`transcribe`, `tts`, or `segment`).");
        return 2;
    }
    match args[0].as_str() {
        "transcribe" => stub_verb_transcribe(&args[1..]),
        "tts" => stub_verb_tts(&args[1..]),
        "segment" => stub_verb_segment(&args[1..]),
        other => {
            eprintln!("framesmith __ml-stub refused: unknown stub runner {other:?}.");
            2
        }
    }
}

fn get_flag(args: &[String], flag: &str) -> Option<String> {
    let mut i = 0;
    while i < args.len() {
        if args[i] == flag && i + 1 < args.len() {
            return Some(args[i + 1].clone());
        }
        i += 1;
    }
    None
}

fn stub_verb_transcribe(args: &[String]) -> i32 {
    let (Some(input), Some(_model)) = (get_flag(args, "--input"), get_flag(args, "--model")) else {
        eprintln!("framesmith __ml-stub transcribe refused: needs `--input` and `--model`.");
        return 2;
    };
    let wav = match crate::wav::read_wav(std::path::Path::new(&input)) {
        Ok(w) => w,
        Err(cause) => {
            eprintln!("framesmith __ml-stub transcribe refused: {cause}.");
            return 1;
        }
    };
    print!("{}", stub_transcript_json(wav.duration));
    0
}

fn stub_verb_tts(args: &[String]) -> i32 {
    let (Some(text), Some(voice), Some(out)) = (
        get_flag(args, "--text"),
        get_flag(args, "--voice"),
        get_flag(args, "--out"),
    ) else {
        eprintln!("framesmith __ml-stub tts refused: needs `--text`, `--voice`, and `--out`.");
        return 2;
    };
    let speed: f64 = get_flag(args, "--speed")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.0);
    let samples = stub_tts_samples(&text, &voice, speed);
    match crate::wav::write_wav_file(std::path::Path::new(&out), &samples, 48_000) {
        Ok(()) => 0,
        Err(cause) => {
            eprintln!("framesmith __ml-stub tts refused: {cause}.");
            1
        }
    }
}

fn stub_verb_segment(args: &[String]) -> i32 {
    let (Some(input), Some(alpha_out), Some(plate_out)) = (
        get_flag(args, "--input"),
        get_flag(args, "--alpha-out"),
        get_flag(args, "--plate-out"),
    ) else {
        eprintln!(
            "framesmith __ml-stub segment refused: needs `--input`, `--alpha-out`, and `--plate-out`."
        );
        return 2;
    };
    match crate::matting::stub_segment_files(&input, &alpha_out, &plate_out) {
        Ok(()) => 0,
        Err(cause) => {
            eprintln!("framesmith __ml-stub segment refused: {cause}.");
            1
        }
    }
}
