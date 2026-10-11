//! Drives a real browser. A Chromium-based browser must be installed, or
//! named by FRAMESMITH_BROWSER.

use browser::{find, Browser, Error, Places, Served};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

const PAGE: &str = "<!doctype html><html><body style='margin:0;background:rgb(200,30,30)'>\
<img id='ok' src='/dot.svg'><img id='net' src='https://example.com/a.png'><script src='/app.js'></script></body></html>";

fn start(name: &str) -> (Browser, PathBuf) {
    let path = find(&Places::from_process()).expect("a Chromium-based browser is needed for this test");
    let profile = Path::new(env!("CARGO_TARGET_TMPDIR")).join("browser-page").join(name);
    let _ = std::fs::remove_dir_all(&profile);
    let browser = Browser::launch(&path, &profile, 320, 180, Arc::new(AtomicBool::new(false))).unwrap();
    (browser, profile)
}

fn serve(url: &str) -> Served {
    match url.strip_prefix("http://framesmith.localhost") {
        Some("/index.html") => Served::File {
            bytes: PAGE.as_bytes().to_vec(),
            content_type: "text/html; charset=utf-8",
        },
        Some("/app.js") => Served::File {
            bytes: b"window.fromScript = boot + 1;".to_vec(),
            content_type: "text/javascript",
        },
        Some("/dot.svg") => Served::File {
            bytes: b"<svg xmlns='http://www.w3.org/2000/svg' width='4' height='4'><rect width='4' height='4'/></svg>".to_vec(),
            content_type: "image/svg+xml",
        },
        Some(_) => Served::NotFound,
        None => Served::Refused,
    }
}

fn alive(pid: u32) -> bool {
    // SAFETY: signal 0 only asks whether the process exists.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[test]
fn t010_a_page_is_served_run_and_pictured_at_the_declared_size() {
    let (mut browser, profile) = start("served");
    let pid = browser.pid();
    browser
        .open(
            "http://framesmith.localhost/index.html",
            320,
            180,
            "window.boot = 41;",
            Box::new(serve),
        )
        .unwrap();
    let loaded = "new Promise(done => document.readyState === 'complete' ? done(1) : addEventListener('load', () => done(1)))";
    assert_eq!(browser.evaluate(loaded, Some(Duration::from_secs(20))).unwrap(), 1);
    // The script given beforehand ran before the page's own script.
    assert_eq!(browser.evaluate("window.fromScript", None).unwrap(), 42);
    assert_eq!(
        browser
            .evaluate("[innerWidth, innerHeight, devicePixelRatio].join()", None)
            .unwrap(),
        "320,180,1"
    );
    assert_eq!(browser.evaluate("document.getElementById('ok').naturalWidth", None).unwrap(), 4);
    assert_eq!(browser.evaluate("new Date(0).getTimezoneOffset()", None).unwrap(), 0);

    // t030_c3: the network address was refused, not fetched.
    assert_eq!(browser.evaluate("document.getElementById('net').naturalWidth", None).unwrap(), 0);
    assert_eq!(browser.refused(), ["https://example.com/a.png"]);

    let png = browser.screenshot().unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let size = |at: usize| u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]);
    assert_eq!((size(16), size(20)), (320, 180));

    assert!(matches!(browser.evaluate("nothing.here", None), Err(Error::Script(why)) if why.contains("nothing")));
    assert_eq!(
        browser.evaluate("new Promise(() => {})", Some(Duration::from_millis(300))),
        Err(Error::Timeout)
    );

    // t039: closing leaves no process and no profile behind.
    assert!(alive(pid));
    browser.close();
    assert!(!alive(pid));
    assert!(!profile.exists());
}

#[test]
fn t038_a_browser_that_dies_is_reported_with_its_exit() {
    let (mut browser, _) = start("dies");
    browser
        .open("http://framesmith.localhost/index.html", 320, 180, "", Box::new(serve))
        .unwrap();
    // SAFETY: the process is the browser this test started.
    unsafe { libc::kill(browser.pid() as libc::pid_t, libc::SIGKILL) };
    let outcome = (0..200).find_map(|_| browser.evaluate("1", None).err());
    assert!(matches!(outcome, Some(Error::Stopped(_))), "{outcome:?}");
}

#[test]
fn a_program_that_is_not_a_browser_is_reported() {
    let profile = Path::new(env!("CARGO_TARGET_TMPDIR")).join("browser-page").join("not-a-browser");
    let mut browser = Browser::launch(Path::new("/usr/bin/true"), &profile, 320, 180, Arc::new(AtomicBool::new(false))).unwrap();
    let outcome = browser.call("Browser.getVersion", serde_json::json!({}));
    assert!(matches!(outcome, Err(Error::Stopped(_))), "{outcome:?}");
    assert!(matches!(
        Browser::launch(
            Path::new("/nonexistent/browser"),
            &profile,
            320,
            180,
            Arc::new(AtomicBool::new(false))
        ),
        Err(Error::Start(_))
    ));
}
