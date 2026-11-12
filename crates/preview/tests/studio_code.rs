//! Code panel regressions (Sprint 18, T003 / CAP-003, J-03,
//! REC-003): the file tree applies new, rename, duplicate, delete,
//! and drag-into-folders on disk; the notation editor saves through
//! validated turns with syntax for notation, JSON, and Markdown;
//! jump lands on the selected element source line; binary files open
//! as named read-only placeholders; invalid saves refuse keeping
//! the last good file; deleting a referenced file refuses naming
//! its referrers.

use preview::studio_code;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-code-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const COMPOSITION: &str = "composition name=\"t\" width=320 height=180 duration=2 fps=10 background=\"#112233\" version=1 {\n    text id=\"title\" value=\"Hi\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n    image id=\"pic\" path=\"pic.svg\" start=0 duration=2 x=0 y=100 width=64 height=64\n    grade {\n        lut src=\"look.cube\" amount=1\n    }\n}\n";

const PIC_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"8\" height=\"8\"><rect width=\"8\" height=\"8\" fill=\"#ff0000\"/></svg>\n";

const LOOK_CUBE: &str = "TITLE \"t\"\nLUT_3D_SIZE 2\n0.0 0.0 0.0\n1.0 0.0 0.0\n0.0 1.0 0.0\n1.0 1.0 0.0\n0.0 0.0 1.0\n1.0 0.0 1.0\n0.0 1.0 1.0\n1.0 1.0 1.0\n";

/// A valid project with a footage clip, a LUT reference, JSON, Markdown,
/// config, and hidden studio state decoys.
fn rich_project(folder: &Path) {
    std::fs::write(folder.join("composition.kdl"), COMPOSITION).unwrap();
    std::fs::write(folder.join("pic.svg"), PIC_SVG).unwrap();
    std::fs::write(folder.join("look.cube"), LOOK_CUBE).unwrap();
    std::fs::write(folder.join("data.json"), "{\"a\": 1}\n").unwrap();
    std::fs::write(folder.join("notes.md"), "# Notes\n").unwrap();
    std::fs::write(
        folder.join("framesmith.toml"),
        "ffmpeg_auto_download = true\n",
    )
    .unwrap();
    let hidden = folder.join(".framesmith");
    std::fs::create_dir_all(&hidden).unwrap();
    std::fs::write(hidden.join("decoy.json"), "{}").unwrap();
}

fn proj(tag: &str) -> PathBuf {
    let root = tmp(tag);
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    rich_project(&folder);
    folder
}

fn tree_action(actor: &str, action: &str, path: &str) -> studio_code::TreeAction {
    studio_code::TreeAction {
        action: action.to_string(),
        actor: actor.to_string(),
        path: path.to_string(),
        name: None,
        target: None,
        content: None,
        confirm: false,
    }
}

fn turns(folder: &Path) -> usize {
    sdk::history::list_turns(folder).unwrap().turns.len()
}

#[test]
fn tree_lists_files_with_languages_and_skips_hidden() {
    let folder = proj("tree");
    let entries = studio_code::list_tree(&folder).unwrap();
    let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    for want in [
        "composition.kdl",
        "data.json",
        "framesmith.toml",
        "look.cube",
        "notes.md",
        "pic.svg",
    ] {
        assert!(paths.contains(&want), "tree holds {want:?}: {paths:?}");
    }
    assert!(
        !paths.iter().any(|p| p.starts_with(".framesmith")),
        "hidden studio state never lists: {paths:?}"
    );
    let lang = |name: &str| {
        entries
            .iter()
            .find(|e| e.path == name)
            .map(|e| e.language)
            .unwrap_or("?")
    };
    assert_eq!(lang("composition.kdl"), "notation");
    assert_eq!(lang("data.json"), "json");
    assert_eq!(lang("notes.md"), "markdown");
    assert_eq!(lang("framesmith.toml"), "toml");
    assert_eq!(lang("pic.svg"), "text");
}

