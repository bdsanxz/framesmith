//! The command line of the comparison tool.

use crate::report::{Report, EXIT_USAGE};
use crate::tools::Tools;
use crate::{blackbox, compare, container, frames, pictures, timing};
use std::path::PathBuf;

const USAGE: &str = "\
parity: compare a Framesmith video with the reference's, one fact at a time (development only)

  parity reference <counterpart-folder> <output.mp4> [--fps N] [--quality Q] [--blackbox DIR] [--browser PATH]
      run the reference on a counterpart composition (single worker, usage-data and update checks off);
      --fps none passes no frame rate, so the reference fills in its own
  parity facts <ours.mp4> <reference.mp4> [--json]
      frame count, frame rate, size, codec and pixel format of the video streams
  parity pictures <ours.mp4> <reference.mp4> [--samples N | --frames A,B,C] [--threshold X] [--sheet out.png] [--json]
      SSIM on sampled frames (evenly spaced, or the frames named) and one side-by-side sheet
  parity timing <ours.mp4> <reference.mp4> --background RRGGBB [--clip RRGGBB] [--clip NAME=RRGGBB ...]
                [--mover RRGGBB] [--tolerance N] [--min-pixels N] [--region X,Y,W,H] [--per-frame] [--json]
      frames on which a clip appears and disappears, and the tracked position of one moving element;
      --clip NAME=RRGGBB may be given once for each clip of a chain, and each is reported under its name;
      --region looks only inside that rectangle of the picture (positions stay those of the whole picture)
  parity compare <ours.mp4> <reference.mp4> [--background RRGGBB] [--clip NAME=RRGGBB ...] [--mover RRGGBB]
                [--samples N | --frames A,B,C] [--threshold X] [--sheet out.png] [--tolerance N]
                [--min-pixels N] [--region X,Y,W,H] [--per-frame] [--json]
      every group of checks in one report (container facts, pictures, the timing of each clip, the position
      of the moving element), each check with what was measured and its threshold; --background is needed
      with --mover

Every report says pass, fail or not run for each check. Exit codes: 0 all passed, 1 a check failed,
2 the tool was called wrongly, 3 something could not be run (for example the reference is not installed).
";

struct Args {
    positional: Vec<String>,
    options: Vec<(String, Option<String>)>,
}

/// The options of `compare`: those of `pictures` and of `timing`.
const COMPARE_OPTIONS: &[&str] = &[
    "--background",
    "--clip",
    "--mover",
    "--samples",
    "--frames",
    "--threshold",
    "--sheet",
    "--tolerance",
    "--min-pixels",
    "--region",
    "--per-frame",
    "--json",
];

const FLAGS: &[&str] = &["--json", "--per-frame"];

fn parse(args: &[String]) -> Result<Args, String> {
    let (mut positional, mut options) = (Vec::new(), Vec::new());
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg.starts_with("--") {
            if FLAGS.contains(&arg.as_str()) {
                options.push((arg.clone(), None));
            } else {
                let value = it.next().ok_or_else(|| format!("{arg} needs a value"))?;
                options.push((arg.clone(), Some(value.clone())));
            }
        } else {
            positional.push(arg.clone());
        }
    }
    Ok(Args { positional, options })
}

impl Args {
    fn value(&self, name: &str) -> Option<&str> {
        self.options.iter().find(|(n, _)| n == name).and_then(|(_, v)| v.as_deref())
    }

    /// Every value given for an option that may be repeated.
    fn values(&self, name: &str) -> Vec<&str> {
        self.options
            .iter()
            .filter(|(n, _)| n == name)
            .filter_map(|(_, v)| v.as_deref())
            .collect()
    }

    fn flag(&self, name: &str) -> bool {
        self.options.iter().any(|(n, _)| n == name)
    }

    fn number<T: std::str::FromStr>(&self, name: &str) -> Result<Option<T>, String> {
        self.value(name)
            .map(|v| v.parse().map_err(|_| format!("{name} needs a number, not '{v}'")))
            .transpose()
    }

    fn known(&self, allowed: &[&str]) -> Result<(), String> {
        match self.options.iter().find(|(n, _)| !allowed.contains(&n.as_str())) {
            Some((n, _)) => Err(format!("{n} is not an option of this command")),
            None => Ok(()),
        }
    }

    fn two_videos(&self) -> Result<(PathBuf, PathBuf), String> {
        match self.positional.as_slice() {
            [a, b] => Ok((PathBuf::from(a), PathBuf::from(b))),
            _ => Err("give two video files: ours, then the reference's".into()),
        }
    }
}

/// Parses a rectangle written as `X,Y,W,H`.
fn parse_region(text: &str) -> Result<(u32, u32, u32, u32), String> {
    let parts: Vec<u32> = text.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    match parts.as_slice() {
        [x, y, w, h] if text.split(',').count() == 4 => Ok((*x, *y, *w, *h)),
        _ => Err(format!("--region needs four whole numbers written as X,Y,W,H, not '{text}'")),
    }
}

/// Parses the frames asked for, written `A,B,C`.
fn parse_frames(text: &str) -> Result<Vec<usize>, String> {
    text.split(',')
        .map(|part| {
            part.trim()
                .parse()
                .map_err(|_| format!("--frames needs whole numbers written as A,B,C, not '{text}'"))
        })
        .collect()
}

type Clips = (Option<measure::Rgb>, Vec<(String, measure::Rgb)>);

