//! Intent router (Sprint 20, T003 / CAP-003, J-02, STATE-03, INT-001).
//!
//! The router turns plain words into the right creation workflow. It
//! reads the request plus project state (`brief.md` and
//! `storyboard.md` when a project folder is given), runs the intent
//! interview (saved preferences via `--pref`, saved recipes via
//! `--recipe`, one question per must-have), pitches five concepts,
//! offers run-shape choices, and names the chosen workflow with
//! reasons. A creation workflow skill installs only when the request
//! needs one (`--install` writes exactly that workflow's files into a
//! skill location). An ambiguous request offers the general workflow
//! plus a Not-sure path instead of guessing; a corrupt brief is
//! reported with a fresh brief offered.
//!
//! All text here is framesmith's own clean-room writing: it names
//! only commands this build ships and never promises capabilities
//! this build lacks. Steps the build cannot do stop naming the
//! capability instead of simulating it.
//!
//! Read-only boundary: routing never writes into the project folder.
//! It only reads `brief.md` / `storyboard.md`, so no staged receipt
//! and no Sprint 15 patch/history turn is produced here, and default
//! renders stay byte-identical (nothing the renderer reads changes).
//! The only writes this slice performs are skill-location installs
//! via the skills installer (same boundary as install/update: skill
//! locations only, never project content).

use std::path::{Path, PathBuf};

use super::skills;

/// One must-have slot for a creation workflow: the answer key a saved
/// preference or recipe can fill, and the question asked when pending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MustHave {
    /// Answer key (`--pref key=value` matches this exactly).
    pub key: &'static str,
    /// The single interview question for this slot.
    pub question: &'static str,
}

/// One routable creation workflow: its skill name, the request words
/// that select it, and its interview slots (one question per slot).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkflowRule {
    /// Creation workflow skill name (exact).
    pub name: &'static str,
    /// Request phrases that vote for this workflow (singular; a
    /// trailing `s` plural also matches).
    pub keywords: &'static [&'static str],
    /// Must-have interview slots.
    pub must_haves: &'static [MustHave],
}

/// The ten creation workflows the router can name, in skill order.
pub const WORKFLOWS: &[WorkflowRule] = &[
    WorkflowRule {
        name: "product-launch",
        keywords: &[
            "product launch",
            "launch",
            "product",
            "release",
            "announce",
            "promo",
        ],
        must_haves: &[
            MustHave {
                key: "product",
                question: "Which product is the film about?",
            },
            MustHave {
                key: "audience",
                question: "Who is the audience?",
            },
            MustHave {
                key: "promise",
                question: "What one promise should it land?",
            },
        ],
    },
    WorkflowRule {
        name: "faceless-explainer",
        keywords: &[
            "explainer",
            "tutorial",
            "how-to",
            "how to",
            "walkthrough",
            "voice-free",
            "voice free",
        ],
        must_haves: &[
            MustHave {
                key: "topic",
                question: "What topic does it explain?",
            },
            MustHave {
                key: "duration",
                question: "How long should it run?",
            },
        ],
    },
    WorkflowRule {
        name: "pr-to-video",
        keywords: &["pull request", "diff", "changelog", "commit", "merge"],
        must_haves: &[
            MustHave {
                key: "sources",
                question: "Which diff or change notes should it cover?",
            },
            MustHave {
                key: "audience",
                question: "Who is it for (reviewers or makers)?",
            },
        ],
    },
    WorkflowRule {
        name: "embedded-captions",
        keywords: &["caption", "subtitle"],
        must_haves: &[
            MustHave {
                key: "lines",
                question: "Where do the caption lines with times come from?",
            },
            MustHave {
                key: "language",
                question: "Which language and line length?",
            },
        ],
    },
    WorkflowRule {
        name: "talking-head-recut",
        keywords: &[
            "talking head",
            "talking-head",
            "recut",
            "interview footage",
            "interview",
        ],
        must_haves: &[
            MustHave {
                key: "footage",
                question: "Which talking-head footage and keep notes?",
            },
            MustHave {
                key: "length",
                question: "What target length?",
            },
        ],
    },
    WorkflowRule {
        name: "motion-graphics",
        keywords: &[
            "motion graphic",
            "motion-graphic",
            "kinetic",
            "title sequence",
        ],
        must_haves: &[
            MustHave {
                key: "beats",
                question: "What beat list or script drives it?",
            },
            MustHave {
                key: "duration",
                question: "What duration budget?",
            },
        ],
    },
    WorkflowRule {
        name: "music-to-video",
        keywords: &["music", "song", "soundtrack", "beat", "band"],
        must_haves: &[
            MustHave {
                key: "track",
                question: "Which music track?",
            },
            MustHave {
                key: "beats",
                question: "What edit marks go with it?",
            },
            MustHave {
                key: "length",
                question: "What target length?",
            },
        ],
    },
    WorkflowRule {
        name: "slideshow",
        keywords: &["slideshow", "slide", "still", "photo", "gallery"],
        must_haves: &[
            MustHave {
                key: "stills",
                question: "Which stills in which order?",
            },
            MustHave {
                key: "holds",
                question: "How long is each hold?",
            },
            MustHave {
                key: "length",
                question: "What target length?",
            },
        ],
    },
    WorkflowRule {
        name: "general-video",
        keywords: &[],
        must_haves: &[
            MustHave {
                key: "goal",
                question: "What should the video do?",
            },
            MustHave {
                key: "duration",
                question: "How long should it run?",
            },
        ],
    },
    WorkflowRule {
        name: "migration",
        keywords: &[
            "migration",
            "migrate",
            "import",
            "convert",
            "foreign",
            "remake",
        ],
        must_haves: &[
            MustHave {
                key: "sources",
                question: "Which foreign project shape is being left behind?",
            },
            MustHave {
                key: "timing",
                question: "What timing must the rebuild keep?",
            },
            MustHave {
                key: "palette",
                question: "What palette must the rebuild keep?",
            },
        ],
    },
];

