//! The sound files of a film, opened through frx7 before the first frame (`docs/spec/sound.md` section 5): each
//! `<audio>` the scene places is read by the system's sound file reader, from its `media-start`, or reported on the
//! line of its element with the file as `src` writes it and the reason.

use base::Error;
use codec::{AudioSource, SoundKind};
use project::{Code, Problem, Report};
use timeline::Sound;

fn codecs() -> codec::Registry {
    let mut codecs = codec::Registry::new();
    codecs.register(backend_os::BACKEND);
    codecs
}

/// The kinds of sound file read, as a message names them: "wav, aiff, mp3 and aac-in-mp4".
fn kinds_read() -> String {
    let names: Vec<&str> = SoundKind::READ.iter().map(|kind| kind.name()).collect();
    match names.split_last() {
        Some((last, [])) => last.to_string(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        None => String::new(),
    }
}

/// Opens the file of every sound of `file` (the composition, as reports name it), moved to its `media-start`, ready
/// to be placed in the mix; or gives the report of every sound that cannot be, in the order of the lines.
pub fn open_sounds(file: &str, sounds: &[Sound]) -> Result<Vec<mix::Sound>, Vec<Report>> {
    let codecs = codecs();
    let mut opened = Vec::new();
    let mut reports = Vec::new();
    for sound in sounds {
        match open(&codecs, sound) {
            Ok(source) => {
                tracing::debug!(
                    line = sound.line,
                    first = sound.first,
                    frames = sound.end - sound.first,
                    "sound placed"
                );
                opened.push(mix::Sound {
                    first: sound.first,
                    end: sound.end,
                    skip: sound.skip(source.sample_rate()),
                    source,
                    level: mix::Level {
                        volume: sound.level.volume,
                        points: sound.level.points.iter().map(|(at, value)| (at.to_f64(), *value)).collect(),
                    },
                });
            }
            Err((code, message)) => reports.push(Report::new(file, Problem::new(code, sound.line, message))),
        }
    }
    if !reports.is_empty() {
        return Err(reports);
    }
    if !opened.is_empty() {
        tracing::debug!(sounds = opened.len(), "sounds placed");
    }
    Ok(opened)
}

/// Opens one sound's file and judges its `media-start` against the length frx7 gives.
fn open(codecs: &codec::Registry, sound: &Sound) -> Result<Box<dyn AudioSource>, (Code, String)> {
    let src = &sound.src;
    // A file the machine does not give is told apart here: the system's reader would call it damaged.
    if let Err(error) = std::fs::File::open(&sound.path) {
        return Err((
            Code::SoundUnreadable,
            format!("`src=\"{src}\"` could not be read: {}", reason(&error)),
        ));
    }
    let source = codecs.audio_source(&sound.path).map_err(|error| match error {
        Error::Io(error) => (
            Code::SoundUnreadable,
            format!("`src=\"{src}\"` could not be read: {}", reason(&error)),
        ),
        Error::Unsupported(what) => (
            Code::SoundNotRead,
            format!(
                "`src=\"{src}\"` is not sound this version reads: {what}. The kinds read are {}",
                kinds_read()
            ),
        ),
        other => (Code::SoundDamaged, format!("`src=\"{src}\"` cannot be decoded: {other}")),
    })?;
    if let (Some((_, written)), Some(frames)) = (&sound.media_start, source.frames()) {
        if sound.skip(source.sample_rate()) >= frames {
            let seconds = frames as f64 / f64::from(source.sample_rate());
            return Err((
                Code::SoundStartPastEnd,
                format!("`media-start=\"{written}\"` is at or past the end of `src=\"{src}\"`, which lasts {seconds:.3} s"),
            ));
        }
    }
    Ok(source)
}

/// The system's reason for an error, without the number it adds.
fn reason(error: &std::io::Error) -> String {
    let text = error.to_string();
    match text.rfind(" (os error ") {
        Some(at) => text[..at].to_string(),
        None => text,
    }
}
