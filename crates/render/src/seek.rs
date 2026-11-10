//! Cross-adapter time seek (Sprint 21 adapters, T006 / CAP-006).
//!
//! Every adapter kind shows its time-correct frame at any film time:
//! looped Lottie wraps instead of freezing, interpolated 3D cameras
//! resolve at the clip-local instant, static charts and maps ignore
//! the clock, and custom sources gate their declared range. Seeking
//! never re-decodes what readiness already resolved (the prepare
//! stage owns every decode; seeks read the prepared bytes), a seek
//! the source cannot satisfy holds the last good frame and reports
//! the hold, and a still-loading source waits briefly then either
//! proceeds or fails naming the source (REC-004).
//!
//! Pure standard library: no browser, no JavaScript, no network at
//! render time. All time maps are pure functions of the film time,
//! the clip start, and the declared source range, so reruns agree
//! byte for byte.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::custom_source::{CustomSourceError, Readiness};
use notation::model::LoopMode;

/// The five adapter kinds that ride the composition clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdapterKind {
    Lottie,
    Chart,
    Scene3d,
    Map,
    Custom,
}

impl AdapterKind {
    /// Hyphenated style-attribute spelling of the kind, matching the
    /// clip vocabulary (`lottie`, `chart`, `scene3d`, `map`, custom
    /// sources register under their own names).
    pub fn name(self) -> &'static str {
        match self {
            AdapterKind::Lottie => "lottie",
            AdapterKind::Chart => "chart",
            AdapterKind::Scene3d => "scene3d",
            AdapterKind::Map => "map",
            AdapterKind::Custom => "custom",
        }
    }

    /// True for kinds whose pixels never move with the clock: charts
    /// and maps pre-render once and ride the shared image stack, so
    /// every film time shows the same prepared bytes.
    pub fn is_static(self) -> bool {
        matches!(self, AdapterKind::Chart | AdapterKind::Map)
    }
}

/// One loop-mapped Lottie instant (Sprint 21, T006): the animation
/// seconds the painter rasterizes for a film time, plus how the map
/// got there. `wrapped` means a looping mode turned the corner (the
/// frame matches the base instant one period back); `held_edge`
/// means `none` clamped past the window (the last good frame holds).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LottieInstant {
    pub mapped: f64,
    pub wrapped: bool,
    pub held_edge: bool,
}

/// Clip-local seconds for `film_t` against a clip opening at
/// `clip_start`: `None` when the film time is not finite, so no
/// adapter ever rasterizes a NaN clock — the caller holds last good
/// instead.
pub fn film_local_t(film_t: f64, clip_start: f64) -> Option<f64> {
    if !film_t.is_finite() || !clip_start.is_finite() {
        return None;
    }
    Some((film_t - clip_start).max(0.0))
}

/// Lottie source instant for one film time (Sprint 21, T006): the
/// clip-local seconds loop-mapped through [`lottie::sample_time`],
/// so `loop` wraps instead of freezing and `none` holds its edge
/// frame past the window, exactly like the painter. `None` only when
/// the film time itself is not finite (nothing time-correct exists
/// to show — hold last good).
pub fn lottie_instant(
    len_secs: f64,
    mode: LoopMode,
    film_t: f64,
    clip_start: f64,
) -> Option<LottieInstant> {
    let local = film_local_t(film_t, clip_start)?;
    let mapped = lottie::sample_time(len_secs, mode, local);
    let wrapped = matches!(mode, LoopMode::Loop | LoopMode::PingPong)
        && len_secs.is_finite()
        && len_secs > 0.0
        && (mapped - local).abs() > 1e-9;
    // `none` shows the genuine frame inside the window and clamps to
    // the edge outside it: past the end holds the last frame, before
    // the start holds the first.
    let held_edge = matches!(mode, LoopMode::None)
        && len_secs.is_finite()
        && len_secs > 0.0
        && (film_t > clip_start + len_secs || film_t < clip_start);
    Some(LottieInstant {
        mapped,
        wrapped,
        held_edge,
    })
}

