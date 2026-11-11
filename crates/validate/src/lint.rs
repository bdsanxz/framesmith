//! Named lint rules (Sprint 13 check, T001 / CAP-001).
//!
//! The rule registry, severity plumbing, and report shape every later
//! check speaks. Individual rule packs (layout, motion, contrast, audio)
//! plug their detections into [`LintReport`] with their own tasks; this
//! module owns the fourteen stable rule ids, their meanings, their
//! static-vs-measured sources, and the batched report `check` and `lint`
//! both print.
//!
//! Rules are read-only analysis: findings never change pixels and never
//! block a default run. Every finding carries its rule id, severity,
//! file, line, column, and fix; a clean project reports zero findings
//! explicitly; findings batch together in one run. When a rule itself
//! cannot run (missing media, undecodable frame), the run continues and
//! the gap is recorded as a [`LintNote`] naming the rule, the cause, and
//! the workaround (REC-001).

/// Every lint rule id, in specification order. Ids are stable across
/// releases: renaming a rule is a breaking change surfaced in the
/// changelog.
pub const RULE_IDS: [&str; 16] = [
    "overlap",
    "off-canvas",
    "caption-zone",
    "caption-notes",
    "appear-by",
    "stay-inside",
    "keep-moving",
    "frozen-timeline",
    "contrast",
    "silent-audio",
    "discarded-audio",
    "unused-variable",
    "unused-style",
    "unused-theme",
    "empty-deck",
    "bad-autoplay",
];

/// Rules strict mode elevates (spec CAP-007): `--strict` turns exactly
/// these warnings into errors, `--strict-all` turns every warning into
/// an error. Named here so the release gate (T007) and the registry
/// agree; elevation itself lives with strictness, never here.
pub const STRICT_RULES: [&str; 6] = [
    "off-canvas",
    "contrast",
    "unused-variable",
    "unused-style",
    "unused-theme",
    "discarded-audio",
];

/// Stable meaning of one rule id. Every meaning is worded so a maker
/// reading a finding knows what was checked without opening the spec.
pub fn rule_description(id: &str) -> Option<&'static str> {
    Some(match id {
        "overlap" => "two opaque clips share one track and time with overlapping pixels",
        "off-canvas" => "an element lies fully outside the canvas bounds",
        "caption-zone" => "text touches the caption band (bottom 12% of the frame)",
        "caption-notes" => "a captions track drops content by note (zero-length words, empty cues, dangling overrides, cues past the film range)",
        "appear-by" => "an element with `assert-appear` is not visible by its deadline",
        "stay-inside" => "an element with `assert-inside` leaves the frame while visible",
        "keep-moving" => "an element with `assert-moving` shows no pixel change across its range",
        "frozen-timeline" => "identical frames hold still for more than 1s",
        "contrast" => "text contrast against its effective background misses WCAG AA",
        "silent-audio" => "the mix peak sits below −60 dBFS",
        "discarded-audio" => "an audio clip never contributes to the mix",
        "unused-variable" => "a `variable` declaration is never bound by `var(--id)`",
        "unused-style" => "a `style` block is never referenced by `style=\"...\"`",
        "unused-theme" => "a `theme` block is never selected by a composition or `--theme`",
        "empty-deck" => "a `slideshow` declares no slides, so present has nothing to show",
        "bad-autoplay" => "a `slideshow` autoplay duration is zero, negative, or non-finite",
        _ => return None,
    })
}

/// True for a registered rule id.
pub fn is_known_rule(id: &str) -> bool {
    RULE_IDS.contains(&id)
}

/// Where a finding's evidence comes from. Static findings read the
/// structure (overlap, off-canvas, unused); measured findings read
/// rendered pixels or probed audio (contrast, frozen spans, silence).
/// Both appear in one report with their source labeled, and when they
/// disagree the measured value wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintSource {
    Static,
    Measured,
}

impl LintSource {
    pub fn as_str(self) -> &'static str {
        match self {
            LintSource::Static => "static",
            LintSource::Measured => "measured",
        }
    }
}

impl std::fmt::Display for LintSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Evidence source of one rule id.
pub fn rule_source(id: &str) -> Option<LintSource> {
    Some(match id {
        "overlap" | "off-canvas" | "caption-zone" | "caption-notes" | "appear-by"
        | "unused-variable" | "unused-style" | "unused-theme" | "discarded-audio"
        | "empty-deck" | "bad-autoplay" => LintSource::Static,
        "stay-inside" | "keep-moving" | "frozen-timeline" | "contrast" | "silent-audio" => {
            LintSource::Measured
        }
        _ => return None,
    })
}

