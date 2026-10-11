//! The log file: one line per event, each with a UTC timestamp, the level,
//! the component, and the run id. The file is internal; see
//! `docs/spec/commands.md`.

use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Metadata, Subscriber};

pub const FILE_NAME: &str = "framesmith.log";
/// The size past which the file is set aside and a new one started.
pub const SIZE_LIMIT: u64 = 5 * 1024 * 1024;
pub const DIR_VAR: &str = "FRAMESMITH_LOG_DIR";
pub const LEVEL_VAR: &str = "FRAMESMITH_LOG";

/// A new run id: twelve hexadecimal digits from the time and the process.
pub fn new_run_id() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos()) as u64;
    let mut mixed = nanos ^ (u64::from(std::process::id()) << 32);
    // A fixed scramble, so ids that start close together do not look alike.
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    mixed ^= mixed >> 31;
    format!("{:012x}", mixed & 0xffff_ffff_ffff)
}

/// The level a written setting names; `info` for anything else.
pub fn level_from(text: Option<&str>) -> Level {
    match text.map(|text| text.trim().to_ascii_lowercase()).as_deref() {
        Some("error") => Level::ERROR,
        Some("warn" | "warning") => Level::WARN,
        Some("debug") => Level::DEBUG,
        Some("trace") => Level::TRACE,
        _ => Level::INFO,
    }
}

/// The folder the log goes to: the named one, else the user's log folder.
pub fn log_dir(named: Option<PathBuf>, home: Option<PathBuf>) -> Option<PathBuf> {
    named.or_else(|| {
        let home = home?;
        Some(if cfg!(target_os = "macos") {
            home.join("Library/Logs/framesmith")
        } else {
            home.join(".local/state/framesmith")
        })
    })
}

/// A time as `2026-10-11T08:15:30.123Z`.
pub fn timestamp(time: SystemTime) -> String {
    let since = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = since.as_secs() as i64;
    let (days, of_day) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days since 1970-01-01 to a calendar date, counting from 1 March.
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        of_day / 3600,
        of_day % 3600 / 60,
        of_day % 60,
        since.subsec_millis()
    )
}

/// One line of the log, without its line break.
pub fn line(time: SystemTime, level: &Level, component: &str, run_id: &str, message: &str) -> String {
    let message = message.replace('\n', "\\n").replace('\r', "\\r");
    format!("{} {level:<5} {component} run={run_id} {message}", timestamp(time))
}

/// Writes lines to the log file and sets the file aside when it grows
/// past the limit.
pub struct Writer {
    path: PathBuf,
    file: File,
    size: u64,
    limit: u64,
}

impl Writer {
    pub fn open(dir: &Path, limit: u64) -> Result<Writer, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{} cannot be made ({e})", dir.display()))?;
        let path = dir.join(FILE_NAME);
        let file = Self::open_file(&path)?;
        let size = file.metadata().map_or(0, |m| m.len());
        Ok(Writer { path, file, size, limit })
    }

    fn open_file(path: &Path) -> Result<File, String> {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| format!("{} cannot be opened ({e})", path.display()))
    }

    pub fn write_line(&mut self, line: &str) -> Result<(), String> {
        if self.size > self.limit {
            let mut aside = self.path.clone().into_os_string();
            aside.push(".1");
            std::fs::rename(&self.path, &aside).map_err(|e| format!("{} cannot be set aside ({e})", self.path.display()))?;
            self.file = Self::open_file(&self.path)?;
            self.size = 0;
        }
        let text = format!("{line}\n");
        self.file
            .write_all(text.as_bytes())
            .map_err(|e| format!("{} cannot be written ({e})", self.path.display()))?;
        self.size += text.len() as u64;
        Ok(())
    }
}

#[derive(Default)]
struct Words {
    message: String,
    fields: String,
}

impl Visit for Words {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.message, "{value:?}");
            return;
        }
        let text = format!("{value:?}");
        let quoted = text.chars().any(char::is_whitespace) && !text.starts_with('"');
        let _ = if quoted {
            write!(self.fields, " {}=\"{text}\"", field.name())
        } else {
            write!(self.fields, " {}={text}", field.name())
        };
    }
}

fn unavailable(reason: &str) {
    eprintln!("logging is unavailable: {reason}");
}

struct FileLog {
    level: Level,
    run_id: String,
    /// `None` once writing has failed and the user has been told.
    writer: Mutex<Option<Writer>>,
}

impl Subscriber for FileLog {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        *metadata.level() <= self.level
    }

    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }

    fn record(&self, _: &Id, _: &Record<'_>) {}

    fn record_follows_from(&self, _: &Id, _: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut words = Words::default();
        event.record(&mut words);
        let metadata = event.metadata();
        let message = format!("{}{}", words.message, words.fields);
        let line = line(SystemTime::now(), metadata.level(), metadata.target(), &self.run_id, &message);
        let Ok(mut writer) = self.writer.lock() else {
            return;
        };
        let failed = writer.as_mut().and_then(|writer| writer.write_line(&line).err());
        if let Some(reason) = failed {
            *writer = None;
            unavailable(&reason);
        }
    }

    fn enter(&self, _: &Id) {}

    fn exit(&self, _: &Id) {}
}

/// Starts logging for this run. When the log cannot be written the
/// command says so once and carries on without it.
pub fn install(run_id: &str) {
    let var = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    let Some(dir) = log_dir(var(DIR_VAR).map(PathBuf::from), var("HOME").map(PathBuf::from)) else {
        unavailable("no log folder is known; set FRAMESMITH_LOG_DIR");
        return;
    };
    let writer = match Writer::open(&dir, SIZE_LIMIT) {
        Ok(writer) => writer,
        Err(reason) => {
            unavailable(&reason);
            return;
        }
    };
    let log = FileLog {
        level: level_from(var(LEVEL_VAR).map(|v| v.to_string_lossy().into_owned()).as_deref()),
        run_id: run_id.to_string(),
        writer: Mutex::new(Some(writer)),
    };
    // A second install in one process is refused by tracing; the first one stays.
    let _ = tracing::subscriber::set_global_default(log);
}
