//! Beat and BPM analysis (Sprint 23, T004 / CAP-005).
//!
//! `beats <input>` runs a pure-Rust signal analyzer — no model, no
//! download, ever — and emits BPM plus a beat file with beat times
//! snapped to the audio signal and silent stretches marked.
//! Low-confidence or steady-less audio yields no grid with the reason
//! named (no file is written); undecodable audio refuses naming the
//! input.

use crate::config::MlConfig;

/// Beat-file schema version.
pub const BEAT_FILE_VERSION: u32 = 1;

/// Minimum confidence for a grid: below this the audio yields no
/// grid with the reason named.
pub const MIN_CONFIDENCE: f64 = 0.35;

/// Analyzed beat grid.
#[derive(Debug, Clone)]
pub struct BeatGrid {
    /// Beats per minute, quantized to 0.1.
    pub bpm: f64,
    /// Beat times in seconds, snapped to the signal, millisecond
    /// quantized.
    pub beats: Vec<f64>,
    /// Silent stretches as `(start, end)` seconds.
    pub silent: Vec<(f64, f64)>,
    /// Fraction of grid points with a nearby onset.
    pub confidence: f64,
}

fn round_ms(t: f64) -> f64 {
    (t * 1000.0).round() / 1000.0
}

/// Find silent stretches: amplitude under `thresh` for at least
/// `min_len` seconds, merging gaps under 0.1 s.
fn find_silence(samples: &[f32], rate: u32, thresh: f32, min_len: f64) -> Vec<(f64, f64)> {
    let mut ranges = Vec::new();
    let mut start: Option<usize> = None;
    let mut gap = 0usize;
    let gap_max = (rate as f64 * 0.1) as usize;
    for (i, s) in samples.iter().enumerate() {
        if s.abs() < thresh {
            if start.is_none() {
                start = Some(i);
            }
            gap = 0;
        } else if let Some(st) = start {
            gap += 1;
            if gap > gap_max {
                let end = i - gap;
                if (end - st) as f64 / rate as f64 >= min_len {
                    ranges.push((
                        round_ms(st as f64 / rate as f64),
                        round_ms(end as f64 / rate as f64),
                    ));
                }
                start = None;
                gap = 0;
            }
        }
    }
    if let Some(st) = start {
        let end = samples.len().saturating_sub(gap);
        if (end.saturating_sub(st)) as f64 / rate as f64 >= min_len {
            ranges.push((
                round_ms(st as f64 / rate as f64),
                round_ms(end as f64 / rate as f64),
            ));
        }
    }
    ranges
}

/// Onset times in seconds via half-wave-rectified energy flux over
/// 256-sample hops with local-maximum peak picking plus parabolic
/// sub-hop interpolation, so the hop grid cannot pull beat times (or
/// the tempo fit) off by milliseconds.
fn find_onsets(samples: &[f32], rate: u32) -> Vec<f64> {
    const HOP: usize = 256;
    if samples.len() < HOP * 2 {
        return Vec::new();
    }
    let mut energy = Vec::new();
    let mut i = 0;
    while i < samples.len() {
        let end = (i + HOP).min(samples.len());
        let mut acc = 0.0f64;
        for s in &samples[i..end] {
            acc += (*s as f64) * (*s as f64);
        }
        energy.push((acc / (end - i) as f64).sqrt());
        i += HOP;
    }
    let mut flux = vec![0.0f64; energy.len()];
    let mut maxf = 0.0f64;
    for (k, f) in flux.iter_mut().enumerate().skip(1) {
        *f = (energy[k] - energy[k - 1]).max(0.0);
        if *f > maxf {
            maxf = *f;
        }
    }
    if maxf <= 1e-9 {
        return Vec::new();
    }
    let thresh = 0.2 * maxf;
    let min_gap = ((0.08 * rate as f64) / HOP as f64).ceil() as usize;
    let min_gap = min_gap.max(2);
    let mut onsets = Vec::new();
    let mut last: Option<usize> = None;
    for k in 1..flux.len().saturating_sub(1) {
        if flux[k] < thresh {
            continue;
        }
        if flux[k] < flux[k - 1] || flux[k] <= flux[k + 1] {
            continue;
        }
        if let Some(prev) = last {
            if k - prev < min_gap {
                continue;
            }
        }
        last = Some(k);
        // Parabolic interpolation around the flux peak: sub-hop
        // refinement from the two neighbors, clamped to one hop.
        let denom = flux[k - 1] - 2.0 * flux[k] + flux[k + 1];
        let mut off = if denom.abs() > 1e-12 {
            0.5 * (flux[k - 1] - flux[k + 1]) / denom
        } else {
            0.0
        };
        off = off.clamp(-1.0, 1.0);
        onsets.push(round_ms((k as f64 + off) * HOP as f64 / rate as f64));
    }
    onsets
}