/// Finding severity. Every lint finding starts as a warning: warnings
/// never block a default run. Strict flags (T007) elevate warnings to
/// errors; strictness never invents findings, it only changes severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintSeverity {
    Warning,
    Error,
}

impl LintSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            LintSeverity::Warning => "warning",
            LintSeverity::Error => "error",
        }
    }
}

impl std::fmt::Display for LintSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Default severity of one rule id: warnings across the whole set, so a
/// default run reports without blocking.
pub fn default_severity(id: &str) -> Option<LintSeverity> {
    if is_known_rule(id) {
        Some(LintSeverity::Warning)
    } else {
        None
    }
}

/// Release-gate strictness (Sprint 13 check, T007 / CAP-007): which
/// warnings the strict flags elevate to errors. Off keeps every
/// warning a warning; Named (`--strict`) elevates exactly
/// [`STRICT_RULES`]; All (`--strict-all`) elevates every warning.
/// Strictness never invents findings; it only changes severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrictMode {
    /// Default runs: every warning stays a warning.
    #[default]
    Off,
    /// `--strict`: the named release-blocking set becomes errors.
    Named,
    /// `--strict-all`: every warning becomes an error.
    All,
}

impl StrictMode {
    /// Machine-readable mode name for JSON reports (`off`,
    /// `strict`, or `strict-all`).
    pub fn as_str(self) -> &'static str {
        match self {
            StrictMode::Off => "off",
            StrictMode::Named => "strict",
            StrictMode::All => "strict-all",
        }
    }

    /// CLI flag spelling that selects this mode (empty for Off,
    /// which is the absence of both flags).
    pub fn flag(self) -> &'static str {
        match self {
            StrictMode::Off => "",
            StrictMode::Named => "--strict",
            StrictMode::All => "--strict-all",
        }
    }
}

/// True when `rule` elevates to an error under `mode`. Off elevates
/// nothing; Named elevates exactly the [`STRICT_RULES`] set; All
/// elevates every registered rule. Unknown rule ids never elevate.
pub fn strict_elevates(rule: &str, mode: StrictMode) -> bool {
    match mode {
        StrictMode::Off => false,
        StrictMode::Named => STRICT_RULES.contains(&rule),
        StrictMode::All => is_known_rule(rule),
    }
}

/// Elevate every matching finding in `report` from warning to error
/// under `mode`, returning the elevated count. The finding count
/// never changes: findings already at error severity (none exist
/// yet, since every rule defaults to warning) are not double
/// counted. Notes are untouched: only warnings elevate.
pub fn apply_strictness(report: &mut LintReport, mode: StrictMode) -> usize {
    if mode == StrictMode::Off {
        return 0;
    }
    let mut elevated = 0;
    for finding in &mut report.findings {
        if strict_elevates(finding.rule, mode) && finding.severity != LintSeverity::Error {
            finding.severity = LintSeverity::Error;
            elevated += 1;
        }
    }
    elevated
}

/// One lint finding: the rule, its severity, its source, its location,
/// and its fix. Findings batch together in [`LintReport`]; each names
/// the rule and the fix.
#[derive(Debug, Clone)]
pub struct LintFinding {
    pub rule: &'static str,
    pub severity: LintSeverity,
    pub source: LintSource,
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub fix: String,
}

impl LintFinding {
    /// Build one finding for a registered rule id. Returns `None` for
    /// an unknown rule so custom rule names can never enter a report.
    pub fn new(
        rule: &'static str,
        file: &str,
        line: usize,
        column: usize,
        message: impl Into<String>,
        fix: impl Into<String>,
    ) -> Option<Self> {
        Some(Self {
            rule,
            severity: default_severity(rule)?,
            source: rule_source(rule)?,
            file: file.into(),
            line,
            column,
            message: message.into(),
            fix: fix.into(),
        })
    }
}

impl std::fmt::Display for LintFinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "lint[{}] {}:{}:{}: {} ({}): {} (fix: {})",
            self.rule,
            self.file,
            self.line,
            self.column,
            self.severity,
            self.source,
            self.message,
            self.fix
        )
    }
}

/// One unrunnable rule (REC-001): the rule that could not run, the
/// cause, and the workaround. The run continues; every runnable
/// finding is still reported. This is the only refusal path inside a
/// lint run: findings batch, notes explain gaps.
#[derive(Debug, Clone)]
pub struct LintNote {
    pub rule: &'static str,
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub cause: String,
    pub workaround: String,
}

