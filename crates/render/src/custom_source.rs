//! Framesmith-owned custom frame sources (Sprint 21 adapters, T005 / CAP-005).
//!
//! Makers extend the engine without forking it: a named in-process
//! source registers with a declared source-time range, reports
//! readiness (`loading`, then `ready` or `failed` with a cause),
//! renders one deterministic RGBA frame per source time inside that
//! range, and releases everything on dispose. Sources run in-process
//! under the same determinism contract as every other clip family:
//! the same registered name, range, dimensions, and source time
//! render byte-identical pixels on every run.
//!
//! The range gate is half-open (`start <= t < end`, matching clip
//! timing): an out-of-range call refuses naming the range, a call on
//! a failed source refuses naming the cause, and any call naming an
//! unknown or disposed source refuses as unregistered. Refusals
//! happen before any render work, so validation can gate on
//! readiness exactly like media (REC-001/REC-003). Disposing drops
//! the entry with its retained payload, so [`CustomSourceRegistry`]
//! reports zero retained bytes afterwards and every follow-up call
//! refuses as unregistered (STATE-04).
//!
//! Pure standard library: no browser, no JavaScript, no network at
//! render time. Pixels derive from a wrapping FNV-1a mix over the
//! source name, the exact source-time bits, and the pixel position,
//! so reruns agree byte for byte on every platform.

use std::collections::BTreeMap;
use std::fmt;

/// Largest frame edge a custom source may declare, in pixels. Absurd
/// dimensions refuse at registration (fail closed), never allocate.
pub const MAX_CUSTOM_EDGE: u32 = 8192;

/// Readiness of one registered source (STATE-01/02/03): `Loading`
/// while the source is still resolving, `Ready` once it decodes, or
/// `Failed` with the cause when it cannot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Readiness {
    Loading,
    Ready,
    Failed { cause: String },
}

impl Readiness {
    /// Inspector spelling (`loading`, `ready`, or `failed`): the same
    /// word the render path gates on, so reports never disagree.
    pub fn as_str(&self) -> &'static str {
        match self {
            Readiness::Loading => "loading",
            Readiness::Ready => "ready",
            Readiness::Failed { .. } => "failed",
        }
    }

    /// True only for [`Readiness::Ready`].
    pub fn is_ready(&self) -> bool {
        matches!(self, Readiness::Ready)
    }
}

impl fmt::Display for Readiness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Readiness::Loading => write!(f, "loading"),
            Readiness::Ready => write!(f, "ready"),
            Readiness::Failed { cause } => write!(f, "failed: {cause}"),
        }
    }
}

/// One deterministic custom frame: opaque RGBA bytes in row-major
/// order (`width * height * 4`), ready to enter the same composition
/// stack as media frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl CustomFrame {
    /// Byte count of the pixel store (`width * height * 4`).
    pub fn byte_len(&self) -> usize {
        self.pixels.len()
    }

    /// Deterministic digest of the frame (FNV-1a over dimensions plus
    /// pixels): equal frames share a digest, so reruns compare with
    /// one integer.
    pub fn digest(&self) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for b in self
            .width
            .to_le_bytes()
            .iter()
            .chain(self.height.to_le_bytes().iter())
        {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        for b in &self.pixels {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }
}

/// Refusal for a custom-source call. Every variant names the source;
/// range refusals name the declared range and failure refusals name
/// the cause, so makers can fix the call without guessing.
#[derive(Debug, Clone, PartialEq)]
pub enum CustomSourceError {
    /// No live source under this name (never registered, or disposed).
    Unregistered { name: String },
    /// A live source already holds this name; register is refused.
    Duplicate { name: String },
    /// The source is still resolving; no frame exists yet.
    Loading { name: String },
    /// The source failed; the cause travels with the refusal.
    Failed { name: String, cause: String },
    /// The requested source time sits outside the declared range.
    OutOfRange {
        name: String,
        t: f64,
        start: f64,
        end: f64,
    },
    /// The declared range is unusable (non-finite, negative start, or
    /// an end at or before the start).
    InvalidRange {
        name: String,
        start: f64,
        end: f64,
        reason: String,
    },
    /// The declared dimensions are unusable (zero or past the cap).
    InvalidSize {
        name: String,
        width: u32,
        height: u32,
    },
    /// The registration name is empty or blank.
    InvalidName { name: String },
    /// A failure was reported with no cause to act on.
    EmptyCause { name: String },
}