#[test]
fn tree_new_rename_duplicate_move_delete_roundtrip() {
    let folder = proj("roundtrip");
    // New into a fresh folder creates parents and records no turn
    // (Markdown is untracked).
    let mut new = tree_action("maker", "new", "sub/note.md");
    new.content = Some("# Sub\n".to_string());
    let receipt = studio_code::apply_tree_action(&folder, &new).unwrap();
    assert_eq!(receipt.path, "sub/note.md");
    assert_eq!(receipt.turn, None);
    assert!(folder.join("sub/note.md").is_file());
    assert_eq!(turns(&folder), 0);

    // Rename inside the folder.
    let mut rename = tree_action("maker", "rename", "sub/note.md");
    rename.name = Some("renamed.md".to_string());
    let receipt = studio_code::apply_tree_action(&folder, &rename).unwrap();
    assert_eq!(receipt.path, "sub/renamed.md");
    assert!(!folder.join("sub/note.md").exists());

    // Duplicate beside the original, never clobbering.
    let dup = tree_action("maker", "duplicate", "sub/renamed.md");
    let receipt = studio_code::apply_tree_action(&folder, &dup).unwrap();
    assert_eq!(receipt.path, "sub/renamed-copy.md");
    assert!(folder.join("sub/renamed-copy.md").is_file());

    // Drag (move) into the project root.
    let mut mv = tree_action("maker", "move", "sub/renamed.md");
    mv.target = Some(".".to_string());
    let receipt = studio_code::apply_tree_action(&folder, &mv).unwrap();
    assert_eq!(receipt.path, "renamed.md");
    assert!(folder.join("renamed.md").is_file());

    // Confirmed delete removes.
    let mut del = tree_action("maker", "delete", "renamed.md");
    del.confirm = true;
    let receipt = studio_code::apply_tree_action(&folder, &del).unwrap();
    assert_eq!(receipt.action, "delete");
    assert!(!folder.join("renamed.md").exists());
}

#[test]
fn tree_new_folder_then_drag_into_it() {
    let folder = proj("folders");
    let new = tree_action("maker", "new", "assets/");
    let receipt = studio_code::apply_tree_action(&folder, &new).unwrap();
    assert_eq!(receipt.path, "assets");
    assert!(folder.join("assets").is_dir());

    let mut mv = tree_action("maker", "move", "notes.md");
    mv.target = Some("assets".to_string());
    let receipt = studio_code::apply_tree_action(&folder, &mv).unwrap();
    assert_eq!(receipt.path, "assets/notes.md");
    assert!(folder.join("assets/notes.md").is_file());

    let entries = studio_code::list_tree(&folder).unwrap();
    let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    assert!(paths.contains(&"assets"));
    assert!(paths.contains(&"assets/notes.md"));
}

#[test]
fn tree_new_kdl_records_turn_and_undo_restores() {
    let folder = proj("kdl-turn");
    let new = tree_action("maker", "new", "extra.kdl");
    let receipt = studio_code::apply_tree_action(&folder, &new).unwrap();
    let seq = receipt.turn.expect("kdl tree writes record a turn");
    assert!(folder.join("extra.kdl").is_file());
    assert_eq!(turns(&folder), 1);
    sdk::history::undo_turn(&folder, seq).expect("undo walks the tree turn");
    assert!(
        !folder.join("extra.kdl").exists(),
        "undo removes the created kdl file"
    );
}

#[test]
fn tree_delete_needs_confirmation_and_guards_root() {
    let folder = proj("confirm");
    let del = tree_action("maker", "delete", "notes.md");
    let err = studio_code::apply_tree_action(&folder, &del).unwrap_err();
    assert!(err.contains("confirm"), "delete asks first: {err}");
    assert!(folder.join("notes.md").is_file());

    for verb in ["delete", "rename", "move"] {
        let mut action = tree_action("maker", verb, "composition.kdl");
        action.name = Some("other.kdl".to_string());
        action.target = Some(".".to_string());
        action.confirm = true;
        let err = studio_code::apply_tree_action(&folder, &action).unwrap_err();
        assert!(
            err.contains("project root"),
            "{verb} guards the root composition: {err}"
        );
    }
    assert!(folder.join("composition.kdl").is_file());
}

