//! Project resume (Sprint 20, T008 / CAP-003, J-05, REC-005).
//!
//! Resume reopens a project's `brief.md`, `storyboard.md`, and the
//! storyboard gate state on the maker's return. It reuses the router
//! slice's read model ([`super::router`]) for the two markdown files
//! so both slices agree on what counts as a valid brief, a corrupt
//! file, or an absent one, and the storyboard slice's persisted gate
//! state ([`preview::studio_stills`]) for the locked frames, so resume
//! continues from exactly the board the maker approved:
//!
//! * Locked frames (a presented board in
//!   `.framesmith/storyboard.json`) continue the same workflow in the
//!   same state; nothing is written. The approval flag is reported but
//!   never flipped here: the gate stays owned by the storyboard slice.
//! * No locked frames offers a fresh interview: the brief request (when
//!   one is on file) is scored through the router's workflow rules and
//!   the winning workflow's must-have questions are offered; without a
//!   brief the general workflow plus a Not-sure path is offered.
//! * The maker's current words (`--words`) win over a stale brief: the
//!   `Request:` line is rewritten. The write prints a staged receipt
//!   first, then travels the shared Sprint 15 history machinery via
//!   [`sdk::history::record_file_turn`], so the agent turn is listed
//!   under its actor like every other agent turn. Only `brief.md`
//!   changes; every composition source stays byte-identical, which is
//!   what keeps default renders byte-identical.
//! * A corrupt or unreadable resume file (`brief.md`, `storyboard.md`,
//!   or the gate state) is reported naming the file and the reason,
//!   the file is kept untouched for inspection, a fresh brief is
//!   offered (printed, never written by the failing run), and the
//!   project is otherwise never touched by the failure.
//!
//! All prose here is framesmith's own clean-room writing: it names
//! only commands this build ships and never promises capabilities
//! this build lacks.

use std::path::{Path, PathBuf};

use super::router;

/// Actor every resume brief turn records under unless `--who` names
/// another: the agent driving the resume.
pub const RESUME_ACTOR: &str = "agent";

/// Marker for the optional workflow line a brief may carry.
const WORKFLOW_PREFIX: &str = "Workflow:";

/// Fresh-brief workflow: the standing offer when nothing else is
/// known, mirroring the router's ambiguous-request fallback.
const FALLBACK_WORKFLOW: &str = "general-video";

/// One locked still to continue from, in board order.
#[derive(Debug, Clone, PartialEq)]
pub struct LockedStill {
    /// Caption shown under the still on the board page.
    pub label: String,
    /// Frame index the still was drawn from.
    pub frame: u64,
    /// Film seconds the still was drawn from.
    pub at: f64,
    /// Still file name inside the board output directory.
    pub still: String,
}

impl LockedStill {
    /// One-line human naming for listings.
    pub fn display(&self) -> String {
        format!("\"{}\" (frame {}, {}s)", self.label, self.frame, self.at)
    }
}

/// A presented board to continue from: the gate state plus the
/// workflow the brief names.
#[derive(Debug, Clone, PartialEq)]
pub struct ContinueBoard {
    /// Board title shown on the one-page presentation.
    pub title: String,
    /// Composition the board was presented from.
    pub composition: String,
    /// True once the maker approved the stills.
    pub approved: bool,
    /// Locked stills, in presentation order.
    pub frames: Vec<LockedStill>,
}

