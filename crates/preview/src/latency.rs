//! Transport latency bars (Sprint 14, T002 / CAP-002).
//!
//! Step, scrub, and jump actions report per-session medians labeled
//! warm versus cold: a frame drawn fresh through the render pipeline
//! ([`CacheSource::Rendered`]) is a cold sample, while a frame served
//! from either exact-byte cache layer ([`CacheSource::Memory`] or
//! [`CacheSource::Disk`]) is a warm sample. Bars apply to the warm
//! medians only — cold draws are reported labeled and never counted
//! toward the bar verdict — and a miss names the measured median
//! against its bar (REC-004: honest medians, nothing silently
//! substituted).
//!
//! Transport controls live in T003; this slice is measurement plus
//! reporting. [`PreviewSession`](crate::PreviewSession) times each
//! [`frame_png_timed`](crate::PreviewSession::frame_png_timed) call and
//! files the sample here; tests and scripts can also drive
//! [`LatencyTracker`] directly with synthetic durations.

use std::time::Duration;

use crate::CacheSource;

/// Bar for step actions in milliseconds (Sprint 14, T002 / CAP-002):
/// the warm step median must sit at most this far.
pub const STEP_BAR_MS: f64 = 50.0;
/// Bar for scrub-drag actions in milliseconds: the warm scrub median
/// must sit at most this far.
pub const SCRUB_BAR_MS: f64 = 50.0;
/// Bar for arbitrary-jump actions in milliseconds: the warm jump
/// median must sit at most this far.
pub const JUMP_BAR_MS: f64 = 100.0;

/// Which page gesture a latency sample belongs to (Sprint 14, T002).
/// The page (T003) maps its controls onto these: one frame forward or
/// back is [`Step`](Self::Step), a drag along the timeline is
/// [`Scrub`](Self::Scrub), and an arbitrary seek is
/// [`Jump`](Self::Jump).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportAction {
    /// One-frame step forward or back. Bar: 50 ms warm median.
    Step,
    /// Scrub-drag along the timeline. Bar: 50 ms warm median.
    Scrub,
    /// Arbitrary jump to another frame. Bar: 100 ms warm median.
    Jump,
}

impl TransportAction {
    /// Bar in milliseconds for this action under the given bars.
    pub fn bar_ms(&self, bars: &LatencyBars) -> f64 {
        match self {
            Self::Step => bars.step_ms,
            Self::Scrub => bars.scrub_ms,
            Self::Jump => bars.jump_ms,
        }
    }
}

impl std::fmt::Display for TransportAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Step => write!(f, "step"),
            Self::Scrub => write!(f, "scrub"),
            Self::Jump => write!(f, "jump"),
        }
    }
}

/// Whether a sample ran warm from cache or cold from a fresh draw
/// (Sprint 14, T002): cache hits are warm, pipeline draws are cold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Warmth {
    /// Drawn fresh through the render pipeline. Reported labeled and
    /// excluded from the bar verdict.
    Cold,
    /// Served from the memory or disk exact-byte cache.
    Warm,
}

impl std::fmt::Display for Warmth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cold => write!(f, "cold"),
            Self::Warm => write!(f, "warm"),
        }
    }
}

impl From<CacheSource> for Warmth {
    fn from(source: CacheSource) -> Self {
        match source {
            CacheSource::Rendered => Self::Cold,
            CacheSource::Memory | CacheSource::Disk => Self::Warm,
        }
    }
}

/// Bars in milliseconds for one session report (Sprint 14, T002).
/// Defaults are the specification bars: 50 ms step, 50 ms scrub-drag,
/// 100 ms jump.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatencyBars {
    /// Warm step median bar in milliseconds.
    pub step_ms: f64,
    /// Warm scrub-drag median bar in milliseconds.
    pub scrub_ms: f64,
    /// Warm jump median bar in milliseconds.
    pub jump_ms: f64,
}

impl Default for LatencyBars {
    fn default() -> Self {
        Self {
            step_ms: STEP_BAR_MS,
            scrub_ms: SCRUB_BAR_MS,
            jump_ms: JUMP_BAR_MS,
        }
    }
}

/// One timed transport action (Sprint 14, T002).
#[derive(Debug, Clone, Copy)]
pub struct LatencySample {
    /// Which gesture this sample belongs to.
    pub action: TransportAction,
    /// Warm (cache hit) or cold (fresh draw).
    pub warmth: Warmth,
    /// Wall-clock time the frame request took.
    pub elapsed: Duration,
}

impl LatencySample {
    /// Elapsed time in milliseconds.
    pub fn elapsed_ms(&self) -> f64 {
        self.elapsed.as_secs_f64() * 1000.0
    }
}

/// Per-session collector of transport latency samples (Sprint 14,
/// T002). Every sample lands in exactly one `(action, warmth)` bucket;
/// medians are computed per bucket so warm and cold never mix.
#[derive(Debug, Clone, Default)]
pub struct LatencyTracker {
    samples: Vec<LatencySample>,
}

impl LatencyTracker {
    /// Empty collector.
    pub fn new() -> Self {
        Self::default()
    }

    /// File one sample.
    pub fn record(&mut self, action: TransportAction, warmth: Warmth, elapsed: Duration) {
        self.samples.push(LatencySample {
            action,
            warmth,
            elapsed,
        });
    }

    /// File one sample from the cache layer that served it: rendered
    /// frames file cold, memory and disk hits file warm.
    pub fn record_source(
        &mut self,
        action: TransportAction,
        source: CacheSource,
        elapsed: Duration,
    ) {
        self.record(action, Warmth::from(source), elapsed);
    }

