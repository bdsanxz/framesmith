//! The sounds of the composition (`docs/spec/sound.md`): each `<audio>` in the root, placed to the sample of the
//! mix. Its file is looked for here, as a font's is; it is opened and judged by the render, which reads sound.

use crate::paragraph::Fault;
use project::{Code, Element, Node, Project};
use std::path::PathBuf;
use timebase::Time;

/// Sample frames a second of the film's sound (`docs/spec/render-command.md` section 5.2).
const MIX_RATE: u32 = 48_000;

/// A sound of the film: an `<audio>` of the root, with the file it names and where it is heard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sound {
    /// The line of the `<audio>`.
    pub line: u32,
    /// `src` as it is written.
    pub src: String,
    /// The file `src` names, found from the project folder.
    pub path: PathBuf,
    /// The first sample frame of the mix it is heard in.
    pub first: u64,
    /// The sample frame it is heard up to, not on.
    pub end: u64,
    /// `media-start`: the time of its source it is heard from, with the value as written.
    pub media_start: Option<(Time, String)>,
}

impl Sound {
    /// The first sample frame of its source that is heard, at the source's `rate`.
    pub fn skip(&self, rate: u32) -> u64 {
        self.media_start
            .as_ref()
            .map_or(0, |(time, _)| time.nearest_count(rate).max(0) as u64)
    }
}

/// The sounds of the project's composition that can be placed, in document order: those `gather` gives.
pub fn sounds(project: &Project) -> Vec<Sound> {
    gather(project, project.tree().nodes()).0
}

/// The sounds of the root's `<audio>` elements in document order, and the faults of the ones that cannot be
/// placed: no `src`, no `duration` above 0, or no file at `src`.
pub(crate) fn gather(project: &Project, tree: &[Node]) -> (Vec<Sound>, Vec<Fault>) {
    let mut sounds = Vec::new();
    let mut faults = Vec::new();
    for node in tree
        .iter()
        .filter(|node| node.element() == Element::Audio && node.parent() == Some(0))
    {
        let line = node.line();
        let fault = |code, message: String| Fault {
            code,
            line,
            message,
            style: false,
        };
        let src = node.attribute("src").map(|attribute| attribute.value());
        if src.is_none() {
            faults.push(fault(
                Code::SoundMissing,
                "`<audio>` needs the attribute `src`, the path of a sound file: from the project folder, or a path that begins with `/`"
                    .to_string(),
            ));
        }
        let duration = node.attribute("duration");
        let length = duration.and_then(|attribute| attribute.time()).filter(|time| time.is_positive());
        if length.is_none() {
            // The sound is named by its `src` as written; an `<audio>` with none is named as the element.
            let named = match src {
                Some(src) => format!("`<audio src=\"{src}\">`"),
                None => "`<audio>`".to_string(),
            };
            let message = match duration {
                Some(attribute) => format!(
                    "{named} needs a `duration` above 0, the seconds the sound plays, not `duration=\"{}\"`",
                    attribute.shown()
                ),
                None => format!("{named} needs a `duration` above 0, the seconds the sound plays"),
            };
            faults.push(fault(Code::SoundNoDuration, message));
        }
        let Some(src) = src else { continue };
        let path = project.resolve(src);
        if !path.is_file() {
            faults.push(fault(
                Code::SoundMissing,
                format!("`src=\"{src}\"` names no file: a sound file, from the project folder or a path that begins with `/`"),
            ));
            continue;
        }
        let Some(length) = length else { continue };
        let start = node.attribute("start").and_then(|attribute| attribute.time()).unwrap_or(Time::ZERO);
        sounds.push(Sound {
            line,
            src: src.to_string(),
            path,
            first: start.nearest_count(MIX_RATE).max(0) as u64,
            end: (start + length).nearest_count(MIX_RATE).max(0) as u64,
            media_start: node
                .attribute("media-start")
                .and_then(|attribute| attribute.time().map(|time| (time, attribute.shown()))),
        });
    }
    faults.sort_by_key(|fault| fault.line);
    (sounds, faults)
}