/// First `Workflow:` line of a brief body, if the brief names one.
pub fn brief_workflow(body: &str) -> Option<String> {
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(WORKFLOW_PREFIX) {
            let name = rest.trim().to_string();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

/// Squeeze whitespace and lowercase for request/words comparison.
fn squeeze(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// True when the maker's words already agree with the brief request:
/// equal after squeezing, or one containing the other.
pub fn words_match(request: &str, words: &str) -> bool {
    let want = squeeze(words);
    if want.is_empty() {
        return false;
    }
    let have = squeeze(request);
    have == want || have.contains(&want) || want.contains(&have)
}

/// Rewrite the first `Request:` line of a valid brief body to carry
/// `words`, preserving every other line byte for byte. Returns the
/// new body, or `None` when no `Request:` line exists (the caller
/// treats that brief as corrupt before ever reaching this).
pub fn rewrite_request(body: &str, words: &str) -> Option<String> {
    let mut done = false;
    let mut out: Vec<String> = Vec::new();
    for line in body.lines() {
        if !done && line.trim().strip_prefix("Request:").is_some() {
            let leading: String = line.chars().take_while(|c| c.is_whitespace()).collect();
            out.push(format!("{leading}Request: {}", words.trim()));
            done = true;
        } else {
            out.push(line.to_string());
        }
    }
    if !done {
        return None;
    }
    let mut text = out.join("\n");
    if body.ends_with('\n') {
        text.push('\n');
    }
    Some(text)
}

/// A fresh brief carrying `request`, valid under the router's read
/// model. Offered (printed) on corruption and written when the maker
/// briefs with `--words` and no brief is on file yet.
pub fn fresh_brief(request: &str) -> String {
    format!(
        "# Framesmith brief\nRequest: {}\n{} {}\n",
        request.trim(),
        WORKFLOW_PREFIX,
        FALLBACK_WORKFLOW
    )
}

fn resume_usage_error(msg: &str) -> ! {
    eprintln!("framesmith resume: {msg}.");
    eprintln!("usage: framesmith resume [--json] [--words <text>] [--who <name>] <folder>");
    std::process::exit(2);
}

fn json_escape(text: &str) -> String {
    let rendered = serde_json::Value::String(text.to_string()).to_string();
    rendered[1..rendered.len() - 1].to_string()
}

/// `resume [--json] [--words <text>] [--who <name>] <folder>`:
/// reopen the brief, the storyboard note, and the locked-frames gate
/// state; continue from locked frames or offer a fresh interview; let
/// current words win over a stale brief. Returns the exit code.
pub fn cmd_resume(args: &[String]) -> i32 {
    let mut json = false;
    let mut words: Option<String> = None;
    let mut who: Option<String> = None;
    let mut folder: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                if json {
                    resume_usage_error("duplicate --json flag");
                }
                json = true;
            }
            "--words" => {
                i += 1;
                if i >= args.len() {
                    resume_usage_error("--words needs a value (the maker's current words)");
                }
                if words.is_some() {
                    resume_usage_error("duplicate --words flag");
                }
                words = Some(args[i].clone());
            }
            "--who" => {
                i += 1;
                if i >= args.len() {
                    resume_usage_error("--who needs a value (an actor name)");
                }
                if who.is_some() {
                    resume_usage_error("duplicate --who flag");
                }
                who = Some(args[i].clone());
            }
            other if other.starts_with('-') => {
                resume_usage_error(&format!("unknown flag {other:?}"));
            }
            other => {
                if folder.is_some() {
                    resume_usage_error("expected exactly one project folder");
                }
                folder = Some(PathBuf::from(other));
            }
        }
        i += 1;
    }
    let folder = match folder {
        Some(f) => f,
        None => resume_usage_error("expected exactly one project folder"),
    };
    if let Some(w) = &words {
        if w.trim().is_empty() {
            resume_usage_error("--words needs non-empty text");
        }
    }
    let actor = who.unwrap_or_else(|| RESUME_ACTOR.to_string());
    run_resume(&folder, words.as_deref(), &actor, json)
}

/// Outcome of one resume run, shared by the human and `--json`
/// renderings.
enum Outcome {
    Continue {
        workflow: String,
        brief_request: Option<String>,
        board: ContinueBoard,
    },
    FreshInterview {
        workflow: String,
        reason: String,
        questions: Vec<(String, String)>,
        story_note: Option<String>,
    },
    InSync {
        request: String,
    },
    BriefUpdated {
        turn: u64,
        actor: String,
        summary: String,
    },
    Corrupt {
        faults: Vec<String>,
        offer: String,
    },
    Refused {
        message: String,
    },
}

