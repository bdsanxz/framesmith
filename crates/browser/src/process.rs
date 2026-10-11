//! The browser process and the conversation with it.

use crate::base64;
use serde_json::{json, Value};
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{FromRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long the browser is given to leave when asked.
const LEAVE_TIME: Duration = Duration::from_secs(2);
/// How much of what the browser wrote to its error stream is kept.
const STDERR_KEPT: usize = 4000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The browser could not be started.
    Start(String),
    /// The browser stopped, or its page crashed.
    Stopped(String),
    /// The browser answered a command with an error.
    Refused(String),
    /// The expression threw.
    Script(String),
    /// The time allowed ran out.
    Timeout,
    /// The run was cancelled while waiting.
    Cancelled,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Start(why) => write!(f, "the browser could not be started: {why}"),
            Error::Stopped(why) => write!(f, "the browser stopped: {why}"),
            Error::Refused(why) => write!(f, "the browser refused a command: {why}"),
            Error::Script(why) => write!(f, "a page script failed: {why}"),
            Error::Timeout => write!(f, "the browser did not answer in time"),
            Error::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// The answer to one request the page makes.
pub enum Served {
    File {
        bytes: Vec<u8>,
        content_type: &'static str,
    },
    NotFound,
    /// Not sent at all: an address outside the project.
    Refused,
}

type Server = Box<dyn FnMut(&str) -> Served + Send>;

pub struct Browser {
    child: Child,
    input: File,
    messages: Receiver<Vec<u8>>,
    stderr: Arc<Mutex<Vec<u8>>>,
    next_id: u64,
    session: Option<String>,
    profile: PathBuf,
    cancel: Arc<AtomicBool>,
    server: Option<Server>,
    refused: Vec<String>,
    crashed: bool,
    closed: bool,
}

fn pipe() -> std::io::Result<(RawFd, RawFd)> {
    let mut ends = [0 as RawFd; 2];
    // SAFETY: `ends` is a valid array of two descriptors for pipe to fill.
    if unsafe { libc::pipe(ends.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    for end in ends {
        // SAFETY: `end` is a descriptor this function just opened. Marking it
        // close-on-exec keeps it out of every child but the one it is copied into.
        unsafe { libc::fcntl(end, libc::F_SETFD, libc::FD_CLOEXEC) };
    }
    Ok((ends[0], ends[1]))
}

impl Browser {
    /// Starts the browser at `path` with a new, empty profile in `profile`
    /// and a window of the given size. Nothing is loaded yet.
    pub fn launch(path: &Path, profile: &Path, width: u32, height: u32, cancel: Arc<AtomicBool>) -> Result<Browser, Error> {
        std::fs::create_dir_all(profile)
            .map_err(|e| Error::Start(format!("the profile folder {} could not be made: {e}", profile.display())))?;
        let (child_reads, host_writes) = pipe().map_err(|e| Error::Start(e.to_string()))?;
        let (host_reads, child_writes) = pipe().map_err(|e| Error::Start(e.to_string()))?;
        let mut command = Command::new(path);
        command
            .arg("--headless")
            .arg("--remote-debugging-pipe")
            .arg(format!("--user-data-dir={}", profile.display()))
            .arg(format!("--window-size={width},{height}"))
            .args([
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-background-networking",
                "--disable-component-update",
                "--disable-sync",
                "--disable-extensions",
                "--disable-default-apps",
                "--disable-breakpad",
                "--disable-features=Translate,MediaRouter,OptimizationHints",
                "--password-store=basic",
                "--use-mock-keychain",
                "--mute-audio",
                "--hide-scrollbars",
                "--force-device-scale-factor=1",
                "--force-color-profile=srgb",
                "--disable-gpu",
                "--disable-lcd-text",
                "--font-render-hinting=none",
                // No name resolves, so nothing can be fetched from a network.
                "--host-resolver-rules=MAP * ~NOTFOUND",
                "about:blank",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            // Its own group: a terminal interrupt reaches Framesmith alone,
            // and the whole group can be stopped together.
            .process_group(0);
        // SAFETY: the closure runs in the child between fork and exec and
        // calls only fcntl, dup2 and close, which are safe there. It puts
        // the two pipe ends where the browser looks for them: 3 and 4.
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
        // SAFETY: these two descriptors belong to this function and are used
        // nowhere else in this process; the child has its own copies.
        unsafe {
            libc::close(child_reads);
            libc::close(child_writes);
        }
        // SAFETY: each descriptor is open, owned here, and handed to exactly one File.
        let (input, mut output) = unsafe { (File::from_raw_fd(host_writes), File::from_raw_fd(host_reads)) };
        let mut child = spawned.map_err(|e| Error::Start(format!("{}: {e}", path.display())))?;
        tracing::info!(target: "browser", program = %path.display(), pid = child.id(), "child process started");

        let (sender, messages) = mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut pending: Vec<u8> = Vec::new();
            let mut buffer = vec![0u8; 1 << 16];
            loop {
                match output.read(&mut buffer) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => {
                        pending.extend_from_slice(&buffer[..n]);
                        while let Some(end) = pending.iter().position(|&b| b == 0) {
                            let message: Vec<u8> = pending.drain(..=end).take(end).collect();
                            if sender.send(message).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        });
        let stderr = Arc::new(Mutex::new(Vec::new()));
        if let Some(mut stream) = child.stderr.take() {
            let kept = Arc::clone(&stderr);
            std::thread::spawn(move || {
                let mut buffer = [0u8; 4096];
                while let Ok(n @ 1..) = stream.read(&mut buffer) {
                    let mut kept = kept.lock().unwrap_or_else(|p| p.into_inner());
                    kept.extend_from_slice(&buffer[..n]);
                    let extra = kept.len().saturating_sub(STDERR_KEPT);
                    kept.drain(..extra);
                }
            });
        }
        Ok(Browser {
            child,
            input,
            messages,
            stderr,
            next_id: 1,
            session: None,
            profile: profile.to_path_buf(),
            cancel,
            server: None,
            refused: Vec::new(),
            crashed: false,
            closed: false,
        })
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// The addresses the page asked for and was refused, in order.
    pub fn refused(&self) -> &[String] {
        &self.refused
    }

    fn last_words(&self) -> String {
        let kept = self.stderr.lock().unwrap_or_else(|p| p.into_inner());
        let text = String::from_utf8_lossy(&kept);
        let line = text
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("it gave no reason");
        line.trim().chars().take(300).collect()
    }

    fn stopped(&mut self) -> Error {
        let status = self.child.try_wait().ok().flatten();
        let how = match status {
            Some(status) => format!("it exited ({status})"),
            None => "it closed its connection".to_string(),
        };
        Error::Stopped(format!("{how}; {}", self.last_words()))
    }

    fn send(&mut self, method: &str, params: Value) -> Result<u64, Error> {
        let id = self.next_id;
        self.next_id += 1;
        let mut message = json!({ "id": id, "method": method, "params": params });
        if let Some(session) = &self.session {
            message["sessionId"] = json!(session);
        }
        let mut bytes = message.to_string().into_bytes();
        bytes.push(0);
        if self.input.write_all(&bytes).and_then(|()| self.input.flush()).is_err() {
            return Err(self.stopped());
        }
        Ok(id)
    }

    /// Answers one request of the page.
    fn serve(&mut self, params: &Value) -> Result<(), Error> {
        let request_id = params["requestId"].as_str().unwrap_or_default().to_string();
        let url = params["request"]["url"].as_str().unwrap_or_default().to_string();
        let served = match self.server.as_mut() {
            Some(server) => server(&url),
            None => Served::Refused,
        };
        match served {
            Served::File { bytes, content_type } => self.send(
                "Fetch.fulfillRequest",
                json!({
                    "requestId": request_id,
                    "responseCode": 200,
                    "responseHeaders": [
                        { "name": "Content-Type", "value": content_type },
                        { "name": "Cache-Control", "value": "no-store" },
                    ],
                    "body": base64::encode(&bytes),
                }),
            )?,
            Served::NotFound => self.send(
                "Fetch.fulfillRequest",
                json!({ "requestId": request_id, "responseCode": 404, "responseHeaders": [], "body": "" }),
            )?,
            Served::Refused => {
                tracing::warn!(target: "browser", "a request to an address outside the project was refused");
                self.refused.push(url);
                self.send(
                    "Fetch.failRequest",
                    json!({ "requestId": request_id, "errorReason": "BlockedByClient" }),
                )?
            }
        };
        Ok(())
    }

    /// Waits for the answer to command `id`, serving the page meanwhile.
    fn wait(&mut self, id: u64, method: &str, deadline: Option<Instant>) -> Result<Value, Error> {
        loop {
            if self.cancel.load(Ordering::SeqCst) {
                return Err(Error::Cancelled);
            }
            if self.crashed {
                return Err(Error::Stopped("the page crashed".to_string()));
            }
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return Err(Error::Timeout);
            }
            let bytes = match self.messages.recv_timeout(Duration::from_millis(25)) {
                Ok(bytes) => bytes,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return Err(self.stopped()),
            };
            let Ok(mut message) = serde_json::from_slice::<Value>(&bytes) else {
                continue;
            };
            if message["id"].as_u64() == Some(id) {
                if let Some(error) = message.get("error") {
                    let why = error["message"].as_str().unwrap_or("no reason given");
                    return Err(Error::Refused(format!("{method}: {why}")));
                }
                return Ok(message["result"].take());
            }
            match message["method"].as_str() {
                Some("Fetch.requestPaused") => {
                    let params = message["params"].take();
                    self.serve(&params)?;
                }
                Some("Inspector.targetCrashed") => self.crashed = true,
                _ => {}
            }
        }
    }

    /// Sends a command and waits for its answer.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, Error> {
        let id = self.send(method, params)?;
        self.wait(id, method, None)
    }

    /// Opens `url` in a new page of the given size. Every request the page
    /// makes is answered by `server`. `before` is a script that runs in the
    /// page before any of its own.
    pub fn open(&mut self, url: &str, width: u32, height: u32, before: &str, server: Server) -> Result<(), Error> {
        self.server = Some(server);
        let target = self.call("Target.createTarget", json!({ "url": "about:blank" }))?;
        let target_id = target["targetId"].as_str().unwrap_or_default().to_string();
        let attached = self.call("Target.attachToTarget", json!({ "targetId": target_id, "flatten": true }))?;
        self.session = attached["sessionId"].as_str().map(str::to_string);
        if self.session.is_none() {
            return Err(Error::Refused("Target.attachToTarget: no session was given".to_string()));
        }
        self.call("Page.enable", json!({}))?;
        self.call("Runtime.enable", json!({}))?;
        self.call("Fetch.enable", json!({ "patterns": [{ "urlPattern": "*" }] }))?;
        self.call(
            "Emulation.setDeviceMetricsOverride",
            json!({ "width": width, "height": height, "deviceScaleFactor": 1, "mobile": false }),
        )?;
        self.call("Emulation.setTimezoneOverride", json!({ "timezoneId": "UTC" }))?;
        self.call("Page.addScriptToEvaluateOnNewDocument", json!({ "source": before }))?;
        let navigated = self.call("Page.navigate", json!({ "url": url }))?;
        if let Some(why) = navigated["errorText"].as_str() {
            return Err(Error::Refused(format!("Page.navigate: {why}")));
        }
        Ok(())
    }

    /// Runs an expression in the page and returns its value. With `wait`,
    /// a promise is awaited for at most that long.
    pub fn evaluate(&mut self, expression: &str, wait: Option<Duration>) -> Result<Value, Error> {
        let id = self.send(
            "Runtime.evaluate",
            json!({ "expression": expression, "awaitPromise": wait.is_some(), "returnByValue": true }),
        )?;
        let mut result = self.wait(id, "Runtime.evaluate", wait.map(|wait| Instant::now() + wait))?;
        if let Some(details) = result.get("exceptionDetails") {
            let why = details["exception"]["description"]
                .as_str()
                .or_else(|| details["text"].as_str())
                .unwrap_or("an exception was thrown");
            return Err(Error::Script(why.lines().next().unwrap_or(why).to_string()));
        }
        Ok(result["result"]["value"].take())
    }

    /// Takes a picture of the page as it is now, as PNG.
    pub fn screenshot(&mut self) -> Result<Vec<u8>, Error> {
        let result = self.call(
            "Page.captureScreenshot",
            json!({ "format": "png", "optimizeForSpeed": true, "captureBeyondViewport": false }),
        )?;
        result["data"]
            .as_str()
            .and_then(base64::decode)
            .filter(|png| png.starts_with(b"\x89PNG"))
            .ok_or_else(|| Error::Refused("Page.captureScreenshot: no picture was given".to_string()))
    }

    /// Stops the browser: asks it to leave, waits two seconds, then stops it
    /// and everything it started, and deletes its profile. Safe to call twice.
    pub fn close(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        self.session = None;
        let pid = self.child.id();
        let asked = self.child.try_wait().ok().flatten().is_none() && self.send("Browser.close", json!({})).is_ok();
        let give_up = Instant::now() + if asked { LEAVE_TIME } else { Duration::ZERO };
        let mut status = self.child.try_wait().ok().flatten();
        while status.is_none() && Instant::now() < give_up {
            std::thread::sleep(Duration::from_millis(20));
            status = self.child.try_wait().ok().flatten();
        }
        let forced = status.is_none();
        // The group is the browser's own (see `launch`), so this reaches the
        // browser and the helpers it started, and nothing else.
        // SAFETY: plain system call; `pid` is the group this process created.
        unsafe { libc::killpg(pid as libc::pid_t, libc::SIGKILL) };
        let status = self.child.wait().ok();
        let status = status.map_or_else(|| "unknown".to_string(), |s| s.to_string());
        tracing::info!(target: "browser", pid, status = %status, forced, "child process exited");
        for _ in 0..20 {
            if std::fs::remove_dir_all(&self.profile).is_ok() || !self.profile.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        self.close();
    }
}