impl LintNote {
    /// Record one unrunnable rule. Returns `None` for an unknown rule
    /// id, like [`LintFinding::new`].
    pub fn new(
        rule: &'static str,
        file: &str,
        line: usize,
        column: usize,
        cause: impl Into<String>,
        workaround: impl Into<String>,
    ) -> Option<Self> {
        if !is_known_rule(rule) {
            return None;
        }
        Some(Self {
            rule,
            file: file.into(),
            line,
            column,
            cause: cause.into(),
            workaround: workaround.into(),
        })
    }
}

impl std::fmt::Display for LintNote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "lint-note[{}] {}:{}:{}: {} (workaround: {})",
            self.rule, self.file, self.line, self.column, self.cause, self.workaround
        )
    }
}

/// The batched lint report of one run: every finding plus every
/// unrunnable-rule note, in emission order. `check` and `lint` print
/// the same report built by the same engine.
#[derive(Debug, Clone, Default)]
pub struct LintReport {
    pub findings: Vec<LintFinding>,
    pub notes: Vec<LintNote>,
}

impl LintReport {
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty() && self.notes.is_empty()
    }

    pub fn finding_count(&self) -> usize {
        self.findings.len()
    }

    /// Push one finding for a registered rule. Unknown rule ids are
    /// refused silently: the registry is closed, so a typo in a rule
    /// pack can never invent a rule.
    pub fn push(
        &mut self,
        rule: &'static str,
        file: &str,
        line: usize,
        column: usize,
        message: impl Into<String>,
        fix: impl Into<String>,
    ) {
        if let Some(f) = LintFinding::new(rule, file, line, column, message, fix) {
            self.findings.push(f);
        }
    }

    /// Record one unrunnable rule (REC-001) and keep the run going.
    pub fn note_blocked(
        &mut self,
        rule: &'static str,
        file: &str,
        line: usize,
        column: usize,
        cause: impl Into<String>,
        workaround: impl Into<String>,
    ) {
        if let Some(n) = LintNote::new(rule, file, line, column, cause, workaround) {
            self.notes.push(n);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_holds_sixteen_unique_rules_with_stable_meanings() {
        assert_eq!(RULE_IDS.len(), 16);
        let mut seen = std::collections::BTreeSet::new();
        for id in RULE_IDS {
            assert!(seen.insert(id), "duplicate rule id {id:?}");
            let meaning = rule_description(id);
            assert!(
                meaning.is_some_and(|m| !m.is_empty()),
                "rule {id:?} carries no stable meaning"
            );
        }
        assert!(rule_description("bogus-rule").is_none());
        assert!(!is_known_rule("bogus-rule"));
    }

    #[test]
    fn registry_covers_static_and_measured_sources() {
        let mut static_count = 0;
        let mut measured_count = 0;
        for id in RULE_IDS {
            match rule_source(id) {
                Some(LintSource::Static) => static_count += 1,
                Some(LintSource::Measured) => measured_count += 1,
                None => panic!("rule {id:?} has no source"),
            }
        }
        assert!(static_count > 0, "no static rule registered");
        assert!(measured_count > 0, "no measured rule registered");
        assert_eq!(static_count + measured_count, RULE_IDS.len());
    }

    #[test]
    fn every_rule_defaults_to_warning() {
        for id in RULE_IDS {
            assert_eq!(default_severity(id), Some(LintSeverity::Warning));
        }
        assert_eq!(default_severity("bogus-rule"), None);
    }

    #[test]
    fn strict_set_names_six_registered_rules() {
        assert_eq!(STRICT_RULES.len(), 6);
        for id in STRICT_RULES {
            assert!(is_known_rule(id), "strict rule {id:?} is not registered");
        }
        for expected in [
            "off-canvas",
            "contrast",
            "unused-variable",
            "unused-style",
            "unused-theme",
            "discarded-audio",
        ] {
            assert!(STRICT_RULES.contains(&expected));
        }
    }

    #[test]
    fn every_finding_carries_rule_severity_location_source_and_fix() {
        let f = LintFinding::new(
            "overlap",
            "proj/composition.kdl",
            3,
            5,
            "clips overlap",
            "move one clip",
        )
        .unwrap();
        assert_eq!(f.rule, "overlap");
        assert_eq!(f.severity, LintSeverity::Warning);
        assert_eq!(f.source, LintSource::Static);
        assert_eq!(f.file, "proj/composition.kdl");
        assert_eq!((f.line, f.column), (3, 5));
        assert!(!f.message.is_empty());
        assert!(!f.fix.is_empty());
        let text = f.to_string();
        assert!(text.contains("lint[overlap]"), "{text}");
        assert!(text.contains("warning"), "{text}");
        assert!(text.contains("(static)"), "{text}");
        assert!(text.contains("fix:"), "{text}");
        assert!(LintFinding::new("bogus", "f", 1, 1, "m", "x").is_none());
    }

    #[test]
    fn blocked_rule_leaves_a_named_note() {
        let n = LintNote::new(
            "contrast",
            "proj/composition.kdl",
            7,
            2,
            "background is an animated gradient, no single ratio to measure",
            "re-run against a still frame",
        )
        .unwrap();
        let text = n.to_string();
        assert!(text.contains("lint-note[contrast]"), "{text}");
        assert!(text.contains("animated gradient"), "{text}");
        assert!(text.contains("workaround:"), "{text}");
        assert!(LintNote::new("bogus", "f", 1, 1, "c", "w").is_none());
    }

    #[test]
    fn strict_modes_elevate_without_inventing_findings() {
        for mode in [StrictMode::Off, StrictMode::Named, StrictMode::All] {
            assert_eq!(mode, mode);
        }
        assert_eq!(StrictMode::default(), StrictMode::Off);
        assert_eq!(StrictMode::Off.as_str(), "off");
        assert_eq!(StrictMode::Named.as_str(), "strict");
        assert_eq!(StrictMode::All.as_str(), "strict-all");
        assert_eq!(StrictMode::Named.flag(), "--strict");
        assert_eq!(StrictMode::All.flag(), "--strict-all");
        // Off elevates nothing; Named elevates exactly the strict
        // set; All elevates every registered rule and nothing else.
        for id in RULE_IDS {
            assert!(!strict_elevates(id, StrictMode::Off), "{id}");
            assert_eq!(
                strict_elevates(id, StrictMode::Named),
                STRICT_RULES.contains(&id),
                "{id}"
            );
            assert!(strict_elevates(id, StrictMode::All), "{id}");
        }
        assert!(!strict_elevates("bogus-rule", StrictMode::All));
        // Elevation changes severity only: counts hold, notes hold.
        let mut report = LintReport::default();
        report.push("off-canvas", "f", 1, 1, "m1", "x1");
        report.push("overlap", "f", 2, 3, "m2", "x2");
        report.note_blocked("contrast", "f", 1, 1, "c", "w");
        assert_eq!(apply_strictness(&mut report, StrictMode::Off), 0);
        assert!(report
            .findings
            .iter()
            .all(|f| f.severity == LintSeverity::Warning));
        assert_eq!(apply_strictness(&mut report, StrictMode::Named), 1);
        assert_eq!(report.finding_count(), 2);
        assert_eq!(report.notes.len(), 1);
        assert_eq!(report.findings[0].severity, LintSeverity::Error);
        assert_eq!(report.findings[1].severity, LintSeverity::Warning);
        assert_eq!(apply_strictness(&mut report, StrictMode::All), 1);
        assert!(report
            .findings
            .iter()
            .all(|f| f.severity == LintSeverity::Error));
        assert_eq!(report.finding_count(), 2);
        // Re-applying never double counts.
        assert_eq!(apply_strictness(&mut report, StrictMode::All), 0);
    }

    #[test]
    fn findings_batch_and_notes_batch_alongside() {
        let mut report = LintReport::default();
        assert!(report.is_clean());
        report.push("overlap", "f", 1, 1, "m1", "x1");
        report.push("contrast", "f", 2, 3, "m2", "x2");
        report.note_blocked(
            "frozen-timeline",
            "f",
            1,
            1,
            "no frames decoded",
            "fix the media",
        );
        assert!(!report.is_clean());
        assert_eq!(report.finding_count(), 2);
        assert_eq!(report.findings[0].rule, "overlap");
        assert_eq!(report.findings[1].rule, "contrast");
        assert_eq!(report.notes.len(), 1);
        // Unknown rule ids never enter the report.
        report.push("bogus", "f", 1, 1, "m", "x");
        report.note_blocked("bogus", "f", 1, 1, "c", "w");
        assert_eq!(report.finding_count(), 2);
        assert_eq!(report.notes.len(), 1);
    }
}
