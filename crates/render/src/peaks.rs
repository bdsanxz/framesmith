//! Peak analysis for the Framesmith 48 kHz stereo mix (Sprint 10
//! sound, CAP-008/CAP-011).
//!
//! Every measurement here is pure sample math over interleaved s16
//! frames, so `check`, the render summary, and `waveform` name the
//! same peaks for the same bytes: the master peak, one peak per
//! audible clip window (sections), and per-bucket peaks for the
//! waveform listing. Levels print in dBFS; digital silence (`0`) has
//! no finite dBFS, so it reports as `None` and prints `-inf dBFS`.

use notation::model::{Clip, Composition};

/// Mix sample rate the frame math below assumes (matches
/// [`crate::render::MIX_RATE`]).
pub const PEAK_RATE: f64 = 48_000.0;

/// Master-peak heat ceiling (Sprint 10 sound, CAP-011/REC-005): a
/// master above −1 dBFS warns but still renders.
pub const HOT_MASTER_DBFS: f64 = -1.0;

/// Largest absolute sample across both channels (`0` for an empty
/// mix). Both channels count: a hot right channel alone still warns.
pub fn peak_of(pcm: &[i16]) -> u32 {
    pcm.iter()
        .map(|s| s.unsigned_abs() as u32)
        .max()
        .unwrap_or(0)
}

/// Largest absolute sample in the half-open frame window
/// `[from, to)`, clamped to the mix. An empty or backward window
/// measures `0` instead of failing.
pub fn peak_in_frames(mix: &[i16], from: usize, to: usize) -> u32 {
    const CHANNELS: usize = 2;
    let frames = mix.len() / CHANNELS;
    let from = from.min(frames);
    let to = to.min(frames);
    if to <= from {
        return 0;
    }
    peak_of(&mix[from * CHANNELS..to * CHANNELS])
}

/// Largest absolute sample in the half-open composition-second window
/// `[start, end)`, quantized exactly like mix placement (round to the
/// nearest sample). An empty or backward window measures `0`.
pub fn peak_in_seconds(mix: &[i16], start: f64, end: f64) -> u32 {
    if !start.is_finite() || !end.is_finite() || end <= start {
        return 0;
    }
    let from = (start * PEAK_RATE).round().max(0.0) as usize;
    let to = (end * PEAK_RATE).round().max(0.0) as usize;
    peak_in_frames(mix, from, to)
}

/// dBFS of one linear peak (`peak / 32768`): `None` for digital
/// silence, which has no finite level.
pub fn dbfs_of(peak: u32) -> Option<f64> {
    if peak == 0 {
        None
    } else {
        Some(20.0 * (peak as f64 / 32768.0).log10())
    }
}

/// Whether one linear peak exceeds the hot-master ceiling: strictly
/// above −1 dBFS, so a master sitting exactly on −1 dBFS stays quiet.
pub fn is_hot(peak: u32) -> bool {
    dbfs_of(peak)
        .map(|db| db > HOT_MASTER_DBFS)
        .unwrap_or(false)
}

/// Linear amplitude of one peak as a fraction of full scale
/// (`peak / 32768`), for machine-readable reports.
pub fn linear_of(peak: u32) -> f64 {
    peak as f64 / 32768.0
}

/// Human level for one linear peak: `-3.2 dBFS`, or `-inf dBFS
/// (digital silence)` for `0`.
pub fn format_dbfs(peak: u32) -> String {
    match dbfs_of(peak) {
        Some(db) => format!("{db:.1} dBFS"),
        None => "-inf dBFS (digital silence)".to_string(),
    }
}

/// One audible timeline section (Sprint 10 sound, CAP-008): an audio,
/// video, or nest clip window clamped to the composition, in document
/// order. Hidden clips are absent from the mix and report nothing;
/// soundless video still counts (its window carries the mix playing
/// under it), so accidents under a mute picture stay visible.
#[derive(Debug, Clone)]
pub struct SoundSection {
    /// Clip kind (`audio`, `video`, or `nest`), for the label.
    pub kind: &'static str,
    /// The clip's `id`, when declared.
    pub id: Option<String>,
    /// Document index of the clip, for `#n` labels.
    pub index: usize,
    /// Audible window in composition seconds.
    pub start: f64,
    pub end: f64,
}

impl SoundSection {
    /// Human label for listings: `audio "vox"`, or `video #3`.
    pub fn label(&self) -> String {
        match &self.id {
            Some(id) => format!("{} {id:?}", self.kind),
            None => format!("{} #{}", self.kind, self.index),
        }
    }
}

