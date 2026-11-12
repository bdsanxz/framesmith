//! Equivalence probe (Sprint 26, T004 / CAP-004, CAP-009, REC-008):
//! the reassembled file is decoded and compared against the
//! single-pass oracle inside the codec's own noise band —
//! frame-aligned decoded video, sample-aligned decoded audio, and
//! exact counts (frames, duration). The verdict claims only what
//! was measured: within-noise equivalence of a chunked reassembly,
//! never single-pass byte identity. A mismatch beyond the band
//! refuses naming every disagreeing frame, and the file is
//! withheld.

use std::path::Path;
use std::process::Command;

/// The codec's own noise band (Sprint 26, T004 / CAP-004): decoded
/// differences inside these bounds are encoder noise; outside them,
/// the outputs genuinely disagree. Calibrated from measured
/// chunked-vs-single-pass divergence (video max 38/255 on the T001
/// fixture at mean 0.478 / PSNR 44.05 dB, and max 66/255 at mean
/// 0.321 / PSNR 44.2 dB on denser text content; silent audio peaks
/// <=2 both sides). A single pixel at a chunk start spikes because
/// the chunk's first frames encode without lookahead, so the byte
/// max carries content-variance headroom while mean and PSNR gate
/// structural disagreement; tampered or misaligned output still
/// fails by a wide margin.
#[derive(Debug, Clone)]
pub struct NoiseBand {
    /// Largest allowed per-byte video difference (0..=255 scale).
    pub video_max: u8,
    /// Largest allowed mean absolute video difference.
    pub video_mean: f64,
    /// Smallest allowed video PSNR (dB).
    pub video_psnr_min: f64,
    /// Largest allowed per-sample audio difference (i16 scale).
    pub audio_max: i32,
    /// Largest allowed mean absolute audio difference.
    pub audio_mean: f64,
    /// Largest allowed audio length drift, in interleaved samples
    /// (one AAC frame is 1024 samples; 1152 covers priming slack).
    pub max_audio_drift_samples: usize,
    /// Largest allowed container duration drift, in seconds.
    pub max_duration_drift: f64,
}

impl Default for NoiseBand {
    fn default() -> Self {
        NoiseBand {
            video_max: 96,
            video_mean: 2.0,
            video_psnr_min: 35.0,
            audio_max: 64,
            audio_mean: 2.0,
            max_audio_drift_samples: 1152,
            max_duration_drift: 0.05,
        }
    }
}

/// Measured comparison of one candidate against one oracle
/// (Sprint 26, T004 / CAP-004): every statistic the verdict rests
/// on, including the per-frame maxima behind the refusal. Claims
/// stay inside the measurement — `within_noise` means the decoded
/// pictures, audio statistics, and duration agree, never that the
/// container bytes agree.
#[derive(Debug, Clone)]
pub struct ProbeReport {
    /// Picture width in pixels.
    pub width: u32,
    /// Picture height in pixels.
    pub height: u32,
    /// Frames in the candidate.
    pub frames_candidate: u64,
    /// Frames in the oracle.
    pub frames_oracle: u64,
    /// Largest per-byte video difference over the overlapping
    /// frames.
    pub video_max: u8,
    /// Mean absolute video difference over the overlapping frames.
    pub video_mean: f64,
    /// Video PSNR (dB) over the overlapping frames; 99.0 is exact.
    pub video_psnr: f64,
    /// Per-frame maximum difference over the overlapping frames.
    pub video_frame_max: Vec<u8>,
    /// Per-frame mean absolute difference over the overlapping
    /// frames.
    pub video_frame_mean: Vec<f64>,
    /// Samples compared on the shorter audio stream (interleaved
    /// across both channels).
    pub audio_samples: usize,
    /// Largest per-sample audio difference.
    pub audio_max: i32,
    /// Mean absolute audio difference.
    pub audio_mean: f64,
    /// Largest absolute candidate sample.
    pub audio_peak_candidate: i32,
    /// Largest absolute oracle sample.
    pub audio_peak_oracle: i32,
    /// Candidate container duration (seconds).
    pub duration_candidate: f64,
    /// Oracle container duration (seconds).
    pub duration_oracle: f64,
    /// True when every measurement sits inside the noise band.
    pub within_noise: bool,
    /// Frames whose per-frame maximum exceeds the band (plus tail
    /// frames present on one side only), in ascending order.
    pub disagreeing_frames: Vec<u64>,
}

