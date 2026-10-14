//! The picture check: the score of each frame, scenes, causes for low frames, and the result for
//! each scene and for the whole video (`docs/dev/comparison.md`, section 4). No video is read here;
//! the numbers come in as counts of pixels.

/// A frame is low when more than this share of its pixels differ: 1 in 100.
const LOW_ABOVE_PER_HUNDRED: u128 = 1;

/// A range of frames with a name, both ends included.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scene {
    pub name: String,
    pub first: u64,
    pub last: u64,
}

/// A cause recorded for the low frames of a range, both ends included.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Excuse {
    pub first: u64,
    pub last: u64,
    pub cause: String,
}

fn parse_range(text: &str) -> Result<(u64, u64), String> {
    let number = |part: &str| {
        part.trim()
            .parse::<u64>()
            .map_err(|_| format!("'{text}' is not a frame number or a range of them written as FIRST-LAST"))
    };
    let (first, last) = match text.split_once('-') {
        Some((first, last)) => (number(first)?, number(last)?),
        None => (number(text)?, number(text)?),
    };
    if first > last {
        return Err(format!("the range '{text}' ends before it begins"));
    }
    Ok((first, last))
}

/// Reads a scene written as `NAME=FIRST-LAST`.
pub fn parse_scene(text: &str) -> Result<Scene, String> {
    let (name, range) = text
        .rsplit_once('=')
        .ok_or_else(|| format!("a scene is written NAME=FIRST-LAST, not '{text}'"))?;
    if name.trim().is_empty() {
        return Err(format!("the scene '{text}' has no name"));
    }
    let (first, last) = parse_range(range)?;
    Ok(Scene {
        name: name.trim().to_string(),
        first,
        last,
    })
}

/// Reads a cause written as `FIRST-LAST=CAUSE` or `N=CAUSE`.
pub fn parse_excuse(text: &str) -> Result<Excuse, String> {
    let (range, cause) = text
        .split_once('=')
        .ok_or_else(|| format!("a cause is written FIRST-LAST=CAUSE, not '{text}'"))?;
    let (first, last) = parse_range(range)?;
    if cause.trim().is_empty() {
        return Err(format!(
            "the cause for frames {range} is empty; a cause is a sentence that names it"
        ));
    }
    Ok(Excuse {
        first,
        last,
        cause: cause.trim().to_string(),
    })
}

/// The scenes to judge by: those given, or one scene called `all` with every frame. They must cover
/// the frames exactly, once each.
pub fn scenes_for(given: &[Scene], frames: u64) -> Result<Vec<Scene>, String> {
    if given.is_empty() {
        return Ok(vec![Scene {
            name: "all".to_string(),
            first: 0,
            last: frames.saturating_sub(1),
        }]);
    }
    let mut seen: Vec<&str> = Vec::new();
    for scene in given {
        if seen.contains(&scene.name.as_str()) {
            return Err(format!("the scene name '{}' is used twice", scene.name));
        }
        seen.push(&scene.name);
        if scene.last >= frames {
            return Err(format!(
                "scene '{}' ends at frame {}, and the videos have {frames} frames (numbered 0 to {})",
                scene.name,
                scene.last,
                frames.saturating_sub(1)
            ));
        }
    }
    let mut owner: Vec<Option<&str>> = vec![None; frames as usize];
    for scene in given {
        for frame in scene.first..=scene.last {
            if let Some(other) = owner[frame as usize] {
                return Err(format!("frame {frame} is in the scenes '{other}' and '{}'", scene.name));
            }
            owner[frame as usize] = Some(&scene.name);
        }
    }
    if let Some(frame) = owner.iter().position(Option::is_none) {
        return Err(format!("frame {frame} is in no scene; the scenes must cover every frame"));
    }
    Ok(given.to_vec())
}

/// Checks that the causes lie inside the video and that no frame has two.
pub fn check_excuses(excuses: &[Excuse], frames: u64) -> Result<(), String> {
    for (at, excuse) in excuses.iter().enumerate() {
        if excuse.last >= frames {
            return Err(format!(
                "a cause is recorded for frame {}, and the videos have {frames} frames (numbered 0 to {})",
                excuse.last,
                frames.saturating_sub(1)
            ));
        }
        if let Some(other) = excuses[..at].iter().find(|o| o.first <= excuse.last && excuse.first <= o.last) {
            return Err(format!(
                "frames {} to {} have two causes: '{}' and '{}'",
                excuse.first.max(other.first),
                excuse.last.min(other.last),
                other.cause,
                excuse.cause
            ));
        }
    }
    Ok(())
}

/// What the report says of one frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// At 99 % or better.
    Ok,
    /// Under 99 %, with a named cause.
    Excused(String),
    /// Under 99 %, with no cause.
    Fail,
}

impl State {
    pub fn word(&self) -> &'static str {
        match self {
            State::Ok => "ok",
            State::Excused(_) => "excused",
            State::Fail => "fail",
        }
    }
}

