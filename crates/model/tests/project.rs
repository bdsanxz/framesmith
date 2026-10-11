use model::{check, classify, default_output, Reference, Severity};
use std::path::{Path, PathBuf};

/// Makes a fresh project folder holding the given files.
fn project(name: &str, files: &[(&str, &[u8])]) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("model-project").join(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for (path, content) in files {
        let full = root.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, content).unwrap();
    }
    root
}

fn page(body: &str) -> String {
    format!("<!doctype html>\n<html>\n<head>\n<link rel=\"stylesheet\" href=\"style.css\">\n</head>\n<body>\n<div data-fs-composition=\"main\" data-fs-size=\"640x360\" data-fs-duration=\"2\">\n{body}\n</div>\n</body>\n</html>\n")
}

/// The code, file and line of every diagnostic.
fn found(root: &Path) -> Vec<(String, String, u32)> {
    check(root).diagnostics.into_iter().map(|d| (d.code, d.file, d.line)).collect()
}

fn one(code: &str, file: &str, line: u32) -> Vec<(String, String, u32)> {
    vec![(code.to_string(), file.to_string(), line)]
}

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x04IHDR\x00\x00\x00\x00\x00\x00\x00\x00";

#[test]
fn t007_c1_a_valid_project_passes_with_its_facts() {
    let body = "<h1 id=\"title\" data-fs-clip data-fs-start=\"0\" data-fs-duration=\"2\">Hi</h1>\n<img src=\"art/dot.png\">\n<script type=\"application/framesmith-motion+json\">\n[ { \"target\": \"#title\", \"to\": { \"x\": 100 }, \"at\": 0, \"duration\": 1 } ]\n</script>";
    let root = project(
        "valid",
        &[
            ("index.html", page(body).as_bytes()),
            (
                "style.css",
                b"@import 'more.css';\nh1 { color: white; background: url(art/dot.png) }",
            ),
            ("more.css", b"@font-face { font-family: Ours; src: url(/fonts/ours.woff2) }"),
            ("art/dot.png", PNG),
            ("fonts/ours.woff2", b"wOF2"),
        ],
    );
    let report = check(&root);
    assert_eq!(report.diagnostics, vec![]);
    assert_eq!(report.exit_code(), 0);
    let manifest = report.manifest.unwrap();
    assert_eq!(
        (manifest.name.as_str(), manifest.width, manifest.height, manifest.fps),
        ("main", 640, 360, 30)
    );
    assert_eq!((manifest.clips.len(), manifest.tweens.len(), manifest.tweens[0].matches), (1, 1, 1));
    assert_eq!(manifest.clips[0].label, "#title");
}

#[test]
fn t007_c3_check_changes_nothing_in_the_project() {
    let root = project("untouched", &[("index.html", page("").as_bytes()), ("style.css", b"")]);
    let listing = |root: &Path| {
        let mut names: Vec<_> = std::fs::read_dir(root)
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                (
                    e.file_name(),
                    e.metadata().unwrap().len(),
                    e.metadata().unwrap().modified().unwrap(),
                )
            })
            .collect();
        names.sort();
        names
    };
    let before = listing(&root);
    check(&root);
    assert_eq!(listing(&root), before);
}

#[test]
fn t009_a_missing_folder_or_entry_page_is_an_error() {
    let root = project("no-entry", &[("readme.txt", b"hello")]);
    let report = check(&root);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!((report.diagnostics[0].code.as_str(), report.exit_code()), ("FS-E002", 1));
    assert!(report.diagnostics[0].description.contains("index.html"));

    let missing = root.join("nowhere");
    let report = check(&missing);
    assert_eq!((report.diagnostics[0].code.as_str(), report.exit_code()), ("FS-E001", 1));
    assert!(report.diagnostics[0].description.contains("nowhere"));
    let report = check(&root.join("readme.txt"));
    assert_eq!(report.diagnostics[0].code, "FS-E001");
    assert!(report.diagnostics[0].description.contains("not a folder"));
}

