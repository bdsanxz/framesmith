//! The render job: everything one run needs, as one value that can be
//! written as JSON and read back. An internal form, not a public format.

use model::Manifest;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobFormat {
    pub container: String,
    pub codec: String,
    pub pixel_format: String,
    pub quality: u32,
    pub preset: String,
}

impl Default for JobFormat {
    fn default() -> JobFormat {
        JobFormat {
            container: "mp4".to_string(),
            codec: "h264".to_string(),
            pixel_format: "yuv420p".to_string(),
            quality: media::QUALITY,
            preset: media::PRESET.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub run_id: String,
    pub project: PathBuf,
    /// The entry page, as a path from the project folder.
    pub entry: String,
    pub output: PathBuf,
    pub manifest: Manifest,
    /// The frame rate of the run: the composition's, or the one asked for.
    pub fps: u32,
    pub frames: u64,
    pub format: JobFormat,
}

impl Job {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Reads a job. A missing field, an unknown field or a value of the
    /// wrong type is rejected, and the message names the field.
    pub fn from_json(text: &str) -> Result<Job, String> {
        serde_json::from_str::<Job>(text).map_err(|error| {
            let message = error.to_string();
            let message = message.split(" at line ").next().unwrap_or(&message);
            // A missing field is noticed at the end of the object it belongs in.
            let place = path_at(text, error.line(), error.column(), message.starts_with("missing field"));
            if place.is_empty() {
                format!("the render job is not valid: {message}")
            } else {
                format!("the render job is not valid at `{place}`: {message}")
            }
        })
    }
}

/// The field a position in a JSON text belongs to, written like
/// `manifest.clips[1].start`. Positions count lines and columns from 1.
/// With `whole_object`, the answer is the object the position is in, not its last field.
fn path_at(text: &str, line: usize, column: usize, whole_object: bool) -> String {
    enum Level {
        Object(Option<String>),
        List(usize),
    }
    let mut levels: Vec<Level> = Vec::new();
    let (mut at_line, mut at_column) = (1usize, 0usize);
    let mut chars = text.chars().peekable();
    // In an object, a string is a key until the colon after it is seen.
    let mut expect_key = false;
    while let Some(c) = chars.next() {
        if c == '\n' {
            at_line += 1;
            at_column = 0;
        } else {
            at_column += 1;
        }
        if (at_line, at_column) > (line, column) {
            break;
        }
        match c {
            '{' => {
                levels.push(Level::Object(None));
                expect_key = true;
            }
            '[' => {
                levels.push(Level::List(0));
                expect_key = false;
            }
            '}' | ']' => {
                // Stay inside the value when the fault is at its very end.
                if (at_line, at_column) == (line, column) {
                    break;
                }
                levels.pop();
                expect_key = false;
            }
            ',' => match levels.last_mut() {
                Some(Level::List(index)) => *index += 1,
                Some(Level::Object(_)) => expect_key = true,
                None => {}
            },
            '"' => {
                let mut string = String::new();
                while let Some(c) = chars.next() {
                    at_column += 1;
                    match c {
                        '\\' => {
                            chars.next();
                            at_column += 1;
                        }
                        '"' => break,
                        c => string.push(c),
                    }
                }
                if expect_key {
                    if let Some(Level::Object(key)) = levels.last_mut() {
                        *key = Some(string);
                    }
                    expect_key = false;
                }
            }
            _ => {}
        }
    }
    if whole_object {
        if let Some(Level::Object(key)) = levels.last_mut() {
            *key = None;
        }
    }
    let mut path = String::new();
    for level in &levels {
        match level {
            Level::Object(Some(key)) => {
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(key);
            }
            Level::Object(None) => {}
            Level::List(index) => path.push_str(&format!("[{index}]")),
        }
    }
    path
}