/// One frame's count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scored {
    pub number: u64,
    pub pixels: u64,
    pub differing: u64,
    pub state: State,
}

/// Whether `differing` pixels of `pixels` is more than 1 %: decided in whole numbers.
pub fn is_low(pixels: u64, differing: u64) -> bool {
    u128::from(differing) * 100 > u128::from(pixels) * LOW_ABOVE_PER_HUNDRED
}

/// The share of pixels that are the same, in thousandths of a percent (99.5 % is 99500), rounded down
/// so that a score is never shown higher than it is.
pub fn score_milli(pixels: u64, differing: u64) -> u64 {
    if pixels == 0 {
        return 0;
    }
    (u128::from(pixels - differing.min(pixels)) * 100_000 / u128::from(pixels)) as u64
}

/// A score written as `99.500`.
pub fn percent(pixels: u64, differing: u64) -> String {
    let milli = score_milli(pixels, differing);
    format!("{}.{:03}", milli / 1000, milli % 1000)
}

/// The score as a number for the JSON report.
pub fn score(pixels: u64, differing: u64) -> f64 {
    if pixels == 0 {
        0.0
    } else {
        100.0 * (pixels - differing.min(pixels)) as f64 / pixels as f64
    }
}

/// The result for a scene.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneResult {
    pub scene: Scene,
    pub pixels: u64,
    pub differing: u64,
    /// The frame with the lowest score and its counts.
    pub lowest: (u64, u64, u64),
    pub low: u64,
    pub excused: u64,
    pub failed: u64,
}

impl SceneResult {
    /// The average is 99 % or better and no low frame is without a cause.
    pub fn passes(&self) -> bool {
        !is_low(self.pixels, self.differing) && self.failed == 0
    }
}

/// The picture check as a whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Judgement {
    pub frames: Vec<Scored>,
    pub scenes: Vec<SceneResult>,
    pub pixels: u64,
    pub differing: u64,
}

impl Judgement {
    /// The average over all frames is 99 % or better.
    pub fn average_passes(&self) -> bool {
        !is_low(self.pixels, self.differing)
    }

    /// The check passes only when the average and every scene pass.
    pub fn passes(&self) -> bool {
        self.average_passes() && self.scenes.iter().all(SceneResult::passes)
    }

    /// The frame with the lowest score: its number and counts.
    pub fn lowest(&self) -> Option<&Scored> {
        self.frames.iter().min_by(|a, b| {
            (u128::from(b.differing) * u128::from(a.pixels))
                .cmp(&(u128::from(a.differing) * u128::from(b.pixels)))
                .then(a.number.cmp(&b.number))
        })
    }

    pub fn low(&self) -> impl Iterator<Item = &Scored> {
        self.frames.iter().filter(|f| f.state != State::Ok)
    }
}

/// Judges the frames. `counts` holds, for each frame in order, its pixels and the pixels that
/// differ. `scenes` must cover the frames (see [`scenes_for`]) and `excuses` must lie inside them.
pub fn judge(counts: &[(u64, u64)], scenes: &[Scene], excuses: &[Excuse]) -> Judgement {
    let frames: Vec<Scored> = counts
        .iter()
        .enumerate()
        .map(|(at, &(pixels, differing))| {
            let number = at as u64;
            let state = if !is_low(pixels, differing) {
                State::Ok
            } else {
                match excuses.iter().find(|e| e.first <= number && number <= e.last) {
                    Some(excuse) => State::Excused(excuse.cause.clone()),
                    None => State::Fail,
                }
            };
            Scored {
                number,
                pixels,
                differing,
                state,
            }
        })
        .collect();
    let results = scenes
        .iter()
        .map(|scene| {
            let inside: Vec<&Scored> = frames
                .iter()
                .filter(|f| scene.first <= f.number && f.number <= scene.last)
                .collect();
            let lowest = inside
                .iter()
                .min_by(|a, b| {
                    (u128::from(b.differing) * u128::from(a.pixels))
                        .cmp(&(u128::from(a.differing) * u128::from(b.pixels)))
                        .then(a.number.cmp(&b.number))
                })
                .map_or((scene.first, 0, 0), |f| (f.number, f.pixels, f.differing));
            SceneResult {
                scene: scene.clone(),
                pixels: inside.iter().map(|f| f.pixels).sum(),
                differing: inside.iter().map(|f| f.differing).sum(),
                lowest,
                low: inside.iter().filter(|f| f.state != State::Ok).count() as u64,
                excused: inside.iter().filter(|f| matches!(f.state, State::Excused(_))).count() as u64,
                failed: inside.iter().filter(|f| f.state == State::Fail).count() as u64,
            }
        })
        .collect();
    Judgement {
        pixels: frames.iter().map(|f| f.pixels).sum(),
        differing: frames.iter().map(|f| f.differing).sum(),
        frames,
        scenes: results,
    }
}