#[test]
fn t013_missing_and_outside_references_are_errors() {
    let body = "<img src=\"art/gone.png\">\n<img src=\"../secret.png\">\n<img src=\"file:///etc/hosts\">\n<img src=\"/__framesmith__/loader.js\">\n<img src=\"data:image/png;base64,AAAA\">";
    let root = project(
        "references",
        &[
            ("index.html", page(body).as_bytes()),
            (
                "style.css",
                b"\n.a { background: url(nope.png) }\n@font-face { src: url('fonts/gone.woff2') }",
            ),
        ],
    );
    assert_eq!(
        found(&root),
        vec![
            ("FS-E030".to_string(), "index.html".to_string(), 8),
            ("FS-E031".to_string(), "index.html".to_string(), 9),
            ("FS-E031".to_string(), "index.html".to_string(), 10),
            ("FS-E031".to_string(), "index.html".to_string(), 11),
            ("FS-E030".to_string(), "style.css".to_string(), 2),
            ("FS-E030".to_string(), "style.css".to_string(), 3),
        ]
    );
    let report = check(&root);
    assert!(report.diagnostics[0].description.contains("art/gone.png"));
    assert!(report.diagnostics.iter().all(|d| d.kind == Severity::Error));

    let missing_sheet = project("no-sheet", &[("index.html", page("").as_bytes())]);
    assert_eq!(found(&missing_sheet), one("FS-E030", "index.html", 4));
}

#[test]
fn t029_media_and_embedded_documents_are_unsupported() {
    let body = "<video src=\"clip.mp4\"></video>\n<audio></audio>\n<iframe></iframe>\n<embed>\n<object></object>\n<svg><rect><animate/></rect><set/></svg>\n<set></set>";
    let root = project("elements", &[("index.html", page(body).as_bytes()), ("style.css", b"")]);
    let codes: Vec<(String, u32)> = found(&root).into_iter().map(|(code, _, line)| (code, line)).collect();
    let want = [
        ("FS-U001", 8),
        ("FS-U002", 9),
        ("FS-U018", 10),
        ("FS-U018", 11),
        ("FS-U018", 12),
        ("FS-U019", 13),
        ("FS-U019", 13),
    ];
    assert_eq!(codes, want.map(|(code, line)| (code.to_string(), line)).to_vec());
    assert_eq!(check(&root).exit_code(), 2);
}

#[test]
fn t029_a_composition_in_another_page_is_unsupported() {
    let root = project(
        "two-pages",
        &[
            ("index.html", page("").as_bytes()),
            ("style.css", b""),
            (
                "other.html",
                b"<html>\n<body>\n<div data-fs-composition=\"second\" data-fs-size=\"2x2\"></div>",
            ),
            ("notes.html", b"<p>no composition here</p>"),
            ("sub/deep.html", b"<div data-fs-composition=\"deep\"></div>"),
        ],
    );
    assert_eq!(found(&root), one("FS-U005", "other.html", 3));
}

#[test]
fn t030_animated_images_and_network_assets_are_unsupported() {
    let mut gif = b"GIF89a\x01\x00\x01\x00\x00\x00\x00".to_vec();
    for _ in 0..2 {
        gif.extend(b"\x2C\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00");
    }
    gif.push(0x3B);
    let body = "<img src=\"spin.gif\">\n<img src=\"https://example.com/a.png\">\n<img src=\"//example.com/b.png\">\n<script src=\"http://example.com/lib.js\"></script>";
    let root = project(
        "assets",
        &[
            ("index.html", page(body).as_bytes()),
            ("style.css", b"@import url(https://fonts.example.com/css);\n.a { background: url(spin.gif) }\n@font-face { src: url(\"HTTPS://example.com/f.woff2\") }"),
            ("spin.gif", &gif),
        ],
    );
    let want = [
        ("FS-U008", "index.html", 8),
        ("FS-U009", "index.html", 9),
        ("FS-U009", "index.html", 10),
        ("FS-U009", "index.html", 11),
        ("FS-U009", "style.css", 1),
        ("FS-U008", "style.css", 2),
        ("FS-U009", "style.css", 3),
    ];
    assert_eq!(found(&root), want.map(|(c, f, l)| (c.to_string(), f.to_string(), l)).to_vec());
    assert_eq!(check(&root).exit_code(), 2);
}