/// The clips asked for with `--clip`: one bare colour (the single clip), and any number of `NAME=RRGGBB`.
fn parse_clips(args: &Args) -> Result<Clips, String> {
    let (mut single, mut named) = (None, Vec::new());
    for value in args.values("--clip") {
        match value.split_once('=') {
            Some((name, colour)) => {
                let usable = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
                if !usable {
                    return Err(format!("a clip name is letters, digits, - and _, not '{name}'"));
                }
                if named.iter().any(|(n, _): &(String, measure::Rgb)| n == name) {
                    return Err(format!("the clip '{name}' is named twice"));
                }
                named.push((name.to_string(), frames::parse_colour(colour)?));
            }
            None => {
                if single.is_some() {
                    return Err("give at most one bare --clip RRGGBB; name the others as --clip NAME=RRGGBB".into());
                }
                single = Some(frames::parse_colour(value)?);
            }
        }
    }
    Ok((single, named))
}

/// What the timing and position checks are asked to look at. `background` is read only when given.
fn timing_options(args: &Args, background: Option<&str>) -> Result<timing::Options, String> {
    let mut options = timing::Options::default();
    if let Some(background) = background {
        options.background = frames::parse_colour(background)?;
    }
    let (single, named) = parse_clips(args)?;
    options.clip = single;
    options.named_clips = named;
    options.mover = args.value("--mover").map(frames::parse_colour).transpose()?;
    options.tolerance = args.number("--tolerance")?.unwrap_or(options.tolerance);
    options.min_pixels = args.number("--min-pixels")?.unwrap_or(options.min_pixels);
    options.per_frame = args.flag("--per-frame");
    options.region = args.value("--region").map(parse_region).transpose()?;
    Ok(options)
}

/// The options of the picture checks.
fn picture_options(args: &Args) -> Result<pictures::Options, String> {
    let mut options = pictures::Options::default();
    if args.value("--samples").is_some() && args.value("--frames").is_some() {
        return Err("give --samples or --frames, not both".into());
    }
    options.samples = args.number("--samples")?.unwrap_or(options.samples);
    options.frames = args.value("--frames").map(parse_frames).transpose()?;
    options.threshold = args.number("--threshold")?.unwrap_or(options.threshold);
    options.sheet = args.value("--sheet").map(PathBuf::from);
    Ok(options)
}

/// `tools/blackbox` in the current folder, or in `framesmith/` when run from the workspace root.
fn default_blackbox() -> PathBuf {
    ["tools/blackbox", "framesmith/tools/blackbox"]
        .into_iter()
        .map(PathBuf::from)
        .find(|p| p.is_dir())
        .unwrap_or_else(|| PathBuf::from("tools/blackbox"))
}

fn show(report: &Report, json: bool) -> (String, i32) {
    let text = if json {
        format!("{}\n", serde_json::to_string_pretty(&report.json()).unwrap_or_default())
    } else {
        report.text()
    };
    (text, report.exit_code())
}

fn run_inner(args: &[String]) -> Result<(String, i32), String> {
    let Some((command, rest)) = args.split_first() else {
        return Ok((USAGE.to_string(), EXIT_USAGE));
    };
    let args = parse(rest)?;
    let json = args.flag("--json");
    match command.as_str() {
        "reference" => {
            args.known(&["--fps", "--quality", "--blackbox", "--browser"])?;
            let [counterpart, output] = args.positional.as_slice() else {
                return Err("give the counterpart folder and the output video path".into());
            };
            let job = blackbox::Job {
                blackbox: args.value("--blackbox").map_or_else(default_blackbox, PathBuf::from),
                counterpart: PathBuf::from(counterpart),
                output: PathBuf::from(output),
                fps: match args.value("--fps") {
                    Some("none") => None,
                    _ => Some(args.number("--fps")?.unwrap_or(blackbox::FPS)),
                },
                quality: args.value("--quality").unwrap_or(blackbox::QUALITY).to_string(),
                browser: args.value("--browser").map(PathBuf::from),
            };
            let outcome = blackbox::run(&job);
            Ok((outcome.text(), outcome.exit_code()))
        }
        "facts" => {
            args.known(&["--json"])?;
            let (ours, theirs) = args.two_videos()?;
            let tools = Tools::find()?;
            Ok(show(&container::compare_files(&tools.ffprobe, &ours, &theirs)?, json))
        }
        "pictures" => {
            args.known(&["--samples", "--frames", "--threshold", "--sheet", "--json"])?;
            let (ours, theirs) = args.two_videos()?;
            let tools = Tools::find()?;
            let options = picture_options(&args)?;
            Ok(show(&pictures::compare(&tools, &ours, &theirs, &options)?, json))
        }
        "timing" => {
            args.known(&[
                "--background",
                "--clip",
                "--mover",
                "--tolerance",
                "--min-pixels",
                "--region",
                "--per-frame",
                "--json",
            ])?;
            let (ours, theirs) = args.two_videos()?;
            let tools = Tools::find()?;
            let background = args.value("--background").ok_or("--background RRGGBB is needed")?;
            let options = timing_options(&args, Some(background))?;
            Ok(show(&timing::compare_files(&tools, &ours, &theirs, &options)?, json))
        }
        "compare" => {
            args.known(COMPARE_OPTIONS)?;
            let (ours, theirs) = args.two_videos()?;
            if args.value("--mover").is_some() && args.value("--background").is_none() {
                return Err("--background RRGGBB is needed with --mover".into());
            }
            let tools = Tools::find()?;
            let options = compare::Options {
                pictures: picture_options(&args)?,
                timing: timing_options(&args, args.value("--background"))?,
            };
            Ok(show(&compare::compare_files(&tools, &ours, &theirs, &options)?, json))
        }
        "help" | "--help" | "-h" => Ok((USAGE.to_string(), 0)),
        other => Err(format!("'{other}' is not a command of this tool")),
    }
}

/// Runs the tool and returns what it prints and its exit code.
pub fn run(args: &[String]) -> (String, i32) {
    run_inner(args).unwrap_or_else(|why| (format!("parity: {why}\n\n{USAGE}"), EXIT_USAGE))
}