/// 3D camera instant for one film time (Sprint 21, T006): the
/// clip-local seconds the camera keys lerp at (ends clamp downstream
/// in [`crate::scene3d::resolve_camera`], so an overrunning seek
/// holds its edge view). `None` only when the film time itself is
/// not finite.
pub fn scene3d_instant(film_t: f64, clip_start: f64) -> Option<f64> {
    film_local_t(film_t, clip_start)
}

/// Custom source instant for one film time (Sprint 21, T006): the
/// clip-local seconds gated to the declared half-open range. Ready
/// in-range seeks render; anything else refuses naming the source —
/// out-of-range renders hold last good through [`SeekCache`].
pub fn custom_instant(
    name: &str,
    film_t: f64,
    clip_start: f64,
    range_start: f64,
    range_end: f64,
) -> Result<f64, CustomSourceError> {
    let local = film_local_t(film_t, clip_start).ok_or(CustomSourceError::OutOfRange {
        name: name.to_string(),
        t: film_t,
        start: range_start,
        end: range_end,
    })?;
    if !local.is_finite() || local < range_start || local >= range_end {
        return Err(CustomSourceError::OutOfRange {
            name: name.to_string(),
            t: local,
            start: range_start,
            end: range_end,
        });
    }
    Ok(local)
}

/// Last-good frames keyed by source name (Sprint 21, T006): a seek
/// the source cannot satisfy holds the frame the previous seek left
/// here and reports the hold, so the film never flashes empty and
/// the report always names the source and the cause.
#[derive(Debug, Default)]
pub struct SeekCache {
    last_good: HashMap<String, Vec<u8>>,
}

impl SeekCache {
    /// Empty cache holding no frames.
    pub fn new() -> Self {
        Self::default()
    }

    /// Remember the frame one seek rendered for `source`, so a later
    /// unsatisfiable seek can hold it.
    pub fn store(&mut self, source: &str, frame: Vec<u8>) {
        self.last_good.insert(source.to_string(), frame);
    }

    /// True while `source` holds a frame to fall back on.
    pub fn has(&self, source: &str) -> bool {
        self.last_good.contains_key(source)
    }

    /// Hold the last good frame for `source`: a clone of the stored
    /// bytes plus a report naming the kind, the source, and the
    /// cause. Refuses naming the source when no seek has stored a
    /// frame yet — there is nothing good to hold.
    pub fn hold(
        &self,
        kind: AdapterKind,
        source: &str,
        cause: &str,
    ) -> Result<(Vec<u8>, String), String> {
        match self.last_good.get(source) {
            Some(frame) => {
                let report = format!(
                    "seek hold: {} source {:?} keeps its last good frame ({})",
                    kind.name(),
                    source,
                    cause
                );
                Ok((frame.clone(), report))
            }
            None => Err(format!(
                "seek refused: {} source {:?} has no last good frame yet ({}); nothing to hold",
                kind.name(),
                source,
                cause
            )),
        }
    }
}

/// Decode-once ledger (Sprint 21, T006): the first seek resolving a
/// source decodes it; every later seek reuses the prepared bytes.
/// The render prepare stage owns the real decodes; the ledger is the
/// counting contract tests assert — N seeks, one decode.
#[derive(Debug, Default)]
pub struct DecodeLedger {
    decodes: HashMap<String, usize>,
}

impl DecodeLedger {
    /// Empty ledger with no source decoded.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one decode of `source` (called by the decode path, not
    /// by seeks).
    pub fn note_decode(&mut self, source: &str) {
        *self.decodes.entry(source.to_string()).or_insert(0) += 1;
    }

    /// Decode count for `source` (zero when never decoded).
    pub fn decode_count(&self, source: &str) -> usize {
        self.decodes.get(source).copied().unwrap_or(0)
    }