#[test]
fn tree_guards_clobber_missing_and_folders() {
    let folder = proj("guards");
    let new = tree_action("maker", "new", "notes.md");
    assert!(studio_code::apply_tree_action(&folder, &new)
        .unwrap_err()
        .contains("already lives there"));

    let mut rename = tree_action("maker", "rename", "notes.md");
    rename.name = Some("data.json".to_string());
    assert!(studio_code::apply_tree_action(&folder, &rename)
        .unwrap_err()
        .contains("already lives there"));

    let mut mv = tree_action("maker", "move", "notes.md");
    mv.target = Some("nope".to_string());
    assert!(studio_code::apply_tree_action(&folder, &mv)
        .unwrap_err()
        .contains("no folder lives there"));

    let del = tree_action("maker", "delete", "missing.md");
    let mut del = del;
    del.confirm = true;
    assert!(studio_code::apply_tree_action(&folder, &del)
        .unwrap_err()
        .contains("no file lives there"));
}

#[test]
fn save_valid_kdl_records_turn_and_undo_restores() {
    let folder = proj("save-turn");
    let updated = COMPOSITION.replace("value=\"Hi\"", "value=\"Hey\"");
    let save = studio_code::CodeSave {
        actor: "maker".to_string(),
        path: "composition.kdl".to_string(),
        content: updated.clone(),
    };
    let receipt = studio_code::save_code_file(&folder, &save).unwrap();
    let seq = receipt.turn.expect("kdl saves record a turn");
    assert_eq!(
        std::fs::read_to_string(folder.join("composition.kdl")).unwrap(),
        updated
    );
    sdk::history::undo_turn(&folder, seq).expect("undo walks the save");
    assert_eq!(
        std::fs::read_to_string(folder.join("composition.kdl")).unwrap(),
        COMPOSITION,
        "undo restores the last good bytes"
    );
}

#[test]
fn save_invalid_kdl_refuses_keeping_last_good() {
    let folder = proj("bad-kdl");
    let save = studio_code::CodeSave {
        actor: "maker".to_string(),
        path: "composition.kdl".to_string(),
        content: "}\n".to_string(),
    };
    let before = std::fs::read_to_string(folder.join("composition.kdl")).unwrap();
    let err = studio_code::save_code_file(&folder, &save).unwrap_err();
    assert!(
        err.contains("no longer validates") || err.contains("does not scan"),
        "refusal names the cause: {err}"
    );
    assert_eq!(
        std::fs::read_to_string(folder.join("composition.kdl")).unwrap(),
        before,
        "the last good file stays"
    );
    assert_eq!(turns(&folder), 0, "no turn records a refused save");
}

#[test]
fn save_invalid_kdl_semantics_refuses_with_diagnostics() {
    let folder = proj("bad-sem");
    // Well-formed but invalid: the image points at a missing file.
    let updated = COMPOSITION.replace("path=\"pic.svg\"", "path=\"gone.svg\"");
    let save = studio_code::CodeSave {
        actor: "maker".to_string(),
        path: "composition.kdl".to_string(),
        content: updated,
    };
    let err = studio_code::save_code_file(&folder, &save).unwrap_err();
    assert!(err.contains("E_"), "refusal carries diagnostics: {err}");
    assert_eq!(
        std::fs::read_to_string(folder.join("composition.kdl")).unwrap(),
        COMPOSITION
    );
    assert_eq!(turns(&folder), 0);
}

#[test]
fn save_invalid_json_and_config_refuse_keeping_last_good() {
    let folder = proj("bad-json");
    let save = studio_code::CodeSave {
        actor: "maker".to_string(),
        path: "data.json".to_string(),
        content: "{bad".to_string(),
    };
    let err = studio_code::save_code_file(&folder, &save).unwrap_err();
    assert!(err.contains("does not parse"), "json refusal: {err}");
    assert_eq!(
        std::fs::read_to_string(folder.join("data.json")).unwrap(),
        "{\"a\": 1}\n"
    );

    let save = studio_code::CodeSave {
        actor: "maker".to_string(),
        path: "framesmith.toml".to_string(),
        content: "bogus = 1\n".to_string(),
    };
    let err = studio_code::save_code_file(&folder, &save).unwrap_err();
    assert!(err.contains("no longer validates"), "config refusal: {err}");
    assert_eq!(
        std::fs::read_to_string(folder.join("framesmith.toml")).unwrap(),
        "ffmpeg_auto_download = true\n"
    );

    let save = studio_code::CodeSave {
        actor: "maker".to_string(),
        path: "framesmith.toml".to_string(),
        content: "ffmpeg_auto_download = false\n".to_string(),
    };
    let receipt = studio_code::save_code_file(&folder, &save).unwrap();
    assert_eq!(receipt.turn, None, "config saves directly without a turn");
    assert_eq!(
        std::fs::read_to_string(folder.join("framesmith.toml")).unwrap(),
        "ffmpeg_auto_download = false\n"
    );
}