impl fmt::Display for CustomSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CustomSourceError::Unregistered { name } => write!(
                f,
                "custom source \"{name}\" is not registered (unknown name or already disposed)"
            ),
            CustomSourceError::Duplicate { name } => write!(
                f,
                "custom source \"{name}\" is already registered; dispose it first or pick another name"
            ),
            CustomSourceError::Loading { name } => {
                write!(f, "custom source \"{name}\" is still loading; no frame exists yet")
            }
            CustomSourceError::Failed { name, cause } => {
                write!(f, "custom source \"{name}\" failed: {cause}")
            }
            CustomSourceError::OutOfRange { name, t, start, end } => write!(
                f,
                "custom source \"{name}\" render at source-time {t:.3}s is out of range [{start:.3}s, {end:.3}s)"
            ),
            CustomSourceError::InvalidRange { name, start, end, reason } => write!(
                f,
                "custom source \"{name}\" declares an unusable range [{start:.3}s, {end:.3}s): {reason}"
            ),
            CustomSourceError::InvalidSize { name, width, height } => write!(
                f,
                "custom source \"{name}\" declares unusable dimensions {width}x{height}; edges must run 1 through {MAX_CUSTOM_EDGE}"
            ),
            CustomSourceError::InvalidName { name } => write!(
                f,
                "custom source name {name:?} is empty; registration needs a non-blank name"
            ),
            CustomSourceError::EmptyCause { name } => write!(
                f,
                "custom source \"{name}\" failed with no cause; report a cause so the failure stays actionable"
            ),
        }
    }
}

impl std::error::Error for CustomSourceError {}

/// One registered source: its declared half-open range, frame size,
/// readiness, and retained decoded payload.
#[derive(Debug)]
struct Entry {
    start: f64,
    end: f64,
    width: u32,
    height: u32,
    readiness: Readiness,
    /// Retained decoded bytes (the file or rows the source parsed).
    /// Dropped whole with the entry on dispose, so nothing leaks.
    retained: Vec<u8>,
}

/// Registry of framesmith-owned custom frame sources.
///
/// Registration starts at [`Readiness::Loading`]; the owner moves
/// each source to `Ready` or `Failed` with a cause. Rendering gates
/// on registration, then readiness, then the declared range, and
/// only then derives pixels — refusals name the source plus the
/// range or cause and cost no render work.
#[derive(Debug, Default)]
pub struct CustomSourceRegistry {
    sources: BTreeMap<String, Entry>,
}

impl CustomSourceRegistry {
    /// Empty registry holding no sources and no retained bytes.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `name` with its declared half-open source-time range
    /// `[range-start, range-end)`, frame dimensions, and decoded
    /// payload (retained until dispose). Starts at `Loading`.
    /// Refuses blank names, live duplicates, unusable ranges, and
    /// unusable dimensions without storing anything.
    pub fn register(
        &mut self,
        name: &str,
        range_start: f64,
        range_end: f64,
        width: u32,
        height: u32,
        payload: Vec<u8>,
    ) -> Result<(), CustomSourceError> {
        if name.trim().is_empty() {
            return Err(CustomSourceError::InvalidName {
                name: name.to_string(),
            });
        }
        if self.sources.contains_key(name) {
            return Err(CustomSourceError::Duplicate {
                name: name.to_string(),
            });
        }
        if let Some(reason) = bad_range(range_start, range_end) {
            return Err(CustomSourceError::InvalidRange {
                name: name.to_string(),
                start: range_start,
                end: range_end,
                reason: reason.to_string(),
            });
        }
        if width == 0 || height == 0 || width > MAX_CUSTOM_EDGE || height > MAX_CUSTOM_EDGE {
            return Err(CustomSourceError::InvalidSize {
                name: name.to_string(),
                width,
                height,
            });
        }
        self.sources.insert(
            name.to_string(),
            Entry {
                start: range_start,
                end: range_end,
                width,
                height,
                readiness: Readiness::Loading,
                retained: payload,
            },
        );
        Ok(())
    }