/// Look up a workflow rule by exact skill name.
pub fn find_workflow(name: &str) -> Option<&'static WorkflowRule> {
    WORKFLOWS.iter().find(|w| w.name == name)
}

/// Workflow names in routing order.
pub fn workflow_list_line() -> String {
    WORKFLOWS
        .iter()
        .map(|w| w.name.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Lowercase, squeeze every non-alphanumeric run to one space, and pad
/// with spaces so whole-phrase matching respects word boundaries.
fn normalize(text: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    format!(" {} ", words.join(" "))
}

/// True when the normalized haystack holds the phrase as whole words.
/// A trailing `s` plural on the last word also matches, so a `photo`
/// request still votes for the keyword `photo` and a `captions`
/// request still votes for `caption`.
fn contains_phrase(hay: &str, phrase: &str) -> bool {
    let needle = format!(" {phrase} ");
    if hay.contains(&needle) {
        return true;
    }
    let mut words: Vec<&str> = phrase.split_whitespace().collect();
    if let Some(last) = words.pop() {
        if !last.ends_with('s') {
            let mut plural = words.join(" ");
            if !plural.is_empty() {
                plural.push(' ');
            }
            plural.push_str(last);
            plural.push('s');
            if hay.contains(&format!(" {plural} ")) {
                return true;
            }
        }
    }
    false
}

/// Score every workflow against the request text: the matched keywords
/// per workflow, keeping routing order. `general-video` never scores
/// (it is the fallback offer, never a match).
pub fn score_request(text: &str) -> Vec<(&'static WorkflowRule, Vec<&'static str>)> {
    let hay = normalize(text);
    let mut out = Vec::new();
    for rule in WORKFLOWS {
        let mut matched = Vec::new();
        for kw in rule.keywords {
            if contains_phrase(&hay, kw) {
                matched.push(*kw);
            }
        }
        if !matched.is_empty() {
            out.push((rule, matched));
        }
    }
    out
}

// --- Project state --------------------------------------------------
// The router reads `brief.md` and `storyboard.md` when a project
// folder is given. A valid brief carries a `Request:` line; anything
// else on disk is reported as corrupt and offered a fresh start, and
// the corrupt files are never touched (resume keeps them for
// inspection). The storyboard schema belongs to the storyboard slice:
// here its presence and title line only inform the choice.

/// Conventional brief file inside a project folder.
pub const BRIEF_FILE: &str = "brief.md";
/// Conventional storyboard file inside a project folder.
pub const STORYBOARD_FILE: &str = "storyboard.md";

/// What the router found for `brief.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BriefState {
    /// No brief file; the interview starts fresh.
    Absent,
    /// A valid brief carrying this request text.
    Ok { request: String },
    /// Present but unusable; the message names why.
    Corrupt { reason: String },
}

/// What the router found for `storyboard.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoryState {
    /// No storyboard file.
    Absent,
    /// Present; the title is its first non-empty line.
    Present { title: String },
    /// Present but unreadable; the message names why.
    Unreadable { reason: String },
}

/// Read `brief.md` under `folder` without writing anything.
pub fn read_brief(folder: &Path) -> BriefState {
    let path = folder.join(BRIEF_FILE);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return BriefState::Absent,
        Err(e) => {
            return BriefState::Corrupt {
                reason: format!("cannot be read: {e}"),
            };
        }
    };
    let body = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => {
            return BriefState::Corrupt {
                reason: "is not valid UTF-8".to_string(),
            };
        }
    };
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Request:") {
            let request = rest.trim().to_string();
            if request.is_empty() {
                return BriefState::Corrupt {
                    reason: "its `Request:` line is empty".to_string(),
                };
            }
            return BriefState::Ok { request };
        }
    }
    BriefState::Corrupt {
        reason: "it has no `Request:` line".to_string(),
    }
}

/// Read `storyboard.md` under `folder` without writing anything.
pub fn read_storyboard(folder: &Path) -> StoryState {
    let path = folder.join(STORYBOARD_FILE);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return StoryState::Absent,
        Err(e) => {
            return StoryState::Unreadable {
                reason: format!("cannot be read: {e}"),
            };
        }
    };
    let body = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => {
            return StoryState::Unreadable {
                reason: "is not valid UTF-8".to_string(),
            };
        }
    };
    let title = body
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("(empty storyboard)")
        .to_string();
    StoryState::Present { title }
}

// --- Preferences, recipes, run shapes -------------------------------