/// Every audible section of one composition (Sprint 10 sound,
/// CAP-008): audio clips, video clips, and nest uses with a non-empty
/// window clamped to the composition, in document order. Hidden clips
/// contribute nothing and are skipped. Pure model math, so `check`,
/// the render summary, and `waveform` slice the same windows.
pub fn mix_sections(comp: &Composition) -> Vec<SoundSection> {
    let mut out = Vec::new();
    for (idx, clip) in comp.clips.iter().enumerate() {
        if clip.hidden() {
            continue;
        }
        let kind = match clip {
            Clip::Audio(_) => "audio",
            Clip::Video(_) => "video",
            Clip::Nest(_) => "nest",
            _ => continue,
        };
        let end = (clip.start() + clip.duration()).min(comp.duration);
        if !end.is_finite() || end <= clip.start() || clip.start() >= comp.duration {
            continue;
        }
        out.push(SoundSection {
            kind,
            id: clip.id().map(str::to_string),
            index: idx,
            start: clip.start(),
            end,
        });
    }
    out
}

/// Peak of the final mix inside every section window.
pub fn section_peaks(mix: &[i16], sections: &[SoundSection]) -> Vec<u32> {
    sections
        .iter()
        .map(|s| peak_in_seconds(mix, s.start, s.end))
        .collect()
}

/// Per-bucket peaks of the whole mix (Sprint 10 sound, CAP-008):
/// bucket `i` of `n` spans frames `[i*F/n, (i+1)*F/n)` for `F` mix
/// frames, so every sample lands in exactly one bucket and the last
/// bucket ends exactly at the mix end. An empty mix (or zero buckets)
/// reports all zeros instead of failing; callers refuse `n < 1`
/// before calling.
pub fn bucket_peaks(mix: &[i16], buckets: usize) -> Vec<u32> {
    const CHANNELS: usize = 2;
    let frames = mix.len() / CHANNELS;
    if buckets == 0 || frames == 0 {
        return vec![0; buckets];
    }
    (0..buckets)
        .map(|i| {
            let from = (i * frames) / buckets;
            let to = ((i + 1) * frames) / buckets;
            peak_in_frames(mix, from, to)
        })
        .collect()
}