/// Offer the fresh interview for `request` (or none when no brief is
/// on file): score the request through the router rules and offer the
/// winning workflow's must-have questions; fall back to the general
/// workflow plus a Not-sure path, exactly like the router's
/// ambiguous-request rule.
fn interview_offer(request: Option<&str>) -> (String, String, Vec<(String, String)>) {
    match request {
        Some(text) => {
            let scored = router::score_request(text);
            if let Some((rule, matched)) = scored.first() {
                let questions = router::interview_for(rule, &[], None)
                    .into_iter()
                    .map(|q| (q.key, q.question))
                    .collect();
                let reason = format!(
                    "the brief request matches {} ({})",
                    rule.name,
                    matched.join(", ")
                );
                (rule.name.to_string(), reason, questions)
            } else {
                let rule = router::find_workflow(FALLBACK_WORKFLOW)
                    .expect("general-video is a routed workflow");
                let questions = router::interview_for(rule, &[], None)
                    .into_iter()
                    .map(|q| (q.key, q.question))
                    .collect();
                (
                    rule.name.to_string(),
                    "the brief request matches no workflow; the general workflow plus a Not-sure path covers it".to_string(),
                    questions,
                )
            }
        }
        None => {
            let rule = router::find_workflow(FALLBACK_WORKFLOW)
                .expect("general-video is a routed workflow");
            let questions = router::interview_for(rule, &[], None)
                .into_iter()
                .map(|q| (q.key, q.question))
                .collect();
            (
                rule.name.to_string(),
                "no brief is on file; the general workflow plus a Not-sure path covers an ambiguous start".to_string(),
                questions,
            )
        }
    }
}

fn run_resume(folder: &Path, words: Option<&str>, actor: &str, json: bool) -> i32 {
    let folder_text = folder.display().to_string();
    if !folder.is_dir() {
        let message = format!(
            "resume refused: {folder_text} has no project directory; pass an existing project folder. Nothing was read or written"
        );
        if json {
            println!(
                "{{\"tool\":\"framesmith\",\"command\":\"resume\",\"folder\":\"{}\",\"status\":\"refused\",\"exit\":1,\"message\":\"{}\"}}",
                json_escape(&folder_text),
                json_escape(&message)
            );
        } else {
            println!("{message}.");
        }
        return 1;
    }
    // Every read happens before any write: a later failure cannot
    // have touched the project.
    let brief = router::read_brief(folder);
    let story = router::read_storyboard(folder);
    let gate = preview::studio_stills::load_storyboard(folder);
    let mut faults: Vec<String> = Vec::new();
    if let router::BriefState::Corrupt { reason } = &brief {
        faults.push(format!("brief corrupt: {} {reason}", router::BRIEF_FILE));
    }
    if let router::StoryState::Unreadable { reason } = &story {
        faults.push(format!(
            "storyboard unreadable: {} {reason}",
            router::STORYBOARD_FILE
        ));
    }
    if gate.restored && gate.state.is_none() {
        let why = gate
            .notice
            .clone()
            .unwrap_or_else(|| "it cannot be used".to_string());
        faults.push(format!(
            "gate state corrupt: {} ({why})",
            preview::studio_stills::storyboard_path_for(folder).display()
        ));
    }
    if !faults.is_empty() {
        let offer = fresh_brief("(write the maker's request here)");
        return emit(Outcome::Corrupt { faults, offer }, &folder_text, json, 1);
    }
    let brief_request = match &brief {
        router::BriefState::Ok { request } => Some(request.clone()),
        router::BriefState::Absent => None,
        router::BriefState::Corrupt { .. } => unreachable!("corrupt brief returns above"),
    };
    if let Some(current) = words {
        return resume_with_words(folder, &folder_text, current, actor, brief_request, json);
    }
    let story_note = match &story {
        router::StoryState::Present { title } => Some(title.clone()),
        router::StoryState::Absent => None,
        router::StoryState::Unreadable { .. } => {
            unreachable!("unreadable storyboard returns above")
        }
    };
    if let Some(state) = gate.state {
        let workflow =
            brief_workflow_of(folder).unwrap_or_else(|| "(unstated workflow)".to_string());
        return emit(
            Outcome::Continue {
                workflow,
                brief_request,
                board: ContinueBoard {
                    title: state.title,
                    composition: state.composition,
                    approved: state.approved,
                    frames: state
                        .board
                        .into_iter()
                        .map(|f| LockedStill {
                            label: f.label,
                            frame: f.frame,
                            at: f.at,
                            still: f.still,
                        })
                        .collect(),
                },
            },
            &folder_text,
            json,
            0,
        );
    }
    let (workflow, reason, questions) = interview_offer(brief_request.as_deref());
    emit(
        Outcome::FreshInterview {
            workflow,
            reason,
            questions,
            story_note,
        },
        &folder_text,
        json,
        0,
    )
}