    /// All samples filed so far, in filing order.
    pub fn samples(&self) -> &[LatencySample] {
        &self.samples
    }

    /// Sample count for one `(action, warmth)` bucket.
    pub fn count(&self, action: TransportAction, warmth: Warmth) -> usize {
        self.samples
            .iter()
            .filter(|s| s.action == action && s.warmth == warmth)
            .count()
    }

    /// Median elapsed milliseconds for one bucket, or `None` when the
    /// bucket is empty. Even counts average the two middle values.
    pub fn median_ms(&self, action: TransportAction, warmth: Warmth) -> Option<f64> {
        let mut values: Vec<f64> = self
            .samples
            .iter()
            .filter(|s| s.action == action && s.warmth == warmth)
            .map(LatencySample::elapsed_ms)
            .collect();
        median_of(&mut values)
    }

    /// Per-session report under the specification bars.
    pub fn report(&self) -> SessionLatencyReport {
        self.report_with(&LatencyBars::default())
    }

    /// Per-session report under custom bars (same shape, different
    /// thresholds; custom bars never relabel samples).
    pub fn report_with(&self, bars: &LatencyBars) -> SessionLatencyReport {
        let actions = [TransportAction::Step, TransportAction::Scrub, TransportAction::Jump]
            .iter()
            .map(|action| {
                let bar = action.bar_ms(bars);
                let warm_median_ms = self.median_ms(*action, Warmth::Warm);
                ActionLatency {
                    action: *action,
                    bar_ms: bar,
                    warm_median_ms,
                    warm_samples: self.count(*action, Warmth::Warm),
                    cold_median_ms: self.median_ms(*action, Warmth::Cold),
                    cold_samples: self.count(*action, Warmth::Cold),
                    meets_bar: warm_median_ms.map(|m| m <= bar),
                }
            })
            .collect();
        SessionLatencyReport {
            bars: *bars,
            actions,
        }
    }
}

/// Median of an unsorted millisecond list (sorted in place). `None`
/// for no samples; the average of the two middle values for even
/// counts.
fn median_of(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let n = values.len();
    Some(if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    })
}

/// One action row of a session latency report (Sprint 14, T002): the
/// warm median against its bar plus the cold median labeled and
/// excluded from the verdict.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActionLatency {
    /// Which gesture this row covers.
    pub action: TransportAction,
    /// Bar in milliseconds this row is judged against.
    pub bar_ms: f64,
    /// Warm median in milliseconds, or `None` with no warm samples.
    pub warm_median_ms: Option<f64>,
    /// Warm sample count behind the median.
    pub warm_samples: usize,
    /// Cold median in milliseconds, or `None` with no cold samples.
    /// Labeled cold, never counted toward [`meets_bar`](Self::meets_bar).
    pub cold_median_ms: Option<f64>,
    /// Cold sample count behind the median.
    pub cold_samples: usize,
    /// Whether the warm median meets the bar. `None` with no warm
    /// samples: no claim, never a silent pass.
    pub meets_bar: Option<bool>,
}

impl std::fmt::Display for ActionLatency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.warm_median_ms {
            Some(m) => {
                let verdict = if self.meets_bar == Some(true) {
                    "PASS".to_string()
                } else {
                    format!("MISS: measured {m:.3}ms exceeds bar {:.1}ms", self.bar_ms)
                };
                write!(
                    f,
                    "{}: warm median {m:.3}ms over {} sample(s) vs bar {:.1}ms: {verdict}",
                    self.action, self.warm_samples, self.bar_ms
                )?;
            }
            None => {
                write!(
                    f,
                    "{}: no warm samples (bar {:.1}ms): no claim",
                    self.action, self.bar_ms
                )?;
            }
        }
        match self.cold_median_ms {
            Some(m) => write!(
                f,
                "; cold median {m:.3}ms over {} sample(s) (labeled cold, excluded from bar)",
                self.cold_samples
            ),
            None => write!(f, "; no cold samples"),
        }
    }
}

/// Per-session latency medians labeled warm versus cold (Sprint 14,
/// T002 / CAP-002, REC-004).
#[derive(Debug, Clone, PartialEq)]
pub struct SessionLatencyReport {
    /// Bars the rows were judged against.
    pub bars: LatencyBars,
    /// One row per action in step, scrub, jump order.
    pub actions: Vec<ActionLatency>,
}

impl SessionLatencyReport {
    /// Row for one action.
    pub fn action(&self, action: TransportAction) -> Option<&ActionLatency> {
        self.actions.iter().find(|a| a.action == action)
    }

    /// Overall bar verdict: `Some(true)` when every action with warm
    /// samples meets its bar, `Some(false)` when any warm median
    /// misses, `None` with no warm samples at all — an honest no-claim,
    /// never a pass by default.
    pub fn all_bars_met(&self) -> Option<bool> {
        let mut any = false;
        for row in &self.actions {
            match row.meets_bar {
                Some(false) => return Some(false),
                Some(true) => any = true,
                None => {}
            }
        }
        if any { Some(true) } else { None }
    }
}

impl std::fmt::Display for SessionLatencyReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, row) in self.actions.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{row}")?;
        }
        match self.all_bars_met() {
            Some(true) => write!(f, "\noverall: all bars met"),
            Some(false) => write!(f, "\noverall: BAR MISSED (honest medians above)"),
            None => write!(f, "\noverall: no warm samples: no bar claim"),
        }
    }
}
