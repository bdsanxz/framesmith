//! Headless open regressions (Sprint 15, T001 / CAP-003): every
//! acceptance criterion — ready open with an addressable first frame
//! and enabled controls, observable persist/preview/overrides/history/
//! coalesce options, refusals naming rule and location, no render and
//! no project write outside the hidden history directory, and refused
//! opens leaving prior state exactly unchanged.

use sdk::{history_dir_for, OpenOptions, RefusalKind, SdkSession};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-sdk-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

fn text_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
}

fn variable_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    variable id=\"title\" type=\"string\" default=\"News\"\n    text value=\"var(--title)\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
}

fn defaultless_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    variable id=\"title\" type=\"string\"\n    text value=\"var(--title)\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
}

fn overrides_one() -> validate::validate::VariableOverrides {
    let mut map = BTreeMap::new();
    map.insert(
        "title".to_string(),
        validate::validate::OverrideValue::Str("Sale".to_string()),
    );
    map
}

fn text_value(session: &SdkSession) -> String {
    for clip in &session.effective_composition().clips {
        if let notation::model::Clip::Text(t) = clip {
            return t.value.clone();
        }
    }
    panic!("effective composition holds no text clip");
}

/// Sorted map of every file under `folder` to its bytes: the
/// never-writes-into-the-project oracle.
fn tree_bytes(folder: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let key = path
                    .strip_prefix(folder)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                out.insert(key, std::fs::read(&path).unwrap());
            }
        }
    }
    out
}

fn has_render_output(folder: &Path) -> bool {
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if matches!(ext, "mp4" | "png" | "mov" | "mkv" | "webm") {
                    return true;
                }
            }
        }
    }
    false
}

#[test]
fn valid_open_is_ready_with_first_frame_and_controls() {
    let root = tmp("ready");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let session = SdkSession::open(&folder, OpenOptions::default()).expect("valid open");
    assert!(session.is_ready());
    assert_eq!(session.state(), sdk::State::Ready);
    assert!(session.controls_enabled());
    assert_eq!(session.composition_name(), "t");
    assert_eq!(session.canvas(), (320, 180));
    assert_eq!(session.frame_count(), 10);
    assert!((session.duration_secs() - 1.0).abs() < 1e-9);
    assert!((session.fps().value() - 10.0).abs() < 1e-9);
    let first = session.first_frame().expect("first frame addressable");
    assert_eq!(first.index, 0);
    assert_eq!((first.width, first.height), (320, 180));
}

#[test]
fn persist_false_is_read_only_and_writes_nothing() {
    let root = tmp("persist-off");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let session = SdkSession::open(
        &folder,
        OpenOptions {
            persist: false,
            ..OpenOptions::default()
        },
    )
    .expect("valid open");
    assert!(!session.can_persist());
    assert!(!session.history_available());
    assert!(
        !folder.join(".framesmith").exists(),
        "read-only open writes nothing"
    );
}

#[test]
fn persist_true_with_history_ensures_only_the_hidden_history_dir() {
    let root = tmp("history-on");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let before = tree_bytes(&folder);
    let session = SdkSession::open(&folder, OpenOptions::default()).expect("valid open");
    assert!(session.can_persist());
    assert!(session.history_available());
    assert_eq!(session.history_dir(), history_dir_for(&folder));
    assert!(session.history_dir().is_dir());
    // The hidden history directory is empty at open: the file tree is
    // otherwise byte-identical, and no render output exists.
    assert_eq!(tree_bytes(&folder), before);
    assert!(!has_render_output(&folder));
}

#[test]
fn history_false_creates_nothing() {
    let root = tmp("history-off");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let session = SdkSession::open(
        &folder,
        OpenOptions {
            history: false,
            ..OpenOptions::default()
        },
    )
    .expect("valid open");
    assert!(!session.history_available());
    assert!(
        !folder.join(".framesmith").exists(),
        "history:false writes nothing"
    );
}

#[test]
fn preview_open_resolves_but_writes_nothing() {
    let root = tmp("preview");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    variable_project(&folder);
    let session = SdkSession::open(
        &folder,
        OpenOptions {
            preview: true,
            overrides: overrides_one(),
            ..OpenOptions::default()
        },
    )
    .expect("valid preview open");
    assert!(session.is_preview());
    assert!(!session.can_persist());
    assert!(!session.history_available());
    assert_eq!(text_value(&session), "Sale");
    assert!(
        !folder.join(".framesmith").exists(),
        "preview writes nothing"
    );
}