/// First `Workflow:` line of the brief on disk, if any.
fn brief_workflow_of(folder: &Path) -> Option<String> {
    let bytes = std::fs::read(folder.join(router::BRIEF_FILE)).ok()?;
    let body = String::from_utf8(bytes).ok()?;
    brief_workflow(&body)
}

/// Resume with the maker's current words: create the brief when none
/// is on file, report in-sync when the words already agree, else
/// rewrite the `Request:` line through a staged receipt plus one
/// shared-history turn.
fn resume_with_words(
    folder: &Path,
    folder_text: &str,
    current: &str,
    actor: &str,
    brief_request: Option<String>,
    json: bool,
) -> i32 {
    let current = current.trim();
    match brief_request {
        None => {
            let body = fresh_brief(current);
            apply_brief(
                folder,
                folder_text,
                body,
                actor,
                json,
                "brief created from the maker's words",
            )
        }
        Some(request) => {
            if words_match(&request, current) {
                return emit(Outcome::InSync { request }, folder_text, json, 0);
            }
            let old = std::fs::read(folder.join(router::BRIEF_FILE))
                .ok()
                .and_then(|b| String::from_utf8(b).ok())
                .unwrap_or_default();
            match rewrite_request(&old, current) {
                Some(body) => apply_brief(
                    folder,
                    folder_text,
                    body,
                    actor,
                    json,
                    "brief updated with the maker's current words",
                ),
                None => {
                    // Unreachable: the router just read a valid brief.
                    let message = format!(
                        "resume refused: {} changed under this run; re-run resume to read it again. Nothing was written",
                        router::BRIEF_FILE
                    );
                    emit(Outcome::Refused { message }, folder_text, json, 1)
                }
            }
        }
    }
}

/// Stage the new brief body with a receipt, then record it through
/// the shared history machinery. Only `brief.md` changes on disk.
fn apply_brief(
    folder: &Path,
    folder_text: &str,
    body: String,
    actor: &str,
    json: bool,
    summary: &str,
) -> i32 {
    let summary = summary.to_string();
    let path = folder.join(router::BRIEF_FILE);
    match sdk::history::record_file_turn(folder, actor, || {
        std::fs::write(&path, &body)
            .map_err(|e| format!("{} cannot be written ({e})", router::BRIEF_FILE))?;
        Ok((summary.clone(), vec![router::BRIEF_FILE.to_string()]))
    }) {
        Ok(record) => emit(
            Outcome::BriefUpdated {
                turn: record.seq,
                actor: record.actor.clone(),
                summary: record.summary.clone(),
            },
            folder_text,
            json,
            0,
        ),
        Err(e) => {
            // A refused write stages nothing: no staged line, just
            // the cause. The project is untouched.
            let message = format!("resume refused: {e} Nothing was written");
            emit(Outcome::Refused { message }, folder_text, json, 1)
        }
    }
}