    /// Resolve `source` from the prepared entry, decoding exactly
    /// once: the first call runs `decode` and records it, later
    /// calls return the cached entry without touching the decoder.
    pub fn resolve_cached<'a, T>(
        &mut self,
        cache: &'a mut HashMap<String, T>,
        source: &str,
        decode: impl FnOnce() -> T,
    ) -> &'a T {
        if !cache.contains_key(source) {
            let value = decode();
            cache.insert(source.to_string(), value);
            self.note_decode(source);
        }
        &cache[source]
    }
}

/// How long a still-loading source waits before the render either
/// proceeds or fails naming the source (REC-004).
#[derive(Debug, Clone, Copy)]
pub struct WaitBudget {
    /// Readiness polls before giving up.
    pub attempts: u32,
    /// Sleep between polls.
    pub interval: Duration,
}

impl WaitBudget {
    /// The brief default wait: tens of polls a millisecond apart, so
    /// a resolving source proceeds within a blink and a stuck one
    /// fails fast instead of hanging the render.
    pub fn brief() -> Self {
        Self {
            attempts: 100,
            interval: Duration::from_millis(2),
        }
    }
}

impl Default for WaitBudget {
    fn default() -> Self {
        Self::brief()
    }
}

/// Wait for one source's readiness (Sprint 21, T006 / REC-004): poll
/// `readiness` under `budget`, then either proceed (ready), fail
/// naming the source and its cause (failed), or fail naming the
/// still-loading source (the brief wait ran out). No partial frame
/// is ever emitted as final.
pub fn wait_for_ready(
    source: &str,
    budget: WaitBudget,
    mut readiness: impl FnMut() -> Readiness,
) -> Result<(), String> {
    for _ in 0..budget.attempts {
        match readiness() {
            Readiness::Ready => return Ok(()),
            Readiness::Failed { cause } => {
                return Err(format!(
                    "render refused: custom source {:?} failed ({}); no MP4 was written",
                    source, cause
                ));
            }
            Readiness::Loading => std::thread::sleep(budget.interval),
        }
    }
    // One last poll after the wait, so a source that resolved on the
    // final instant still proceeds instead of failing.
    match readiness() {
        Readiness::Ready => Ok(()),
        Readiness::Failed { cause } => Err(format!(
            "render refused: custom source {:?} failed ({}); no MP4 was written",
            source, cause
        )),
        Readiness::Loading => Err(format!(
            "render refused: custom source {:?} is still loading after the brief readiness wait; no MP4 was written",
            source
        )),
    }
}

