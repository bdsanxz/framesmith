//! The record of Sprint 1 frames (Sprint 2, ticket T003): reading it, making it,
//! and comparing a render with it. Used by `s2_t003_sprint1_frames.rs`.
//!
//! The record is one plain text file for each kind of browser build, in
//! `tests/records/`. A line holds a fixture, a frame number counted from 0,
//! and the hash of that frame's pixels (`measure::Frame::hash`). Lines that
//! begin with `#` are the header and notes.

use super::{browser, fixture, render_frames};
use measure::Frame;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The eight fixtures of Sprint 1 (not `fixtures/unsupported`).
pub const FIXTURES: [&str; 8] = [
    "easings",
    "empty",
    "late-from",
    "pair-move",
    "repeat",
    "still",
    "title-card",
    "title-card-unwritten",
];

/// The kinds of browser build that have a record.
pub const KINDS: [&str; 2] = ["full", "headless"];

/// The folder of the records. The maintainer switch `FRAMESMITH_S2_T003_RECORD_DIR`
/// names another folder (used to take trial measurements without touching the records).
pub fn records_dir() -> PathBuf {
    match std::env::var_os("FRAMESMITH_S2_T003_RECORD_DIR").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/records"),
    }
}

pub fn record_file(kind: &str) -> PathBuf {
    records_dir().join(format!("sprint1-frames-{kind}.txt"))
}

/// The kind of the browser in use, and the version line it prints.
/// A browser program whose file name holds "headless" is a headless-only build
/// (its version line does not tell the two apart); any other is a full build.
pub fn browser_in_use() -> (&'static str, String) {
    let program = browser();
    let name = program.file_name().unwrap().to_string_lossy().to_lowercase();
    let kind = if name.contains("headless") { "headless" } else { "full" };
    let ran = measure::run(&program, &["--version"], Path::new("/"), &[]);
    (kind, format!("{} ({})", ran.stdout.trim(), program.display()))
}

/// What a record holds: the hashes of each fixture's frames, in frame order.
pub type Hashes = BTreeMap<String, Vec<u64>>;

/// Reads record text. A line that is not a comment must be `fixture frame hash`
/// with the frames of a fixture in order from 0.
pub fn parse(text: &str) -> Result<Hashes, String> {
    let mut found = Hashes::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bad = || format!("line {} of the record is not `fixture frame hash`: {line:?}", number + 1);
        let parts: Vec<&str> = line.split_whitespace().collect();
        let [fixture, frame, hash] = parts[..] else {
            return Err(bad());
        };
        let frame: usize = frame.parse().map_err(|_| bad())?;
        let hash = u64::from_str_radix(hash, 16).map_err(|_| bad())?;
        let hashes = found.entry(fixture.to_string()).or_default();
        if frame != hashes.len() {
            return Err(format!(
                "line {} of the record: frame {frame} of {fixture} is out of order",
                number + 1
            ));
        }
        hashes.push(hash);
    }
    Ok(found)
}

/// The record for a kind of browser. A missing record is an error that says so.
pub fn load(kind: &str, version: &str) -> Result<Hashes, String> {
    let file = record_file(kind);
    let text = std::fs::read_to_string(&file).map_err(|e| {
        format!(
            "there is no record of Sprint 1 frames for the browser in use ({kind} build: {version}): {} cannot be read ({e}). \
             The record is made once with the Sprint 1 release build and changes only with the product owner's approval; \
             this test does not make it.",
            file.display()
        )
    })?;
    parse(&text).map_err(|e| format!("{}: {e}", file.display()))
}

/// Renders the project (a fixture or a copy of one) and returns the hash of every frame.
pub fn render_hashes(project: &Path) -> Result<Vec<u64>, String> {
    let (ran, frames) = render_frames(project, &[], &[]);
    if ran.code != 0 {
        return Err(format!("the render ended with code {}: {}", ran.code, ran.all()));
    }
    Ok(frames.iter().map(Frame::hash).collect())
}

/// Compares the hashes of a render with the record of `name`. A differing hash is
/// a defect: the message names the fixture and the first frame that differs.
pub fn compare(record: &Hashes, name: &str, rendered: &[u64]) -> Result<(), String> {
    let Some(recorded) = record.get(name) else {
        return Err(format!("fixture {name}: the record has no frames of this fixture"));
    };
    if let Some(at) = rendered.iter().zip(recorded).position(|(now, then)| now != then) {
        return Err(format!(
            "fixture {name}: frame {at} is the first that differs (hash {:016x}, record {:016x}); \
             a differing hash is a defect to report, and the record is not to be replaced to make this pass",
            rendered[at], recorded[at]
        ));
    }
    if rendered.len() != recorded.len() {
        return Err(format!(
            "fixture {name}: {} frames were rendered, the record has {}; \
             the first {} frames are equal (a differing frame count is a defect to report)",
            rendered.len(),
            recorded.len(),
            rendered.len().min(recorded.len())
        ));
    }
    Ok(())
}

/// Renders a project that stands for the fixture `name` and compares it with the record.
pub fn compare_project(record: &Hashes, name: &str, project: &Path) -> Result<(), String> {
    let rendered = render_hashes(project).map_err(|e| format!("fixture {name}: {e}"))?;
    compare(record, name, &rendered)
}

/// Renders a fresh copy of each of the eight fixtures and returns its hashes.
pub fn render_all(test: &str) -> Vec<(&'static str, Result<Vec<u64>, String>)> {
    FIXTURES
        .iter()
        .map(|name| (*name, render_hashes(&fixture(&format!("{test}_{name}"), name))))
        .collect()
}

/// Writes a record file. It never replaces one that exists: the record changes only
/// with the product owner's approval, and then by a person moving the old file away.
pub fn write_new(file: &Path, text: &str) -> Result<(), String> {
    std::fs::create_dir_all(file.parent().unwrap()).map_err(|e| e.to_string())?;
    let mut out = std::fs::OpenOptions::new().write(true).create_new(true).open(file).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            format!(
                "{} already exists and is not replaced: the record changes only with the product owner's approval",
                file.display()
            )
        } else {
            format!("{}: {e}", file.display())
        }
    })?;
    out.write_all(text.as_bytes()).map_err(|e| e.to_string())
}

/// The text of a record: a header, then one line for each frame of each fixture.
pub fn record_text(kind: &str, header: &[String], hashes: &[(&str, Vec<u64>)]) -> String {
    let mut text = format!("# Framesmith: hashes of every frame of the eight Sprint 1 fixtures, {kind} browser build\n");
    for line in header {
        text.push_str("# ");
        text.push_str(line);
        text.push('\n');
    }
    text.push_str("#\n# One line for each frame: fixture, frame number (from 0), hash of the frame's pixels.\n");
    for (name, frames) in hashes {
        text.push_str(&format!("#\n# {name}: {} frames\n", frames.len()));
        for (at, hash) in frames.iter().enumerate() {
            text.push_str(&format!("{name} {at:05} {hash:016x}\n"));
        }
    }
    text
}