/// One run shape the maker can pick: what a routed workflow produces
/// once its approvals land. Every shape names only shipped verbs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunShape {
    /// One MP4 after storyboard and preview approval (`render`).
    Single,
    /// One MP4 per rows file after approvals (`render --batch`).
    Batch,
    /// Board plus frame stills only, no render (`snapshot`).
    StillsOnly,
}

impl RunShape {
    /// Parse the `--shape` value (exact, case-sensitive).
    pub fn parse(name: &str) -> Option<RunShape> {
        match name {
            "single" => Some(RunShape::Single),
            "batch" => Some(RunShape::Batch),
            "stills-only" => Some(RunShape::StillsOnly),
            _ => None,
        }
    }

    /// The `--shape` spelling.
    pub fn name(self) -> &'static str {
        match self {
            RunShape::Single => "single",
            RunShape::Batch => "batch",
            RunShape::StillsOnly => "stills-only",
        }
    }

    /// One-line meaning for listings.
    pub fn blurb(self) -> &'static str {
        match self {
            RunShape::Single => "one MP4 after approvals (`render`)",
            RunShape::Batch => "one MP4 per rows file after approvals (`render --batch`)",
            RunShape::StillsOnly => "board plus frame stills only, no render (`snapshot`)",
        }
    }
}

/// Every run shape in listing order.
pub const RUN_SHAPES: &[RunShape] = &[RunShape::Single, RunShape::Batch, RunShape::StillsOnly];

/// Comma-separated run-shape list for listings and refusals.
pub fn shape_list_line() -> String {
    RUN_SHAPES
        .iter()
        .map(|s| s.name().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// A saved recipe: a named bundle of interview answers plus a default
/// run shape. Explicit `--pref` answers win over recipe answers, and
/// an explicit `--shape` wins over the recipe shape.
#[derive(Debug, Clone, Copy)]
pub struct Recipe {
    /// `--recipe` name (exact, case-sensitive).
    pub name: &'static str,
    /// One-line purpose shown in listings.
    pub blurb: &'static str,
    /// Interview answers as `(must-have key, answer)`.
    pub answers: &'static [(&'static str, &'static str)],
    /// Default run shape this recipe assumes.
    pub shape: RunShape,
}

/// Every saved recipe, in listing order.
pub const RECIPES: &[Recipe] = &[
    Recipe {
        name: "social-30s",
        blurb: "a 30-second social cut for the feed",
        answers: &[("duration", "30 seconds"), ("audience", "social feed")],
        shape: RunShape::Single,
    },
    Recipe {
        name: "deep-dive",
        blurb: "a 3-minute patient explainer",
        answers: &[("duration", "3 minutes"), ("audience", "patient viewers")],
        shape: RunShape::Single,
    },
    Recipe {
        name: "batch-variants",
        blurb: "one cut per row for variant releases",
        answers: &[("length", "per-row cut")],
        shape: RunShape::Batch,
    },
];

/// Look up a recipe by exact name.
pub fn find_recipe(name: &str) -> Option<&'static Recipe> {
    RECIPES.iter().find(|r| r.name == name)
}

/// Comma-separated recipe list for listings and refusals.
pub fn recipe_list_line() -> String {
    RECIPES
        .iter()
        .map(|r| format!("{} ({})", r.name, r.blurb))
        .collect::<Vec<_>>()
        .join(", ")
}

// --- Interview ------------------------------------------------------

/// One interview question: exactly one per must-have slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    /// Must-have key this question fills.
    pub key: String,
    /// The question text.
    pub question: String,
    /// Where the answer came from: `preference`, `recipe`, or pending.
    pub answered_by: String,
    /// The saved answer, when one filled this slot.
    pub answer: Option<String>,
}

/// Build the interview for `workflow`: one question per must-have.
/// Explicit `--pref` answers fill their slots first, then the recipe
/// answers fill what is left; the rest stay pending for the maker.
pub fn interview_for(
    workflow: &WorkflowRule,
    prefs: &[(String, String)],
    recipe: Option<&Recipe>,
) -> Vec<Question> {
    workflow
        .must_haves
        .iter()
        .map(|slot| {
            if let Some((_, value)) = prefs.iter().find(|(k, _)| k == slot.key) {
                return Question {
                    key: slot.key.to_string(),
                    question: slot.question.to_string(),
                    answered_by: "preference".to_string(),
                    answer: Some(value.clone()),
                };
            }
            if let Some(r) = recipe {
                if let Some((_, value)) = r.answers.iter().find(|(k, _)| *k == slot.key) {
                    return Question {
                        key: slot.key.to_string(),
                        question: slot.question.to_string(),
                        answered_by: format!("recipe {}", r.name),
                        answer: Some(value.to_string()),
                    };
                }
            }
            Question {
                key: slot.key.to_string(),
                question: slot.question.to_string(),
                answered_by: "pending".to_string(),
                answer: None,
            }
        })
        .collect()
}

// --- Five-concept pitch ----------------------------------------------

/// One pitched concept: a creative proposal, clearly the router's
/// own, never presented as a project fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Concept {
    /// Concept title.
    pub title: String,
    /// One-line hook tying the concept to the request.
    pub hook: String,
}

