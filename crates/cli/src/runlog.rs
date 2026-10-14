//! The run log: one line for every step of a command, under one run id.
//!
//! Behaviour is specified in `docs/spec/run-log.md`. A run starts a [`Log`], runs the command inside
//! [`Log::scope`], and ends with [`Log::finish`]. The log never stops a command: when it cannot be
//! written, the command runs on and `finish` says so in one line.

use std::fmt::{self, Write as _};
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::level_filters::LevelFilter;
use tracing::{Dispatch, Event, Subscriber};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::{FormatTime, SystemTime};
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields, MakeWriter};
use tracing_subscriber::registry::LookupSpan;

/// The environment variable that sets the level (`run-log.md` section 5).
pub const LEVEL_VARIABLE: &str = "FRAMESMITH_LOG";
/// The folder of the log under the folder the command is run from.
pub const LOG_FOLDER: &str = "logs";
/// The name of the log file.
pub const LOG_FILE: &str = "framesmith.log";
/// A log file larger than this many bytes at the start of a run is rotated.
pub const ROTATE_ABOVE: u64 = 5 * 1024 * 1024;
/// How many rotated files are kept.
pub const KEPT: u32 = 3;

/// The one line printed on standard error when the log cannot be written (`run-log.md` section 7).
pub const UNAVAILABLE: &str = "framesmith: the log is unavailable: logs/framesmith.log under the folder you ran from could not be written. The command ran as usual; make that place writable to keep a log.";

/// The log of one run.
pub struct Log {
    dispatch: Dispatch,
    shared: Option<Arc<Shared>>,
    level_unreadable: bool,
}

/// Where lines go, and whether a write to it has failed.
struct Shared {
    writer: Mutex<Box<dyn Write + Send>>,
    failed: AtomicBool,
}

impl Log {
    /// Starts the log of a run: reads the level, makes the run id, rotates and opens the file. When
    /// the file cannot be opened, the log is unavailable and every line is dropped.
    pub fn start() -> Log {
        let (level, level_unreadable) = read_level();
        let mut log = match open() {
            Ok(file) => Log::to_writer(Box::new(file), level),
            Err(_) => Log {
                dispatch: Dispatch::new(tracing::subscriber::NoSubscriber::default()),
                shared: None,
                level_unreadable: false,
            },
        };
        log.level_unreadable = level_unreadable;
        log
    }

    /// A log whose lines go to `writer`, at `level`, under a run id of its own. [`Log::start`] gives
    /// it the log file; a test can give it a writer that fails.
    pub fn to_writer(writer: Box<dyn Write + Send>, level: LevelFilter) -> Log {
        let shared = Arc::new(Shared {
            writer: Mutex::new(writer),
            failed: AtomicBool::new(false),
        });
        let subscriber = tracing_subscriber::fmt::Subscriber::builder()
            .event_format(Line { run_id: new_run_id() })
            .with_writer(Sink(shared.clone()))
            .with_max_level(level)
            .finish();
        Log {
            dispatch: Dispatch::new(subscriber),
            shared: Some(shared),
            level_unreadable: false,
        }
    }

    /// Runs `command` with this log receiving every event of the run.
    pub fn scope<T>(&self, command: impl FnOnce() -> T) -> T {
        tracing::dispatcher::with_default(&self.dispatch, command)
    }

    /// Writes the warning for a level that is not a level, when the variable held one. The value is
    /// not written. Call it inside [`Log::scope`].
    pub fn note_unreadable_level(&self) {
        if self.level_unreadable {
            tracing::warn!("the level in {LEVEL_VARIABLE} is not one of error, warn, info, debug, trace; info is used");
        }
    }

    /// Whether the log could not be written, at the start or later.
    pub fn is_unavailable(&self) -> bool {
        match &self.shared {
            None => true,
            Some(shared) => shared.failed.load(Ordering::SeqCst),
        }
    }

    /// Ends the log of the run: when it could not be written, says so in one line on `err`.
    pub fn finish(self, err: &mut dyn Write) {
        if self.is_unavailable() {
            // A line that cannot be told is not retried: the command's result stands.
            let _ = writeln!(err, "{UNAVAILABLE}");
        }
    }
}

/// The run id: 16 lowercase hexadecimal digits from the system's random numbers, the time and the
/// process. It is made from nothing the command reads.
fn new_run_id() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hash, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut hasher = RandomState::new().build_hasher();
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0)
        .hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// The level from [`LEVEL_VARIABLE`], and whether the variable held something that is not a level.
fn read_level() -> (LevelFilter, bool) {
    match std::env::var_os(LEVEL_VARIABLE) {
        None => (LevelFilter::INFO, false),
        Some(value) => match value.to_str().map(str::trim) {
            Some("") => (LevelFilter::INFO, false),
            Some(text) => match parse_level(text) {
                Some(level) => (level, false),
                None => (LevelFilter::INFO, true),
            },
            None => (LevelFilter::INFO, true),
        },
    }
}

/// A level written as `error`, `warn`, `info`, `debug` or `trace`, in any case.
pub fn parse_level(text: &str) -> Option<LevelFilter> {
    match text.to_ascii_lowercase().as_str() {
        "error" => Some(LevelFilter::ERROR),
        "warn" => Some(LevelFilter::WARN),
        "info" => Some(LevelFilter::INFO),
        "debug" => Some(LevelFilter::DEBUG),
        "trace" => Some(LevelFilter::TRACE),
        _ => None,
    }
}

