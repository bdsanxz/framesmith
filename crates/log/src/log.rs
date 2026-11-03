//! Minimal run log (supports T009; full per-run coverage belongs to T015).
//!
//! Notable bootstrap events — the pinned FFmpeg download announcement
//! and download/refusal outcomes — are appended to
//! `<folder>/logs/framesmith.log`, one line per event. Each line
//! carries a UTC timestamp, a level, a component, and a run id:
//!
//! ```text
//! ts=2026-11-02T10:00:00Z level=info component=bootstrap run=run-123-1 msg=...
//! ```
//!
//! The log is append-only observability: a failure to write it is
//! reported to the caller and never changes the run verdict.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static RUN_SEQ: AtomicU64 = AtomicU64::new(1);

/// Unique id for this process's runs: `run-<pid>-<seq>`.
pub fn run_id() -> String {
    format!(
        "run-{}-{}",
        std::process::id(),
        RUN_SEQ.fetch_add(1, Ordering::SeqCst)
    )
}

/// Current UTC time as ISO-8601 `YYYY-MM-DDTHH:MM:SSZ`.
pub fn utc_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_timestamp(secs)
}

fn format_timestamp(secs: u64) -> String {
    // Howard Hinnant's civil-from-days algorithm (proleptic Gregorian).
    let days = (secs / 86_400) as i64;
    let time = secs % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    y += if m <= 2 { 1 } else { 0 };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m,
        d,
        time / 3_600,
        (time % 3_600) / 60,
        time % 60
    )
}

/// Append one line to `<folder>/logs/framesmith.log`.
/// Creates the `logs` directory when needed. Never touches the composition.
pub fn append(
    folder: &Path,
    level: &str,
    component: &str,
    run: &str,
    msg: &str,
) -> Result<(), String> {
    let dir = folder.join("logs");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Err(format!("cannot create {}: {e}", dir.display()));
    }
    let line = format!(
        "ts={} level={} component={} run={} msg={}\n",
        utc_now(),
        level,
        component,
        run,
        msg.replace('\n', " ")
    );
    let path = dir.join("framesmith.log");
    use std::io::Write;
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(mut f) => f
            .write_all(line.as_bytes())
            .map_err(|e| format!("cannot write {}: {e}", path.display())),
        Err(e) => Err(format!("cannot open {}: {e}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_shape() {
        assert_eq!(format_timestamp(0), "1970-01-01T00:00:00Z");
        // 2026-11-02T10:01:18Z
        assert_eq!(format_timestamp(1793613678), "2026-11-02T10:01:18Z");
        let now = utc_now();
        assert_eq!(now.len(), 20, "{now}");
        assert!(now.ends_with('Z'));
    }

    #[test]
    fn run_ids_differ() {
        assert_ne!(run_id(), run_id());
    }

    #[test]
    fn append_writes_four_fields() {
        let dir = std::env::temp_dir().join(format!("framesmith-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        append(&dir, "info", "bootstrap", "run-1-1", "hello world").unwrap();
        let text = std::fs::read_to_string(dir.join("logs").join("framesmith.log")).unwrap();
        assert!(text.contains("ts="), "{text}");
        assert!(text.contains("level=info"), "{text}");
        assert!(text.contains("component=bootstrap"), "{text}");
        assert!(text.contains("run=run-1-1"), "{text}");
        assert!(text.contains("msg=hello world"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_failure_is_an_error_not_a_panic() {
        // A regular file where the logs/ directory should be.
        let dir = std::env::temp_dir().join(format!("framesmith-logbad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("logs"), b"blocker").unwrap();
        let err = append(&dir, "info", "bootstrap", "run-x", "msg").unwrap_err();
        assert!(!err.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
