//! The preview server on its own, driven with plain sockets: where it listens, what each path answers,
//! and what it refuses. Specified in `docs/spec/preview.md`.

use server::Preview;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/preview/frame-zero")
}

struct Reply {
    status: u16,
    head: String,
    body: Vec<u8>,
}

/// Sends `request` as it is written and reads the whole answer (the server closes the connection).
fn send(port: u16, request: &str) -> Reply {
    let mut stream = TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), Duration::from_secs(5)).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut bytes = Vec::new();
    let _ = stream.read_to_end(&mut bytes);
    let at = bytes.windows(4).position(|w| w == b"\r\n\r\n").expect("an answer with a head");
    let head = String::from_utf8_lossy(&bytes[..at]).into_owned();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    Reply {
        status,
        head,
        body: bytes[at + 4..].to_vec(),
    }
}

fn get(port: u16, target: &str) -> Reply {
    send(port, &format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"))
}

fn text(reply: &Reply) -> String {
    String::from_utf8_lossy(&reply.body).into_owned()
}

#[test]
fn s2_t060_c1_the_preview_gives_an_address_on_the_loopback_address_that_answers_at_once() {
    let preview = Preview::start(&project()).unwrap();
    assert_eq!(preview.address(), format!("http://127.0.0.1:{}/", preview.port()));
    assert_ne!(preview.port(), 0);
    let page = get(preview.port(), "/");
    assert_eq!(page.status, 200, "{}", page.head);
    let body = text(&page);
    assert!(body.contains("data-state=\"loading\""), "{body}");
    assert!(body.contains("src=\"/index.html\""), "{body}");
    assert!(page.head.contains("Cache-Control: no-store"), "{}", page.head);
}

#[test]
fn s2_t060_c1_the_entry_page_is_the_prepared_one_and_the_runtime_files_are_served() {
    let preview = Preview::start(&project()).unwrap();
    let entry = get(preview.port(), "/index.html");
    assert_eq!(entry.status, 200);
    assert!(text(&entry).contains("/__framesmith__/loader.js"), "{}", text(&entry));
    // An encoded spelling of the entry's name is the prepared page too, never the raw file.
    assert_eq!(text(&get(preview.port(), "/%69ndex.html")), text(&entry));
    let loader = get(preview.port(), "/__framesmith__/loader.js");
    assert_eq!(loader.status, 200);
    assert!(loader.head.contains("text/javascript"), "{}", loader.head);
    let wasm = get(preview.port(), "/__framesmith__/runtime.wasm");
    assert_eq!(wasm.status, 200);
    assert_eq!(&wasm.body[..4], b"\0asm");
    assert_eq!(get(preview.port(), "/__framesmith__/preview.js").status, 200);
    assert_eq!(get(preview.port(), "/__framesmith__/other").status, 404);
    let style = get(preview.port(), "/style.css");
    assert_eq!(style.status, 200);
    assert!(style.head.contains("text/css"), "{}", style.head);
}

#[test]
fn s2_t060_c3_the_address_is_the_loopback_address_and_it_answers_there() {
    // The test of the other addresses of the machine is in the command's tests, which list them.
    let preview = Preview::start(&project()).unwrap();
    assert!(preview.address().starts_with("http://127.0.0.1:"));
    let stream = TcpStream::connect_timeout(
        &SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), preview.port()),
        Duration::from_secs(3),
    )
    .unwrap();
    assert!(stream.peer_addr().unwrap().ip().is_loopback());
}

#[test]
fn s2_t060_c1_a_method_other_than_get_a_broken_request_and_a_long_head_are_refused() {
    let preview = Preview::start(&project()).unwrap();
    let port = preview.port();
    let post = send(port, "POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\n\r\n");
    assert_eq!(post.status, 405);
    assert!(post.head.contains("Allow: GET"), "{}", post.head);
    assert_eq!(send(port, "HEAD / HTTP/1.1\r\nHost: x\r\n\r\n").status, 405);
    assert_eq!(send(port, "GET\r\n\r\n").status, 400);
    assert_eq!(send(port, "GET index.html HTTP/1.1\r\n\r\n").status, 400);
    assert_eq!(send(port, "GET / SPDY/3\r\n\r\n").status, 400);
    let long = format!("GET / HTTP/1.1\r\nX-Long: {}\r\n\r\n", "a".repeat(20_000));
    assert_eq!(send(port, &long).status, 431);
}