/// Rotates a log file that is too large, then opens `logs/framesmith.log` for appending, making the
/// folder when it is missing.
fn open() -> io::Result<RollingFileAppender> {
    let folder = Path::new(LOG_FOLDER);
    rotate(folder);
    let name = LOG_FILE.strip_suffix(".log").unwrap_or(LOG_FILE);
    RollingFileAppender::builder()
        .rotation(Rotation::NEVER)
        .filename_prefix(name)
        .filename_suffix("log")
        .build(folder)
        .map_err(|error| io::Error::other(error.to_string()))
}

/// Renames a log file larger than [`ROTATE_ABOVE`] to `.1`, shifting the older ones up and removing
/// the one that would pass [`KEPT`]. A rename that fails does not stop the run.
fn rotate(folder: &Path) {
    let file = folder.join(LOG_FILE);
    match fs::metadata(&file) {
        Ok(metadata) if metadata.len() > ROTATE_ABOVE => {}
        _ => return,
    }
    let numbered = |number: u32| folder.join(format!("{LOG_FILE}.{number}"));
    let _ = fs::remove_file(numbered(KEPT));
    for number in (1..KEPT).rev() {
        let _ = fs::rename(numbered(number), numbered(number + 1));
    }
    let _ = fs::rename(&file, numbered(1));
}

/// Where lines go. A failed write is remembered and every later line is dropped, without a word:
/// the run goes on and `finish` tells once.
#[derive(Clone)]
struct Sink(Arc<Shared>);

struct SinkWriter(Arc<Shared>);

impl<'a> MakeWriter<'a> for Sink {
    type Writer = SinkWriter;

    fn make_writer(&'a self) -> SinkWriter {
        SinkWriter(self.0.clone())
    }
}

impl Write for SinkWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if !self.0.failed.load(Ordering::SeqCst) {
            let written = match self.0.writer.lock() {
                Ok(mut writer) => writer.write_all(buf),
                Err(_) => Err(io::Error::other("the log is locked")),
            };
            if written.is_err() {
                self.0.failed.store(true, Ordering::SeqCst);
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The form of a line (`run-log.md` section 3). It is the only place that makes one.
struct Line {
    run_id: String,
}

impl<S, N> FormatEvent<S, N> for Line
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(&self, _context: &FmtContext<'_, S, N>, mut writer: Writer<'_>, event: &Event<'_>) -> fmt::Result {
        let metadata = event.metadata();
        SystemTime.format_time(&mut writer)?;
        write!(
            writer,
            " {:<5} {} run={} ",
            metadata.level().as_str(),
            metadata.target(),
            self.run_id
        )?;
        let mut fields = Fields::default();
        event.record(&mut fields);
        writer.write_str(&fields.message)?;
        writer.write_str(&fields.rest)?;
        writeln!(writer)
    }
}

/// The message and the fields of an event, each on one line.
#[derive(Default)]
struct Fields {
    message: String,
    rest: String,
}

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.take(field, &format!("{value:?}"));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.take(field, &format!("{value:?}"));
    }
}

impl Fields {
    fn take(&mut self, field: &Field, text: &str) {
        let text = one_line(text);
        if field.name() == "message" {
            self.message = text;
        } else {
            let _ = write!(self.rest, " {}={}", field.name(), text);
        }
    }
}

/// `text` with every control character written as `\u{...}`, so that an event is one line.
fn one_line(text: &str) -> String {
    let mut line = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_control() {
            let _ = write!(line, "\\u{{{:x}}}", character as u32);
        } else {
            line.push(character);
        }
    }
    line
}

/// A word of the command line as the log may hold it: a path under a home folder has the user's
/// name replaced, `~` for the home folder in `home` and `~other` for any other user's
/// (`run-log.md` section 6). `home` is only compared with; it is never written.
pub fn input_word(word: &str, home: Option<&str>) -> String {
    let mut written = String::new();
    let mut rest = word;
    loop {
        let next = ["/Users/", "/home/"]
            .iter()
            .filter_map(|marker| rest.find(marker).map(|at| (at, marker.len())))
            .min();
        let Some((at, marker_len)) = next else {
            written.push_str(rest);
            return written;
        };
        let name_end = rest[at + marker_len..].find('/').map_or(rest.len(), |n| at + marker_len + n);
        written.push_str(&rest[..at]);
        if home.is_some_and(|home| home.trim_end_matches('/') == &rest[at..name_end]) {
            written.push('~');
        } else {
            written.push_str("~other");
        }
        rest = &rest[name_end..];
    }
}

/// The words of the command line as one list for a field: `["check","demo"]`, each word as
/// [`input_word`] allows and quoted.
pub fn word_list<S: AsRef<std::ffi::OsStr>>(words: &[S]) -> String {
    let home = std::env::var("HOME").ok();
    let quoted: Vec<String> = words
        .iter()
        .map(|word| format!("{:?}", input_word(&word.as_ref().to_string_lossy(), home.as_deref())))
        .collect();
    format!("[{}]", quoted.join(","))
}