#[test]
fn save_markdown_applies_without_turn() {
    let folder = proj("md-save");
    let save = studio_code::CodeSave {
        actor: "maker".to_string(),
        path: "notes.md".to_string(),
        content: "# Notes\n\nEdited.\n".to_string(),
    };
    let receipt = studio_code::save_code_file(&folder, &save).unwrap();
    assert_eq!(receipt.turn, None, "untracked files save without a turn");
    assert_eq!(
        std::fs::read_to_string(folder.join("notes.md")).unwrap(),
        "# Notes\n\nEdited.\n"
    );
}

#[test]
fn save_missing_and_binary_refuse() {
    let folder = proj("save-guard");
    let save = studio_code::CodeSave {
        actor: "maker".to_string(),
        path: "missing.md".to_string(),
        content: "x".to_string(),
    };
    assert!(studio_code::save_code_file(&folder, &save)
        .unwrap_err()
        .contains("tree `new`"));

    std::fs::write(folder.join("clip.bin"), [0u8, 159, 146, 150]).unwrap();
    let save = studio_code::CodeSave {
        actor: "maker".to_string(),
        path: "clip.bin".to_string(),
        content: "text".to_string(),
    };
    assert!(studio_code::save_code_file(&folder, &save)
        .unwrap_err()
        .contains("read-only placeholder"));
}

#[test]
fn jump_lands_on_selected_element_source_line() {
    let folder = proj("jump");
    let title = studio_code::jump_to_selection(&folder, None, "title").unwrap();
    assert_eq!(title.file, "composition.kdl");
    assert_eq!(title.line, 2, "jump lands on the text line");
    assert!(title.column > 0);
    assert_eq!(title.kind, "text");

    let pic = studio_code::jump_to_selection(&folder, None, "pic").unwrap();
    assert_eq!((pic.line, pic.kind.as_str()), (3, "image"));

    let hash = studio_code::jump_to_selection(&folder, None, "#1").unwrap();
    assert_eq!(hash.line, 3);
    let bare = studio_code::jump_to_selection(&folder, None, "0").unwrap();
    assert_eq!(bare.line, 2);

    let err = studio_code::jump_to_selection(&folder, None, "nope").unwrap_err();
    assert!(
        err.contains("\"title\"") && err.contains("\"pic\""),
        "unknown keys name what the composition holds: {err}"
    );
}

#[test]
fn read_text_and_binary_placeholder() {
    let folder = proj("read");
    let file = studio_code::read_code_file(&folder, "notes.md").unwrap();
    assert!(!file.placeholder);
    assert_eq!(file.language, "markdown");
    assert_eq!(file.content.as_deref(), Some("# Notes\n"));

    std::fs::write(folder.join("clip.bin"), [0u8, 159, 146, 150]).unwrap();
    let bin = studio_code::read_code_file(&folder, "clip.bin").unwrap();
    assert!(bin.placeholder, "binaries open as placeholders");
    assert_eq!(bin.content, None);
    assert_eq!(bin.path, "clip.bin");
    assert_eq!(bin.size, 4);
}

