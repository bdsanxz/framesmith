//! A browser window opened at the preview's address, driven from a test over the browser's debugging
//! pipe. It is the author's way of using the preview: the browser itself fetches every file from the
//! address over the network. (The `browser` crate's own driver is made for renders: it answers every request
//! itself and lets none reach the network, so it cannot be used here.)

use crate::common::{browser, scratch};
use browser::base64;
use serde_json::{json, Value};
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{FromRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// How long one command to the browser may take.
const COMMAND_TIME: Duration = Duration::from_secs(60);

fn pipe() -> (RawFd, RawFd) {
    let mut ends = [0 as RawFd; 2];
    // SAFETY: `ends` is a valid array of two descriptors for pipe to fill.
    assert_eq!(unsafe { libc::pipe(ends.as_mut_ptr()) }, 0, "pipe failed");
    for end in ends {
        // SAFETY: `end` was opened just above; close-on-exec keeps it out of every child but the one it is copied into.
        unsafe { libc::fcntl(end, libc::F_SETFD, libc::FD_CLOEXEC) };
    }
    (ends[0], ends[1])
}

pub struct Window {
    child: Child,
    input: File,
    messages: Receiver<Vec<u8>>,
    session: String,
    next_id: u64,
    profile: PathBuf,
    closed: bool,
}

impl Window {
    /// Opens `address` in a browser whose window is `width` by `height` pixels. The browser is the one the
    /// tests use (`FRAMESMITH_BROWSER`, else the first one found), with an empty profile of its own.
    pub fn open(test: &str, address: &str, width: u32, height: u32) -> Window {
        let profile = scratch(test).join("browser-profile");
        std::fs::create_dir_all(&profile).unwrap();
        let (child_reads, host_writes) = pipe();
        let (host_reads, child_writes) = pipe();
        let mut command = Command::new(browser());
        command
            .arg("--headless")
            .arg("--remote-debugging-pipe")
            .arg(format!("--user-data-dir={}", profile.display()))
            .arg(format!("--window-size={},{}", width.max(500), height.max(500)))
            .args([
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-background-networking",
                "--disable-component-update",
                "--disable-sync",
                "--disable-extensions",
                "--disable-default-apps",
                "--disable-breakpad",
                "--password-store=basic",
                "--use-mock-keychain",
                "--mute-audio",
                "--hide-scrollbars",
                "--force-device-scale-factor=1",
                "--force-color-profile=srgb",
                "--disable-gpu",
                "--disable-lcd-text",
                "--font-render-hinting=none",
                "about:blank",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            // Its own process group, so that the browser and its helpers are stopped together.
            .process_group(0);
        // SAFETY: the closure runs in the child between fork and exec and calls only fcntl, dup2 and close,
        // which are safe there. It puts the two pipe ends where the browser looks for them: 3 and 4.
        unsafe {
            command.pre_exec(move || {
                let reads = libc::fcntl(child_reads, libc::F_DUPFD, 10);
                let writes = libc::fcntl(child_writes, libc::F_DUPFD, 10);
                if reads < 0 || writes < 0 || libc::dup2(reads, 3) < 0 || libc::dup2(writes, 4) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                libc::close(reads);
                libc::close(writes);
                Ok(())
            });
        }
        let spawned = command.spawn();
        // SAFETY: these two descriptors belong to this function; the child has its own copies.
        unsafe {
            libc::close(child_reads);
            libc::close(child_writes);
        }
        // SAFETY: each descriptor is open, owned here, and handed to exactly one File.
        let (input, mut output) = unsafe { (File::from_raw_fd(host_writes), File::from_raw_fd(host_reads)) };
        let child = spawned.expect("the browser could not be started");
        let (sender, messages) = mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut pending: Vec<u8> = Vec::new();
            let mut buffer = vec![0u8; 1 << 16];
            loop {
                match output.read(&mut buffer) {
                    Ok(0) | Err(_) => return,
                    Ok(count) => {
                        pending.extend_from_slice(&buffer[..count]);
                        while let Some(end) = pending.iter().position(|&byte| byte == 0) {
                            let message: Vec<u8> = pending.drain(..=end).take(end).collect();
                            if sender.send(message).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        });
        let mut window = Window {
            child,
            input,
            messages,
            session: String::new(),
            next_id: 1,
            profile,
            closed: false,
        };
        let target = window.call("Target.createTarget", json!({ "url": "about:blank" }), false);
        let attached = window.call(
            "Target.attachToTarget",
            json!({ "targetId": target["targetId"].as_str().expect("no target"), "flatten": true }),
            false,
        );
        window.session = attached["sessionId"].as_str().expect("no session").to_string();
        window.call("Page.enable", json!({}), true);
        window.call("Runtime.enable", json!({}), true);
        window.resize(width, height);
        window.go(address);
        window
    }

    fn call(&mut self, method: &str, params: Value, in_page: bool) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let mut message = json!({ "id": id, "method": method, "params": params });
        if in_page {
            message["sessionId"] = json!(self.session);
        }
        let mut bytes = message.to_string().into_bytes();
        bytes.push(0);
        self.input
            .write_all(&bytes)
            .and_then(|()| self.input.flush())
            .unwrap_or_else(|e| panic!("the browser closed its connection before {method}: {e}"));
        let until = Instant::now() + COMMAND_TIME;
        loop {
            let bytes = match self.messages.recv_timeout(Duration::from_millis(100)) {
                Ok(bytes) => bytes,
                Err(RecvTimeoutError::Timeout) => {
                    assert!(Instant::now() < until, "the browser did not answer {method}");
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => panic!("the browser stopped during {method}"),
            };
            let Ok(mut message) = serde_json::from_slice::<Value>(&bytes) else {
                continue;
            };
            if message["id"].as_u64() == Some(id) {
                assert!(message.get("error").is_none(), "{method}: {}", message["error"]);
                return message["result"].take();
            }
        }
    }

    pub fn go(&mut self, address: &str) {
        let navigated = self.call("Page.navigate", json!({ "url": address }), true);
        assert!(
            navigated["errorText"].is_null(),
            "the browser could not open {address}: {navigated}"
        );
    }

    /// Makes the window `width` by `height` pixels.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.call(
            "Emulation.setDeviceMetricsOverride",
            json!({ "width": width, "height": height, "deviceScaleFactor": 1, "mobile": false }),
            true,
        );
    }

    pub fn eval(&mut self, expression: &str) -> Value {
        let mut result = self.call("Runtime.evaluate", json!({ "expression": expression, "returnByValue": true }), true);
        assert!(
            result.get("exceptionDetails").is_none(),
            "{expression}: {}",
            result["exceptionDetails"]
        );
        result["result"]["value"].take()
    }

    /// Waits until the page's `data-state` is `state`. Panics with what the page says when it does not come.
    pub fn wait_state(&mut self, state: &str) {
        let until = Instant::now() + Duration::from_secs(30);
        loop {
            let now = self.eval("document.body ? document.body.dataset.state || '' : ''");
            if now == state {
                return;
            }
            if Instant::now() > until {
                let page = self.eval("document.body ? document.body.innerText : '(no body)'");
                panic!("the page did not reach the state {state:?}; it is {now}; it says {page}");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Waits until the page's scale (`data-scale`) is within a thousandth of `scale`.
    pub fn wait_scale(&mut self, scale: f64) {
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            let now = self.eval("Number(document.body.dataset.scale)").as_f64().unwrap_or(f64::NAN);
            if (now - scale).abs() < 0.001 {
                return;
            }
            assert!(Instant::now() < until, "the page's scale is {now}, not {scale}");
            std::thread::sleep(Duration::from_millis(30));
        }
    }

    /// Where the parts of the page lie: the window, the room for the picture, the picture, and the frame.
    pub fn layout(&mut self) -> Value {
        self.eval(
            "(() => { const r = id => { const b = document.getElementById(id).getBoundingClientRect(); \
             return { left: b.left, top: b.top, width: b.width, height: b.height }; }; \
             return { window: { width: innerWidth, height: innerHeight }, state: document.body.dataset.state, \
             frame: document.body.dataset.frame || null, scale: Number(document.body.dataset.scale), \
             stage: r('stage'), picture: r('picture'), composition: r('composition') }; })()",
        )
    }

    /// A picture of the window as it is now, as PNG.
    pub fn screenshot(&mut self) -> Vec<u8> {
        let result = self.call(
            "Page.captureScreenshot",
            json!({ "format": "png", "captureBeyondViewport": false }),
            true,
        );
        let png = base64::decode(result["data"].as_str().expect("no picture")).expect("the picture is not base64");
        assert!(png.starts_with(b"\x89PNG"), "the picture is not a PNG");
        png
    }

    /// Stops the browser and everything it started, and deletes its profile. Safe to call twice.
    pub fn close(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        let pid = self.child.id();
        let mut message = json!({ "id": self.next_id, "method": "Browser.close" }).to_string().into_bytes();
        message.push(0);
        let _ = self.input.write_all(&message);
        let until = Instant::now() + Duration::from_secs(2);
        while self.child.try_wait().ok().flatten().is_none() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(20));
        }
        // SAFETY: plain system call; `pid` is the group this process created for the browser.
        unsafe { libc::killpg(pid as libc::pid_t, libc::SIGKILL) };
        let _ = self.child.wait();
        for _ in 0..20 {
            if std::fs::remove_dir_all(&self.profile).is_ok() || !self.profile.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        self.close();
    }
}