#[test]
fn coalesced_overrides_merge_into_the_effective_composition() {
    let root = tmp("coalesce-on");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    variable_project(&folder);
    let session = SdkSession::open(
        &folder,
        OpenOptions {
            overrides: overrides_one(),
            coalesce: true,
            history: false,
            ..OpenOptions::default()
        },
    )
    .expect("valid open");
    assert!(session.coalescing());
    assert_eq!(text_value(&session), "Sale");
    assert_eq!(session.applied_overrides().len(), 1);
    assert!(session.pending_overrides().is_empty());
}

#[test]
fn staged_overrides_stay_pending_leaving_disk_truth() {
    let root = tmp("coalesce-off");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    variable_project(&folder);
    let session = SdkSession::open(
        &folder,
        OpenOptions {
            overrides: overrides_one(),
            coalesce: false,
            history: false,
            ..OpenOptions::default()
        },
    )
    .expect("valid open");
    assert!(!session.coalescing());
    assert!(session.applied_overrides().is_empty());
    assert_eq!(session.pending_overrides().len(), 1);
    // Staged means unapplied: the effective composition still carries
    // the on-disk default.
    assert_eq!(text_value(&session), "News");
}

#[test]
fn defaultless_variable_refuses_staged_but_opens_coalesced() {
    let root = tmp("defaultless");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    defaultless_project(&folder);
    // Staged: validation sees the on-disk values, so the default-less
    // variable with no value refuses naming rule and location.
    let refused = SdkSession::open(
        &folder,
        OpenOptions {
            overrides: overrides_one(),
            coalesce: false,
            history: false,
            ..OpenOptions::default()
        },
    )
    .expect_err("staged default-less variable must refuse");
    assert_eq!(refused.kind, RefusalKind::Invalid);
    assert!(!refused.diagnostics.is_empty());
    assert!(
        refused
            .diagnostics
            .iter()
            .any(|d| d.code == "E_BAD_VARIABLE" && d.message.contains("--title")),
        "{refused:?}"
    );
    // Coalesced: the override supplies the missing value, so the same
    // project opens and the effective composition carries it.
    let session = SdkSession::open(
        &folder,
        OpenOptions {
            overrides: overrides_one(),
            coalesce: true,
            history: false,
            ..OpenOptions::default()
        },
    )
    .expect("coalesced override supplies the value");
    assert_eq!(text_value(&session), "Sale");
}

#[test]
fn unknown_override_keys_are_reported_never_silently_created() {
    for coalesce in [true, false] {
        let root = tmp(if coalesce {
            "unknown-on"
        } else {
            "unknown-off"
        });
        let folder = root.join("proj");
        std::fs::create_dir_all(&folder).unwrap();
        text_project(&folder);
        let mut overrides = BTreeMap::new();
        overrides.insert(
            "ghost".to_string(),
            validate::validate::OverrideValue::Str("x".to_string()),
        );
        let session = SdkSession::open(
            &folder,
            OpenOptions {
                overrides,
                coalesce,
                history: false,
                ..OpenOptions::default()
            },
        )
        .expect("unknown keys warn without blocking");
        let hit = session
            .override_warnings()
            .iter()
            .find(|w| w.key == "ghost");
        assert!(hit.is_some(), "coalesce={coalesce}: {hit:?}");
        let warning = hit.unwrap();
        assert!(warning.message.contains("--ghost"), "{warning:?}");
        // The project declares no `title` variable, so nothing merged:
        // undeclared keys never create variables.
        assert!(session.effective_composition().variables.is_empty());
    }
}

#[test]
fn font_family_try_order_survives_open_unchanged() {
    let root = tmp("font-order");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\" font-family=\"Alpha, Beta\"\n}\n",
    );
    let session = SdkSession::open(
        &folder,
        OpenOptions {
            history: false,
            ..OpenOptions::default()
        },
    )
    .expect("syntax-valid families open like check");
    for clip in &session.effective_composition().clips {
        if let notation::model::Clip::Text(t) = clip {
            assert_eq!(
                t.font_family,
                Some(vec!["Alpha".to_string(), "Beta".to_string()])
            );
            return;
        }
    }
    panic!("effective composition holds no text clip");
}

#[test]
fn missing_project_dir_refuses_before_anything_loads() {
    let root = tmp("missing-dir");
    let folder = root.join("nope");
    let refused = SdkSession::open(&folder, OpenOptions::default()).expect_err("must refuse");
    assert_eq!(refused.kind, RefusalKind::Invalid);
    assert!(
        refused.message.contains("has no project directory"),
        "{refused:?}"
    );
}