/// First words of the request used to ground each concept.
fn short_request(request: &str) -> String {
    let words: Vec<&str> = request.split_whitespace().collect();
    if words.len() <= 10 {
        return words.join(" ");
    }
    format!("{} …", words[..10].join(" "))
}

/// Pitch exactly five concepts for `workflow_name`: five fixed angles
/// grounded in the request words and the workflow's own purpose, so
/// every route pitches before it chooses.
pub fn pitch_concepts(workflow_name: &str, request: &str) -> Vec<Concept> {
    let short = short_request(request);
    let purpose = skills::find_skill(workflow_name)
        .map(|s| s.purpose)
        .unwrap_or("a video the maker asked for");
    vec![
        Concept {
            title: "Cold open on the promise".to_string(),
            hook: format!("Proposal: open on the promise of {short:?}, shaped as {purpose}."),
        },
        Concept {
            title: "One idea in three beats".to_string(),
            hook: format!("Proposal: tell {short:?} in three beats under {workflow_name}."),
        },
        Concept {
            title: "Built from the local shelves".to_string(),
            hook: format!(
                "Proposal: dress {short:?} with pieces from the offline catalog, still {purpose}."
            ),
        },
        Concept {
            title: "Quiet middle, loud close".to_string(),
            hook: format!(
                "Proposal: hold {short:?} back, then close loud; {workflow_name} carries the timing."
            ),
        },
        Concept {
            title: "The honest minimum".to_string(),
            hook: format!(
                "Proposal: the smallest {workflow_name} that still answers {short:?} ({purpose})."
            ),
        },
    ]
}

// --- Route decision ---------------------------------------------------

/// The routing outcome: either a named workflow with reasons, or the
/// ambiguity offer (the general workflow plus a Not-sure path).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteDecision {
    /// A workflow was chosen; the reasons name why.
    Chosen {
        workflow: &'static WorkflowRule,
        reasons: Vec<String>,
    },
    /// No workflow won; offer the fallback instead of guessing.
    Ambiguous { reasons: Vec<String> },
}

