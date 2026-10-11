use compile::{base_style, entry_page, LOADER_JS, RUNTIME_WASM};

#[test]
fn the_loader_and_base_style_come_first_after_the_document_type() {
    let source = "<!doctype html>\n<html>\n<head><script>first()</script></head>\n<body></body>\n</html>\n";
    let page = entry_page(source, 640, 360);
    assert!(page.starts_with("<!doctype html><script src=\"/__framesmith__/loader.js\"></script><style>"));
    assert!(page.find("loader.js").unwrap() < page.find("first()").unwrap());
    assert!(page.ends_with("</style>\n<html>\n<head><script>first()</script></head>\n<body></body>\n</html>\n"));
    // Nothing is added to the line count, so messages keep the author's lines.
    assert_eq!(page.lines().count(), source.lines().count());

    let bare = entry_page("<div></div>", 2, 2);
    assert!(bare.starts_with("<script src="));
    assert!(bare.ends_with("<div></div>"));
}

#[test]
fn t010_c1_the_base_style_sizes_the_composition_and_hides_clips() {
    let style = base_style(1920, 1080);
    assert!(style.contains("width:1920px;height:1080px"));
    assert!(style.contains("background:#000"));
    assert!(style.contains(":where(html){background:#000;text-rendering:geometricPrecision}"));
    assert!(style.contains("[data-fs-clip]:not([fs-shown])"));
    assert!(style.contains("visibility:hidden!important"));
}

#[test]
fn the_runtime_files_are_embedded() {
    // A WebAssembly module starts with "\0asm" and version 1.
    assert_eq!(&RUNTIME_WASM[..8], b"\0asm\x01\0\0\0");
    assert!(LOADER_JS.contains("__framesmith"));
    // The loader stays small: logic belongs in Rust.
    assert!(LOADER_JS.lines().count() < 300, "loader.js has {} lines", LOADER_JS.lines().count());
}
