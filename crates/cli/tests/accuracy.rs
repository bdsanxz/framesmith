//! T027: the title-card fixture, measured on the painted frames against
//! the positions and blends computed from its motion block.

mod common;

use common::*;
use measure::{corner_distance, expected_corners, shape, Frame, Rgb};
use model::Manifest;
use motion::{Timeline, Tween, PROPS};
use std::path::{Path, PathBuf};
use timebase::Time;

const GROUND: Rgb = [0x10, 0x18, 0x20];
const WHITE: Rgb = [255, 255, 255];
/// The five chosen times, as frames: around the start of the bar's clip
/// (14, 15), inside the bar's and the title's first tweens (27), inside the
/// badge's and the subtitle's tweens (52), inside the tag's opacity tween
/// (82), and around the end of the three clips (104, 105).
const SAMPLED: [usize; 7] = [14, 15, 27, 52, 82, 104, 105];
/// A frame on which both texts are at rest: where their ink is laid out.
const REST: usize = 75;
const CORNER_LIMIT: f64 = 1.0;
const LEVEL_LIMIT: f64 = 2.0;

struct Element {
    target: &'static str,
    colour: Rgb,
    /// The part of the frame only this element is painted in: left, top, width, height.
    area: (u32, u32, u32, u32),
    /// Where the style sheet lays a box out: left, top, width, height. Text has none.
    laid: Option<(f64, f64, f64, f64)>,
}

const ELEMENTS: [Element; 5] = [
    Element {
        target: "#bar",
        colour: [0xff, 0x5a, 0x36],
        area: (0, 404, 1280, 48),
        laid: Some((140.0, 420.0, 360.0, 16.0)),
    },
    Element {
        target: "#title",
        colour: WHITE,
        area: (0, 230, 860, 170),
        laid: None,
    },
    Element {
        target: "#badge",
        colour: [0x2e, 0xcc, 0x71],
        area: (860, 40, 420, 330),
        laid: Some((960.0, 150.0, 160.0, 100.0)),
    },
    Element {
        target: "#subtitle",
        colour: WHITE,
        area: (0, 566, 1280, 80),
        laid: None,
    },
    Element {
        target: "#tag",
        colour: [0x34, 0x98, 0xdb],
        area: (0, 464, 1280, 92),
        laid: Some((140.0, 480.0, 200.0, 60.0)),
    },
];

/// What was measured of one element on one frame.
struct Seen {
    corners: [(f64, f64); 4],
    /// The colour of a pixel wholly inside the element.
    inside: Rgb,
}

fn cropped(frame: &Frame, element: &Element) -> Frame {
    let (left, top, width, height) = element.area;
    frame.crop(left, top, width, height)
}

fn moved(point: (f64, f64), by: (f64, f64)) -> (f64, f64) {
    (point.0 + by.0, point.1 + by.1)
}

fn seen_box(frame: &Frame, element: &Element) -> Option<Seen> {
    let found = shape(&cropped(frame, element), GROUND, element.colour)?;
    let origin = (f64::from(element.area.0), f64::from(element.area.1));
    let centre = moved(found.centre, origin);
    Some(Seen {
        corners: found.corners.map(|corner| moved(corner, origin)),
        inside: frame.pixel(centre.0 as u32, centre.1 as u32),
    })
}

/// The ink of a text: its centre of weight, the box of its solid part, and its strongest pixel.
struct Ink {
    centre: (f64, f64),
    edges: (f64, f64, f64, f64),
    strongest: Rgb,
}

fn ink(frame: &Frame, element: &Element) -> Option<Ink> {
    let (left, top, width, height) = element.area;
    let share = |pixel: Rgb| {
        let (mut over, mut span) = (0.0, 0.0);
        for k in 0..3 {
            over += f64::from(pixel[k]) - f64::from(GROUND[k]);
            span += f64::from(element.colour[k]) - f64::from(GROUND[k]);
        }
        (over / span).max(0.0)
    };
    let (mut peak, mut strongest) = (0.0, GROUND);
    for y in top..top + height {
        for x in left..left + width {
            let s = share(frame.pixel(x, y));
            if s > peak {
                (peak, strongest) = (s, frame.pixel(x, y));
            }
        }
    }
    if peak < 0.02 {
        return None;
    }
    let (mut mass, mut sx, mut sy) = (0.0, 0.0, 0.0);
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, 0.0f64, 0.0f64);
    for y in top..top + height {
        for x in left..left + width {
            let s = share(frame.pixel(x, y)) / peak;
            mass += s;
            sx += s * (f64::from(x) + 0.5);
            sy += s * (f64::from(y) + 0.5);
            if s >= 0.5 {
                (x0, y0) = (x0.min(f64::from(x)), y0.min(f64::from(y)));
                (x1, y1) = (x1.max(f64::from(x) + 1.0), y1.max(f64::from(y) + 1.0));
            }
        }
    }
    Some(Ink {
        centre: (sx / mass, sy / mass),
        edges: (x0, y0, x1, y1),
        strongest,
    })
}