#[test]
fn s2_t060_c1_a_silent_connection_does_not_stop_the_server_from_answering_others() {
    let preview = Preview::start(&project()).unwrap();
    let _silent = TcpStream::connect((Ipv4Addr::LOCALHOST, preview.port())).unwrap();
    assert_eq!(get(preview.port(), "/").status, 200);
}

#[test]
fn s2_t060_c1_a_path_that_climbs_out_of_the_project_or_is_not_there_is_not_found() {
    let preview = Preview::start(&project()).unwrap();
    let port = preview.port();
    for target in [
        "/../Cargo.toml",
        "/%2e%2e/Cargo.toml",
        "/%2E%2E/%2e%2e/etc/passwd",
        "/a/../../index.rs",
        "/missing.png",
        "/..%2f..%2fCargo.toml",
        "/.",
    ] {
        let reply = get(port, target);
        assert!(matches!(reply.status, 404 | 200), "{target}: {} {}", reply.status, reply.head);
        if reply.status == 200 {
            // `/.` is the project's own root, which is the preview page.
            assert!(text(&reply).contains("data-state"), "{target}");
        }
    }
    assert_eq!(get(port, "/../Cargo.toml").status, 404);
    assert_eq!(get(port, "/%2e%2e/Cargo.toml").status, 404);
    assert_eq!(get(port, "/missing.png").status, 404);
}

#[cfg(unix)]
#[test]
fn s2_t060_c1_a_link_that_leads_out_of_the_project_is_not_served() {
    let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join("s2_t060_links");
    let _ = std::fs::remove_dir_all(&base);
    let inside = base.join("project");
    std::fs::create_dir_all(&inside).unwrap();
    std::fs::write(
        inside.join("index.html"),
        "<!doctype html><html><body><div data-fs-composition=\"t\" data-fs-size=\"64x36\" data-fs-fps=\"30\" data-fs-duration=\"1\"></div></body></html>",
    )
    .unwrap();
    std::fs::write(base.join("secret.txt"), "outside").unwrap();
    std::fs::write(inside.join("inside.txt"), "inside").unwrap();
    std::os::unix::fs::symlink(base.join("secret.txt"), inside.join("out.txt")).unwrap();
    std::os::unix::fs::symlink(inside.join("inside.txt"), inside.join("in.txt")).unwrap();
    let preview = Preview::start(&inside).unwrap();
    assert_eq!(text(&get(preview.port(), "/inside.txt")), "inside");
    assert_eq!(text(&get(preview.port(), "/in.txt")), "inside");
    assert_eq!(get(preview.port(), "/out.txt").status, 404);
    // The server wrote nothing in the project.
    let mut names: Vec<_> = std::fs::read_dir(&inside).unwrap().map(|e| e.unwrap().file_name()).collect();
    names.sort();
    assert_eq!(names, ["in.txt", "index.html", "inside.txt", "out.txt"]);
    drop(preview);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn s2_t060_c1_a_project_that_stops_passing_after_the_start_gets_a_notice_and_a_503() {
    let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join("s2_t060_changes");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let good = "<!doctype html><html><body><div data-fs-composition=\"t\" data-fs-size=\"64x36\" data-fs-fps=\"30\" data-fs-duration=\"1\"></div></body></html>";
    std::fs::write(base.join("index.html"), good).unwrap();
    let preview = Preview::start(&base).unwrap();
    assert!(text(&get(preview.port(), "/")).contains("data-state=\"loading\""));
    std::fs::write(base.join("index.html"), good.replace("64x36", "65x36")).unwrap();
    let page = get(preview.port(), "/");
    assert_eq!(page.status, 200);
    assert!(text(&page).contains("framesmith check"), "{}", text(&page));
    assert!(!text(&page).contains("data-state=\"loading\""));
    assert_eq!(get(preview.port(), "/index.html").status, 503);
    drop(preview);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn s2_t060_c5_a_folder_that_does_not_exist_does_not_start() {
    let error = Preview::start(Path::new("/no/such/folder/for/the/preview"))
        .err()
        .expect("it must not start");
    assert!(error.reason.contains("cannot be read"), "{}", error.reason);
}

#[test]
fn s2_t060_c1_dropping_the_preview_frees_the_port() {
    let preview = Preview::start(&project()).unwrap();
    let port = preview.port();
    assert_eq!(get(port, "/").status, 200);
    drop(preview);
    assert!(TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), Duration::from_secs(2)).is_err());
}
