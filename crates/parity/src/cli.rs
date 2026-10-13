//! The command line of the comparison tool.

use crate::report::{Report, EXIT_USAGE};
use crate::tools::Tools;
use crate::{blackbox, container, frames, pictures, timing};
use std::path::PathBuf;

const USAGE: &str = "\
parity: compare a Framesmith video with the reference's, one fact at a time (development only)

  parity reference <counterpart-folder> <output.mp4> [--fps N] [--quality Q] [--blackbox DIR] [--browser PATH]
      run the reference on a counterpart composition (single worker, usage-data and update checks off);
      --fps none passes no frame rate, so the reference fills in its own;
      --blackbox names the folder the reference is installed in; without it, the nearest
      tools/blackbox folder at or above the current folder is used (the workspace's)
  parity facts <ours.mp4> <reference.mp4> [--json]
      frame count, frame rate, size, codec and pixel format of the video streams
  parity pictures <ours.mp4> <reference.mp4> [--samples N] [--threshold X] [--sheet out.png] [--json]
      SSIM on sampled frames and one side-by-side sheet
  parity timing <ours.mp4> <reference.mp4> --background RRGGBB [--clip RRGGBB] [--mover RRGGBB]
                [--tolerance N] [--min-pixels N] [--region X,Y,W,H] [--per-frame] [--json]
      frames on which a clip appears and disappears, and the tracked position of one moving element;
      --region looks only inside that rectangle of the picture (positions stay those of the whole picture)

Every report says pass, fail or not run for each check. Exit codes: 0 all passed, 1 a check failed,
2 the tool was called wrongly, 3 something could not be run (for example the reference is not installed).
";

struct Args {
    positional: Vec<String>,
    options: Vec<(String, Option<String>)>,
}

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

/// The folder the reference is installed in when `--blackbox` is not given: see `blackbox::default_folder`.
fn default_blackbox() -> PathBuf {
    let here = std::env::current_dir().unwrap_or_default();
    blackbox::default_folder(&here)
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
            args.known(&["--samples", "--threshold", "--sheet", "--json"])?;
            let (ours, theirs) = args.two_videos()?;
            let tools = Tools::find()?;
            let mut options = pictures::Options::default();
            options.samples = args.number("--samples")?.unwrap_or(options.samples);
            options.threshold = args.number("--threshold")?.unwrap_or(options.threshold);
            options.sheet = args.value("--sheet").map(PathBuf::from);
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
            let mut options = timing::Options::default();
            options.background = frames::parse_colour(args.value("--background").ok_or("--background RRGGBB is needed")?)?;
            options.clip = args.value("--clip").map(frames::parse_colour).transpose()?;
            options.mover = args.value("--mover").map(frames::parse_colour).transpose()?;
            options.tolerance = args.number("--tolerance")?.unwrap_or(options.tolerance);
            options.min_pixels = args.number("--min-pixels")?.unwrap_or(options.min_pixels);
            options.per_frame = args.flag("--per-frame");
            options.region = args.value("--region").map(parse_region).transpose()?;
            Ok(show(&timing::compare_files(&tools, &ours, &theirs, &options)?, json))
        }
        "help" | "--help" | "-h" => Ok((USAGE.to_string(), 0)),
        other => Err(format!("'{other}' is not a command of this tool")),
    }
}

/// Runs the tool and returns what it prints and its exit code.
pub fn run(args: &[String]) -> (String, i32) {
    run_inner(args).unwrap_or_else(|why| (format!("parity: {why}\n\n{USAGE}"), EXIT_USAGE))
}