#[test]
fn delete_referenced_file_refuses_naming_referrers() {
    let folder = proj("referrers");
    let mut del = tree_action("maker", "delete", "look.cube");
    del.confirm = true;
    let err = studio_code::apply_tree_action(&folder, &del).unwrap_err();
    assert!(
        err.contains("lut of composition") && err.contains("look.cube"),
        "cube delete names the lut referrer: {err}"
    );

    let mut del = tree_action("maker", "delete", "pic.svg");
    del.confirm = true;
    let err = studio_code::apply_tree_action(&folder, &del).unwrap_err();
    assert!(
        err.contains("image clip \"pic\""),
        "footage delete names the clip referrer: {err}"
    );
    assert!(
        folder.join("pic.svg").is_file(),
        "blocked deletes remove nothing"
    );

    // Unreferencing first frees the files.
    let freed = "composition name=\"t\" width=320 height=180 duration=2 fps=10 background=\"#112233\" version=1 {\n    text id=\"title\" value=\"Hi\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n}\n";
    let save = studio_code::CodeSave {
        actor: "maker".to_string(),
        path: "composition.kdl".to_string(),
        content: freed.to_string(),
    };
    studio_code::save_code_file(&folder, &save).unwrap();
    for name in ["look.cube", "pic.svg"] {
        let mut del = tree_action("maker", "delete", name);
        del.confirm = true;
        studio_code::apply_tree_action(&folder, &del)
            .unwrap_or_else(|e| panic!("free file {name:?} deletes: {e}"));
        assert!(!folder.join(name).exists());
    }
}

#[test]
fn rename_and_move_referenced_file_refuse() {
    let folder = proj("ref-move");
    let mut rename = tree_action("maker", "rename", "pic.svg");
    rename.name = Some("other.svg".to_string());
    assert!(studio_code::apply_tree_action(&folder, &rename)
        .unwrap_err()
        .contains("image clip \"pic\""));

    std::fs::create_dir_all(folder.join("sub")).unwrap();
    let mut mv = tree_action("maker", "move", "look.cube");
    mv.target = Some("sub".to_string());
    assert!(studio_code::apply_tree_action(&folder, &mv)
        .unwrap_err()
        .contains("lut of composition"));
}

#[test]
fn escapes_and_hidden_paths_refuse() {
    let folder = proj("escape");
    for bad in [
        "../evil.md",
        ".framesmith/x.md",
        "/abs.md",
        "a/../../b.md",
        "",
    ] {
        assert!(
            studio_code::resolve_code_path(&folder, bad).is_err(),
            "path {bad:?} refuses"
        );
    }
    let save = studio_code::CodeSave {
        actor: "maker".to_string(),
        path: "../evil.md".to_string(),
        content: "x".to_string(),
    };
    assert!(studio_code::save_code_file(&folder, &save).is_err());
}

#[test]
fn language_hints_cover_notation_json_markdown() {
    assert_eq!(studio_code::language_of("composition.kdl"), "notation");
    assert_eq!(studio_code::language_of("data.JSON"), "json");
    assert_eq!(studio_code::language_of("notes.md"), "markdown");
    assert_eq!(studio_code::language_of("notes.markdown"), "markdown");
    assert_eq!(studio_code::language_of("framesmith.toml"), "toml");
    assert_eq!(studio_code::language_of("pic.svg"), "text");
    assert_eq!(studio_code::language_of("README"), "text");
}

#[test]
fn panel_fragments_carry_tree_editor_and_jump() {
    let html = studio_code::code_html();
    assert!(html.contains("code-tree"), "tree lists");
    assert!(html.contains("code-editor"), "editor edits");
    assert!(html.contains("code-jump"), "jump acts");
    let js = studio_code::code_js();
    for endpoint in [
        "/api/studio/code/tree",
        "/api/studio/code/file",
        "/api/studio/code/save",
        "/api/studio/code/jump",
    ] {
        assert!(js.contains(endpoint), "script calls {endpoint}");
    }
    assert!(js.contains("confirm("), "deletes confirm in the panel");
    let css = studio_code::code_css();
    assert!(
        css.contains("background-color"),
        "hyphenated style vocabulary"
    );
    assert!(!css.contains("backgroundColor"), "no camelCase style keys");
}