fn emit(outcome: Outcome, folder: &str, json: bool, code: i32) -> i32 {
    if json {
        println!("{}", outcome_json(&outcome, folder, code));
    } else {
        print!("{}", outcome_text(&outcome, folder));
    }
    code
}

fn outcome_json(outcome: &Outcome, folder: &str, code: i32) -> String {
    let folder = json_escape(folder);
    match outcome {
        Outcome::Continue {
            workflow,
            brief_request,
            board,
        } => {
            let frames: Vec<String> = board
                .frames
                .iter()
                .map(|f| {
                    format!(
                        "{{\"label\":\"{}\",\"frame\":{},\"at\":{},\"still\":\"{}\"}}",
                        json_escape(&f.label),
                        f.frame,
                        f.at,
                        json_escape(&f.still)
                    )
                })
                .collect();
            let request = match brief_request {
                Some(r) => format!("\"{}\"", json_escape(r)),
                None => "null".to_string(),
            };
            format!(
                "{{\"tool\":\"framesmith\",\"command\":\"resume\",\"folder\":\"{folder}\",\"status\":\"continue\",\"exit\":{code},\"workflow\":\"{}\",\"request\":{request},\"composition\":\"{}\",\"board\":\"{}\",\"approved\":{},\"locked-frames\":[{}]}}",
                json_escape(workflow),
                json_escape(&board.composition),
                json_escape(&board.title),
                board.approved,
                frames.join(",")
            )
        }
        Outcome::FreshInterview {
            workflow,
            reason,
            questions,
            story_note,
        } => {
            let qs: Vec<String> = questions
                .iter()
                .map(|(k, q)| {
                    format!("{{\"key\":\"{}\",\"question\":\"{}\"}}", json_escape(k), json_escape(q))
                })
                .collect();
            let note = match story_note {
                Some(n) => format!("\"{}\"", json_escape(n)),
                None => "null".to_string(),
            };
            format!(
                "{{\"tool\":\"framesmith\",\"command\":\"resume\",\"folder\":\"{folder}\",\"status\":\"fresh-interview\",\"exit\":{code},\"workflow\":\"{}\",\"reason\":\"{}\",\"storyboard-note\":{note},\"questions\":[{}]}}",
                json_escape(workflow),
                json_escape(reason),
                qs.join(",")
            )
        }
        Outcome::InSync { request } => format!(
            "{{\"tool\":\"framesmith\",\"command\":\"resume\",\"folder\":\"{folder}\",\"status\":\"in-sync\",\"exit\":{code},\"request\":\"{}\"}}",
            json_escape(request)
        ),
        Outcome::BriefUpdated {
            turn,
            actor,
            summary,
        } => format!(
            "{{\"tool\":\"framesmith\",\"command\":\"resume\",\"folder\":\"{folder}\",\"status\":\"brief-updated\",\"exit\":{code},\"turn\":{turn},\"actor\":\"{}\",\"summary\":\"{}\",\"targets\":[\"{}\"]}}",
            json_escape(actor),
            json_escape(summary),
            router::BRIEF_FILE
        ),
        Outcome::Corrupt { faults, offer } => {
            let items: Vec<String> = faults
                .iter()
                .map(|f| format!("\"{}\"", json_escape(f)))
                .collect();
            format!(
                "{{\"tool\":\"framesmith\",\"command\":\"resume\",\"folder\":\"{folder}\",\"status\":\"corrupt\",\"exit\":{code},\"faults\":[{}],\"offer\":\"{}\"}}",
                items.join(","),
                json_escape(offer)
            )
        }
        Outcome::Refused { message } => format!(
            "{{\"tool\":\"framesmith\",\"command\":\"resume\",\"folder\":\"{folder}\",\"status\":\"refused\",\"exit\":{code},\"message\":\"{}\"}}",
            json_escape(message)
        ),
    }
}

