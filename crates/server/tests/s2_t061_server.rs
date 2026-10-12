//! The preview server on its own: waiting until it is asked to stop, and what is left once it is dropped.
//! Specified in `docs/spec/preview.md`, "Stopping".

use server::Preview;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/preview/frame-zero")
}

fn connects(port: u16) -> bool {
    TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), Duration::from_secs(2)).is_ok()
}

#[test]
fn s2_t061_c2_a_preview_waits_until_it_is_asked_to_stop_and_then_the_address_is_refused_and_the_port_is_free() {
    let preview = Preview::start(&project()).unwrap();
    let port = preview.port();
    let flag = Arc::new(AtomicBool::new(false));
    let waiting = {
        let flag = Arc::clone(&flag);
        std::thread::spawn(move || {
            preview.wait_until(&flag);
            let began = Instant::now();
            drop(preview);
            began.elapsed()
        })
    };
    // Control: asked for nothing, it keeps waiting and keeps answering.
    std::thread::sleep(Duration::from_millis(400));
    assert!(!waiting.is_finished());
    assert!(connects(port));
    flag.store(true, Ordering::SeqCst);
    let stopping = waiting.join().unwrap();
    assert!(stopping < Duration::from_secs(2), "stopping took {stopping:?}");
    assert!(!connects(port));
    TcpListener::bind((Ipv4Addr::LOCALHOST, port)).expect("the port is free at once");
}

#[test]
fn s2_t061_c2_a_stop_asked_before_the_wait_returns_at_once() {
    let preview = Preview::start(&project()).unwrap();
    let flag = AtomicBool::new(true);
    let began = Instant::now();
    preview.wait_until(&flag);
    assert!(began.elapsed() < Duration::from_secs(1));
}