/// Decide the workflow from the scored matches. A strict winner is
/// named with its matched keywords; a tie or an empty field is
/// ambiguous, never a guess.
pub fn decide(scored: &[(&'static WorkflowRule, Vec<&'static str>)]) -> RouteDecision {
    let mut ranked: Vec<(&'static WorkflowRule, usize, Vec<&'static str>)> = scored
        .iter()
        .map(|(rule, matched)| (*rule, matched.len(), matched.clone()))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1));
    match ranked.as_slice() {
        [] => RouteDecision::Ambiguous {
            reasons: vec!["no workflow keywords matched the request".to_string()],
        },
        [(rule, _, matched)] => {
            let quoted = matched
                .iter()
                .map(|k| format!("{k:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            RouteDecision::Chosen {
                workflow: rule,
                reasons: vec![format!("request mentions {quoted}")],
            }
        }
        [(first, top, first_matched), (second, runner, _), ..] => {
            if top > runner {
                let quoted = first_matched
                    .iter()
                    .map(|k| format!("{k:?}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                RouteDecision::Chosen {
                    workflow: first,
                    reasons: vec![
                        format!("request mentions {quoted}"),
                        format!("{} outranks {}", first.name, second.name),
                    ],
                }
            } else {
                RouteDecision::Ambiguous {
                    reasons: vec![format!(
                        "matches {} and {} equally; needs a maker pick, not a guess",
                        first.name, second.name
                    )],
                }
            }
        }
    }
}

// --- Route report -----------------------------------------------------

/// Everything one `route` run learned, rendered as human text or JSON.
pub struct RouteReport {
    /// The maker's current words.
    pub request: String,
    /// Project folder, when one was given.
    pub folder: Option<PathBuf>,
    /// What `brief.md` held (`ignored` when `--fresh` or no folder).
    pub brief: String,
    /// The brief's own request line, when valid.
    pub brief_request: Option<String>,
    /// What `storyboard.md` held (`ignored` when `--fresh` or no folder).
    pub storyboard: String,
    /// Saved preferences as given.
    pub prefs: Vec<(String, String)>,
    /// Saved recipe, when one was named.
    pub recipe: Option<&'static Recipe>,
    /// Run shape and where it came from.
    pub shape: RunShape,
    /// Why this run shape.
    pub shape_source: String,
    /// One question per must-have.
    pub questions: Vec<Question>,
    /// Exactly five pitched concepts.
    pub concepts: Vec<Concept>,
    /// The named workflow with reasons, or none when ambiguous.
    pub choice: Option<(String, Vec<String>)>,
    /// The ambiguity offer: the general workflow plus a Not-sure path.
    pub offered: Vec<String>,
    /// Extra notes (brief drift, storyboard presence, install hints).
    pub notes: Vec<String>,
    /// Install outcome, when `--install` ran.
    pub install: Option<String>,
}

/// Usage error for `route`: prints usage, exits 2 like every other
/// CLI usage error.
fn route_usage_error(msg: &str) -> ! {
    eprintln!("framesmith route: {msg}.");
    eprintln!(
        "usage: framesmith route --request <text> [<folder>] [--pref <key=value> ...] [--recipe <name>] [--shape single|batch|stills-only] [--workflow <name>] [--fresh] [--install --host <host> [--dest <dir>]] [--json]"
    );
    std::process::exit(2);
}

/// Refusal for an unknown workflow: exit 1 listing every workflow.
fn refuse_unknown_workflow(name: &str) -> i32 {
    eprintln!(
        "framesmith route refused: unknown workflow {name:?}.\n\
         available workflows: {}.\n\
         fix: pass one of those names with `--workflow`; nothing was installed or changed.",
        workflow_list_line()
    );
    1
}

/// Refusal for an unknown recipe: exit 1 listing every recipe.
fn refuse_unknown_recipe(name: &str) -> i32 {
    eprintln!(
        "framesmith route refused: unknown recipe {name:?}.\n\
         available recipes: {}.\n\
         fix: pass one of those names with `--recipe`; nothing was installed or changed.",
        recipe_list_line()
    );
    1
}

/// Render one route report as human text.
pub fn format_report_human(r: &RouteReport) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(out, "request: {:?}", r.request);
    match &r.folder {
        Some(folder) => {
            let _ = writeln!(
                out,
                "project: {} (brief: {}; storyboard: {})",
                folder.display(),
                r.brief,
                r.storyboard
            );
        }
        None => {
            let _ = writeln!(
                out,
                "project: none (no folder given; brief and storyboard ignored)"
            );
        }
    }
    if r.prefs.is_empty() {
        let _ = writeln!(out, "preferences: none");
    } else {
        let prefs = r
            .prefs
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(out, "preferences: {prefs}");
    }
    match r.recipe {
        Some(recipe) => {
            let _ = writeln!(out, "recipe: {} ({})", recipe.name, recipe.blurb);
        }
        None => {
            let _ = writeln!(out, "recipe: none");
        }
    }
    let _ = writeln!(
        out,
        "run shape: {} ({}) [choices: {}]",
        r.shape.name(),
        r.shape.blurb(),
        shape_list_line()
    );
    let _ = writeln!(
        out,
        "interview ({} question(s), one per must-have):",
        r.questions.len()
    );
    for (i, q) in r.questions.iter().enumerate() {
        match &q.answer {
            Some(answer) => {
                let _ = writeln!(
                    out,
                    "  {}. [{}] {} Answer: {:?} (from {})",
                    i + 1,
                    q.key,
                    q.question,
                    answer,
                    q.answered_by
                );
            }
            None => {
                let _ = writeln!(out, "  {}. [{}] {} (pending)", i + 1, q.key, q.question);
            }
        }
    }
    let _ = writeln!(
        out,
        "pitch ({} concept(s), proposals, not facts):",
        r.concepts.len()
    );
    for (i, c) in r.concepts.iter().enumerate() {
        let _ = writeln!(out, "  {}. {} — {}", i + 1, c.title, c.hook);
    }
    match &r.choice {
        Some((workflow, reasons)) => {
            let _ = writeln!(out, "chosen workflow: {workflow}");
            for reason in reasons {
                let _ = writeln!(out, "  reason: {reason}");
            }
        }
        None => {
            let _ = writeln!(out, "chosen workflow: none (ambiguous; not guessing)");
            let _ = writeln!(
                out,
                "offer: {} — answer its questions and re-run, or stay Not-sure and reply in words",
                r.offered.join(", ")
            );
        }
    }
    for note in &r.notes {
        let _ = writeln!(out, "note: {note}");
    }
    if let Some(install) = &r.install {
        let _ = writeln!(out, "{install}");
    }
    out
}

/// Render one route report as JSON.
pub fn format_report_json(r: &RouteReport) -> serde_json::Value {
    let questions: Vec<serde_json::Value> = r
        .questions
        .iter()
        .map(|q| {
            serde_json::json!({
                "key": q.key,
                "question": q.question,
                "answered_by": q.answered_by,
                "answer": q.answer,
            })
        })
        .collect();
    let concepts: Vec<serde_json::Value> = r
        .concepts
        .iter()
        .map(|c| {
            serde_json::json!({
                "title": c.title,
                "hook": c.hook,
            })
        })
        .collect();
    let prefs: Vec<serde_json::Value> = r
        .prefs
        .iter()
        .map(|(k, v)| serde_json::json!({ "key": k, "value": v }))
        .collect();
    serde_json::json!({
        "request": r.request,
        "folder": r.folder.as_ref().map(|f| f.display().to_string()),
        "brief": r.brief,
        "brief_request": r.brief_request,
        "storyboard": r.storyboard,
        "preferences": prefs,
        "recipe": r.recipe.map(|x| x.name),
        "run_shape": r.shape.name(),
        "run_shape_source": r.shape_source,
        "interview": questions,
        "concepts": concepts,
        "choice": r.choice.as_ref().map(|(w, reasons)| serde_json::json!({
            "workflow": w,
            "reasons": reasons,
        })),
        "offered": r.offered,
        "notes": r.notes,
        "install": r.install,
    })
}

