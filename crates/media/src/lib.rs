//! Encoding with the user's own `ffmpeg`, run as a separate program.
//! Framesmith never links it and never ships it.

use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long `ffmpeg` is given to leave when asked.
const LEAVE_TIME: Duration = Duration::from_secs(2);
const STDERR_KEPT: usize = 4000;

/// The fixed format of the output: H.264 in MP4, 8-bit 4:2:0, one quality.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Format {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// The encoder's constant-quality setting.
    pub quality: u32,
    /// The encoder's speed setting.
    pub preset: String,
}

/// The one quality setting of this version.
pub const QUALITY: u32 = 18;
pub const PRESET: &str = "medium";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// `ffmpeg` could not be started.
    Start(String),
    /// `ffmpeg` stopped or reported an error. `words` is the last thing it said.
    Stopped { status: String, words: String },
    /// The run was cancelled while waiting.
    Cancelled,
}

/// The first `ffmpeg` in the given folders (the search path).
pub fn find_ffmpeg(path: &[PathBuf]) -> Option<PathBuf> {
    path.iter()
        .map(|folder| folder.join("ffmpeg"))
        .find(|candidate| std::fs::metadata(candidate).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
}

/// The arguments `ffmpeg` is run with: PNG pictures in on standard input,
/// an MP4 file out.
pub fn arguments(format: &Format, output: &Path) -> Vec<String> {
    let fps = format.fps.to_string();
    let mut args: Vec<String> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "image2pipe",
        "-framerate",
        &fps,
        "-c:v",
        "png",
        "-i",
        "pipe:0",
        "-an",
        // Full-range RGB in, limited-range BT.709 out, and say so in the file.
        "-vf",
        "scale=in_range=pc:out_range=tv:out_color_matrix=bt709,format=yuv420p,\
setparams=range=tv:colorspace=bt709:color_primaries=bt709:color_trc=bt709",
        "-c:v",
        "libx264",
        "-preset",
        &format.preset,
        "-crf",
        &format.quality.to_string(),
        "-pix_fmt",
        "yuv420p",
        // The index goes first, so playback can begin before the file is all read.
        "-movflags",
        "+faststart",
        "-f",
        "mp4",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.push(output.display().to_string());
    args
}

/// The reason in what `ffmpeg` printed on its error stream: its last line.
/// When it names an encoder it does not have, that line comes first, since
/// the last line then only says that an encoder was not found.
pub fn last_words(printed: &str) -> String {
    let lines: Vec<&str> = printed.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    let Some(last) = lines.last() else {
        return "it gave no reason".to_string();
    };
    let words = match lines.iter().find_map(|line| line.find("Unknown encoder").map(|at| &line[at..])) {
        Some(named) if !last.contains(named) => format!("{named}; {last}"),
        _ => last.to_string(),
    };
    words.chars().take(300).collect()
}

pub struct Encoder {
    child: Child,
    input: Option<ChildStdin>,
    stderr: Arc<Mutex<Vec<u8>>>,
    done: bool,
}

impl Encoder {
    /// Starts `ffmpeg` writing to `output`.
    pub fn start(ffmpeg: &Path, format: &Format, output: &Path) -> Result<Encoder, Error> {
        let mut child = Command::new(ffmpeg)
            .args(arguments(format, output))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            // Its own group, so a terminal interrupt reaches Framesmith alone.
            .process_group(0)
            .spawn()
            .map_err(|e| Error::Start(format!("{}: {e}", ffmpeg.display())))?;
        tracing::info!(target: "media", program = %ffmpeg.display(), pid = child.id(), "child process started");
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
        let input = child.stdin.take();
        Ok(Encoder {
            child,
            input,
            stderr,
            done: false,
        })
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    fn words(&self) -> String {
        // The error stream is read on another thread; give it a moment to catch up.
        std::thread::sleep(Duration::from_millis(50));
        let kept = self.stderr.lock().unwrap_or_else(|p| p.into_inner());
        last_words(&String::from_utf8_lossy(&kept))
    }

    fn stopped(&mut self) -> Error {
        self.input = None;
        let status = self.child.wait().map_or_else(|_| "unknown".to_string(), |s| s.to_string());
        self.done = true;
        tracing::info!(target: "media", pid = self.child.id(), status = %status, forced = false, "child process exited");
        Error::Stopped {
            status,
            words: self.words(),
        }
    }

    /// Hands one picture to `ffmpeg`.
    pub fn frame(&mut self, png: &[u8]) -> Result<(), Error> {
        // An ffmpeg that has already ended is noticed now, not when its pipe fills.
        let ended = matches!(self.child.try_wait(), Ok(Some(_)));
        let written = match self.input.as_mut() {
            Some(input) if !ended => input.write_all(png).is_ok(),
            _ => false,
        };
        if written {
            Ok(())
        } else {
            Err(self.stopped())
        }
    }

    /// Says there are no more pictures and waits for the file to be finished.
    pub fn finish(&mut self, cancel: &AtomicBool) -> Result<(), Error> {
        self.input = None;
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err(Error::Cancelled);
            }
            match self.child.try_wait() {
                Ok(Some(status)) => {
                    self.done = true;
                    tracing::info!(target: "media", pid = self.child.id(), status = %status, forced = false, "child process exited");
                    return if status.success() {
                        Ok(())
                    } else {
                        Err(Error::Stopped {
                            status: status.to_string(),
                            words: self.words(),
                        })
                    };
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(15)),
                Err(e) => {
                    return Err(Error::Stopped {
                        status: "unknown".to_string(),
                        words: e.to_string(),
                    })
                }
            }
        }
    }

    /// Stops `ffmpeg` early: asks it to leave, waits two seconds, then stops
    /// it and anything it started. Safe to call twice.
    pub fn stop(&mut self) {
        if self.done {
            return;
        }
        self.done = true;
        self.input = None;
        let pid = self.child.id();
        // SAFETY: plain system calls on the group this process created for ffmpeg.
        unsafe { libc::killpg(pid as libc::pid_t, libc::SIGTERM) };
        let give_up = Instant::now() + LEAVE_TIME;
        let mut status = self.child.try_wait().ok().flatten();
        while status.is_none() && Instant::now() < give_up {
            std::thread::sleep(Duration::from_millis(20));
            status = self.child.try_wait().ok().flatten();
        }
        let forced = status.is_none();
        // SAFETY: as above.
        unsafe { libc::killpg(pid as libc::pid_t, libc::SIGKILL) };
        let status = self.child.wait().map_or_else(|_| "unknown".to_string(), |s| s.to_string());
        tracing::info!(target: "media", pid, status = %status, forced, "child process exited");
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        self.stop();
    }
}
