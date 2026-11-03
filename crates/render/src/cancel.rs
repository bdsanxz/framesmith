//! Cooperative Ctrl-C cancellation (Sprint 3 T005).
//!
//! One SIGINT during a render sets a process-wide flag; render stages
//! poll it at stage boundaries and between frame batches, then stop
//! workers, terminate the FFmpeg child, remove staged and partial
//! files, and exit 130. The handler only stores an atomic flag, so it
//! is async-signal-safe. Installed only on the render path, never on
//! `check` or `errors`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Set once the first SIGINT arrives; render stages poll it.
static CANCELLED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_sigint(_: libc::c_int) {
    CANCELLED.store(true, Ordering::SeqCst);
}

/// Install the SIGINT handler for one render run. Later renders in the
/// same process reuse the same handler; the flag is cleared here so a
/// previous test cancellation never leaks into the next render.
pub fn install() {
    CANCELLED.store(false, Ordering::SeqCst);
    // SAFETY: `signal` installs a handler that only performs an atomic
    // store, which is async-signal-safe.
    unsafe {
        libc::signal(libc::SIGINT, on_sigint as *const () as libc::sighandler_t);
    }
}

/// True once SIGINT has arrived since the last [`install`].
pub fn is_cancelled() -> bool {
    CANCELLED.load(Ordering::SeqCst)
}

/// Process exit code for a cancelled render (Ctrl-C).
pub const EXIT_CODE: i32 = 130;

/// Sentinel prefix for cancellation errors travelling up the render
/// stack; the CLI maps it to [`EXIT_CODE`] with cleanup.
pub const PREFIX: &str = "cancelled:";

/// Cancellation error naming the interrupted stage.
pub fn error(stage: &str) -> String {
    format!("{PREFIX} render {stage} interrupted by Ctrl-C (SIGINT); no MP4 was written")
}

/// True when an error is a cancellation travelling up the stack.
pub fn is_cancel_error(e: &str) -> bool {
    e.starts_with(PREFIX)
}

/// Request cancellation without a real signal (tests only): sets the
/// same flag the SIGINT handler sets.
pub fn request() {
    CANCELLED.store(true, Ordering::SeqCst);
}

/// Clear a test-requested cancellation.
pub fn clear() {
    CANCELLED.store(false, Ordering::SeqCst);
}

/// Remove staged and partial work files, returning every path that
/// could not be removed so the caller can name the leftovers while
/// still exiting with [`EXIT_CODE`].
pub fn remove_staged(out: &Path, extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut tmp_name = out.as_os_str().to_owned();
    tmp_name.push(".framesmith-part");
    let mut targets = vec![PathBuf::from(tmp_name)];
    targets.extend(extra.iter().cloned());
    let mut leftovers = Vec::new();
    for t in targets {
        match std::fs::remove_file(&t) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => leftovers.push(t),
        }
    }
    leftovers
}
