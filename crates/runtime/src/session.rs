//! One page's run, from the snapshot to the state of a frame.

use model::{Draft, Manifest, RawDoc, SelectorProblem};
use motion::{Timeline, Tween};
use protocol::{Problem, Resolution, ELEMENT_NUMBERS};
use timebase::Time;

struct ClipWindow {
    first: i64,
    end: i64,
    parent: Option<usize>,
}

struct Plan {
    fps: u32,
    clips: Vec<ClipWindow>,
    timeline: Timeline,
}

#[derive(Default)]
pub struct Session {
    draft: Option<Draft>,
    selectors: Vec<String>,
    read_problem: Option<String>,
    manifest: Option<Manifest>,
    targets: Vec<Vec<usize>>,
    plan: Option<Plan>,
    block: Vec<f64>,
}

impl Session {
    pub fn new() -> Session {
        Session::default()
    }

    /// Takes the snapshot of the page (JSON) and returns the selectors the
    /// loader must match, as a JSON list.
    pub fn read(&mut self, snapshot: &str) -> String {
        *self = Session::default();
        match serde_json::from_str::<RawDoc>(snapshot) {
            Ok(raw) => {
                let draft = Draft::read(&raw, model::ENTRY_NAME);
                self.selectors = draft.selectors();
                self.draft = Some(draft);
            }
            Err(error) => self.read_problem = Some(format!("The page snapshot could not be read: {error}.")),
        }
        serde_json::to_string(&self.selectors).unwrap_or_else(|_| "[]".to_string())
    }

    /// Takes what each selector matched (a JSON list in the order `read`
    /// gave: a list of element keys, or `null` for a selector the page
    /// rejects) and returns the resolution as JSON.
    pub fn resolve(&mut self, matches: &str) -> String {
        let resolution = self.resolve_inner(matches);
        serde_json::to_string(&resolution).unwrap_or_else(|_| "null".to_string())
    }

    fn resolve_inner(&mut self, matches: &str) -> Resolution {
        let failed = |description: String| Resolution {
            manifest: None,
            problems: vec![Problem {
                code: "FS-R016".to_string(),
                description,
            }],
            clips: Vec::new(),
        };
        if let Some(problem) = self.read_problem.clone() {
            return failed(problem);
        }
        let Some(draft) = self.draft.take() else {
            return failed("The page was resolved before it was read.".to_string());
        };
        let matched: Vec<Option<Vec<usize>>> = match serde_json::from_str(matches) {
            Ok(matched) => matched,
            Err(error) => return failed(format!("The selector matches could not be read: {error}.")),
        };
        if matched.len() != self.selectors.len() {
            return failed("The page matched a different number of selectors than it was asked for.".to_string());
        }
        let selectors = std::mem::take(&mut self.selectors);
        let resolved = draft.finish(&mut |selector| {
            let index = selectors.iter().position(|s| s == selector);
            match index.and_then(|i| matched[i].clone()) {
                Some(keys) => Ok(keys),
                None => Err(SelectorProblem::Invalid),
            }
        });
        self.manifest = resolved.manifest.clone();
        self.targets = resolved.targets;
        Resolution {
            manifest: resolved.manifest,
            problems: resolved
                .diagnostics
                .into_iter()
                .map(|d| Problem {
                    code: d.code,
                    description: d.description,
                })
                .collect(),
            clips: resolved.clip_elements,
        }
    }

    /// Fixes the frame rate of the run. Returns the keys of the animated
    /// elements in the order their numbers come in the frame block, as a
    /// JSON list.
    pub fn begin(&mut self, fps: u32) -> String {
        let Some(manifest) = &self.manifest else {
            return "[]".to_string();
        };
        let fps = fps.clamp(timebase::MIN_FPS, timebase::MAX_FPS);
        let tweens: Vec<Tween> = manifest.tweens.iter().map(|fact| fact.tween.clone()).collect();
        let timeline = Timeline::build(&tweens, &self.targets, fps);
        let clips = manifest
            .clips
            .iter()
            .map(|clip| ClipWindow {
                first: clip.start.grid_frame(fps),
                end: clip.end().grid_frame(fps),
                parent: clip.parent,
            })
            .collect();
        let order: Vec<usize> = timeline.elements.iter().map(|e| e.element).collect();
        self.plan = Some(Plan { fps, clips, timeline });
        serde_json::to_string(&order).unwrap_or_else(|_| "[]".to_string())
    }

    /// The state of a frame: one number per clip (1 shown, 0 hidden), then
    /// for each animated element a set of flags (bit `i` set when property
    /// `i` is animated) and the five property values.
    pub fn frame(&mut self, frame: u32) -> &[f64] {
        self.block.clear();
        let Some(plan) = &self.plan else {
            return &self.block;
        };
        let frame = i64::from(frame);
        for (index, clip) in plan.clips.iter().enumerate() {
            let inside = clip.first <= frame && frame < clip.end;
            // An enclosing clip comes earlier in the list, so its answer is ready.
            let shown = inside && clip.parent.is_none_or(|parent| parent < index && self.block[parent] == 1.0);
            self.block.push(if shown { 1.0 } else { 0.0 });
        }
        let t = Time::of_frame(frame, plan.fps);
        for element in &plan.timeline.elements {
            let values = element.values(t);
            let mut flags = 0u32;
            let mut numbers = [0.0; ELEMENT_NUMBERS];
            for (index, value) in values.iter().enumerate() {
                if let Some(value) = value {
                    flags |= 1 << index;
                    numbers[index + 1] = *value;
                }
            }
            numbers[0] = f64::from(flags);
            self.block.extend_from_slice(&numbers);
        }
        &self.block
    }
}