impl ProbeReport {
    /// Frames the report counts: the oracle length the candidate
    /// must match exactly.
    pub fn frames(self) -> u64 {
        self.frames_oracle
    }

    /// One line per measurement axis, for the stage report (pure
    /// text, one line each).
    pub fn measurement_lines(&self) -> Vec<String> {
        let disagree = if self.disagreeing_frames.is_empty() {
            "none".to_string()
        } else if self.disagreeing_frames.len() <= 20 {
            format!("{:?}", self.disagreeing_frames)
        } else {
            format!(
                "{:?} and {} more",
                &self.disagreeing_frames[..20],
                self.disagreeing_frames.len() - 20
            )
        };
        vec![
            format!(
                "probe video: {}x{} frames {}/{} max={}/255 mean={:.3} psnr={:.1}dB disagreeing={disagree}",
                self.width,
                self.height,
                self.frames_candidate,
                self.frames_oracle,
                self.video_max,
                self.video_mean,
                self.video_psnr
            ),
            format!(
                "probe audio: samples={} max={} mean={:.3} peaks={}/{}",
                self.audio_samples,
                self.audio_max,
                self.audio_mean,
                self.audio_peak_candidate,
                self.audio_peak_oracle
            ),
            format!(
                "probe duration: {:.3}s vs {:.3}s drift={:.3}s",
                self.duration_candidate,
                self.duration_oracle,
                (self.duration_candidate - self.duration_oracle).abs()
            ),
        ]
    }

    /// The verdict line: what was measured and what it claims.
    /// A chunked reassembly is equivalent within noise — it is
    /// never a single-pass byte-identity claim.
    pub fn verdict_line(&self, chunked: bool) -> String {
        if self.within_noise {
            if chunked {
                "probe verdict: within-noise equivalence of the reassembled output against the single-pass oracle (chunked reassembly, not single-pass byte identity)".to_string()
            } else {
                "probe verdict: within-noise equivalence against the single-pass oracle (decoded comparison, not byte identity)".to_string()
            }
        } else {
            format!(
                "probe verdict: beyond-noise disagreement against the single-pass oracle; file withheld, nothing promoted"
            )
        }
    }

    /// The refusal for a beyond-noise report (Sprint 26, T004 /
    /// REC-008): names every disagreeing frame and the evidence
    /// behind the disagreement.
    pub fn refusal(&self, candidate: &Path, oracle: &Path) -> String {
        let disagree = if self.disagreeing_frames.is_empty() {
            "none".to_string()
        } else if self.disagreeing_frames.len() <= 20 {
            format!("{:?}", self.disagreeing_frames)
        } else {
            format!(
                "{:?} and {} more",
                &self.disagreeing_frames[..20],
                self.disagreeing_frames.len() - 20
            )
        };
        format!(
            "render refused: {} disagrees with the single-pass oracle {} beyond the codec noise band: disagreeing frames {disagree} (video max={}/255 mean={:.3} psnr={:.1}dB, audio max={} mean={:.3}, frames {}/{}); file withheld, no output promoted",
            candidate.display(),
            oracle.display(),
            self.video_max,
            self.video_mean,
            self.video_psnr,
            self.audio_max,
            self.audio_mean,
            self.frames_candidate,
            self.frames_oracle
        )
    }
}