/// A text keeps its shape: its corners are those of its ink at rest, moved as far as its centre of weight moved.
fn seen_text(frame: &Frame, rest: &Ink, element: &Element) -> Option<Seen> {
    let now = ink(frame, element)?;
    let by = (now.centre.0 - rest.centre.0, now.centre.1 - rest.centre.1);
    let (x0, y0, x1, y1) = rest.edges;
    Some(Seen {
        corners: [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].map(|corner| moved(corner, by)),
        inside: now.strongest,
    })
}

/// One line of the report.
struct Line {
    element: &'static str,
    frame: usize,
    pass: bool,
    text: String,
}

fn repository_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/title-card")
}

fn manifest_of(project: &Path) -> Manifest {
    model::check(project).manifest.expect("the fixture passes the check")
}

/// Measures the sampled frames against the values computed from `expected`'s motion block and clips.
fn report(frames: &[Frame], expected: &Manifest) -> Vec<Line> {
    let tweens: Vec<Tween> = expected.tweens.iter().map(|fact| fact.tween.clone()).collect();
    let key = |target: &str| ELEMENTS.iter().position(|e| e.target == target).expect("a known element");
    let targets: Vec<Vec<usize>> = tweens.iter().map(|tween| vec![key(&tween.target)]).collect();
    let timeline = Timeline::build(&tweens, &targets, expected.fps);
    let rest: Vec<Option<Ink>> = ELEMENTS
        .iter()
        .map(|element| element.laid.is_none().then(|| ink(&frames[REST], element)).flatten())
        .collect();
    let mut lines = Vec::new();
    for &frame in &SAMPLED {
        let t = Time::of_frame(frame as i64, expected.fps);
        for (index, element) in ELEMENTS.iter().enumerate() {
            let mut line = |pass: bool, text: String| {
                lines.push(Line {
                    element: element.target,
                    frame,
                    pass,
                    text,
                })
            };
            let shown = expected
                .clips
                .iter()
                .find(|clip| clip.label == element.target)
                .is_none_or(|clip| clip.start <= t && t < clip.end());
            let painted = !cropped(&frames[frame], element).is_all(GROUND, 2);
            if !shown {
                line(
                    !painted,
                    format!("outside its clip: {}", if painted { "painted" } else { "absent" }),
                );
                continue;
            }
            let motion = timeline
                .elements
                .iter()
                .find(|m| m.element == index)
                .expect("every element has a tween");
            let values = motion.values(t);
            let value = |at: usize| values[at].unwrap_or(PROPS[at].own_value());
            let (x, y, scale, rotation, opacity) = (value(0), value(1), value(2), value(3), value(4));
            let (seen, corners) = match (element.laid, &rest[index]) {
                (Some((left, top, width, height)), _) => (
                    seen_box(&frames[frame], element),
                    expected_corners(left, top, width, height, [x, y, scale, rotation]),
                ),
                (None, Some(rest)) => {
                    let (x0, y0, x1, y1) = rest.edges;
                    (
                        seen_text(&frames[frame], rest, element),
                        [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].map(|corner| moved(corner, (x, y))),
                    )
                }
                (None, None) => (None, [(0.0, 0.0); 4]),
            };
            let Some(seen) = seen else {
                line(opacity < 0.02, format!("inside its clip at opacity {opacity:.3}: not found"));
                continue;
            };
            let apart = corner_distance(&corners, &seen.corners);
            let blend = [0, 1, 2].map(|k| f64::from(GROUND[k]) + opacity * (f64::from(element.colour[k]) - f64::from(GROUND[k])));
            let levels = (0..3).map(|k| (f64::from(seen.inside[k]) - blend[k]).abs()).fold(0.0, f64::max);
            line(
                apart <= CORNER_LIMIT && levels <= LEVEL_LIMIT,
                format!(
                    "x {x:.2} y {y:.2} scale {scale:.3} rotation {rotation:.2} opacity {opacity:.3}: corners {apart:.2} px apart (limit {CORNER_LIMIT}), colour {levels:.1} levels apart (limit {LEVEL_LIMIT})"
                ),
            );
        }
    }
    lines
}

fn show(lines: &[Line]) -> String {
    let mut text = String::new();
    for line in lines {
        text.push_str(&format!(
            "frame {:>3}  {:<10} {}  {}\n",
            line.frame,
            line.element,
            if line.pass { "pass" } else { "FAIL" },
            line.text
        ));
    }
    let failed = lines.iter().filter(|line| !line.pass).count();
    text.push_str(&if failed == 0 {
        format!("result: pass ({} measurements)\n", lines.len())
    } else {
        format!("result: fail ({failed} of {} measurements)\n", lines.len())
    });
    text
}