fn outcome_text(outcome: &Outcome, folder: &str) -> String {
    let _ = folder;
    match outcome {
        Outcome::Continue {
            workflow,
            brief_request,
            board,
        } => {
            let frames: Vec<String> = board.frames.iter().map(LockedStill::display).collect();
            let gate = if board.approved {
                "the board is approved; motion and render may proceed"
            } else {
                "the board awaits maker approval; approve the stills first"
            };
            let brief_line = match brief_request {
                Some(r) => format!("brief request: {r}"),
                None => "no brief on file; interview answers will start one".to_string(),
            };
            format!(
                "resume: continuing workflow \"{workflow}\" from {} locked frame(s) ({} / \"{}\"): {}.\nstoryboard: {gate}.\n{brief_line}.\n",
                board.frames.len(),
                board.composition,
                board.title,
                frames.join(", "),
            )
        }
        Outcome::FreshInterview {
            workflow,
            reason,
            questions,
            story_note,
        } => {
            let mut out = format!(
                "resume: no locked frames; offering a fresh interview for \"{workflow}\" ({reason}).\n"
            );
            if let Some(note) = story_note {
                out.push_str(&format!("storyboard note: \"{note}\".\n"));
            }
            for (key, question) in questions {
                out.push_str(&format!("  - [{key}] {question}\n"));
            }
            out.push_str(
                "re-run with --words \"<intent>\" to brief it, or answer the question(s) above.\n",
            );
            out
        }
        Outcome::InSync { request } => {
            format!("resume: the brief already carries the maker's words (Request: {request}); nothing changed.\n")
        }
        Outcome::BriefUpdated {
            turn,
            actor,
            summary,
        } => {
            format!(
                "resume staged: {} rewritten through shared history.\nresume applied: {summary}; history turn {turn} by {actor}: targets [{}].\n",
                router::BRIEF_FILE,
                router::BRIEF_FILE
            )
        }
        Outcome::Corrupt { faults, offer } => {
            let mut out = String::new();
            for fault in faults {
                out.push_str(&format!(
                    "resume refused: {fault}; the file stays for inspection and the project is untouched.\n"
                ));
            }
            out.push_str("resume: fresh brief offer (not written):\n");
            out.push_str(offer);
            out
        }
        Outcome::Refused { message } => format!("{message}.\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_still_display_names_label_frame_and_time() {
        let still = LockedStill {
            label: "Opening".to_string(),
            frame: 12,
            at: 0.4,
            still: "still-0001.png".to_string(),
        };
        assert_eq!(still.display(), "\"Opening\" (frame 12, 0.4s)");
    }

    #[test]
    fn words_match_ignores_case_and_spacing() {
        assert!(words_match("A film about tea", "a  film   about TEA"));
        assert!(words_match("a film about tea for teens", "tea"));
        assert!(!words_match("a film about tea", "a film about coffee"));
        assert!(!words_match("anything", ""));
    }

    #[test]
    fn rewrite_request_replaces_first_line_only() {
        let body = "# Framesmith brief\nRequest: tea\nWorkflow: general-video\n";
        let next = rewrite_request(body, "coffee").unwrap();
        assert!(next.contains("Request: coffee"));
        assert!(next.contains("Workflow: general-video"));
        assert!(!next.contains("tea"));
        assert!(rewrite_request("no request here", "x").is_none());
    }

    #[test]
    fn fresh_brief_reads_back_valid() {
        let body = fresh_brief("a launch film");
        assert!(body.starts_with("# Framesmith brief\n"));
        assert!(body.contains("Request: a launch film"));
    }

    #[test]
    fn brief_workflow_picks_first_line() {
        let body = "# Framesmith brief\nRequest: r\nWorkflow: music-to-video\n";
        assert_eq!(brief_workflow(body).as_deref(), Some("music-to-video"));
        assert_eq!(brief_workflow("nope"), None);
    }
}
