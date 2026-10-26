//! Reading and judging the text of a project file: `docs/spec/project-file.md`.
//!
//! The text is read as YAML with the outside crate `saphyr`; nothing of it leaves this module. Every
//! key is judged on its own and every problem is returned with its line, in the order of the file.

use notation::{shown, Code, Problem};
use saphyr::{LoadableYamlNode, MarkedYaml, Scalar, YamlData};

use crate::settings::{Settings, Word, Workers, MAX_WORKERS};

/// The keys the file has, as the messages list them.
const KEY_LIST: &str = "name, composition, paths.renders, render.output, render.backend, render.quality, \
                        render.encoder, render.codec, render.workers, render.audio-bitrate";

/// The keys of Sprint 2: known, and not supported in this version.
const NOT_SUPPORTED: [&str; 2] = ["compositions", "timeline"];

/// What a key takes, as the messages say it.
const NAME_TAKES: &str = "text that is not empty and has no `/`";
const PATH_TAKES: &str = "a path, which is text that is not empty";
const WORKERS_TAKES: &str = "auto or a whole number from 1 to 16";
const BITRATE_TAKES: &str = "a whole number of kbit/s from 1 to 4294967295";
const GROUP_TAKES: &str = "a group of keys";

/// Which composition the project file points at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CompositionChoice {
    /// `composition` is not written: `composition.xml`.
    Default,
    /// `composition` is written, and right: its path and the line of the key.
    Named { path: String, line: u32 },
    /// Nothing says which file is meant: the text could not be read, or `composition` is wrong.
    Unknown,
}

/// What reading a project file gave.
#[derive(Debug)]
pub(crate) struct Judged {
    /// The values that were right; a key that was wrong keeps its default.
    pub settings: Settings,
    /// Every problem, in the order of the file.
    pub problems: Vec<Problem>,
    /// The composition to read.
    pub composition: CompositionChoice,
}

/// Reads the bytes of a project file and judges every key.
pub(crate) fn read(source: &[u8]) -> Judged {
    let stopped = |problem: Problem| Judged {
        settings: Settings::default(),
        problems: vec![problem],
        composition: CompositionChoice::Unknown,
    };
    let text = match std::str::from_utf8(source) {
        Ok(text) => text,
        Err(error) => {
            let line = 1 + source[..error.valid_up_to()].iter().filter(|&&byte| byte == b'\n').count();
            return stopped(Problem::new(
                Code::ProjectFileNotUtf8,
                line as u32,
                "the file is not valid UTF-8 text".to_string(),
            ));
        }
    };
    // A byte order mark is allowed at the very start and is no part of the first key.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let documents = match MarkedYaml::load_from_str(text) {
        Ok(documents) => documents,
        Err(error) => {
            return stopped(Problem::new(
                Code::ProjectFileNotYaml,
                (error.marker().line() as u32).max(1),
                format!("the file is not well-formed YAML: {}", shown(error.info())),
            ));
        }
    };

    let mut judge = Judge {
        characters: text.chars().collect(),
        found: Vec::new(),
        settings: Settings::default(),
        composition: CompositionChoice::Default,
    };
    // A document that is empty starts after the last line of the text; the line is the last one.
    let last_line = text.lines().count().max(1) as u32;
    if let Some(second) = documents.get(1) {
        let line = line_of(second).min(last_line);
        return stopped(Problem::new(
            Code::ProjectFileNotAMapping,
            line,
            "the project file must hold one YAML document, and a second one starts here".to_string(),
        ));
    }
    match documents.first() {
        None => {}
        Some(document) => match &document.data {
            YamlData::Mapping(_) => judge.top(document),
            // The document of a file that holds `---` and nothing else.
            YamlData::Value(Scalar::Null) => {}
            other => {
                let what = match other {
                    YamlData::Sequence(_) => "a list",
                    _ => "a single value",
                };
                return stopped(Problem::new(
                    Code::ProjectFileNotAMapping,
                    line_of(document).min(last_line),
                    format!("the project file must be one mapping of keys, such as `name: launch-film`, not {what}"),
                ));
            }
        },
    }
    judge.finish()
}