/// Install exactly the chosen workflow's files into the host skill
/// location. Missing or drifted files are written (verified against
/// the manifest afterwards); a clean install reports nothing to do.
/// Only the chosen workflow is touched, never the whole set.
fn install_workflow(workflow: &str, host: &str, dest_override: Option<&str>) -> i32 {
    if skills::find_host(host).is_none() {
        eprintln!(
            "framesmith route refused: unknown host {host:?}.\n\
             supported hosts: {}.\n\
             fix: pass one of those names with `--host`; nothing was installed.",
            skills::host_list_line()
        );
        return 1;
    }
    let Some(dest) = skills::resolve_dest(host, dest_override) else {
        eprintln!(
            "framesmith route refused: cannot locate the skill directory for host {host:?} \
             (no home directory found).\n\
             fix: pass `--dest <dir>`; nothing was installed."
        );
        return 1;
    };
    let scope = skills::UpdateScope::One(workflow.to_string());
    let paths = skills::scope_paths(&scope);
    let drift = skills::compute_drift(&dest, &paths);
    if drift.is_clean() {
        println!(
            "route: workflow {workflow:?} is already installed for host {host:?} at {}; nothing to do.",
            dest.display()
        );
        return 0;
    }
    match skills::write_files(&dest, &paths) {
        Ok(written) => {
            println!(
                "route: installed workflow {workflow:?} for host {host:?} at {}: {}.",
                dest.display(),
                written.join(", ")
            );
            0
        }
        Err((path, detail)) => {
            eprintln!(
                "framesmith route refused: cannot write to {path}: {detail}.\n\
                 fix: choose a writable location (or pass `--dest <dir>`); nothing else was changed."
            );
            1
        }
    }
}

/// Parsed `route` flags.
struct RouteArgs {
    request: String,
    folder: Option<PathBuf>,
    prefs: Vec<(String, String)>,
    recipe: Option<String>,
    shape: Option<RunShape>,
    workflow: Option<String>,
    fresh: bool,
    install: bool,
    host: Option<String>,
    dest: Option<String>,
    json: bool,
}

/// Parse `route` arguments. Usage mistakes exit 2.
fn parse_route_args(args: &[String]) -> RouteArgs {
    let mut request: Option<String> = None;
    let mut folder: Option<PathBuf> = None;
    let mut prefs: Vec<(String, String)> = Vec::new();
    let mut recipe: Option<String> = None;
    let mut shape: Option<RunShape> = None;
    let mut workflow: Option<String> = None;
    let mut fresh = false;
    let mut install = false;
    let mut host: Option<String> = None;
    let mut dest: Option<String> = None;
    let mut json = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--request" => {
                i += 1;
                if i >= args.len() {
                    route_usage_error("--request needs a value (the maker's words)");
                }
                if request.is_some() {
                    route_usage_error("duplicate --request flag");
                }
                if args[i].trim().is_empty() {
                    route_usage_error("--request needs a non-empty value");
                }
                request = Some(args[i].clone());
            }
            "--pref" => {
                i += 1;
                if i >= args.len() {
                    route_usage_error("--pref needs a value (<key=value>)");
                }
                let raw = &args[i];
                let (key, value) = match raw.split_once('=') {
                    Some((k, v)) if !k.trim().is_empty() && !v.trim().is_empty() => {
                        (k.trim().to_string(), v.trim().to_string())
                    }
                    _ => route_usage_error(
                        "--pref needs a value (<key=value> with both sides non-empty)",
                    ),
                };
                if prefs.iter().any(|(k, _)| k == &key) {
                    route_usage_error(&format!("duplicate --pref key {key:?}"));
                }
                prefs.push((key, value));
            }
            "--recipe" => {
                i += 1;
                if i >= args.len() {
                    route_usage_error("--recipe needs a value (a recipe name)");
                }
                if recipe.is_some() {
                    route_usage_error("duplicate --recipe flag");
                }
                recipe = Some(args[i].clone());
            }
            "--shape" => {
                i += 1;
                if i >= args.len() {
                    route_usage_error("--shape needs a value (single, batch, or stills-only)");
                }
                if shape.is_some() {
                    route_usage_error("duplicate --shape flag");
                }
                match RunShape::parse(&args[i]) {
                    Some(s) => shape = Some(s),
                    None => route_usage_error(&format!(
                        "unknown run shape {:?}; expected {}",
                        args[i],
                        shape_list_line()
                    )),
                }
            }
            "--workflow" => {
                i += 1;
                if i >= args.len() {
                    route_usage_error("--workflow needs a value (a workflow name)");
                }
                if workflow.is_some() {
                    route_usage_error("duplicate --workflow flag");
                }
                workflow = Some(args[i].clone());
            }
            "--fresh" => {
                if fresh {
                    route_usage_error("duplicate --fresh flag");
                }
                fresh = true;
            }
            "--install" => {
                if install {
                    route_usage_error("duplicate --install flag");
                }
                install = true;
            }
            "--host" => {
                i += 1;
                if i >= args.len() {
                    route_usage_error("--host needs a value (an agent host name)");
                }
                if host.is_some() {
                    route_usage_error("duplicate --host flag");
                }
                host = Some(args[i].clone());
            }
            "--dest" => {
                i += 1;
                if i >= args.len() {
                    route_usage_error("--dest needs a value (a skill directory)");
                }
                if dest.is_some() {
                    route_usage_error("duplicate --dest flag");
                }
                dest = Some(args[i].clone());
            }
            "--json" => {
                if json {
                    route_usage_error("duplicate --json flag");
                }
                json = true;
            }
            other if other.starts_with('-') => {
                route_usage_error(&format!("unknown flag {other:?}"));
            }
            other => {
                if folder.is_some() {
                    route_usage_error(&format!(
                        "expected at most one <folder>, found extra {other:?}"
                    ));
                }
                folder = Some(PathBuf::from(other));
            }
        }
        i += 1;
    }
    let request = match request {
        Some(r) => r,
        None => route_usage_error("missing --request <text>"),
    };
    if install && host.is_none() {
        route_usage_error("--install needs --host <host>");
    }
    if dest.is_some() && host.is_none() {
        route_usage_error("--dest needs --host <host> (or drop --dest)");
    }
    RouteArgs {
        request,
        folder,
        prefs,
        recipe,
        shape,
        workflow,
        fresh,
        install,
        host,
        dest,
        json,
    }
}