fn rendered(test: &str) -> Vec<Frame> {
    let project = fixture(test, "title-card");
    let (ran, frames) = render_frames(&project, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(frames.len(), 120);
    frames
}

fn title_card() -> &'static (Vec<Frame>, Vec<Line>) {
    static ONCE: std::sync::OnceLock<(Vec<Frame>, Vec<Line>)> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        let frames = rendered("t027_title_card");
        let lines = report(&frames, &manifest_of(&repository_fixture()));
        println!("{}", show(&lines));
        (frames, lines)
    })
}

fn line<'a>(lines: &'a [Line], element: &str, frame: usize) -> &'a Line {
    lines
        .iter()
        .find(|line| line.element == element && line.frame == frame)
        .expect("a measured line")
}

#[test]
fn t027_c1_every_corner_of_every_tweened_element_is_within_one_pixel_at_the_five_times() {
    let (_, lines) = title_card();
    assert_eq!(lines.len(), SAMPLED.len() * ELEMENTS.len());
    let failed: Vec<&Line> = lines.iter().filter(|line| !line.pass).collect();
    assert!(failed.is_empty(), "{}", show(lines));
    // Every element was measured while it moved, not only at rest.
    for (element, frame) in [
        ("#bar", 27),
        ("#title", 27),
        ("#badge", 52),
        ("#subtitle", 52),
        ("#tag", 82),
        ("#bar", 104),
    ] {
        let text = &line(lines, element, frame).text;
        assert!(text.contains("corners"), "{element} at frame {frame}: {text}");
        assert!(
            !text.starts_with("x 0.00 y 0.00 scale 1.000 rotation 0.00 opacity 1.000"),
            "{element} at frame {frame}: {text}"
        );
    }
}

#[test]
fn t027_c2_a_part_faded_element_shows_the_computed_blend_within_two_levels() {
    let (frames, lines) = title_card();
    // The tag is 0.767 opaque on frame 82: its inside is the blend of its colour and the ground.
    let tag = line(lines, "#tag", 82);
    assert!(tag.pass && tag.text.contains("opacity 0.767"), "{}", tag.text);
    let inside = frames[82].pixel(540, 510);
    for (k, &level) in inside.iter().enumerate() {
        let blend = 16.0 + 8.0 * k as f64 + (1.0 - 0.5 * 7.0 / 15.0) * (f64::from(ELEMENTS[4].colour[k]) - 16.0 - 8.0 * k as f64);
        assert!(
            (f64::from(level) - blend).abs() <= LEVEL_LIMIT,
            "channel {k}: {level} against {blend:.1}"
        );
    }
    // The two texts, part-way through their fades.
    for (element, frame, opacity) in [("#title", 27, "opacity 0.306"), ("#subtitle", 52, "opacity 0.407")] {
        let text = line(lines, element, frame);
        assert!(text.pass && text.text.contains(opacity), "{}", text.text);
    }
}

#[test]
fn t027_c3_a_clip_is_absent_on_the_frame_before_its_start_and_in_place_on_its_start_frame() {
    let (frames, lines) = title_card();
    let before = line(lines, "#bar", 14);
    assert!(before.pass && before.text == "outside its clip: absent", "{}", before.text);
    assert!(cropped(&frames[14], &ELEMENTS[0]).is_all(GROUND, 0));
    let start = line(lines, "#bar", 15);
    assert!(start.pass && start.text.starts_with("x 400.00 y 0.00"), "{}", start.text);
    // The same at the other end: in place on the last frame of the window, gone on the next.
    assert!(line(lines, "#bar", 104).text.starts_with("x 2.8"));
    for element in ["#bar", "#title", "#subtitle"] {
        let after = line(lines, element, 105);
        assert!(after.pass && after.text == "outside its clip: absent", "{element}: {}", after.text);
    }
}

#[test]
fn t027_c4_a_tween_altered_by_three_pixels_is_reported_with_its_element_and_frame() {
    let project = fixture("t027_c4", "title-card");
    let page = std::fs::read_to_string(project.join("index.html")).unwrap();
    let altered = page.replace("\"from\": { \"x\": 400 }", "\"from\": { \"x\": 403 }");
    assert_ne!(page, altered);
    write(&project, "index.html", altered);
    let (ran, frames) = render_frames(&project, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    // The expected values still come from the fixture as it is in the repository.
    let lines = report(&frames, &manifest_of(&repository_fixture()));
    let shown = show(&lines);
    println!("{shown}");
    let failed: Vec<(&str, usize)> = lines
        .iter()
        .filter(|line| !line.pass)
        .map(|line| (line.element, line.frame))
        .collect();
    assert_eq!(failed, [("#bar", 15)], "{shown}");
    assert!(shown.contains("frame  15  #bar       FAIL"), "{shown}");
    assert!(shown.contains("corners 3.00 px apart"), "{shown}");
    assert!(shown.ends_with("result: fail (1 of 35 measurements)\n"), "{shown}");
}