/// The line a node starts on, counted from 1.
fn line_of(node: &MarkedYaml) -> u32 {
    (node.span.start.line() as u32).max(1)
}

/// The place a node starts at, for putting problems in order: a line, then a column.
fn place_of(node: &MarkedYaml) -> (u32, u32) {
    (line_of(node), node.span.start.col() as u32)
}

/// What a node holds, as far as the judgement of a value needs to tell.
enum Shape<'n> {
    /// No value, or `null` or `~`.
    Nothing,
    /// Text: a word that YAML reads as text, quoted or not.
    Text(&'n str),
    /// A whole number.
    Whole(i64),
    /// Another number.
    Number,
    /// `true` or `false`.
    Truth,
    /// A list.
    List,
    /// A mapping.
    Group,
    /// Anything else the reader can produce.
    Other,
}

fn shape<'n>(node: &'n MarkedYaml<'_>) -> Shape<'n> {
    match &node.data {
        YamlData::Value(Scalar::Null) => Shape::Nothing,
        YamlData::Value(Scalar::String(text)) => Shape::Text(text),
        YamlData::Value(Scalar::Integer(number)) => Shape::Whole(*number),
        YamlData::Value(Scalar::FloatingPoint(_)) => Shape::Number,
        YamlData::Value(Scalar::Boolean(_)) => Shape::Truth,
        YamlData::Sequence(_) => Shape::List,
        YamlData::Mapping(_) => Shape::Group,
        YamlData::Tagged(_, inner) => shape(inner),
        _ => Shape::Other,
    }
}

/// A problem with the place it points at.
struct Found {
    place: (u32, u32),
    problem: Problem,
}

/// The state of one reading: the text, what has been found wrong, and what has been set.
struct Judge {
    /// The text as characters, to take a value as it was written (a position is a character's).
    characters: Vec<char>,
    found: Vec<Found>,
    settings: Settings,
    composition: CompositionChoice,
}

impl Judge {
    fn finish(mut self) -> Judged {
        // Stable: two problems at one place stay in the order they were judged.
        self.found.sort_by_key(|found| found.place);
        Judged {
            settings: self.settings,
            problems: self.found.into_iter().map(|found| found.problem).collect(),
            composition: self.composition,
        }
    }

    fn report(&mut self, code: Code, at: &MarkedYaml, message: String) {
        let place = place_of(at);
        self.found.push(Found {
            place,
            problem: Problem::new(code, place.0, message),
        });
    }

    /// A node as the author wrote it: the text between its first and last character, and nothing
    /// for a node that has no text.
    fn written(&self, node: &MarkedYaml) -> String {
        let start = node.span.start.index();
        let end = node.span.end.index().min(self.characters.len());
        if start >= end {
            return String::new();
        }
        self.characters[start..end].iter().collect::<String>().trim().to_string()
    }

    /// What was found in place of a value, as a message says it.
    fn found_in(&self, node: &MarkedYaml) -> String {
        match shape(node) {
            Shape::List => "a list".to_string(),
            Shape::Group => "a group of keys".to_string(),
            Shape::Other => "a value this program cannot read".to_string(),
            Shape::Nothing | Shape::Text(_) | Shape::Whole(_) | Shape::Number | Shape::Truth => shown(&self.written(node)),
        }
    }

    /// The report of a value that is not what its key takes. `text` is true for a key that takes
    /// text, whose message also says how to write a number as text.
    fn wrong(&mut self, key: &str, takes: &str, node: &MarkedYaml, text: bool) {
        let key = shown(key);
        let message = match shape(node) {
            Shape::Nothing => format!("`{key}` has no value; it must be {takes}"),
            shape => {
                let found = self.found_in(node);
                let reading = match shape {
                    Shape::Whole(_) | Shape::Number => Some("a number"),
                    Shape::Truth => Some("true or false"),
                    _ => None,
                };
                match reading {
                    Some(reading) if text => {
                        format!("`{key}` must be {takes}, not {found}, which YAML reads as {reading}; put it in quotes to use it as text")
                    }
                    _ => format!("`{key}` must be {takes}, not {found}"),
                }
            }
        };
        self.report(Code::ValueNotAccepted, node, message);
    }

