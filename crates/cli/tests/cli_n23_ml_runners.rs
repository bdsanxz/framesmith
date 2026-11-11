//! Sprint 23 end-to-end: models with consent, stub-driven runners,
//! gates, determinism, and doctor health (T001–T004).
//!
//! Every runner proves against the deterministic stub engine behind
//! the explicit `--engine stub` override with a local registry — no
//! network, no model weights. Refusals name the missing piece and
//! the exact fix; reruns are byte-identical.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n23-ml-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_framesmith"))
}

/// One isolated ML environment: cache, registry with fixture bytes,
/// and matching checksum overrides. Models carry no weights here —
/// the stub proves orchestration, and the manager still verifies
/// every byte before caching.
struct Env {
    root: PathBuf,
}

impl Env {
    fn fresh() -> Self {
        let root = tmp("env");
        std::fs::create_dir_all(root.join("reg")).unwrap();
        for name in ["whisper-tiny-en", "tts-en-aria", "rembg-base"] {
            let bytes = format!("framesmith-e2e-fixture-model-{name}").into_bytes();
            std::fs::write(root.join("reg").join(name), &bytes).unwrap();
        }
        Self { root }
    }

    fn sha(&self, name: &str) -> String {
        let bytes = std::fs::read(self.root.join("reg").join(name)).unwrap();
        ffmpeg_sha256(&bytes)
    }

    fn run(&self, args: &[&str]) -> (i32, String) {
        let mut cmd = Command::new(bin());
        cmd.args(args)
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .env("FRAMESMITH_ML_DIR", self.root.join("cache"))
            .env("FRAMESMITH_ML_REGISTRY_DIR", self.root.join("reg"))
            .env(
                "FRAMESMITH_ML_SHA256_WHISPER_TINY_EN",
                self.sha("whisper-tiny-en"),
            )
            .env("FRAMESMITH_ML_SHA256_TTS_EN_ARIA", self.sha("tts-en-aria"))
            .env("FRAMESMITH_ML_SHA256_REMBG_BASE", self.sha("rembg-base"))
            .env_remove("FRAMESMITH_OFFLINE");
        let out = cmd.output().expect("run framesmith binary");
        let mut text = String::from(String::from_utf8_lossy(&out.stdout));
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        (out.status.code().unwrap_or(-1), text)
    }
}

