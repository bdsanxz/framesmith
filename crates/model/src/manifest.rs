//! The manifest: the composition as resolved from what is declared.

use motion::Tween;
use serde::{Deserialize, Serialize};
use timebase::Time;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub fps_written: bool,
    pub length: Time,
    pub length_written: bool,
    pub clips: Vec<Clip>,
    pub tweens: Vec<TweenFact>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clip {
    /// How messages name the clip: `#id`, or `number 2 <div>`.
    pub label: String,
    pub tag: String,
    pub start: Time,
    pub duration: Time,
    pub duration_written: bool,
    pub track: Option<i64>,
    /// The enclosing clip, as an index into the clip list.
    pub parent: Option<usize>,
}

impl Clip {
    pub fn end(&self) -> Time {
        self.start + self.duration
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TweenFact {
    pub tween: Tween,
    /// How many elements the target matches.
    pub matches: usize,
}

/// The first fact on which two manifests differ.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Difference {
    pub what: String,
    pub files: String,
    pub page: String,
}

fn differ(what: impl Into<String>, files: impl ToString, page: impl ToString) -> Option<Difference> {
    Some(Difference {
        what: what.into(),
        files: files.to_string(),
        page: page.to_string(),
    })
}

fn track_text(track: Option<i64>) -> String {
    track.map_or_else(|| "none".to_string(), |track| track.to_string())
}

/// Compares the manifest read from the files with the one the page reports.
pub fn first_difference(files: &Manifest, page: &Manifest) -> Option<Difference> {
    if files.name != page.name {
        return differ("the composition name", &files.name, &page.name);
    }
    if (files.width, files.height) != (page.width, page.height) {
        return differ(
            "the composition size",
            format!("{}x{}", files.width, files.height),
            format!("{}x{}", page.width, page.height),
        );
    }
    if files.fps != page.fps {
        return differ("the frame rate", files.fps, page.fps);
    }
    if files.length != page.length {
        return differ(
            "the composition length",
            format!("{} s", files.length),
            format!("{} s", page.length),
        );
    }
    for (index, (a, b)) in files.clips.iter().zip(&page.clips).enumerate() {
        let name = format!("clip {} ({})", index + 1, a.label);
        if a.label != b.label || a.tag != b.tag {
            return differ(format!("clip {}", index + 1), &a.label, &b.label);
        }
        if a.start != b.start {
            return differ(format!("the start of {name}"), format!("{} s", a.start), format!("{} s", b.start));
        }
        if a.duration != b.duration {
            return differ(
                format!("the duration of {name}"),
                format!("{} s", a.duration),
                format!("{} s", b.duration),
            );
        }
        if a.track != b.track {
            return differ(format!("the track of {name}"), track_text(a.track), track_text(b.track));
        }
        if a.parent != b.parent {
            return differ(
                format!("the enclosing clip of {name}"),
                format!("{:?}", a.parent),
                format!("{:?}", b.parent),
            );
        }
    }
    if files.clips.len() != page.clips.len() {
        // The first clip that only one side has.
        let shared = files.clips.len().min(page.clips.len());
        return if page.clips.len() > shared {
            differ(format!("the clip {}", page.clips[shared].label), "missing", "present")
        } else {
            differ(format!("the clip {}", files.clips[shared].label), "present", "missing")
        };
    }
    for (index, (a, b)) in files.tweens.iter().zip(&page.tweens).enumerate() {
        let name = format!("tween {} (target \"{}\")", index + 1, a.tween.target);
        if a.tween != b.tween {
            return differ(
                name,
                format!("{} at {} s for {} s", a.tween.kind.name(), a.tween.at, a.tween.duration),
                format!(
                    "{} on \"{}\" at {} s for {} s",
                    b.tween.kind.name(),
                    b.tween.target,
                    b.tween.at,
                    b.tween.duration
                ),
            );
        }
        if a.matches != b.matches {
            return differ(format!("the number of elements matched by {name}"), a.matches, b.matches);
        }
    }
    if files.tweens.len() != page.tweens.len() {
        return differ("the number of tweens", files.tweens.len(), page.tweens.len());
    }
    None
}