#[test]
fn t031_css_animation_and_transition_are_unsupported() {
    let body = "<style>\n.a { animation: spin 1s; }\n.b { animation: none; transition: none; transition-duration: 0s }\n</style>\n<p style=\"transition: opacity 1s\">x</p>";
    let root = project(
        "css-motion",
        &[
            ("index.html", page(body).as_bytes()),
            ("style.css", b"@keyframes spin { to { rotate: 1turn } }\n.c { -webkit-animation-name: spin }\n.d { transition-duration: .3s }\n.e { animation-duration: 2s }"),
        ],
    );
    let want = [
        ("FS-U014", "index.html", 9),
        ("FS-U015", "index.html", 12),
        ("FS-U014", "style.css", 1),
        ("FS-U014", "style.css", 2),
        ("FS-U015", "style.css", 3),
    ];
    assert_eq!(found(&root), want.map(|(c, f, l)| (c.to_string(), f.to_string(), l)).to_vec());
}

#[test]
fn t031_script_uses_are_unsupported_in_the_page_and_in_script_files() {
    let body = "<script>\n  // fine\n  document.querySelector('p').animate([], 500);\n</script>\n<script src=\"app.js\"></script>\n<script type=\"application/json\">{ \"framesmith\": \".animate(\" }</script>";
    let root = project(
        "scripts",
        &[
            ("index.html", page(body).as_bytes()),
            ("style.css", b""),
            ("app.js", b"\n\nframesmith.timeline();\n"),
        ],
    );
    let want = [("FS-U016", "index.html", 10), ("FS-U017", "app.js", 3)];
    assert_eq!(found(&root), want.map(|(c, f, l)| (c.to_string(), f.to_string(), l)).to_vec());
}

#[test]
fn t031_own_transform_properties_on_a_targeted_element_are_unsupported() {
    let body = "<p id=\"a\" style=\"rotate: 10deg\">a</p>\n<p id=\"b\" style=\"rotate: 10deg; transform: scale(2)\">b</p>\n<script type=\"application/framesmith-motion+json\">\n[ { \"target\": \"#a\", \"to\": { \"x\": 1 }, \"at\": 0, \"duration\": 1 } ]\n</script>";
    let root = project("own-transform", &[("index.html", page(body).as_bytes()), ("style.css", b"")]);
    assert_eq!(found(&root), one("FS-U021", "index.html", 8));
}

#[test]
fn t020_targets_are_matched_inside_the_composition() {
    let body = "<p class=\"in\">a</p>\n<script type=\"application/framesmith-motion+json\">\n[\n  { \"target\": \".in\", \"to\": { \"x\": 1 }, \"at\": 0, \"duration\": 1 },\n  { \"target\": \".out\", \"to\": { \"x\": 1 }, \"at\": 0, \"duration\": 1 },\n  { \"target\": \"p:first-child\", \"to\": { \"x\": 1 }, \"at\": 0, \"duration\": 1 }\n]\n</script>";
    let source = page(body).replace("</body>", "<p class=\"out\">outside</p>\n</body>");
    let root = project("targets", &[("index.html", source.as_bytes()), ("style.css", b"")]);
    let want = [("FS-E044", "index.html", 12), ("FS-U020", "index.html", 13)];
    assert_eq!(found(&root), want.map(|(c, f, l)| (c.to_string(), f.to_string(), l)).to_vec());
    assert!(check(&root).diagnostics[0].description.contains(".out"));
}

#[test]
fn references_are_classified() {
    let top = Path::new("");
    let css = Path::new("css");
    assert_eq!(classify("art/a.png", top), Reference::Local(PathBuf::from("art/a.png")));
    assert_eq!(
        classify("./art/../art/a%20b.png?v=2#x", top),
        Reference::Local(PathBuf::from("art/a b.png"))
    );
    assert_eq!(classify("../art/a.png", css), Reference::Local(PathBuf::from("art/a.png")));
    assert_eq!(classify("/art/a.png", css), Reference::Local(PathBuf::from("art/a.png")));
    assert_eq!(classify("../../a.png", css), Reference::Outside);
    assert_eq!(classify("file:///a.png", top), Reference::Outside);
    assert_eq!(classify("HTTP://x/a.png", top), Reference::Network);
    assert_eq!(classify("//x/a.png", top), Reference::Network);
    assert_eq!(classify("/__framesmith__/runtime.wasm", top), Reference::Reserved);
    for nothing in ["", "#frag", "data:image/png;base64,AA", "about:blank", " "] {
        assert_eq!(classify(nothing, top), Reference::Nothing, "{nothing}");
    }
}

#[test]
fn t006_c1_the_default_output_is_named_after_the_project_folder() {
    let root = project("demo-card", &[("index.html", b"")]);
    assert_eq!(default_output(&root), root.join("out").join("demo-card.mp4"));
}