/// Estimate the beat period from onset intervals: the longest run of
/// consecutive inter-onset intervals inside 0.25..1.5 s (60..240 BPM),
/// refined by a least-squares grid fit so hop quantization cannot
/// pull the tempo off by a tenth. `None` when too few intervals
/// agree.
fn estimate_period(onsets: &[f64]) -> Option<f64> {
    if onsets.len() < 4 {
        return None;
    }
    // Split into runs of consecutive in-range intervals; keep the
    // longest run (missed or spurious onsets start new runs).
    let mut best: &[f64] = &[];
    let mut run_start = 0;
    for i in 1..onsets.len() {
        let ioi = onsets[i] - onsets[i - 1];
        if !(0.24..=1.5).contains(&ioi) {
            if onsets[run_start..i].len() > best.len() {
                best = &onsets[run_start..i];
            }
            run_start = i;
        }
    }
    if onsets[run_start..].len() > best.len() {
        best = &onsets[run_start..];
    }
    if best.len() < 4 {
        return None;
    }
    // Least-squares slope of time over index: the period.
    let n = best.len() as f64;
    let mean_i = (n - 1.0) / 2.0;
    let mean_t: f64 = best.iter().sum::<f64>() / n;
    let mut num = 0.0;
    let mut den = 0.0;
    for (i, t) in best.iter().enumerate() {
        num += (i as f64 - mean_i) * (t - mean_t);
        den += (i as f64 - mean_i).powi(2);
    }
    if den <= 0.0 {
        return None;
    }
    let period = num / den;
    if (0.24..=1.5).contains(&period) {
        Some(period)
    } else {
        None
    }
}

/// Analyze decoded audio into a beat grid, or refuse with the reason
/// (silence, too few onsets, or low confidence). No model is ever
/// consulted.
pub fn analyze_beats(
    samples: &[f32],
    rate: u32,
    source_name: &str,
) -> Result<BeatGrid, (String, String)> {
    let duration = samples.len() as f64 / rate as f64;
    if duration < 1.0 {
        return Err((
            "E_NO_GRID".to_string(),
            format!(
                "beats refused (E_NO_GRID): {source_name:?} holds only {duration:.2}s of audio, too short to find a grid.\n\
                 fix: pass at least one second of music with a steady beat. No beat file was written."
            ),
        ));
    }
    let onsets = find_onsets(samples, rate);
    let silent = find_silence(samples, rate, 0.02, 0.4);
    let Some(period) = estimate_period(&onsets) else {
        let why = if onsets.len() < 4 {
            "it holds too few onsets to find a steady pulse"
        } else {
            "its onsets agree on no steady pulse"
        };
        return Err((
            "E_NO_GRID".to_string(),
            format!(
                "beats refused (E_NO_GRID): {source_name:?} yields no grid — {why} (low confidence).\n\
                 fix: pass music with a steadier beat; silence and steady drones never invent beats. No beat file was written."
            ),
        ));
    };
    let bpm = ((60.0 / period) * 10.0).round() / 10.0;
    // Grid from the first onset, snapped: each grid point takes the
    // nearest onset within 60 ms, else it stays on the grid.
    let first = onsets[0];
    let mut beats = Vec::new();
    let mut matched = 0usize;
    let mut t = first;
    while t <= duration + 1e-9 {
        let snapped = onsets
            .iter()
            .find(|o| (**o - t).abs() <= 0.06)
            .copied()
            .unwrap_or(t);
        if (snapped - t).abs() <= 0.06 && onsets.iter().any(|o| (*o - t).abs() <= 0.06) {
            matched += 1;
        }
        beats.push(round_ms(snapped.min(duration)));
        t += period;
        if beats.len() > 10_000 {
            break;
        }
    }
    let confidence = if beats.is_empty() {
        0.0
    } else {
        (matched as f64 / beats.len() as f64 * 100.0).round() / 100.0
    };
    if confidence < MIN_CONFIDENCE {
        return Err((
            "E_NO_GRID".to_string(),
            format!(
                "beats refused (E_NO_GRID): {source_name:?} yields no grid — only {matched} of {} grid points sit near an onset (confidence {confidence:.2}, minimum {MIN_CONFIDENCE:.2}).\n\
                 fix: pass music with a steadier beat. No beat file was written.",
                beats.len(),
            ),
        ));
    }
    Ok(BeatGrid {
        bpm,
        beats,
        silent,
        confidence,
    })
}