    /// The keys at the top of the file.
    fn top(&mut self, document: &MarkedYaml) {
        let YamlData::Mapping(keys) = &document.data else {
            return;
        };
        for (key, value) in keys.iter() {
            let Some(name) = self.key_name(key) else {
                self.not_a_word(key);
                continue;
            };
            match name.as_str() {
                "name" => {
                    if let Some(name) = self.name_value("name", value) {
                        self.settings.name = Some(name);
                    }
                }
                "composition" => self.composition_value(key, value),
                "paths" => self.group("paths", value, Judge::paths_key),
                "render" => self.group("render", value, Judge::render_key),
                _ if NOT_SUPPORTED.contains(&name.as_str()) => self.report(
                    Code::KeyNotSupported,
                    key,
                    notation::Unsupported::SeveralCompositions.message(&format!("the project file has `{}`", shown(&name))),
                ),
                _ => self.unknown(&name, key, true),
            }
        }
    }

    /// The name of a key, when it is a word or a number written as one.
    fn key_name(&self, key: &MarkedYaml) -> Option<String> {
        match shape(key) {
            Shape::Text(text) => Some(text.to_string()),
            Shape::Whole(_) | Shape::Number | Shape::Truth => Some(self.written(key)),
            Shape::Nothing => Some(self.written(key)).filter(|written| !written.is_empty()),
            Shape::List | Shape::Group | Shape::Other => None,
        }
    }

    fn not_a_word(&mut self, key: &MarkedYaml) {
        self.report(
            Code::KeyNotListed,
            key,
            format!("this key is not a word, and the keys of the project file are: {KEY_LIST}"),
        );
    }

    /// A key that the file does not have. `top` is true at the top of the file, where a key
    /// written with a dot may be a key of a group written flat.
    fn unknown(&mut self, name: &str, key: &MarkedYaml, top: bool) {
        let shown_name = shown(name);
        let flat = top
            .then(|| name.split_once('.'))
            .flatten()
            .filter(|_| KEY_LIST.split(", ").any(|listed| listed == name));
        let message = match flat {
            Some((group, sub)) => {
                format!("`{shown_name}` is not a key at the top of the file; write `{group}` as a group and `{sub}` under it")
            }
            None => format!("`{shown_name}` is not a key of the project file; the keys are: {KEY_LIST}"),
        };
        self.report(Code::KeyNotListed, key, message);
    }

    /// A group: its keys are judged by `each`, which returns false for a key the group does not have.
    fn group(&mut self, name: &str, value: &MarkedYaml, each: fn(&mut Judge, &str, &MarkedYaml) -> bool) {
        match &value.data {
            YamlData::Mapping(keys) => {
                for (key, sub_value) in keys.iter() {
                    let Some(sub) = self.key_name(key) else {
                        self.not_a_word(key);
                        continue;
                    };
                    if !each(self, &sub, sub_value) {
                        self.unknown(&format!("{name}.{sub}"), key, false);
                    }
                }
            }
            // A group with no value holds no key.
            YamlData::Value(Scalar::Null) => {}
            _ => self.wrong(name, GROUP_TAKES, value, false),
        }
    }

    /// The keys of the group `paths`; false for a key it does not have.
    fn paths_key(&mut self, sub: &str, value: &MarkedYaml) -> bool {
        match sub {
            "renders" => {
                if let Some(path) = self.path_value("paths.renders", value) {
                    self.settings.renders = Some(path);
                }
                true
            }
            _ => false,
        }
    }