    /// Readiness of `name` for inspector reports and validation
    /// gates. Unknown or disposed names refuse as unregistered.
    pub fn readiness(&self, name: &str) -> Result<Readiness, CustomSourceError> {
        match self.sources.get(name) {
            Some(entry) => Ok(entry.readiness.clone()),
            None => Err(CustomSourceError::Unregistered {
                name: name.to_string(),
            }),
        }
    }

    /// Move `name` from loading to ready. Unknown or disposed names
    /// refuse as unregistered.
    pub fn mark_ready(&mut self, name: &str) -> Result<(), CustomSourceError> {
        match self.sources.get_mut(name) {
            Some(entry) => {
                entry.readiness = Readiness::Ready;
                Ok(())
            }
            None => Err(CustomSourceError::Unregistered {
                name: name.to_string(),
            }),
        }
    }

    /// Move `name` to failed with `cause`. A blank cause refuses
    /// (fail closed: a failure with nothing to act on is not
    /// actionable); unknown or disposed names refuse as unregistered.
    pub fn mark_failed(&mut self, name: &str, cause: &str) -> Result<(), CustomSourceError> {
        if cause.trim().is_empty() {
            return Err(CustomSourceError::EmptyCause {
                name: name.to_string(),
            });
        }
        match self.sources.get_mut(name) {
            Some(entry) => {
                entry.readiness = Readiness::Failed {
                    cause: cause.to_string(),
                };
                Ok(())
            }
            None => Err(CustomSourceError::Unregistered {
                name: name.to_string(),
            }),
        }
    }

    /// Render the frame at source time `t`. Gates run before any
    /// pixel work, in order: registration, readiness (loading blocks,
    /// failed names the cause), then the declared half-open range
    /// (non-finite times refuse as out of range). Ready in-range
    /// calls return deterministic RGBA pixels.
    pub fn render_at(&self, name: &str, t: f64) -> Result<CustomFrame, CustomSourceError> {
        let entry = match self.sources.get(name) {
            Some(entry) => entry,
            None => {
                return Err(CustomSourceError::Unregistered {
                    name: name.to_string(),
                })
            }
        };
        match &entry.readiness {
            Readiness::Loading => {
                return Err(CustomSourceError::Loading {
                    name: name.to_string(),
                });
            }
            Readiness::Failed { cause } => {
                return Err(CustomSourceError::Failed {
                    name: name.to_string(),
                    cause: cause.clone(),
                });
            }
            Readiness::Ready => {}
        }
        if !t.is_finite() || t < entry.start || t >= entry.end {
            return Err(CustomSourceError::OutOfRange {
                name: name.to_string(),
                t,
                start: entry.start,
                end: entry.end,
            });
        }
        Ok(derive_frame(name, t, entry.width, entry.height))
    }

    /// Dispose `name`: the entry and its retained payload drop here,
    /// so nothing stays allocated. Later calls naming it refuse as
    /// unregistered (STATE-04). Disposing an unknown or already
    /// disposed name refuses as unregistered.
    pub fn dispose(&mut self, name: &str) -> Result<(), CustomSourceError> {
        match self.sources.remove(name) {
            Some(_) => Ok(()),
            None => Err(CustomSourceError::Unregistered {
                name: name.to_string(),
            }),
        }
    }

    /// True while `name` holds a live (not disposed) registration.
    pub fn is_registered(&self, name: &str) -> bool {
        self.sources.contains_key(name)
    }

    /// Count of live registrations.
    pub fn len(&self) -> usize {
        self.sources.len()
    }

    /// True when no source is registered.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    /// Retained decoded bytes across every live source. Zero means
    /// nothing leaks; disposing the last source always returns here
    /// to zero.
    pub fn retained_bytes(&self) -> usize {
        self.sources.values().map(|e| e.retained.len()).sum()
    }
}

/// Range refusal reason, if `[start, end)` cannot gate renders.
fn bad_range(start: f64, end: f64) -> Option<&'static str> {
    if !start.is_finite() || !end.is_finite() {
        return Some("range bounds must both be finite seconds");
    }
    if start < 0.0 {
        return Some("range start must sit at or after source-time 0");
    }
    if end <= start {
        return Some("range end must sit after range start");
    }
    None
}