/// Elapsed wall time of one wait, for tests asserting the wait stays
/// brief. Not part of the render contract.
pub fn timed_wait(
    source: &str,
    budget: WaitBudget,
    readiness: impl FnMut() -> Readiness,
) -> (Result<(), String>, Duration) {
    let start = Instant::now();
    let out = wait_for_ready(source, budget, readiness);
    (out, start.elapsed())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom_source::CustomSourceRegistry;
    use std::sync::{Arc, Mutex};

    #[test]
    fn every_kind_maps_film_time_to_its_source_clock() {
        // Lottie: clip-local seconds, loop-mapped.
        let at = lottie_instant(2.0, LoopMode::Loop, 1.0, 0.0).expect("finite film");
        assert!((at.mapped - 1.0).abs() < 1e-9);
        assert!(!at.wrapped && !at.held_edge);
        // Scene3d: clip-local seconds for the camera keys.
        assert_eq!(scene3d_instant(3.0, 1.0), Some(2.0));
        // Charts and maps are static per film time.
        assert!(AdapterKind::Chart.is_static());
        assert!(AdapterKind::Map.is_static());
        assert!(!AdapterKind::Lottie.is_static());
        assert!(!AdapterKind::Scene3d.is_static());
        assert!(!AdapterKind::Custom.is_static());
        // Custom: clip-local seconds gated to the declared range.
        assert_eq!(custom_instant("fx", 1.5, 1.0, 0.0, 4.0), Ok(0.5));
    }

    #[test]
    fn looped_content_wraps_instead_of_freezing() {
        let len = 2.0;
        let base = lottie_instant(len, LoopMode::Loop, 0.5, 0.0).expect("finite film");
        let wrapped = lottie_instant(len, LoopMode::Loop, 0.5 + len, 0.0).expect("finite film");
        assert!(
            (wrapped.mapped - base.mapped).abs() < 1e-9,
            "wrap lands on base"
        );
        assert!(wrapped.wrapped, "the wrap is reported");
        // Exact multiples land back on zero, never past the end.
        let edge = lottie_instant(len, LoopMode::Loop, 4.0, 0.0).expect("finite film");
        assert!(
            edge.mapped < 1e-9,
            "period edge maps to zero: {}",
            edge.mapped
        );
        // Ping-pong turns around instead of jumping.
        let there = lottie_instant(len, LoopMode::PingPong, 0.5, 0.0).expect("finite film");
        let back = lottie_instant(len, LoopMode::PingPong, 2.0 * len - 0.5, 0.0).expect("mirror");
        assert!((there.mapped - back.mapped).abs() < 1e-9, "mirror matches");
        assert!(back.wrapped, "the mirror leg is reported");
    }

    #[test]
    fn non_looping_seeks_hold_the_edge_frame() {
        let len = 2.0;
        let past = lottie_instant(len, LoopMode::None, 5.0, 0.0).expect("finite film");
        assert!((past.mapped - len).abs() < 1e-9, "past-end holds the edge");
        assert!(past.held_edge, "the hold is reported");
        assert!(!past.wrapped);
        // A clip opening mid-film counts source seconds from its own start.
        let offset = lottie_instant(len, LoopMode::Loop, 3.0, 1.0).expect("finite film");
        assert!(
            (offset.mapped - 0.0).abs() < 1e-9,
            "2.0s local wraps to 0: {}",
            offset.mapped
        );
    }

    #[test]
    fn non_finite_film_time_is_unsatisfiable() {
        assert_eq!(film_local_t(f64::NAN, 0.0), None);
        assert_eq!(film_local_t(f64::INFINITY, 0.0), None);
        assert_eq!(lottie_instant(2.0, LoopMode::Loop, f64::NAN, 0.0), None);
        assert_eq!(scene3d_instant(f64::NAN, 0.0), None);
        assert!(custom_instant("fx", f64::NAN, 0.0, 0.0, 4.0).is_err());
    }

    #[test]
    fn out_of_range_custom_seeks_refuse_naming_source_and_range() {
        let err = custom_instant("fx", 9.0, 0.0, 0.0, 4.0).expect_err("past-end refuses");
        match err {
            CustomSourceError::OutOfRange {
                name, start, end, ..
            } => {
                assert_eq!(name, "fx");
                assert_eq!((start, end), (0.0, 4.0));
            }
            other => panic!("expected OutOfRange, got {other:?}"),
        }
    }

    #[test]
    fn unsatisfiable_seeks_hold_last_good_with_a_report() {
        let mut cache = SeekCache::new();
        let frame = vec![9u8; 12];
        cache.store("fx", frame.clone());
        let (held, report) = cache
            .hold(AdapterKind::Custom, "fx", "out of range [0.000s, 4.000s)")
            .expect("stored frame holds");
        assert_eq!(held, frame, "the hold shows the last good frame");
        assert!(
            report.contains("\"fx\""),
            "report names the source: {report}"
        );
        assert!(
            report.contains("out of range"),
            "report names the cause: {report}"
        );
        assert!(report.contains("custom"), "report names the kind: {report}");
    }

    #[test]
    fn holding_without_a_last_good_frame_refuses_naming_the_source() {
        let cache = SeekCache::new();
        let err = cache
            .hold(
                AdapterKind::Lottie,
                "slide",
                "the animation instant is not finite",
            )
            .expect_err("empty cache refuses");
        assert!(err.contains("\"slide\""), "refusal names the source: {err}");
        assert!(
            err.contains("no last good frame"),
            "refusal says why: {err}"
        );
    }

    #[test]
    fn seeking_never_re_decodes_resolved_sources() {
        let mut ledger = DecodeLedger::new();
        let mut prepared: HashMap<String, Vec<u8>> = HashMap::new();
        let mut decodes = 0usize;
        for _ in 0..20 {
            ledger.resolve_cached(&mut prepared, "slide.json", || {
                decodes += 1;
                vec![1, 2, 3]
            });
        }
        assert_eq!(decodes, 1, "twenty seeks decode once");
        assert_eq!(ledger.decode_count("slide.json"), 1);
        assert_eq!(ledger.decode_count("unseen.json"), 0);
    }

    #[test]
    fn ready_sources_proceed_without_waiting() {
        let (out, _) = timed_wait("fx", WaitBudget::brief(), || Readiness::Ready);
        assert!(out.is_ok(), "ready proceeds");
    }

    #[test]
    fn loading_sources_wait_briefly_then_proceed() {
        let state = Arc::new(Mutex::new(Readiness::Loading));
        let flip = Arc::clone(&state);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            *flip.lock().unwrap() = Readiness::Ready;
        });
        let (out, elapsed) =
            timed_wait("fx", WaitBudget::brief(), || state.lock().unwrap().clone());
        assert!(out.is_ok(), "a resolving source proceeds: {out:?}");
        assert!(
            elapsed < Duration::from_secs(2),
            "the wait stays brief: {elapsed:?}"
        );
    }

    #[test]
    fn failed_sources_name_the_source_and_cause() {
        let out = wait_for_ready("fx", WaitBudget::brief(), || Readiness::Failed {
            cause: "tile pack is corrupt".to_string(),
        });
        let err = out.expect_err("failed refuses");
        assert!(err.contains("\"fx\""), "names the source: {err}");
        assert!(
            err.contains("tile pack is corrupt"),
            "names the cause: {err}"
        );
    }

    #[test]
    fn stuck_loading_sources_fail_naming_the_source() {
        let budget = WaitBudget {
            attempts: 10,
            interval: Duration::from_millis(1),
        };
        let (out, elapsed) = timed_wait("fx", budget, || Readiness::Loading);
        let err = out.expect_err("stuck loading fails");
        assert!(err.contains("\"fx\""), "names the source: {err}");
        assert!(err.contains("still loading"), "names the state: {err}");
        assert!(
            elapsed < Duration::from_secs(2),
            "the wait stays brief: {elapsed:?}"
        );
    }

    #[test]
    fn registry_seeks_coordinate_with_the_hold_cache() {
        let mut registry = CustomSourceRegistry::new();
        registry
            .register("fx", 0.0, 4.0, 8, 8, vec![7u8; 4])
            .unwrap();
        registry.mark_ready("fx").unwrap();
        // Satisfiable seeks render deterministically at any film time.
        let a = registry
            .render_at("fx", custom_instant("fx", 0.5, 0.0, 0.0, 4.0).unwrap())
            .unwrap();
        let again = registry
            .render_at("fx", custom_instant("fx", 0.5, 0.0, 0.0, 4.0).unwrap())
            .unwrap();
        assert_eq!(
            a.digest(),
            again.digest(),
            "same inputs, byte-identical frames"
        );
        let later = registry
            .render_at("fx", custom_instant("fx", 1.5, 0.0, 0.0, 4.0).unwrap())
            .unwrap();
        assert_ne!(a.digest(), later.digest(), "the clock moves the frame");
        // An unsatisfiable seek holds the last good frame with a report.
        let mut cache = SeekCache::new();
        cache.store("fx", a.pixels.clone());
        let cause = custom_instant("fx", 9.0, 0.0, 0.0, 4.0)
            .expect_err("past-end refuses")
            .to_string();
        let (held, report) = cache.hold(AdapterKind::Custom, "fx", &cause).unwrap();
        assert_eq!(held, a.pixels);
        assert!(
            report.contains("\"fx\"") && report.contains("out of range"),
            "{report}"
        );
    }
}