#[test]
fn missing_composition_refuses_with_rule_and_location() {
    let root = tmp("missing-comp");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    let refused = SdkSession::open(&folder, OpenOptions::default()).expect_err("must refuse");
    assert_eq!(refused.kind, RefusalKind::Invalid);
    assert!(!refused.diagnostics.is_empty());
    let d = &refused.diagnostics[0];
    assert_eq!(d.code, "E_MISSING_COMPOSITION");
    assert!(d.line >= 1 && d.column >= 1);
    assert!(!d.file.is_empty() && !d.message.is_empty() && !d.fix.is_empty());
    assert!(
        refused.message.contains("E_MISSING_COMPOSITION"),
        "{refused:?}"
    );
    assert!(
        !folder.join(".framesmith").exists(),
        "refused open creates nothing"
    );
}

#[test]
fn malformed_kdl_refuses_naming_parse_rule_and_location() {
    let root = tmp("malformed");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(&folder, "composition.kdl", "composition name=\n");
    let refused = SdkSession::open(&folder, OpenOptions::default()).expect_err("must refuse");
    assert_eq!(refused.kind, RefusalKind::Invalid);
    assert!(
        refused
            .diagnostics
            .iter()
            .any(|d| d.code == "E_PARSE" && d.line >= 1),
        "{refused:?}"
    );
    for d in &refused.diagnostics {
        assert!(!d.file.is_empty() && !d.message.is_empty() && !d.fix.is_empty());
    }
    assert!(refused.message.contains("E_PARSE"), "{refused:?}");
    assert!(
        !folder.join(".framesmith").exists(),
        "refused open creates nothing"
    );
}

#[test]
fn invalid_element_refuses_with_rule_and_location() {
    let root = tmp("invalid");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"not-a-color\"\n}\n",
    );
    let refused = SdkSession::open(
        &folder,
        OpenOptions {
            persist: true,
            history: true,
            ..OpenOptions::default()
        },
    )
    .expect_err("must refuse");
    assert_eq!(refused.kind, RefusalKind::Invalid);
    assert!(!refused.diagnostics.is_empty());
    for d in &refused.diagnostics {
        assert!(!d.code.is_empty() && !d.file.is_empty(), "{d:?}");
        assert!(d.line >= 1 && d.column >= 1, "{d:?}");
        assert!(refused.message.contains(&d.code), "{refused:?}");
        assert!(
            refused
                .message
                .contains(&format!("{}:{}", d.line, d.column)),
            "{refused:?}"
        );
    }
    // Even with persist and history enabled, a refused open leaves no
    // hidden history directory behind.
    assert!(
        !folder.join(".framesmith").exists(),
        "refused open creates nothing"
    );
}

#[test]
fn later_sprint_elements_refuse_as_unsupported() {
    let root = tmp("unsupported");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n    mixer\n}\n",
    );
    let refused = SdkSession::open(&folder, OpenOptions::default()).expect_err("must refuse");
    assert_eq!(refused.kind, RefusalKind::Unsupported);
    assert!(refused.message.contains("later-sprint"), "{refused:?}");
    assert!(
        !folder.join(".framesmith").exists(),
        "refused open creates nothing"
    );
}

#[test]
fn unknown_composition_refuses_naming_available_names() {
    let root = tmp("unknown-comp");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let refused = SdkSession::open(
        &folder,
        OpenOptions {
            composition: Some("ghost".to_string()),
            history: false,
            ..OpenOptions::default()
        },
    )
    .expect_err("must refuse");
    assert_eq!(refused.kind, RefusalKind::UnknownComposition);
    assert!(refused.message.contains("\"ghost\""), "{refused:?}");
    assert!(refused.message.contains("\"t\""), "{refused:?}");
}

#[test]
fn refused_open_leaves_prior_state_exactly_unchanged() {
    let root = tmp("unchanged");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    // A seeded prior history entry: a refused open must not touch it.
    let prior = history_dir_for(&folder);
    std::fs::create_dir_all(&prior).unwrap();
    write(&prior, "keep.json", "{\"turn\": 1}");
    let before = tree_bytes(&folder);
    let comp_before = std::fs::read(folder.join("composition.kdl")).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"not-a-color\"\n}\n",
    );
    let refused = SdkSession::open(&folder, OpenOptions::default()).expect_err("must refuse");
    assert_eq!(refused.kind, RefusalKind::Invalid);
    // Restore the broken file to its prior bytes, then the whole tree
    // — composition plus seeded history — must match the snapshot.
    std::fs::write(folder.join("composition.kdl"), &comp_before).unwrap();
    assert_eq!(tree_bytes(&folder), before);
    assert_eq!(
        std::fs::read(folder.join("composition.kdl")).unwrap(),
        comp_before
    );
}