/// Compare two decoded streams against the band (Sprint 26, T004 /
/// CAP-004): `video` holds raw `rgb24` bytes (`width*height*3` per
/// frame), `audio` holds interleaved stereo s16 samples, both
/// frame/sample-aligned from the start. Pure and deterministic, so
/// the verdict unit-tests without ffmpeg. Length mismatches report
/// as counts disagreement (dropped or doubled frames, audio
/// drift) — never silently truncated away.
pub fn compare_decoded(
    video_candidate: &[u8],
    audio_candidate: &[i16],
    video_oracle: &[u8],
    audio_oracle: &[i16],
    width: u32,
    height: u32,
    duration_candidate: f64,
    duration_oracle: f64,
    band: &NoiseBand,
) -> Result<ProbeReport, String> {
    let frame_bytes = width as usize * height as usize * 3;
    if frame_bytes == 0 {
        return Err(
            "probe refused: cannot compare decoded video at 0x0; no output was promoted"
                .to_string(),
        );
    }
    if video_candidate.len() % frame_bytes != 0 {
        return Err(format!(
            "probe refused: candidate decodes to {} byte(s), not a whole number of {}x{} rgb24 frames; no output was promoted",
            video_candidate.len(),
            width,
            height
        ));
    }
    if video_oracle.len() % frame_bytes != 0 {
        return Err(format!(
            "probe refused: oracle decodes to {} byte(s), not a whole number of {}x{} rgb24 frames; no output was promoted",
            video_oracle.len(),
            width,
            height
        ));
    }
    let frames_candidate = (video_candidate.len() / frame_bytes) as u64;
    let frames_oracle = (video_oracle.len() / frame_bytes) as u64;
    let frames = frames_candidate.min(frames_oracle);
    let overlap = frames as usize * frame_bytes;
    let mut video_max = 0u8;
    let mut sum = 0u64;
    let mut sq = 0u64;
    let mut frame_max: Vec<u8> = Vec::with_capacity(frames as usize);
    let mut frame_mean: Vec<f64> = Vec::with_capacity(frames as usize);
    let mut disagreeing: Vec<u64> = Vec::new();
    for f in 0..frames as usize {
        let start = f * frame_bytes;
        let mut local_max = 0u8;
        let mut local_sum = 0u64;
        for (x, y) in video_candidate[start..start + frame_bytes]
            .iter()
            .zip(&video_oracle[start..start + frame_bytes])
        {
            let d = x.abs_diff(*y);
            local_max = local_max.max(d);
            local_sum += d as u64;
            video_max = video_max.max(d);
            sum += d as u64;
            sq += d as u64 * d as u64;
        }
        let local_mean = local_sum as f64 / frame_bytes as f64;
        if local_max > band.video_max || local_mean > band.video_mean {
            disagreeing.push(f as u64);
        }
        frame_max.push(local_max);
        frame_mean.push(local_mean);
    }
    let n = overlap as f64;
    let mean = if n > 0.0 { sum as f64 / n } else { 0.0 };
    let mse = if n > 0.0 { sq as f64 / n } else { 0.0 };
    let psnr = if mse == 0.0 {
        99.0
    } else {
        10.0 * (255.0 * 255.0 / mse).log10()
    };
    // Dropped or doubled frames: the tail present on one side only.
    for f in frames..frames_candidate.max(frames_oracle) {
        disagreeing.push(f);
    }
    let audio_len = audio_candidate.len().min(audio_oracle.len());
    let mut audio_max = 0i32;
    let mut audio_sum = 0u64;
    for i in 0..audio_len {
        let d = (audio_candidate[i] as i32 - audio_oracle[i] as i32).abs();
        audio_max = audio_max.max(d);
        audio_sum += d as u64;
    }
    let audio_mean = if audio_len > 0 {
        audio_sum as f64 / audio_len as f64
    } else {
        0.0
    };
    let peak_a = audio_candidate
        .iter()
        .map(|x| (*x as i32).abs())
        .max()
        .unwrap_or(0);
    let peak_b = audio_oracle.iter().map(|x| (*x as i32).abs()).max().unwrap_or(0);
    let drift = audio_candidate
        .len()
        .abs_diff(audio_oracle.len());
    let video_ok = video_max <= band.video_max
        && mean <= band.video_mean
        && psnr >= band.video_psnr_min
        && disagreeing.is_empty();
    let audio_ok = audio_max <= band.audio_max
        && audio_mean <= band.audio_mean
        && drift <= band.max_audio_drift_samples;
    let duration_ok =
        (duration_candidate - duration_oracle).abs() <= band.max_duration_drift;
    Ok(ProbeReport {
        width,
        height,
        frames_candidate,
        frames_oracle,
        video_max,
        video_mean: mean,
        video_psnr: psnr,
        video_frame_max: frame_max,
        video_frame_mean: frame_mean,
        audio_samples: audio_len,
        audio_max,
        audio_mean,
        audio_peak_candidate: peak_a,
        audio_peak_oracle: peak_b,
        duration_candidate,
        duration_oracle,
        within_noise: video_ok && audio_ok && duration_ok,
        disagreeing_frames: disagreeing,
    })
}