/// FNV-1a (64-bit) over the source name: the per-source seed. Hand
/// rolled (like the footage cache) so digests never depend on a
/// hasher build or platform.
fn name_seed(name: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in name.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// One deterministic channel byte from the seed, the exact
/// source-time bits, the pixel position, and the channel index.
/// Wrapping arithmetic only, so every platform agrees.
fn channel_byte(seed: u64, t_bits: u64, x: u32, y: u32, channel: u64) -> u8 {
    let mut h = seed ^ 0x9e3779b97f4a7c15u64.wrapping_add(channel.wrapping_mul(0xbf58476d1ce4e5b9));
    h ^= t_bits.rotate_left(17).wrapping_add(0x94d049bb133111eb);
    h = h.wrapping_mul(0xbf58476d1ce4e5b9);
    h ^= (x as u64)
        .wrapping_mul(0x9e3779b185ebca87)
        .wrapping_add((y as u64).wrapping_mul(0xc2b2ae3d27d4eb4f));
    h = h.wrapping_mul(0x94d049bb133111eb);
    h ^= h >> 29;
    h = h.wrapping_mul(0xbf58476d1ce4e5b9);
    h ^= h >> 32;
    (h & 0xff) as u8
}

/// Derive the deterministic RGBA frame: same name, time, and
/// dimensions always yield the same bytes. Alpha stays opaque so
/// the frame composites as ordinary footage in the stack.
fn derive_frame(name: &str, t: f64, width: u32, height: u32) -> CustomFrame {
    let seed = name_seed(name);
    let t_bits = t.to_bits();
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            pixels.push(channel_byte(seed, t_bits, x, y, 0));
            pixels.push(channel_byte(seed, t_bits, x, y, 1));
            pixels.push(channel_byte(seed, t_bits, x, y, 2));
            pixels.push(255);
        }
    }
    CustomFrame {
        width,
        height,
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready_registry() -> CustomSourceRegistry {
        let mut reg = CustomSourceRegistry::new();
        reg.register("logo", 0.0, 4.0, 8, 6, vec![1, 2, 3, 4])
            .expect("fixture registers");
        reg.mark_ready("logo").expect("fixture readies");
        reg
    }

    #[test]
    fn registration_starts_loading() {
        let mut reg = CustomSourceRegistry::new();
        reg.register("hero", 0.0, 2.0, 4, 4, Vec::new()).unwrap();
        assert_eq!(reg.readiness("hero").unwrap(), Readiness::Loading);
        assert_eq!(reg.readiness("hero").unwrap().as_str(), "loading");
        assert!(!reg.readiness("hero").unwrap().is_ready());
        assert!(reg.is_registered("hero"));
        assert_eq!(reg.len(), 1);
        assert!(!reg.is_empty());
    }

    #[test]
    fn readiness_moves_loading_to_ready() {
        let mut reg = CustomSourceRegistry::new();
        reg.register("hero", 0.0, 2.0, 4, 4, Vec::new()).unwrap();
        reg.mark_ready("hero").unwrap();
        assert_eq!(reg.readiness("hero").unwrap(), Readiness::Ready);
        assert_eq!(reg.readiness("hero").unwrap().as_str(), "ready");
        assert!(reg.readiness("hero").unwrap().is_ready());
        // A ready source renders inside its range.
        let frame = reg.render_at("hero", 1.0).unwrap();
        assert_eq!((frame.width, frame.height), (4, 4));
        assert_eq!(frame.byte_len(), 4 * 4 * 4);
    }

    #[test]
    fn readiness_moves_loading_to_failed_with_cause() {
        let mut reg = CustomSourceRegistry::new();
        reg.register("hero", 0.0, 2.0, 4, 4, Vec::new()).unwrap();
        reg.mark_failed("hero", "unsupported magic bytes").unwrap();
        assert_eq!(
            reg.readiness("hero").unwrap(),
            Readiness::Failed {
                cause: "unsupported magic bytes".to_string()
            }
        );
        assert_eq!(reg.readiness("hero").unwrap().as_str(), "failed");
        assert!(!reg.readiness("hero").unwrap().is_ready());
        assert!(
            reg.is_registered("hero"),
            "a failed source stays registered"
        );
    }

    #[test]
    fn in_range_renders_are_byte_identical() {
        let reg = ready_registry();
        let first = reg.render_at("logo", 1.5).unwrap();
        let second = reg.render_at("logo", 1.5).unwrap();
        assert_eq!(first.pixels, second.pixels);
        assert_eq!(first.digest(), second.digest());
        // Determinism crosses registry instances: same inputs agree.
        let mut other = CustomSourceRegistry::new();
        other
            .register("logo", 0.0, 4.0, 8, 6, vec![9, 9, 9])
            .unwrap();
        other.mark_ready("logo").unwrap();
        let third = other.render_at("logo", 1.5).unwrap();
        assert_eq!(first.pixels, third.pixels, "payload never feeds pixels");
        // Distinct times still resolve to full frames.
        let moved = reg.render_at("logo", 2.5).unwrap();
        assert_eq!(moved.byte_len(), first.byte_len());
        // Alpha stays opaque for the composition stack.
        assert!(first.pixels.chunks_exact(4).all(|px| px[3] == 255));
    }

    #[test]
    fn range_gate_is_half_open() {
        let reg = ready_registry();
        assert!(reg.render_at("logo", 0.0).is_ok(), "range start renders");
        assert!(reg.render_at("logo", 3.999).is_ok());
        let at_end = reg.render_at("logo", 4.0).unwrap_err();
        assert!(
            matches!(at_end, CustomSourceError::OutOfRange { .. }),
            "{at_end}"
        );
    }

    #[test]
    fn out_of_range_refuses_naming_the_range() {
        let reg = ready_registry();
        for t in [-0.5, 4.0, 99.0, f64::NAN, f64::INFINITY] {
            let err = reg.render_at("logo", t).unwrap_err();
            match &err {
                CustomSourceError::OutOfRange {
                    name, start, end, ..
                } => {
                    assert_eq!(name, "logo");
                    assert_eq!((*start, *end), (0.0, 4.0));
                }
                other => panic!("t={t:?} refused as {other:?}, want OutOfRange"),
            }
            let text = err.to_string();
            assert!(text.contains("\"logo\""), "{text}");
            assert!(text.contains("out of range"), "{text}");
            assert!(text.contains("0.000"), "{text}");
            assert!(text.contains("4.000"), "{text}");
        }
        assert!(
            reg.is_registered("logo"),
            "refusals keep the source registered"
        );
    }

    #[test]
    fn failed_source_refuses_naming_the_cause() {
        let mut reg = CustomSourceRegistry::new();
        reg.register("hero", 0.0, 2.0, 4, 4, Vec::new()).unwrap();
        reg.mark_failed("hero", "row 7 field value is not a number")
            .unwrap();
        let err = reg.render_at("hero", 1.0).unwrap_err();
        match &err {
            CustomSourceError::Failed { name, cause } => {
                assert_eq!(name, "hero");
                assert_eq!(cause, "row 7 field value is not a number");
            }
            other => panic!("refused as {other:?}, want Failed"),
        }
        let text = err.to_string();
        assert!(text.contains("\"hero\""), "{text}");
        assert!(text.contains("row 7 field value is not a number"), "{text}");
    }

    #[test]
    fn loading_source_refuses_before_render() {
        let mut reg = CustomSourceRegistry::new();
        reg.register("hero", 0.0, 2.0, 4, 4, Vec::new()).unwrap();
        let err = reg.render_at("hero", 1.0).unwrap_err();
        assert!(matches!(err, CustomSourceError::Loading { .. }), "{err}");
        assert!(err.to_string().contains("\"hero\""));
    }

    #[test]
    fn dispose_retains_nothing_and_unregisters() {
        let mut reg = CustomSourceRegistry::new();
        reg.register("a", 0.0, 2.0, 4, 4, vec![0u8; 64]).unwrap();
        reg.register("b", 1.0, 3.0, 4, 4, vec![0u8; 32]).unwrap();
        reg.mark_ready("a").unwrap();
        reg.mark_ready("b").unwrap();
        assert_eq!(reg.retained_bytes(), 96);
        reg.dispose("a").unwrap();
        assert_eq!(reg.retained_bytes(), 32);
        assert!(!reg.is_registered("a"));
        assert!(reg.is_registered("b"));
        reg.dispose("b").unwrap();
        assert_eq!(reg.retained_bytes(), 0);
        assert!(reg.is_empty());
        // Every follow-up refuses as unregistered (STATE-04).
        assert!(matches!(
            reg.render_at("b", 1.5).unwrap_err(),
            CustomSourceError::Unregistered { .. }
        ));
        assert!(matches!(
            reg.readiness("b").unwrap_err(),
            CustomSourceError::Unregistered { .. }
        ));
        assert!(matches!(
            reg.dispose("b").unwrap_err(),
            CustomSourceError::Unregistered { .. }
        ));
        assert!(matches!(
            reg.mark_ready("b").unwrap_err(),
            CustomSourceError::Unregistered { .. }
        ));
        let text = reg.render_at("b", 1.5).unwrap_err().to_string();
        assert!(text.contains("\"b\""), "{text}");
        assert!(text.contains("not registered"), "{text}");
    }

    #[test]
    fn unknown_names_refuse_as_unregistered() {
        let mut reg = ready_registry();
        assert!(matches!(
            reg.readiness("ghost").unwrap_err(),
            CustomSourceError::Unregistered { .. }
        ));
        assert!(matches!(
            reg.render_at("ghost", 1.0).unwrap_err(),
            CustomSourceError::Unregistered { .. }
        ));
        assert!(matches!(
            reg.dispose("ghost").unwrap_err(),
            CustomSourceError::Unregistered { .. }
        ));
        assert!(matches!(
            reg.mark_ready("ghost").unwrap_err(),
            CustomSourceError::Unregistered { .. }
        ));
        assert!(matches!(
            reg.mark_failed("ghost", "nope").unwrap_err(),
            CustomSourceError::Unregistered { .. }
        ));
    }

    #[test]
    fn registration_refuses_fail_closed() {
        let mut reg = CustomSourceRegistry::new();
        // Blank names never register.
        assert!(matches!(
            reg.register("", 0.0, 1.0, 4, 4, Vec::new()).unwrap_err(),
            CustomSourceError::InvalidName { .. }
        ));
        assert!(matches!(
            reg.register("   ", 0.0, 1.0, 4, 4, Vec::new()).unwrap_err(),
            CustomSourceError::InvalidName { .. }
        ));
        // Unusable ranges never register.
        for (start, end) in [
            (1.0, 1.0),
            (2.0, 1.0),
            (-1.0, 1.0),
            (f64::NAN, 1.0),
            (0.0, f64::INFINITY),
        ] {
            let err = reg
                .register("bad-range", start, end, 4, 4, Vec::new())
                .unwrap_err();
            assert!(
                matches!(err, CustomSourceError::InvalidRange { .. }),
                "{err}"
            );
        }
        // Unusable dimensions never register.
        for (w, h) in [(0, 4), (4, 0), (MAX_CUSTOM_EDGE + 1, 4)] {
            let err = reg
                .register("bad-size", 0.0, 1.0, w, h, Vec::new())
                .unwrap_err();
            assert!(
                matches!(err, CustomSourceError::InvalidSize { .. }),
                "{err}"
            );
        }
        // A live duplicate never overwrites.
        reg.register("hero", 0.0, 2.0, 4, 4, vec![7]).unwrap();
        assert!(matches!(
            reg.register("hero", 0.0, 2.0, 4, 4, Vec::new())
                .unwrap_err(),
            CustomSourceError::Duplicate { .. }
        ));
        assert_eq!(reg.retained_bytes(), 1, "refused writes store nothing");
        // A failure with no cause never lands.
        assert!(matches!(
            reg.mark_failed("hero", "  ").unwrap_err(),
            CustomSourceError::EmptyCause { .. }
        ));
        assert_eq!(reg.readiness("hero").unwrap(), Readiness::Loading);
        assert!(!reg.is_empty());
    }
}
