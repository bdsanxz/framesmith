//! Turning an interrupt or a terminate signal into a cancel flag.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

static CANCEL: OnceLock<Arc<AtomicBool>> = OnceLock::new();

extern "C" fn on_signal(_: libc::c_int) {
    // Only an atomic store: nothing here may allocate or lock.
    if let Some(flag) = CANCEL.get() {
        flag.store(true, Ordering::SeqCst);
    }
}

/// Starts watching for SIGINT and SIGTERM. The flag turns true when one
/// arrives; the command then stops in its own time and cleans up.
pub fn watch() -> Arc<AtomicBool> {
    let flag = Arc::clone(CANCEL.get_or_init(|| Arc::new(AtomicBool::new(false))));
    let handler = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
    // SAFETY: the handler only stores to an atomic, which is safe in a signal handler.
    unsafe {
        libc::signal(libc::SIGINT, handler);
        libc::signal(libc::SIGTERM, handler);
    }
    flag
}