/// True when a container comment is the chunked stamp
/// ([`crate::outputs::Provenance::chunked_stamp`]): the file names
/// chunking, never single-pass byte identity.
pub fn is_chunked_stamp(comment: &str) -> bool {
    comment.contains("render=chunked")
}

/// The `comment` tag of one container (Sprint 26, T004 / CAP-004):
/// the stamp every probe reads back through the `ffprobe` twin
/// beside the resolved ffmpeg binary (never a PATH guess beyond
/// the established sibling lookup). `None` when no comment reads
/// back or no ffprobe sits beside the binary.
pub fn container_comment(ffmpeg_bin: &Path, path: &Path) -> Option<String> {
    let ffprobe = crate::render::ffprobe_for(ffmpeg_bin)?;
    let out = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format_tags=comment",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

struct StreamFacts {
    width: u32,
    height: u32,
    duration: f64,
    has_audio: bool,
}

/// Container facts of one file from `ffmpeg -i` stderr plus the
/// `ffprobe` duration fallback (Sprint 26, T004): picture size,
/// duration, and whether an audio stream exists. Names the file
/// on failure instead of guessing.
fn stream_facts(ffmpeg_bin: &Path, path: &Path) -> Result<StreamFacts, String> {
    if !path.is_file() {
        return Err(format!(
            "probe refused: {} does not exist; no output was promoted",
            path.display()
        ));
    }
    let out = Command::new(ffmpeg_bin)
        .args(["-hide_banner", "-i", &path.display().to_string()])
        .output()
        .map_err(|e| {
            format!(
                "probe refused: cannot probe {} ({e}); no output was promoted",
                path.display()
            )
        })?;
    let text = String::from_utf8_lossy(&out.stderr);
    if !text.contains("Input #0") {
        return Err(format!(
            "probe refused: {} is unreadable (ffmpeg reports no input stream); no output was promoted",
            path.display()
        ));
    }
    let mut video: Option<(u32, u32)> = None;
    let mut has_audio = false;
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with("Stream #") {
            continue;
        }
        if let Some((_, w, h)) = crate::render::parse_video_stream(line) {
            if video.is_none() {
                video = Some((w, h));
            }
        }
        if line.contains("Audio:") {
            has_audio = true;
        }
    }
    let (width, height) = video.ok_or_else(|| {
        format!(
            "probe refused: {} has no decodable video stream; no output was promoted",
            path.display()
        )
    })?;
    let duration = crate::render::parse_duration(&text)
        .or_else(|| crate::render::probe_format_duration(ffmpeg_bin, path))
        .ok_or_else(|| {
            format!(
                "probe refused: {} reports no duration; no output was promoted",
                path.display()
            )
        })?;
    Ok(StreamFacts {
        width,
        height,
        duration,
        has_audio,
    })
}

/// Decode one file's video to raw `rgb24` bytes (frame-aligned
/// like the drawn frames). Names the file on failure.
fn decode_video(ffmpeg_bin: &Path, path: &Path) -> Result<Vec<u8>, String> {
    let out = Command::new(ffmpeg_bin)
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &path.display().to_string(),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .map_err(|e| {
            format!(
                "probe refused: cannot decode the video of {} ({e}); no output was promoted",
                path.display()
            )
        })?;
    if !out.status.success() {
        let tail = crate::render::stderr_tail(&out.stderr, 3);
        return Err(format!(
            "probe refused: the video of {} is undecodable{}; no output was promoted",
            path.display(),
            if tail.is_empty() {
                String::new()
            } else {
                format!(" ({tail})")
            }
        ));
    }
    Ok(out.stdout)
}