    /// The keys of the group `render`; false for a key it does not have.
    fn render_key(&mut self, sub: &str, value: &MarkedYaml) -> bool {
        match sub {
            "output" => {
                if let Some(path) = self.path_value("render.output", value) {
                    self.settings.output = Some(path);
                }
            }
            "backend" => {
                if let Some(word) = self.word_value("render.backend", value) {
                    self.settings.backend = Some(word);
                    self.note_line("render.backend", value);
                }
            }
            "quality" => {
                if let Some(word) = self.word_value("render.quality", value) {
                    self.settings.quality = Some(word);
                    self.note_line("render.quality", value);
                }
            }
            "encoder" => {
                if let Some(word) = self.word_value("render.encoder", value) {
                    self.settings.encoder = Some(word);
                    self.note_line("render.encoder", value);
                }
            }
            "codec" => {
                if let Some(word) = self.word_value("render.codec", value) {
                    self.settings.codec = Some(word);
                    self.note_line("render.codec", value);
                }
            }
            "workers" => {
                if let Some(workers) = self.workers_value(value) {
                    self.settings.workers = Some(workers);
                    self.note_line("render.workers", value);
                }
            }
            "audio-bitrate" => {
                if let Some(rate) = self.bitrate_value(value) {
                    self.settings.audio_bitrate = Some(rate);
                    self.note_line("render.audio-bitrate", value);
                }
            }
            _ => return false,
        }
        true
    }

    /// Notes the line of the value of a key that was written and was right.
    fn note_line(&mut self, key: &'static str, value: &MarkedYaml) {
        self.settings.lines.push((key, line_of(value)));
    }

    /// `name`: text that is not empty and has no `/`.
    fn name_value(&mut self, key: &str, value: &MarkedYaml) -> Option<String> {
        match shape(value) {
            Shape::Text(text) if !text.trim().is_empty() && !text.contains('/') => Some(text.to_string()),
            _ => {
                self.wrong(key, NAME_TAKES, value, true);
                None
            }
        }
    }

    /// A path: text that is not empty.
    fn path_value(&mut self, key: &str, value: &MarkedYaml) -> Option<String> {
        match shape(value) {
            Shape::Text(text) if !text.trim().is_empty() => Some(text.to_string()),
            _ => {
                self.wrong(key, PATH_TAKES, value, true);
                None
            }
        }
    }

    /// `composition`: a path, and the file to read.
    fn composition_value(&mut self, key: &MarkedYaml, value: &MarkedYaml) {
        match self.path_value("composition", value) {
            Some(path) => {
                self.composition = CompositionChoice::Named {
                    path: path.clone(),
                    line: line_of(key),
                };
                self.settings.composition = Some(path);
            }
            None => self.composition = CompositionChoice::Unknown,
        }
    }

    /// A key that takes one of some words, in quotes or not.
    fn word_value<T: Word>(&mut self, key: &str, value: &MarkedYaml) -> Option<T> {
        if let Shape::Text(text) = shape(value) {
            if let Some(word) = T::ALL.iter().find(|word| word.word() == text) {
                return Some(*word);
            }
        }
        let takes = format!("one of {}", T::ALL.iter().map(|word| word.word()).collect::<Vec<_>>().join(", "));
        self.wrong(key, &takes, value, false);
        None
    }

    /// `render.workers`: `auto`, or a whole number from 1 to 16 written in digits.
    fn workers_value(&mut self, value: &MarkedYaml) -> Option<Workers> {
        match shape(value) {
            Shape::Text("auto") => return Some(Workers::Auto),
            Shape::Whole(number) if self.in_digits(value) && (1..=i64::from(MAX_WORKERS)).contains(&number) => {
                return Some(Workers::Count(number as u8));
            }
            _ => {}
        }
        self.wrong("render.workers", WORKERS_TAKES, value, false);
        None
    }

    /// `render.audio-bitrate`: a whole number of kbit/s written in digits.
    fn bitrate_value(&mut self, value: &MarkedYaml) -> Option<u32> {
        if let Shape::Whole(number) = shape(value) {
            if self.in_digits(value) {
                if let Ok(rate @ 1..) = u32::try_from(number) {
                    return Some(rate);
                }
            }
        }
        self.wrong("render.audio-bitrate", BITRATE_TAKES, value, false);
        None
    }

    /// Whether a whole number was written in decimal digits only: no sign, no `0x`, no exponent.
    fn in_digits(&self, node: &MarkedYaml) -> bool {
        let written = self.written(node);
        !written.is_empty() && written.bytes().all(|byte| byte.is_ascii_digit())
    }
}