fn ffmpeg_sha256(data: &[u8]) -> String {
    // Fixture checksums use the same digest the manager verifies.
    // (Computed here without depending on the workspace digest so
    // the test stays an independent check of the cache bytes.)
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for block in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, slot) in w.iter_mut().enumerate().take(16) {
            let o = i * 4;
            *slot = u32::from_be_bytes([block[o], block[o + 1], block[o + 2], block[o + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    let mut out = String::with_capacity(64);
    for v in h {
        out.push_str(&format!("{v:08x}"));
    }
    out
}

fn write_wav(path: &Path, samples: &[f32], rate: u32) {
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + samples.len() * 2) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(samples.len() as u32 * 2).to_le_bytes());
    for s in samples {
        out.extend_from_slice(&(((s.clamp(-1.0, 1.0)) * 32767.0).round() as i16).to_le_bytes());
    }
    std::fs::write(path, &out).unwrap();
}

fn click_track_120() -> Vec<f32> {
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

fn subject_png(path: &Path) {
    let (w, h) = (64u32, 48u32);
    let mut px = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if x >= 20 && x < 44 && y >= 12 && y < 36 {
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

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

#[test]
fn models_list_install_and_consent_gates() {
    let env = Env::fresh();
    let (code, out) = env.run(&["models", "list"]);
    assert_eq!(code, 0, "{out}");
    for name in [
        "whisper-tiny-en",
        "whisper-base-en",
        "parakeet-tdt-0720",
        "tts-en-aria",
        "rembg-fast",
        "rembg-base",
        "rembg-precise",
    ] {
        assert!(out.contains(name), "{out}");
    }
    assert!(out.contains("missing"), "{out}");
    // No consent, non-interactive: refusal, nothing downloads.
    let (code, out) = env.run(&["models", "install", "whisper-tiny-en"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_NO_CONSENT"), "{out}");
    assert!(out.contains("--yes"), "{out}");
    assert!(!env.root.join("cache").exists());
    // Consented install verifies and caches.
    let (code, out) = env.run(&["models", "install", "whisper-tiny-en", "--yes"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("SHA256 verified"), "{out}");
    // Re-install skips with a note.
    let (code, out) = env.run(&["models", "install", "whisper-tiny-en", "--yes"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("skipping"), "{out}");
    // Listing now marks it ready.
    let (code, out) = env.run(&["models", "list"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("whisper-tiny-en") && out.contains("ready"),
        "{out}"
    );
    // Unknown model refuses naming the known set.
    let (code, out) = env.run(&["models", "install", "nope", "--yes"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("unknown model"), "{out}");
}

#[test]
fn transcribe_stub_end_to_end_with_determinism_and_gates() {
    let env = Env::fresh();
    let speech = env.root.join("speech.wav");
    write_wav(&speech, &click_track_120(), 48_000);
    // Missing model refuses with the install fix before any work.
    let (code, out) = env.run(&["transcribe", "speech.wav", "--engine", "stub"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_NO_MODEL"), "{out}");
    assert!(
        out.contains("framesmith models install whisper-tiny-en --yes"),
        "{out}"
    );
    assert!(!env.root.join("speech.wav.transcript.json").exists());
    // Install, then transcribe with the stub.
    assert_eq!(
        env.run(&["models", "install", "whisper-tiny-en", "--yes"])
            .0,
        0
    );
    let (code, out) = env.run(&[
        "transcribe",
        "speech.wav",
        "--engine",
        "stub",
        "--to",
        "srt",
    ]);
    assert_eq!(code, 0, "{out}");
    let json_path = env.root.join("speech.wav.transcript.json");
    let srt_path = env.root.join("speech.wav.transcript.srt");
    assert!(json_path.is_file(), "{out}");
    assert!(srt_path.is_file(), "{out}");
    let body = String::from_utf8(read(&json_path)).unwrap();
    assert!(body.contains("\"cues\""), "{body}");
    assert!(body.contains("\"words\""), "{body}");
    // Reruns are byte-identical.
    let first = read(&json_path);
    let first_srt = read(&srt_path);
    let (code, _) = env.run(&[
        "transcribe",
        "speech.wav",
        "--engine",
        "stub",
        "--to",
        "srt",
    ]);
    assert_eq!(code, 0);
    assert_eq!(read(&json_path), first);
    assert_eq!(read(&srt_path), first_srt);
    // Corrupted cache refuses and is discarded, never used.
    std::fs::write(
        env.root
            .join("cache")
            .join("whisper-tiny-en")
            .join("model.bin"),
        b"corrupt",
    )
    .unwrap();
    let (code, out) = env.run(&["transcribe", "speech.wav", "--engine", "stub"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_NO_MODEL"), "{out}");
}

#[test]
fn tts_stub_and_unknown_voice_gate() {
    let env = Env::fresh();
    assert_eq!(env.run(&["models", "install", "tts-en-aria", "--yes"]).0, 0);
    let (code, out) = env.run(&[
        "tts",
        "hello",
        "framesmith",
        "--engine",
        "stub",
        "-o",
        "voice.wav",
    ]);
    assert_eq!(code, 0, "{out}");
    let first = read(&env.root.join("voice.wav"));
    assert!(first.len() > 44);
    assert_eq!(&first[0..4], b"RIFF");
    let (code, _) = env.run(&[
        "tts",
        "hello",
        "framesmith",
        "--engine",
        "stub",
        "-o",
        "voice.wav",
    ]);
    assert_eq!(code, 0);
    assert_eq!(read(&env.root.join("voice.wav")), first);
    // Unknown voice refuses naming the voice.
    let (code, out) = env.run(&[
        "tts", "hello", "--engine", "stub", "--voice", "nope", "-o", "v2.wav",
    ]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("unknown voice"), "{out}");
    assert!(!env.root.join("v2.wav").exists());
}

#[test]
fn remove_background_stub_and_beats_grid_and_silence() {
    let env = Env::fresh();
    assert_eq!(env.run(&["models", "install", "rembg-base", "--yes"]).0, 0);
    subject_png(&env.root.join("shot.png"));
    let (code, out) = env.run(&[
        "remove-background",
        "shot.png",
        "--engine",
        "stub",
        "-o",
        "bg",
    ]);
    assert_eq!(code, 0, "{out}");
    let alpha = env.root.join("bg").join("subject-alpha.png");
    let plate = env.root.join("bg").join("background-plate.png");
    assert!(alpha.is_file() && plate.is_file(), "{out}");
    let first_a = read(&alpha);
    let first_p = read(&plate);
    let (code, _) = env.run(&[
        "remove-background",
        "shot.png",
        "--engine",
        "stub",
        "-o",
        "bg",
    ]);
    assert_eq!(code, 0);
    assert_eq!(read(&alpha), first_a);
    assert_eq!(read(&plate), first_p);
    // Beats on the click track: 120 BPM grid.
    let click = env.root.join("click.wav");
    write_wav(&click, &click_track_120(), 48_000);
    let (code, out) = env.run(&["beats", "click.wav", "-o", "click.beats.json"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("BPM"), "{out}");
    let body = String::from_utf8(read(&env.root.join("click.beats.json"))).unwrap();
    // File IO quantizes to 16-bit PCM, so the studio click reads
    // 120 BPM within the estimator's tenth-point tolerance.
    let bpm: f64 = body
        .split("\"bpm\":")
        .nth(1)
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.trim().parse().ok())
        .expect("beat file carries numeric bpm");
    assert!((bpm - 120.0).abs() <= 0.2, "{body}");
    assert!(body.contains("\"beats\""), "{body}");
    // Beats on silence: no grid with the reason, no file.
    let silence = env.root.join("silence.wav");
    write_wav(&silence, &vec![0.0f32; 48_000 * 2], 48_000);
    let (code, out) = env.run(&["beats", "silence.wav", "-o", "sil.beats.json"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_NO_GRID"), "{out}");
    assert!(!env.root.join("sil.beats.json").exists());
}

#[test]
fn doctor_reports_ml_health() {
    let env = Env::fresh();
    assert_eq!(
        env.run(&["models", "install", "whisper-tiny-en", "--yes"])
            .0,
        0
    );
    let (code, out) = env.run(&["doctor"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("doctor: ml engine whisper"), "{out}");
    assert!(out.contains("doctor: ml engine stub"), "{out}");
    assert!(
        out.contains("doctor: ml model whisper-tiny-en") && out.contains("ready"),
        "{out}"
    );
    assert!(
        out.contains("doctor: ml model tts-en-aria") && out.contains("missing"),
        "{out}"
    );
    let (code, out) = env.run(&["doctor", "--json"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"ml_engines\""), "{out}");
    assert!(out.contains("\"ml_models\""), "{out}");
    assert!(out.contains("whisper-tiny-en"), "{out}");
}