/// Entry point for `framesmith route ...`: interview, pitch, choose,
/// and optionally install the chosen creation workflow.
pub fn cmd_route(args: &[String]) -> i32 {
    let parsed = parse_route_args(args);

    // The maker's explicit pick wins over scoring; an unknown name
    // refuses listing every workflow.
    let forced = match &parsed.workflow {
        Some(name) => match find_workflow(name) {
            Some(rule) => Some(rule),
            None => return refuse_unknown_workflow(name),
        },
        None => None,
    };

    // The recipe answers and shape come from a baked bundle; an
    // unknown name refuses listing every recipe.
    let recipe = match &parsed.recipe {
        Some(name) => match find_recipe(name) {
            Some(r) => Some(r),
            None => return refuse_unknown_recipe(name),
        },
        None => None,
    };

    // Project state informs the choice but is never written: brief
    // and storyboard files are only read.
    let (brief_state, story_state) = match (&parsed.folder, parsed.fresh) {
        (Some(folder), false) => {
            if !folder.is_dir() {
                eprintln!(
                    "framesmith route refused: no project folder at {}.\n\
                     fix: pass a project folder, or drop the folder to route without project state.",
                    folder.display()
                );
                return 1;
            }
            (read_brief(folder), read_storyboard(folder))
        }
        _ => (BriefState::Absent, StoryState::Absent),
    };
    let project_ignored = parsed.folder.is_none() || parsed.fresh;
    let brief_label = if project_ignored {
        "ignored".to_string()
    } else {
        match &brief_state {
            BriefState::Absent => "absent".to_string(),
            BriefState::Ok { .. } => "ok".to_string(),
            BriefState::Corrupt { reason } => format!("corrupt ({reason})"),
        }
    };
    let story_label = if project_ignored {
        "ignored".to_string()
    } else {
        match &story_state {
            StoryState::Absent => "absent".to_string(),
            StoryState::Present { title } => format!("present ({title})"),
            StoryState::Unreadable { reason } => format!("unreadable ({reason})"),
        }
    };

    // A corrupt brief (or an unreadable storyboard) stops the route:
    // report the file, offer the fresh interview, and change nothing.
    // The corrupt files stay on disk for inspection.
    let corrupt_note = if project_ignored {
        None
    } else {
        match &brief_state {
            BriefState::Corrupt { reason } => Some((
                BRIEF_FILE,
                reason.clone(),
                format!(
                    "corrupt brief at {}: {reason}",
                    parsed
                        .folder
                        .as_ref()
                        .map(|f| f.join(BRIEF_FILE).display().to_string())
                        .unwrap_or_default()
                ),
            )),
            _ => match &story_state {
                StoryState::Unreadable { reason } => Some((
                    STORYBOARD_FILE,
                    reason.clone(),
                    format!(
                        "unreadable storyboard at {}: {reason}",
                        parsed
                            .folder
                            .as_ref()
                            .map(|f| f.join(STORYBOARD_FILE).display().to_string())
                            .unwrap_or_default()
                    ),
                )),
                _ => None,
            },
        }
    };
    if let Some((file, _, detail)) = corrupt_note {
        // The fresh offer is the general-video interview: the same
        // questions a new brief would start from, without guessing.
        let fresh_rule = find_workflow("general-video").expect("general-video ships");
        let fresh_questions = interview_for(fresh_rule, &parsed.prefs, recipe);
        eprintln!("framesmith route refused: {detail}.");
        eprintln!("fresh brief offer: re-run with `--fresh` to start this interview instead:");
        for (i, q) in fresh_questions.iter().enumerate() {
            eprintln!("  {}. [{}] {}", i + 1, q.key, q.question);
        }
        eprintln!(
            "fix: repair or remove {file}, or re-run with `--fresh`; the on-disk files were left untouched."
        );
        return 1;
    }

    // Scoring reads the request plus the brief's own request line when
    // a valid brief is present, so saved project words inform the
    // choice without ever overriding the maker's current words.
    let brief_request = match &brief_state {
        BriefState::Ok { request } if !project_ignored => Some(request.clone()),
        _ => None,
    };
    let scoring_text = match &brief_request {
        Some(b) => format!("{} {b}", parsed.request),
        None => parsed.request.clone(),
    };
    let scored = score_request(&scoring_text);

    // The decision: the maker's explicit workflow, else a strict
    // scoring winner, else the ambiguity offer.
    let mut notes: Vec<String> = Vec::new();
    if let Some(b) = &brief_request {
        if normalize(b) != normalize(&parsed.request) {
            notes.push(
                "brief.md request differs from the current words; the current words win"
                    .to_string(),
            );
        }
    }
    if let StoryState::Present { .. } = story_state {
        if !project_ignored {
            notes.push(
                "storyboard.md is present; its locks and approvals belong to the later gates"
                    .to_string(),
            );
        }
    }
    let decision = match forced {
        Some(rule) => RouteDecision::Chosen {
            workflow: rule,
            reasons: vec![format!(
                "maker chose {:?} explicitly with `--workflow`",
                rule.name
            )],
        },
        None => decide(&scored),
    };
    // Name every matched keyword in the choice reasons; when the brief
    // matched too, say the brief agrees.
    let decision = match decision {
        RouteDecision::Chosen {
            workflow,
            mut reasons,
        } => {
            for (k, v) in &parsed.prefs {
                if workflow.must_haves.iter().any(|m| m.key == k.as_str()) {
                    reasons.push(format!("preference {k}={v} answers a must-have"));
                }
            }
            if let Some(r) = recipe {
                for (k, _) in r.answers {
                    if workflow.must_haves.iter().any(|m| m.key == *k) {
                        reasons.push(format!("recipe {} answers {k}", r.name));
                    }
                }
            }
            if let Some(b) = &brief_request {
                let brief_hits: Vec<&str> = workflow
                    .keywords
                    .iter()
                    .filter(|kw| contains_phrase(&normalize(b), kw))
                    .copied()
                    .collect();
                if !brief_hits.is_empty() {
                    reasons.push(format!(
                        "brief.md agrees (mentions {})",
                        brief_hits
                            .iter()
                            .map(|k| format!("{k:?}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            }
            RouteDecision::Chosen { workflow, reasons }
        }
        other => other,
    };

    // The interview and pitch always run: one question per must-have
    // for the chosen (or general) workflow, then five concepts.
    let interview_workflow = match &decision {
        RouteDecision::Chosen { workflow, .. } => *workflow,
        RouteDecision::Ambiguous { .. } => {
            find_workflow("general-video").expect("general-video ships")
        }
    };
    let questions = interview_for(interview_workflow, &parsed.prefs, recipe);
    let concepts = pitch_concepts(interview_workflow.name, &parsed.request);

    // Run shape: explicit `--shape` wins, then the recipe default,
    // then `single`.
    let (shape, shape_source) = match parsed.shape {
        Some(s) => (s, format!("explicit `--shape {}`", s.name())),
        None => match recipe {
            Some(r) => (r.shape, format!("recipe {}", r.name)),
            None => (RunShape::Single, "default".to_string()),
        },
    };
    if let Some(r) = recipe {
        if r.shape != shape {
            notes.push(format!(
                "explicit `--shape {}` overrides recipe {}",
                shape.name(),
                r.name
            ));
        }
    }

    let (choice, offered) = match &decision {
        RouteDecision::Chosen { workflow, reasons } => (
            Some((workflow.name.to_string(), reasons.clone())),
            Vec::new(),
        ),
        RouteDecision::Ambiguous { reasons } => {
            notes.extend(reasons.clone());
            (
                None,
                vec!["general-video".to_string(), "not-sure".to_string()],
            )
        }
    };

    // Install on demand: only the chosen workflow, only with
    // `--install`, and only its own files. Ambiguity installs nothing.
    let mut install_note: Option<String> = None;
    let mut install_code = 0;
    if parsed.install {
        match &choice {
            Some((workflow, _)) => {
                let host = parsed.host.as_deref().expect("--install needs --host");
                install_code = install_workflow(workflow, host, parsed.dest.as_deref());
                if install_code == 0 {
                    install_note = Some(format!(
                        "install: workflow {workflow:?} ensured for host {host:?}"
                    ));
                } else {
                    // The installer already reported the refusal.
                    return install_code;
                }
            }
            None => {
                eprintln!(
                    "framesmith route refused: nothing to install: the request is ambiguous.\n\
                     fix: name a workflow with `--workflow` (start with `general-video`), then re-run with `--install`; nothing was installed."
                );
                return 1;
            }
        }
    } else if choice.is_some() {
        notes.push(format!(
            "install: re-run with `--install --host <host>` to install only the {} workflow skill",
            choice.as_ref().map(|(w, _)| w.as_str()).unwrap_or("")
        ));
    }

    let report = RouteReport {
        request: parsed.request,
        folder: if project_ignored { None } else { parsed.folder },
        brief: brief_label,
        brief_request,
        storyboard: story_label,
        prefs: parsed.prefs,
        recipe,
        shape,
        shape_source,
        questions,
        concepts,
        choice,
        offered,
        notes,
        install: install_note,
    };
    if parsed.json {
        println!("{}", format_report_json(&report));
    } else {
        print!("{}", format_report_human(&report));
    }
    install_code
}
