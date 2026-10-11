//! When visible events happen and where a moving element is, frame by frame.

use crate::frames::each_frame;
use crate::report::{Report, Verdict};
use crate::tools::Tools;
use measure::{Frame, Rgb};
use std::path::Path;

/// The agreed limit for an event: at most this many frames apart.
pub const EVENT_LIMIT_FRAMES: i64 = 1;
/// The agreed limit for a tracked position: at most this many pixels apart on every frame.
pub const POSITION_LIMIT_PIXELS: f64 = 1.0;

#[derive(Clone, Debug)]
pub struct Options {
    /// The flat colour of a clip that appears and disappears.
    pub clip: Option<Rgb>,
    /// The flat colour of one moving element.
    pub mover: Option<Rgb>,
    /// The flat colour behind them.
    pub background: Rgb,
    /// How far each channel may be from the clip's colour and still count as that colour.
    pub tolerance: u8,
    /// How many pixels of the colour make the element "there".
    pub min_pixels: usize,
    /// List the tracked position of every frame in the notes.
    pub per_frame: bool,
    /// Look only inside this rectangle of the picture: left, top, width, height.
    pub region: Option<(u32, u32, u32, u32)>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            clip: None,
            mover: None,
            background: [0, 0, 0],
            tolerance: 24,
            min_pixels: 100,
            per_frame: false,
            region: None,
        }
    }
}

/// What was seen in one video.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    pub frames: usize,
    /// Per frame: whether the clip colour is there.
    pub clip_present: Vec<bool>,
    /// Per frame: the centre of the moving element, or `None` when it is not there.
    pub centres: Vec<Option<(f64, f64)>>,
}

impl Track {
    /// The first frame the clip is there.
    pub fn appears(&self) -> Option<usize> {
        self.clip_present.iter().position(|&p| p)
    }

    /// The first frame after the clip appeared on which it is gone again.
    pub fn disappears(&self) -> Option<usize> {
        let from = self.appears()?;
        self.clip_present[from..].iter().position(|&p| !p).map(|at| from + at)
    }
}

/// How much of `colour` a pixel holds over `background`, along the line between them, and how far
/// the pixel is from that line. A pixel of another colour is far from the line.
fn along(pixel: Rgb, background: Rgb, colour: Rgb) -> (f64, f64) {
    let (mut dot, mut span2, mut pix2) = (0.0, 0.0, 0.0);
    for k in 0..3 {
        let span = f64::from(colour[k]) - f64::from(background[k]);
        let rel = f64::from(pixel[k]) - f64::from(background[k]);
        dot += rel * span;
        span2 += span * span;
        pix2 += rel * rel;
    }
    if span2 == 0.0 {
        return (0.0, f64::INFINITY);
    }
    let t = dot / span2;
    let off = (pix2 - t * t * span2).max(0.0).sqrt();
    (t, off)
}

fn brightness(pixel: Rgb) -> f64 {
    0.2126 * f64::from(pixel[0]) + 0.7152 * f64::from(pixel[1]) + 0.0722 * f64::from(pixel[2])
}

/// The centre of one flat-colour element on a flat background to a fraction of a pixel.
///
/// The element is first found by its colour; then, in a box a few pixels wider than it, every pixel
/// counts by how much of the element it holds, so soft edges count in proportion. When the element's
/// brightness differs enough from the background's, the share is read from brightness alone, because
/// video keeps colour at half resolution and its blur would otherwise move the edge; otherwise it is
/// read along the line between the two colours. `None` when too little of the colour shows.
pub fn centre(frame: &Frame, background: Rgb, colour: Rgb, tolerance: u8, min_pixels: usize) -> Option<(f64, f64)> {
    let core = frame.bounds(colour, tolerance)?;
    let margin = 3;
    let (left, top) = (core.left.saturating_sub(margin), core.top.saturating_sub(margin));
    let (right, bottom) = (
        (core.right + margin).min(frame.width - 1),
        (core.bottom + margin).min(frame.height - 1),
    );
    let contrast = brightness(colour) - brightness(background);
    let (mut mass, mut sx, mut sy) = (0.0, 0.0, 0.0);
    for y in top..=bottom {
        for x in left..=right {
            let pixel = frame.pixel(x, y);
            let share = if contrast.abs() >= 24.0 {
                (brightness(pixel) - brightness(background)) / contrast
            } else {
                let (t, off) = along(pixel, background, colour);
                if off <= 40.0 {
                    t
                } else {
                    0.0
                }
            };
            if share >= 0.05 {
                let w = share.min(1.0);
                mass += w;
                sx += w * (f64::from(x) + 0.5);
                sy += w * (f64::from(y) + 0.5);
            }
        }
    }
    (mass >= min_pixels as f64).then(|| (sx / mass, sy / mass))
}

