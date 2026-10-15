//! The options of `render` that replace a setting of the project file for one run:
//! `docs/spec/render-command.md` sections 1 and 1.1.

use crate::settings::{Backend, Codec, Encoder, Quality, Word, Workers, MAX_WORKERS};

/// Every option, in the order the messages list them.
pub const OPTION_NAMES: [&str; 6] = ["--output", "--backend", "--quality", "--encoder", "--codec", "--workers"];

/// What `--workers` takes, as the messages say it.
const WORKERS_TAKES: &str = "auto or a whole number from 1 to 16";

/// The options given on the command line; an option not given is `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub(crate) output: Option<String>,
    pub(crate) backend: Option<Backend>,
    pub(crate) quality: Option<Quality>,
    pub(crate) encoder: Option<Encoder>,
    pub(crate) codec: Option<Codec>,
    pub(crate) workers: Option<Workers>,
}

impl Options {
    /// Whether `name` (`--quality` and the like) is an option.
    pub fn is_option(name: &str) -> bool {
        OPTION_NAMES.contains(&name)
    }

    /// The sentence for a word that begins with `-` and is not an option.
    pub fn unknown(name: &str) -> String {
        format!(
            "`{}` is not an option of `render`; the options are: {}",
            notation::shown(name),
            OPTION_NAMES.join(", ")
        )
    }

    /// The sentence for an option written with no value after it.
    pub fn no_value(name: &str) -> String {
        format!("`{name}` needs a value: {}", takes(name))
    }

    /// Takes the value of the option `name`, or gives the sentence that says why it is not taken:
    /// a value outside the values of the option, or an option given twice.
    pub fn set(&mut self, name: &str, value: &str) -> Result<(), String> {
        let outside = || format!("`{name}` must be {}, not {}", takes(name), notation::shown(value));
        let twice = || format!("`{name}` is given twice; give it once");
        match name {
            "--output" => {
                if value.trim().is_empty() {
                    return Err("`--output` must be a path that is not empty".to_string());
                }
                put(&mut self.output, value.to_string(), twice)
            }
            "--backend" => put(&mut self.backend, Backend::from_word(value).ok_or_else(outside)?, twice),
            "--quality" => put(&mut self.quality, Quality::from_word(value).ok_or_else(outside)?, twice),
            "--encoder" => put(&mut self.encoder, Encoder::from_word(value).ok_or_else(outside)?, twice),
            "--codec" => put(&mut self.codec, Codec::from_word(value).ok_or_else(outside)?, twice),
            "--workers" => put(&mut self.workers, workers(value).ok_or_else(outside)?, twice),
            _ => Err(Options::unknown(name)),
        }
    }

    /// How many options were given.
    pub fn count(&self) -> usize {
        [
            self.output.is_some(),
            self.backend.is_some(),
            self.quality.is_some(),
            self.encoder.is_some(),
            self.codec.is_some(),
            self.workers.is_some(),
        ]
        .iter()
        .filter(|given| **given)
        .count()
    }
}

fn put<T>(slot: &mut Option<T>, value: T, twice: impl Fn() -> String) -> Result<(), String> {
    if slot.is_some() {
        return Err(twice());
    }
    *slot = Some(value);
    Ok(())
}

/// `auto`, or a whole number from 1 to 16 written in decimal digits only.
fn workers(value: &str) -> Option<Workers> {
    if value == "auto" {
        return Some(Workers::Auto);
    }
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    match value.parse::<u32>() {
        Ok(count @ 1..) if count <= u32::from(MAX_WORKERS) => Some(Workers::Count(count as u8)),
        _ => None,
    }
}

/// What the option takes, as the messages say it.
fn takes(name: &str) -> String {
    fn words<T: Word>() -> String {
        format!("one of {}", T::ALL.iter().map(|word| word.word()).collect::<Vec<_>>().join(", "))
    }
    match name {
        "--output" => "a path that is not empty".to_string(),
        "--backend" => words::<Backend>(),
        "--quality" => words::<Quality>(),
        "--encoder" => words::<Encoder>(),
        "--codec" => words::<Codec>(),
        _ => WORKERS_TAKES.to_string(),
    }
}