/// Decode one file's audio to interleaved stereo s16 samples at
/// 48 kHz (sample-aligned like the staged mix). An empty decode
/// when the container carries no audio.
fn decode_audio(ffmpeg_bin: &Path, path: &Path, has_audio: bool) -> Result<Vec<i16>, String> {
    if !has_audio {
        return Ok(Vec::new());
    }
    let out = Command::new(ffmpeg_bin)
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &path.display().to_string(),
            "-f",
            "s16le",
            "-ac",
            "2",
            "-ar",
            "48000",
            "-",
        ])
        .output()
        .map_err(|e| {
            format!(
                "probe refused: cannot decode the audio of {} ({e}); no output was promoted",
                path.display()
            )
        })?;
    if !out.status.success() {
        let tail = crate::render::stderr_tail(&out.stderr, 3);
        return Err(format!(
            "probe refused: the audio of {} is undecodable{}; no output was promoted",
            path.display(),
            if tail.is_empty() {
                String::new()
            } else {
                format!(" ({tail})")
            }
        ));
    }
    Ok(out
        .stdout
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect())
}

/// Probe `candidate` against the single-pass `oracle` (Sprint 26,
/// T004 / CAP-004): decode both with the resolved registry
/// ffmpeg binary and compare inside the noise band. The report
/// carries the measurement and the verdict; internal failures
/// (undecodable files, mismatched picture sizes, unknown
/// durations) refuse naming the file.
pub fn probe_files(
    ffmpeg_bin: &Path,
    candidate: &Path,
    oracle: &Path,
    band: &NoiseBand,
) -> Result<ProbeReport, String> {
    let cand = stream_facts(ffmpeg_bin, candidate)?;
    let orac = stream_facts(ffmpeg_bin, oracle)?;
    if cand.width != orac.width || cand.height != orac.height {
        return Err(format!(
            "probe refused: {} is {}x{} but the oracle {} is {}x{}: picture sizes must match; no output was promoted",
            candidate.display(),
            cand.width,
            cand.height,
            oracle.display(),
            orac.width,
            orac.height
        ));
    }
    let video_candidate = decode_video(ffmpeg_bin, candidate)?;
    let video_oracle = decode_video(ffmpeg_bin, oracle)?;
    let audio_candidate = decode_audio(ffmpeg_bin, candidate, cand.has_audio)?;
    let audio_oracle = decode_audio(ffmpeg_bin, oracle, orac.has_audio)?;
    compare_decoded(
        &video_candidate,
        &audio_candidate,
        &video_oracle,
        &audio_oracle,
        cand.width,
        cand.height,
        cand.duration,
        orac.duration,
        band,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn band() -> NoiseBand {
        NoiseBand::default()
    }

    fn frames(count: usize, frame_bytes: usize, fill: u8) -> Vec<u8> {
        vec![fill; count * frame_bytes]
    }

    #[test]
    fn identical_streams_pass_with_exact_stats() {
        let fb = 4 * 3 * 3;
        let video = frames(5, fb, 100);
        let audio = vec![1000i16; 480];
        let report = compare_decoded(
            &video,
            &audio,
            &video,
            &audio,
            4,
            3,
            1.0,
            1.0,
            &band(),
        )
        .expect("identical decodes compare");
        assert!(report.within_noise);
        assert_eq!(report.frames_candidate, 5);
        assert_eq!(report.video_max, 0);
        assert_eq!(report.video_psnr, 99.0);
        assert!(report.disagreeing_frames.is_empty());
    }

    #[test]
    fn codec_noise_inside_the_band_passes() {
        // Sparse per-byte shifts of 5: max inside the 64 ceiling,
        // mean inside the 2.0 ceiling, PSNR far above 35 dB.
        let fb = 4 * 3 * 3;
        let oracle = frames(5, fb, 100);
        let mut candidate = oracle.clone();
        for (i, b) in candidate.iter_mut().enumerate() {
            if i % 4 == 0 {
                *b = b.saturating_add(5);
            }
        }
        let report = compare_decoded(
            &candidate,
            &[],
            &oracle,
            &[],
            4,
            3,
            1.0,
            1.0,
            &band(),
        )
        .expect("noisy decodes compare");
        assert!(report.within_noise);
        assert_eq!(report.video_max, 5);
        assert!(report.video_mean <= 2.0, "mean {}", report.video_mean);
        assert!(report.video_psnr >= 35.0, "psnr {}", report.video_psnr);
    }

    #[test]
    fn tampered_frame_fails_naming_that_frame() {
        let fb = 4 * 3 * 3;
        let oracle = frames(5, fb, 100);
        let mut candidate = oracle.clone();
        // Invert frame 2: every byte of it moves by 55 — inside the
        // 64 max ceiling alone, but the per-frame mean of 55 is far
        // beyond the 2.0 mean ceiling, so the frame disagrees.
        for b in &mut candidate[2 * fb..3 * fb] {
            *b = 255 - *b;
        }
        let report = compare_decoded(
            &candidate,
            &[],
            &oracle,
            &[],
            4,
            3,
            1.0,
            1.0,
            &band(),
        )
        .expect("tampered decodes compare");
        assert!(!report.within_noise);
        assert_eq!(report.disagreeing_frames, vec![2]);
    }

    #[test]
    fn dropped_frame_fails_naming_the_tail() {
        let fb = 4 * 3 * 3;
        let oracle = frames(5, fb, 100);
        let candidate = frames(4, fb, 100);
        let report = compare_decoded(
            &candidate,
            &[],
            &oracle,
            &[],
            4,
            3,
            1.0,
            1.0,
            &band(),
        )
        .expect("misaligned decodes compare");
        assert!(!report.within_noise);
        assert_eq!(report.frames_candidate, 4);
        assert_eq!(report.frames_oracle, 5);
        assert_eq!(report.disagreeing_frames, vec![4]);
    }

    #[test]
    fn partial_frame_bytes_refuse() {
        let fb = 4 * 3 * 3;
        let oracle = frames(5, fb, 100);
        let mut candidate = frames(5, fb, 100);
        candidate.pop();
        let e = compare_decoded(
            &candidate,
            &[],
            &oracle,
            &[],
            4,
            3,
            1.0,
            1.0,
            &band(),
        )
        .unwrap_err();
        assert!(e.contains("not a whole number"), "{e}");
    }

    #[test]
    fn audio_divergence_and_drift_fail() {
        let fb = 4 * 3 * 3;
        let video = frames(5, fb, 100);
        let oracle_audio = vec![0i16; 480];
        // Loud divergence on the candidate.
        let candidate_audio = vec![5000i16; 480];
        let report = compare_decoded(
            &video,
            &candidate_audio,
            &video,
            &oracle_audio,
            4,
            3,
            1.0,
            1.0,
            &band(),
        )
        .expect("audio decodes compare");
        assert!(!report.within_noise);
        assert_eq!(report.audio_max, 5000);
        // Length drift beyond the band.
        let longer = vec![0i16; 480 + 2000];
        let report = compare_decoded(
            &video,
            &longer,
            &video,
            &oracle_audio,
            4,
            3,
            1.0,
            1.0,
            &band(),
        )
        .expect("drifted audio compares");
        assert!(!report.within_noise);
        assert_eq!(report.audio_samples, 480);
    }

    #[test]
    fn duration_drift_fails() {
        let fb = 4 * 3 * 3;
        let video = frames(5, fb, 100);
        let report = compare_decoded(
            &video,
            &[],
            &video,
            &[],
            4,
            3,
            1.0,
            1.5,
            &band(),
        )
        .expect("duration decodes compare");
        assert!(!report.within_noise);
    }

    #[test]
    fn chunked_stamp_names_chunking_only() {
        let stamp = "framesmith 0.1.0 inputs=abc123 render=chunked chunks=2 frames=30";
        assert!(is_chunked_stamp(stamp));
        assert!(!is_chunked_stamp("framesmith 0.1.0 inputs=abc123"));
    }
}