/// Render a beat grid to the beat-file JSON. All times are
/// millisecond-quantized, so the same input always yields
/// byte-identical bytes.
pub fn format_beat_file(grid: &BeatGrid, source: &str) -> String {
    let mut out = String::new();
    out.push_str("{\"tool\":\"framesmith\",\"kind\":\"beats\",\"version\":");
    out.push_str(&BEAT_FILE_VERSION.to_string());
    out.push_str(&format!(",\"bpm\":{:.1}", grid.bpm));
    out.push_str(",\"beats\":[");
    for (i, b) in grid.beats.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("{b:.3}"));
    }
    out.push_str("],\"silent\":[");
    for (i, (s, e)) in grid.silent.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("[{s:.3},{e:.3}]"));
    }
    out.push_str(&format!(
        "],\"confidence\":{:.2},\"engine\":\"signal\",\"model\":\"none\",\"source\":{}",
        grid.confidence,
        json_string(source)
    ));
    out.push('}');
    out
}

/// Parse a beat file back, fail-closed: the shape above or a refusal
/// naming the cause.
pub fn parse_beat_file(text: &str) -> Result<BeatGrid, String> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("beat file is not JSON ({e})"))?;
    let obj = v
        .as_object()
        .ok_or_else(|| "beat file must hold an object".to_string())?;
    let kind = obj
        .get("kind")
        .and_then(|k| k.as_str())
        .ok_or_else(|| "beat file needs a string `kind`".to_string())?;
    if kind != "beats" {
        return Err(format!("beat file `kind` is {kind:?}, not `\"beats\"`"));
    }
    let bpm = obj
        .get("bpm")
        .and_then(|b| b.as_f64())
        .ok_or_else(|| "beat file needs a numeric `bpm`".to_string())?;
    if !bpm.is_finite() || bpm <= 0.0 {
        return Err("beat file `bpm` must be a positive number".to_string());
    }
    let beats = obj
        .get("beats")
        .and_then(|b| b.as_array())
        .ok_or_else(|| "beat file needs a `beats` array".to_string())?;
    let mut times = Vec::with_capacity(beats.len());
    for (i, b) in beats.iter().enumerate() {
        let t = b
            .as_f64()
            .ok_or_else(|| format!("beat {i} is not a number"))?;
        if !t.is_finite() || t < 0.0 {
            return Err(format!("beat {i} must be a non-negative time"));
        }
        times.push(t);
    }
    let silent = match obj.get("silent").and_then(|s| s.as_array()) {
        Some(list) => {
            let mut ranges = Vec::with_capacity(list.len());
            for (i, r) in list.iter().enumerate() {
                let pair = r
                    .as_array()
                    .ok_or_else(|| format!("silent range {i} must hold `[start, end]`"))?;
                if pair.len() != 2 {
                    return Err(format!("silent range {i} must hold `[start, end]`"));
                }
                let (Some(s), Some(e)) = (pair[0].as_f64(), pair[1].as_f64()) else {
                    return Err(format!("silent range {i} must hold two numbers"));
                };
                if !(s.is_finite() && e.is_finite()) || e <= s || s < 0.0 {
                    return Err(format!("silent range {i} must span a positive time range"));
                }
                ranges.push((s, e));
            }
            ranges
        }
        None => Vec::new(),
    };
    let confidence = obj
        .get("confidence")
        .and_then(|c| c.as_f64())
        .unwrap_or(1.0);
    Ok(BeatGrid {
        bpm,
        beats: times,
        silent,
        confidence,
    })
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Run beat analysis over an audio file. On success returns the beat
/// file path. Onsets come from the signal alone; no model is ever
/// consulted and nothing is downloaded.
pub fn run_beats(
    _cfg: &MlConfig,
    input: &str,
    out: Option<&str>,
) -> Result<String, (String, String)> {
    let in_path = std::path::Path::new(input);
    if !in_path.is_file() {
        return Err((
            "E_BAD_BEATS".to_string(),
            format!(
                "beats refused (E_BAD_BEATS): input {input:?} cannot be read.\n\
                 fix: pass an existing audio file (WAV). No beat file was written."
            ),
        ));
    }
    let wav = crate::wav::read_wav(in_path).map_err(|cause| {
        (
            "E_BAD_BEATS".to_string(),
            format!(
                "beats refused (E_BAD_BEATS): {cause}.\n\
                 fix: pass a readable WAV file. No beat file was written."
            ),
        )
    })?;
    let source = in_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| input.to_string());
    let grid = analyze_beats(&wav.samples, wav.rate, &source)?;
    let body = format_beat_file(&grid, &source);
    let out_path = out
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{input}.beats.json"));
    if let Some(parent) = std::path::Path::new(&out_path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                (
                    "E_BAD_BEATS".to_string(),
                    format!(
                        "beats refused (E_BAD_BEATS): cannot create output dir {}: {e}. No beat file was written.",
                        parent.display()
                    ),
                )
            })?;
        }
    }
    std::fs::write(&out_path, body.as_bytes()).map_err(|e| {
        (
            "E_BAD_BEATS".to_string(),
            format!("beats refused (E_BAD_BEATS): cannot write {out_path:?}: {e}. No beat file was written."),
        )
    })?;
    Ok(out_path)
}