/// Minimal HTTP exchange over loopback. Returns (status code, body).
fn http(port: u16, method: &str, target: &str, body: Option<&str>) -> (u16, Vec<u8>) {
    let addr = format!("127.0.0.1:{port}");
    let mut stream = TcpStream::connect_timeout(&addr.parse().unwrap(), Duration::from_secs(5))
        .expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let payload = body.unwrap_or("");
    let request = format!(
        "{method} {target} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    let text = String::from_utf8_lossy(&raw).into_owned();
    let code: u16 = text
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse().ok())
        .expect("status line");
    let body = match raw.windows(4).position(|w| w == b"\r\n\r\n") {
        Some(i) => raw[i + 4..].to_vec(),
        None => Vec::new(),
    };
    (code, body)
}

fn body_str(port: u16, method: &str, target: &str, body: Option<&str>) -> (u16, String) {
    let (code, raw) = http(port, method, target, body);
    (code, String::from_utf8_lossy(&raw).into_owned())
}

#[test]
fn code_routes_roundtrip_over_http() {
    use preview::daemon;
    use preview::server::{run_serve, ServeOptions};

    let root = tmp("http");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    rich_project(&folder);
    let reg = root.join("registry.json");
    let dir = daemon::canonical_dir(&folder).unwrap();
    let opts = ServeOptions {
        folder: folder.clone(),
        port: 0,
        registry: reg.clone(),
        composition: None,
    };
    let handle = std::thread::spawn(move || run_serve(&opts));
    let start = Instant::now();
    let entry = loop {
        let entries = daemon::load_registry(&reg).unwrap_or_default();
        if let Some(e) = entries
            .into_iter()
            .find(|e| e.dir == dir && daemon::is_live(e.port))
        {
            break e;
        }
        if handle.is_finished() {
            panic!("server exited before serving");
        }
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "server never ready"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    let port = entry.port;

    let (code, tree) = body_str(port, "GET", "/api/studio/code/tree", None);
    assert_eq!(code, 200, "tree lists: {tree}");
    assert!(
        tree.contains("composition.kdl"),
        "tree names the root: {tree}"
    );

    let (code, file) = body_str(
        port,
        "GET",
        "/api/studio/code/file?path=composition.kdl",
        None,
    );
    assert_eq!(code, 200, "file opens: {file}");
    assert!(
        file.contains("title"),
        "editor carries the notation: {file}"
    );

    let (code, jump) = body_str(port, "GET", "/api/studio/code/jump?selection=title", None);
    assert_eq!(code, 200, "jump lands: {jump}");
    assert!(jump.contains("\"line\":2"), "jump lands on line 2: {jump}");

    let (code, missing) = body_str(port, "GET", "/api/studio/code/jump?selection=nope", None);
    assert_eq!(code, 404, "unknown selections refuse: {missing}");

    let new_body = "{\"actor\":\"maker\",\"action\":\"new\",\"path\":\"http-note.md\",\"content\":\"# Hi\\n\"}";
    let (code, created) = body_str(port, "POST", "/api/studio/code/tree", Some(new_body));
    assert_eq!(code, 200, "tree creates: {created}");
    assert!(folder.join("http-note.md").is_file());

    let bad_save = "{\"actor\":\"maker\",\"path\":\"composition.kdl\",\"content\":\"}\\n\"}";
    let (code, refused) = body_str(port, "POST", "/api/studio/code/save", Some(bad_save));
    assert_eq!(code, 409, "invalid saves refuse: {refused}");
    assert_eq!(
        std::fs::read_to_string(folder.join("composition.kdl")).unwrap(),
        COMPOSITION,
        "refused saves keep the last good file"
    );

    let good = COMPOSITION.replace("value=\"Hi\"", "value=\"Served\"");
    let good_body = serde_json::to_string(&serde_json::json!({
        "actor": "maker",
        "path": "composition.kdl",
        "content": good,
    }))
    .unwrap();
    let (code, saved) = body_str(port, "POST", "/api/studio/code/save", Some(&good_body));
    assert_eq!(code, 200, "valid saves apply: {saved}");
    assert!(
        saved.contains("\"turn\":0") || saved.contains("\"turn\":"),
        "save records a turn: {saved}"
    );

    assert!(daemon::request_shutdown(port), "shutdown answers");
    assert_eq!(handle.join().expect("server thread"), 0);
    assert!(!daemon::is_live(port), "port is quiet after shutdown");
}