/// Watches one video.
pub fn track(tools: &Tools, video: &Path, options: &Options) -> Result<Track, String> {
    let facts = measure::facts(&tools.ffprobe, video)?;
    if let Some((left, top, width, height)) = options.region {
        let inside = width > 0 && height > 0 && left + width <= facts.width && top + height <= facts.height;
        if !inside {
            return Err(format!(
                "the region {left},{top},{width},{height} is not inside the {}x{} picture of {}",
                facts.width,
                facts.height,
                video.display()
            ));
        }
    }
    let (left, top) = options.region.map_or((0, 0), |r| (r.0, r.1));
    let mut seen = Track::default();
    seen.frames = each_frame(&tools.ffmpeg, video, facts.width, facts.height, |_, frame| {
        let frame = match options.region {
            Some((left, top, width, height)) => frame.crop(left, top, width, height),
            None => frame,
        };
        if let Some(colour) = options.clip {
            seen.clip_present.push(frame.count(colour, options.tolerance) >= options.min_pixels);
        }
        if let Some(colour) = options.mover {
            // Positions are reported in the whole picture, whatever the region.
            seen.centres.push(
                centre(&frame, options.background, colour, options.tolerance, options.min_pixels)
                    .map(|(x, y)| (x + f64::from(left), y + f64::from(top))),
            );
        }
    })?;
    Ok(seen)
}

fn frame_text(frame: Option<usize>) -> String {
    frame.map_or("never".to_string(), |f| format!("frame {f}"))
}

fn event(report: &mut Report, name: &str, ours: Option<usize>, theirs: Option<usize>) {
    match (ours, theirs) {
        (Some(a), Some(b)) => {
            let apart = (a as i64 - b as i64).abs();
            let verdict = if apart <= EVENT_LIMIT_FRAMES {
                Verdict::Pass
            } else {
                Verdict::Fail
            };
            report.check(
                name,
                verdict,
                format!("ours frame {a}, reference frame {b}, {apart} frame(s) apart (limit {EVENT_LIMIT_FRAMES})"),
            );
        }
        (None, None) => report.check(
            name,
            Verdict::Fail,
            "it never happens in either video, so there is nothing to compare",
        ),
        (a, b) => report.check(name, Verdict::Fail, format!("ours {}, reference {}", frame_text(a), frame_text(b))),
    }
}

/// Compares what was seen in two videos.
pub fn compare(ours: &Track, theirs: &Track, options: &Options) -> Report {
    let mut report = Report::new("timing and position: ours (first) against the reference (second); frames count from 0");
    if options.clip.is_some() {
        event(&mut report, "clip appears", ours.appears(), theirs.appears());
        // A clip that is still there on the last frame has no disappearance in either video.
        match (ours.disappears(), theirs.disappears()) {
            (None, None) if ours.appears().is_some() && theirs.appears().is_some() => report.check(
                "clip disappears",
                Verdict::Pass,
                "it is still there on the last frame in both videos",
            ),
            (a, b) => event(&mut report, "clip disappears", a, b),
        }
    }
    if options.mover.is_some() {
        position(&mut report, ours, theirs);
        if options.per_frame {
            for (frame, (a, b)) in ours.centres.iter().zip(&theirs.centres).enumerate() {
                if let (Some(p), Some(q)) = (a, b) {
                    report.notes.push(format!(
                        "frame {frame}: ours ({:.2}, {:.2}) reference ({:.2}, {:.2}) apart {:.2} px",
                        p.0,
                        p.1,
                        q.0,
                        q.1,
                        (p.0 - q.0).hypot(p.1 - q.1)
                    ));
                }
            }
        }
    }
    if ours.frames != theirs.frames {
        report.notes.push(format!(
            "frame counts differ (ours {}, reference {}); only the frames both have were compared",
            ours.frames, theirs.frames
        ));
    }
    report
}

fn position(report: &mut Report, ours: &Track, theirs: &Track) {
    let name = "position";
    let (mut largest, mut at, mut compared) = (-1.0f64, 0usize, 0usize);
    let mut shown: Option<((f64, f64), (f64, f64))> = None;
    for (frame, (a, b)) in ours.centres.iter().zip(&theirs.centres).enumerate() {
        match (a, b) {
            (Some(p), Some(q)) => {
                compared += 1;
                let apart = (p.0 - q.0).hypot(p.1 - q.1);
                if apart > largest {
                    (largest, at, shown) = (apart, frame, Some((*p, *q)));
                }
            }
            (None, None) => {}
            _ => {
                report.check(
                    name,
                    Verdict::Fail,
                    format!(
                        "frame {frame}: the element is {} in ours and {} in the reference",
                        if a.is_some() { "there" } else { "missing" },
                        if b.is_some() { "there" } else { "missing" }
                    ),
                );
                return;
            }
        }
    }
    let Some((p, q)) = shown else {
        report.check(name, Verdict::Fail, "the element was not found on any frame of both videos");
        return;
    };
    let verdict = if largest <= POSITION_LIMIT_PIXELS {
        Verdict::Pass
    } else {
        Verdict::Fail
    };
    let at_text = |(x, y): (f64, f64)| format!("({x:.2}, {y:.2})");
    report.check(
        name,
        verdict,
        format!(
            "measured: largest difference {largest:.2} px at frame {at} (limit {POSITION_LIMIT_PIXELS} px), ours {} reference {}; {compared} frames compared",
            at_text(p),
            at_text(q)
        ),
    );
}

/// Watches both videos and compares them.
pub fn compare_files(tools: &Tools, ours: &Path, theirs: &Path, options: &Options) -> Result<Report, String> {
    if options.clip.is_none() && options.mover.is_none() {
        return Err("ask for at least one of a clip colour or a mover colour".into());
    }
    let a = track(tools, ours, options)?;
    let b = track(tools, theirs, options)?;
    Ok(compare(&a, &b, options))
}
