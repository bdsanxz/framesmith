//! Renders that are still running, and the other programs around them.

use super::{browser, BIN};
use measure::Ran;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, Command, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// A `framesmith` command that has been started and not yet waited for.
pub struct Running {
    child: Child,
    stderr: BufReader<ChildStderr>,
    seen: String,
    stdout: JoinHandle<String>,
}

impl Running {
    pub fn start(dir: &Path, args: &[&str], env: &[(&str, &OsStr)]) -> Running {
        let mut child = Command::new(BIN)
            .args(args)
            .current_dir(dir)
            .envs(env.iter().copied())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdout = child.stdout.take().unwrap();
        let stderr = BufReader::new(child.stderr.take().unwrap());
        Running {
            child,
            stderr,
            seen: String::new(),
            stdout: std::thread::spawn(move || {
                let mut text = String::new();
                stdout.read_to_string(&mut text).unwrap();
                text
            }),
        }
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Reads standard error until a line holds `text`.
    pub fn wait_for(&mut self, text: &str) {
        loop {
            let mut line = String::new();
            let read = self.stderr.read_line(&mut line).unwrap();
            assert!(read > 0, "the command ended before it said {text:?}: {}", self.seen);
            self.seen.push_str(&line);
            if line.contains(text) {
                return;
            }
        }
    }

    /// Sends the command a signal by name, for example `INT`.
    pub fn signal(&self, name: &str) {
        signal(self.pid(), name);
    }

    /// Waits for the command to end. A command ended by a signal has the code 128 plus the signal.
    pub fn finish(mut self) -> Ran {
        let mut rest = String::new();
        self.stderr.read_to_string(&mut rest).unwrap();
        let status = self.child.wait().unwrap();
        Ran {
            code: status.code().or_else(|| status.signal().map(|s| 128 + s)).unwrap_or(-1),
            stdout: self.stdout.join().unwrap(),
            stderr: format!("{}{rest}", self.seen),
        }
    }
}

pub fn signal(pid: u32, name: &str) {
    measure::run(
        Path::new("/bin/kill"),
        &[&format!("-{name}"), &pid.to_string()],
        Path::new("/"),
        &[],
    );
}

pub fn alive(pid: u32) -> bool {
    measure::run(Path::new("/bin/kill"), &["-0", &pid.to_string()], Path::new("/"), &[]).code == 0
}

/// The first child of `parent` whose command line holds `pattern`.
pub fn child_of(parent: u32, pattern: &str) -> Option<u32> {
    let found = measure::run(
        Path::new("/usr/bin/pgrep"),
        &["-P", &parent.to_string(), "-f", pattern],
        Path::new("/"),
        &[],
    );
    found.stdout.split_whitespace().next().and_then(|pid| pid.parse().ok())
}

/// Every program `parent` has started, directly or through another one:
/// a browser's helper processes are children of the browser.
pub fn descendants(parent: u32) -> Vec<u32> {
    let found = measure::run(Path::new("/usr/bin/pgrep"), &["-P", &parent.to_string()], Path::new("/"), &[]);
    let children: Vec<u32> = found.stdout.split_whitespace().filter_map(|pid| pid.parse().ok()).collect();
    let mut all = children.clone();
    for child in children {
        all.extend(descendants(child));
    }
    all
}

/// The ones among `pids` that are still running.
pub fn still_alive(pids: &[u32]) -> Vec<u32> {
    pids.iter().copied().filter(|pid| alive(*pid)).collect()
}

/// Every running program whose command line holds `marker`, as `pid command` lines.
pub fn running_with(marker: &str) -> Vec<String> {
    let found = measure::run(Path::new("/usr/bin/pgrep"), &["-fl", marker], Path::new("/"), &[]);
    found.stdout.lines().map(str::to_string).collect()
}

/// A program file that runs the given shell text.
pub fn script(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A browser the user is running on their own: started by the test, not by
/// Framesmith, with its own profile folder and one open page.
pub struct UserBrowser {
    child: Child,
    port: u16,
    pub profile: PathBuf,
}

impl UserBrowser {
    pub fn start(dir: &Path) -> UserBrowser {
        let profile = dir.join("the-users-own-profile");
        std::fs::create_dir_all(&profile).unwrap();
        let page = dir.join("mine.html");
        std::fs::write(&page, "<!doctype html><title>a page of the user</title><p>mine</p>").unwrap();
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let child = Command::new(browser())
            .arg("--headless")
            .arg(format!("--remote-debugging-port={port}"))
            .arg(format!("--user-data-dir={}", profile.display()))
            .args(["--no-first-run", "--disable-gpu"])
            .arg(format!("file://{}", page.display()))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let user = UserBrowser { child, port, profile };
        let began = Instant::now();
        // Started means: it answers, and its page has loaded and shows its title.
        while !user
            .windows()
            .is_some_and(|pages| pages.iter().any(|page| page.ends_with("a page of the user")))
        {
            assert!(began.elapsed() < Duration::from_secs(20), "the user's browser did not start");
            std::thread::sleep(Duration::from_millis(100));
        }
        user
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    fn asked(&self) -> Option<serde_json::Value> {
        let url = format!("http://127.0.0.1:{}/json/list", self.port);
        let ran = measure::run(Path::new("/usr/bin/curl"), &["-s", "--max-time", "2", &url], Path::new("/"), &[]);
        serde_json::from_str(&ran.stdout).ok()
    }

    /// Its open pages, each as `id type url title`, sorted. `None` when it no longer answers.
    /// Only pages are listed: a full browser build also reports workers of its own built-in
    /// extensions, which are not windows or tabs and come and go on their own.
    pub fn windows(&self) -> Option<Vec<String>> {
        let list = self.asked()?;
        let mut pages: Vec<String> = list
            .as_array()?
            .iter()
            .filter(|page| page["type"] == "page")
            .map(|page| {
                let field = |key: &str| page[key].as_str().unwrap_or_default().to_string();
                format!("{} {} {} {}", field("id"), field("type"), field("url"), field("title"))
            })
            .collect();
        pages.sort();
        Some(pages)
    }
}

impl Drop for UserBrowser {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