/// Composition-second window of bucket `i` of `n` over a mix of
/// `frames` frames: the same split [`bucket_peaks`] measures.
pub fn bucket_window(frames: usize, buckets: usize, i: usize) -> (f64, f64) {
    if buckets == 0 || frames == 0 {
        return (0.0, 0.0);
    }
    let from = (i * frames) / buckets;
    let to = ((i + 1) * frames) / buckets;
    (from as f64 / PEAK_RATE, to as f64 / PEAK_RATE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine_pcm(frames: usize, amplitude: i16) -> Vec<i16> {
        (0..frames)
            .flat_map(|i| {
                let s = ((i as f64 * 440.0 * 2.0 * std::f64::consts::PI / 48000.0).sin()
                    * amplitude as f64) as i16;
                [s, s]
            })
            .collect()
    }

    #[test]
    fn peak_counts_both_channels() {
        assert_eq!(peak_of(&[]), 0);
        assert_eq!(peak_of(&[0, 0, 0, 0]), 0);
        assert_eq!(peak_of(&[100, -5000, 300, 200]), 5000);
        assert_eq!(peak_of(&[i16::MIN, 0]), 32768);
    }

    #[test]
    fn dbfs_matches_hand_math() {
        assert_eq!(dbfs_of(0), None);
        let full = dbfs_of(32767).unwrap();
        assert!((full + 0.0).abs() < 0.01, "full scale is ~0 dBFS: {full}");
        let half = dbfs_of(16384).unwrap();
        assert!(
            (half + 6.02).abs() < 0.01,
            "half scale is -6.02 dBFS: {half}"
        );
        let tenth = dbfs_of(3277).unwrap();
        assert!(
            (tenth + 20.0).abs() < 0.05,
            "tenth scale is -20 dBFS: {tenth}"
        );
    }

    #[test]
    fn hot_boundary_sits_strictly_above_minus_one() {
        // 32768 * 10^(-1/20) = 29204.26...: 29204 stays quiet, 29205 warns.
        assert!(!is_hot(29204), "exactly -1 dBFS stays quiet");
        assert!(is_hot(29205), "above -1 dBFS warns");
        assert!(!is_hot(0), "silence never warns");
        assert!(is_hot(32767), "full scale warns");
    }

    #[test]
    fn silence_formats_without_finite_level() {
        assert_eq!(format_dbfs(0), "-inf dBFS (digital silence)");
        assert_eq!(format_dbfs(16384), "-6.0 dBFS");
    }

    #[test]
    fn windows_quantize_to_nearest_sample() {
        let pcm = sine_pcm(48000, 10000);
        assert_eq!(peak_in_seconds(&pcm, 0.0, 1.0), peak_of(&pcm));
        assert_eq!(peak_in_seconds(&pcm, 1.0, 0.0), 0);
        assert_eq!(peak_in_seconds(&pcm, 5.0, 6.0), 0);
        assert_eq!(peak_in_seconds(&pcm, f64::NAN, 1.0), 0);
        // A sliver shorter than one sample still catches its sample.
        let one = peak_in_frames(&pcm, 24000, 24001);
        assert_eq!(one, pcm[48000].unsigned_abs() as u32);
    }

    #[test]
    fn buckets_split_every_sample_exactly_once() {
        let pcm = sine_pcm(48000, 20000);
        for n in [1, 2, 3, 16, 17, 100] {
            let peaks = bucket_peaks(&pcm, n);
            assert_eq!(peaks.len(), n);
            let max = peaks.iter().copied().max().unwrap();
            assert_eq!(max, peak_of(&pcm), "one bucket holds the master: n={n}");
        }
        assert_eq!(bucket_peaks(&pcm, 1), vec![peak_of(&pcm)]);
        assert_eq!(bucket_peaks(&[], 4), vec![0, 0, 0, 0]);
        assert!(bucket_peaks(&pcm, 0).is_empty());
        // Windows tile without gaps: end of bucket i is start of i+1.
        let frames = 48000;
        for i in 0..16 {
            let (s, e) = bucket_window(frames, 16, i);
            let (ns, _) = bucket_window(frames, 16, (i + 1).min(15));
            if i < 15 {
                assert!((e - ns).abs() < 1e-9, "no gap between buckets");
            }
            assert!(s < e, "non-empty bucket");
        }
        let (_, last_end) = bucket_window(frames, 16, 15);
        assert!(
            (last_end - 1.0).abs() < 1e-6,
            "last bucket ends at the mix end"
        );
    }

    #[test]
    fn sections_skip_hidden_and_soundless_kinds() {
        use notation::model::{Align, AudioClip, Clip, FontStyle, TextClip, TrackKind, Wrap};
        let text = |start: f64, duration: f64, hidden: bool| {
            Clip::Text(TextClip {
                value: "x".to_string(),
                id: None,
                track: TrackKind::Graphics,
                label: None,
                start_after: None,
                hidden,
                start,
                duration,
                x: 0.0,
                y: 0.0,
                size: 12.0,
                color: notation::model::Rgb { r: 0, g: 0, b: 0 },
                align: Align::Left,
                wrap: Wrap::Off,
                style: FontStyle::Regular,
                order: 0,
                line_height: None,
                letter_spacing: Some(0.0),
                font_family: None,
                direction: notation::model::TextDirection::Ltr,
                spans: Vec::new(),
                split: None,
                typewriter: false,
                count_from: None,
                count_to: None,
                decimals: 0,
                line: 1,
                column: 1,
                turn: notation::model::Turn3D::flat(),
                filter_blur: 0.0,
                grade: notation::model::Grade::default(),
                effects: Vec::new(),
                vfx: Default::default(),
            })
        };
        let audio = |start: f64, duration: f64, hidden: bool, id: Option<&str>| {
            Clip::Audio(AudioClip {
                path: "s.wav".to_string(),
                id: id.map(str::to_string),
                track: TrackKind::Audio,
                label: None,
                start_after: None,
                hidden,
                grade: notation::model::Grade::default(),
                effects: Vec::new(),
                vfx: Default::default(),
                start,
                duration,
                trim_start: 0.0,
                trim_end: None,
                rate: 1.0,
                volume_db: 0.0,
                fade_in: 0.0,
                fade_out: 0.0,
                crossfade: Default::default(),
                duck_under: None,
                fx: Default::default(),
                waveform: Default::default(),
                line: 1,
                column: 1,
            })
        };
        let comp = Composition {
            name: "t".to_string(),
            id: None,
            canvas: None,
            width: 64,
            height: 64,
            duration: 4.0,
            fps: notation::model::Fps::from_int(30),
            background: notation::model::Rgb { r: 0, g: 0, b: 0 },
            version: 1,
            clips: vec![
                text(0.0, 4.0, false),
                audio(0.0, 2.0, false, Some("vox")),
                audio(1.0, 5.0, false, None),
                audio(0.0, 2.0, true, Some("muted")),
            ],
            fonts: Vec::new(),
            tweens: Vec::new(),
            sets: Vec::new(),
            keys: Vec::new(),
            marks: Vec::new(),
            inputs: Vec::new(),
            variables: Vec::new(),
            replicates: Vec::new(),
            autoplay: false,
            normalize: Default::default(),
            normalize_loc: None,
            grade: notation::model::Grade::default(),
            lane_grades: Vec::new(),
            transitions: Vec::new(),
            motion_blur: None,
        };
        let sections = mix_sections(&comp);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].label(), "audio \"vox\"");
        assert_eq!((sections[0].start, sections[0].end), (0.0, 2.0));
        // The overlong window clamps to the composition end.
        assert_eq!((sections[1].start, sections[1].end), (1.0, 4.0));
        assert_eq!(sections[1].label(), "audio #2");
        let mix = sine_pcm(4 * 48000, 20000);
        let peaks = section_peaks(&mix, &sections);
        assert_eq!(peaks.len(), 2);
        assert!(peaks[0] > 19000 && peaks[1] > 19000);
    }
}
