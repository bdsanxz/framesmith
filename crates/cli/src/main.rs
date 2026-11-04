//! Framesmith CLI — Sprint 1 (T001–T014: init/check/render/errors with
//! pinned FFmpeg self-download, CPU text/video rendering, sound
//! passthrough, and standard MP4 output).
//!
//! * `framesmith init <name> [--dir <parent>]` — create a minimal project.
//! * `framesmith check [--json] <folder>` — validate without rendering.
//! * `framesmith errors [--json] [CODE]` — explain error codes.
//! * `framesmith render <folder> [-o <out.mp4>] [--composition <name>] [--start S] [--end E]
//!   [--backend cpu|gpu|auto] [--preset draft|standard|high] [--crf N] [--video-bitrate B] [--json]
//!   [--variables JSON] [--strict-variables] [--theme NAME]`
//!   — validate, bootstrap FFmpeg (pinned self-download or refusal),
//!   refuse or encode with staged progress.
//! * `framesmith render <folder> --batch rows.json [--name-template TPL] [--jobs N] [--fail-fast]`
//!   — one MP4 per rows-file row plus a per-row summary.
//!
//! Drawing, sound and the full per-run log arrive with later tasks.

// Several model/detection helpers exist for the T011+ drawing and
// bootstrap slices and are exercised by unit tests. Silence binary-crate
// dead-code lints until those slices land.
#![allow(dead_code)]

mod commands;

use std::path::{Path, PathBuf};

fn usage() -> ! {
    eprintln!(
        "usage:\n\
         \x20 framesmith init <name> [--dir <parent>]\n\
         \x20 framesmith check [--json] [--backend cpu|gpu|auto] [--variables JSON] [--strict-variables] [--theme NAME] <folder>\n\
         \x20 framesmith errors [--json] [CODE]\n\
         \x20 framesmith compositions [--json] <folder>\n\
         \x20 framesmith timeline [--json] [--composition <name>] <folder> [<name>]\n\
         \x20 framesmith info [--json] [--composition <name>] <folder> [<name>]\n\
         \x20 framesmith waveform [--json] [--composition <name>] [--buckets N] <folder> [<name>]\n\
         \x20 framesmith render <folder> [-o <output.mp4>] [--composition <name>] [--start S] [--end E] [--backend cpu|gpu|auto] [--preset draft|standard|high] [--crf N] [--video-bitrate B] [--variables JSON] [--strict-variables] [--theme NAME] [--json]\n\
         \x20 framesmith render <folder> --batch rows.json [--name-template TPL] [--jobs N] [--fail-fast]  one MP4 per row"
    );
    std::process::exit(2);
}

fn listing_usage_error(cmd: &str, msg: &str) -> ! {
    eprintln!("framesmith {cmd}: {msg}.");
    match cmd {
        "compositions" => eprintln!("usage: framesmith compositions [--json] <folder>"),
        _ => eprintln!("usage: framesmith {cmd} [--json] [--composition <name>] <folder> [<name>]"),
    }
    std::process::exit(2);
}

/// Parse `compositions` / `timeline` / `info` arguments (Sprint 6 T010):
/// at most one `--json`, at most one `--composition <name>`, then one
/// folder plus an optional positional composition name. Returns
/// `(folder, name, json)`. The flag and the positional must agree when
/// both name a composition.
fn parse_listing_args(cmd: &str, args: &[String]) -> (PathBuf, Option<String>, bool) {
    let mut json = false;
    let mut flag_name: Option<String> = None;
    let mut positionals: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                if json {
                    listing_usage_error(cmd, "duplicate --json flag");
                }
                json = true;
            }
            "--composition" => {
                i += 1;
                if i >= args.len() {
                    listing_usage_error(cmd, "--composition needs a value (a composition name)");
                }
                if flag_name.is_some() {
                    listing_usage_error(cmd, "duplicate --composition flag");
                }
                flag_name = Some(args[i].clone());
            }
            other if other.starts_with('-') => {
                listing_usage_error(cmd, &format!("unknown flag {other:?}"));
            }
            other => positionals.push(other.to_string()),
        }
        i += 1;
    }
    if positionals.is_empty() || positionals.len() > 2 {
        listing_usage_error(
            cmd,
            &match cmd {
                "compositions" => "expected exactly one <folder>".to_string(),
                _ => "expected one <folder> plus an optional [name]".to_string(),
            },
        );
    }
    let folder = PathBuf::from(&positionals[0]);
    let positional_name = positionals.get(1).cloned();
    let name = match (flag_name, positional_name) {
        (Some(f), Some(p)) if f != p => listing_usage_error(
            cmd,
            &format!("--composition {f:?} disagrees with positional name {p:?}; pass one"),
        ),
        (Some(f), _) => Some(f),
        (None, p) => p,
    };
    if cmd == "compositions" && name.is_some() {
        listing_usage_error(
            cmd,
            "expected exactly one <folder> (compositions takes no name)",
        );
    }
    (folder, name, json)
}

fn check_usage_error(msg: &str) -> ! {
    eprintln!("framesmith check: {msg}.");
    eprintln!(
        "usage: framesmith check [--json] [--backend cpu|gpu|auto] [--variables JSON] [--strict-variables] [--theme NAME] <folder>"
    );
    std::process::exit(2);
}

fn waveform_usage_error(msg: &str) -> ! {
    eprintln!("framesmith waveform: {msg}.");
    eprintln!("usage: framesmith waveform [--json] [--composition <name>] [--buckets N] <folder> [<name>]");
    std::process::exit(2);
}

fn errors_usage_error(msg: &str) -> ! {
    eprintln!("framesmith errors: {msg}.");
    eprintln!("usage: framesmith errors [--json] [CODE]");
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        usage();
    }
    let code = match args[0].as_str() {
        "init" => {
            let mut name: Option<String> = None;
            let mut dir = PathBuf::from(".");
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--dir" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!("framesmith init: --dir needs a value.");
                            std::process::exit(2);
                        }
                        dir = PathBuf::from(&args[i]);
                    }
                    other if other.starts_with('-') => {
                        eprintln!("framesmith init: unknown flag {other:?}.");
                        std::process::exit(2);
                    }
                    other => {
                        if name.is_some() {
                            eprintln!(
                                "framesmith init: expected one <name>, found extra {other:?}."
                            );
                            std::process::exit(2);
                        }
                        name = Some(other.to_string());
                    }
                }
                i += 1;
            }
            match name {
                Some(n) => commands::cmd_init(&n, &dir),
                None => {
                    eprintln!("framesmith init: missing <name>.");
                    std::process::exit(2);
                }
            }
        }
        "check" => {
            let mut json = false;
            let mut backend: Option<String> = None;
            let mut variables: Option<String> = None;
            let mut strict_variables = false;
            let mut theme: Option<String> = None;
            let mut folder: Option<String> = None;
            let mut i = 1;
            while i < args.len() {
                let arg = &args[i];
                if *arg == "--json" {
                    if json {
                        check_usage_error("duplicate --json flag");
                    }
                    json = true;
                } else if *arg == "--backend" {
                    i += 1;
                    if i >= args.len() {
                        check_usage_error("--backend needs a value (cpu, gpu, or auto)");
                    }
                    if backend.is_some() {
                        check_usage_error("duplicate --backend flag");
                    }
                    backend = Some(args[i].clone());
                } else if *arg == "--variables" {
                    i += 1;
                    if i >= args.len() {
                        check_usage_error(
                            "--variables needs a value (a JSON object of string keys to scalar values)",
                        );
                    }
                    if variables.is_some() {
                        check_usage_error("duplicate --variables flag");
                    }
                    variables = Some(args[i].clone());
                } else if *arg == "--strict-variables" {
                    if strict_variables {
                        check_usage_error("duplicate --strict-variables flag");
                    }
                    strict_variables = true;
                } else if *arg == "--theme" {
                    i += 1;
                    if i >= args.len() {
                        check_usage_error("--theme needs a value (a declared theme name)");
                    }
                    if theme.is_some() {
                        check_usage_error("duplicate --theme flag");
                    }
                    theme = Some(args[i].clone());
                } else if arg.starts_with('-') {
                    check_usage_error(&format!("unknown flag {arg:?}"));
                } else if folder.is_some() {
                    check_usage_error(&format!(
                        "expected exactly one <folder>, found extra {arg:?}"
                    ));
                } else {
                    folder = Some(arg.clone());
                }
                i += 1;
            }
            match folder {
                // A bare `--json` with no folder is unparseable, not silent.
                Some(f) => commands::cmd_check_full_strict_themed(
                    Path::new(&f),
                    json,
                    backend.as_deref(),
                    variables.as_deref(),
                    strict_variables,
                    theme.as_deref(),
                ),
                None => check_usage_error("expected exactly one <folder>"),
            }
        }
        "errors" => {
            let mut json = false;
            let mut code: Option<String> = None;
            for arg in &args[1..] {
                if *arg == "--json" {
                    if json {
                        errors_usage_error("duplicate --json flag");
                    }
                    json = true;
                } else if arg.starts_with('-') {
                    errors_usage_error(&format!("unknown flag {arg:?}"));
                } else if code.is_some() {
                    errors_usage_error(&format!("expected at most one CODE, found extra {arg:?}"));
                } else {
                    code = Some(arg.clone());
                }
            }
            commands::cmd_errors(code.as_deref(), json)
        }
        "render" => {
            let mut folder: Option<String> = None;
            let mut output: Option<PathBuf> = None;
            let mut req = commands::RenderRequest::default();
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "-o" | "--output" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!("framesmith render: -o needs a value.");
                            std::process::exit(2);
                        }
                        output = Some(PathBuf::from(&args[i]));
                    }
                    "--backend" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!(
                                "framesmith render: --backend needs a value (cpu, gpu, or auto)."
                            );
                            std::process::exit(2);
                        }
                        if req.backend.is_some() {
                            eprintln!("framesmith render: duplicate --backend flag.");
                            std::process::exit(2);
                        }
                        req.backend = Some(args[i].clone());
                    }
                    "--preset" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!("framesmith render: --preset needs a value (draft, standard, or high).");
                            std::process::exit(2);
                        }
                        if req.preset.is_some() {
                            eprintln!("framesmith render: duplicate --preset flag.");
                            std::process::exit(2);
                        }
                        req.preset = Some(args[i].clone());
                    }
                    "--crf" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!("framesmith render: --crf needs a value (0 through 51).");
                            std::process::exit(2);
                        }
                        if req.crf.is_some() {
                            eprintln!("framesmith render: duplicate --crf flag.");
                            std::process::exit(2);
                        }
                        req.crf = Some(args[i].clone());
                    }
                    "--video-bitrate" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!("framesmith render: --video-bitrate needs a value (for example 2M).");
                            std::process::exit(2);
                        }
                        if req.video_bitrate.is_some() {
                            eprintln!("framesmith render: duplicate --video-bitrate flag.");
                            std::process::exit(2);
                        }
                        req.video_bitrate = Some(args[i].clone());
                    }
                    "--json" => {
                        if req.json {
                            eprintln!("framesmith render: duplicate --json flag.");
                            std::process::exit(2);
                        }
                        req.json = true;
                    }
                    "--composition" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!("framesmith render: --composition needs a value (a composition name).");
                            std::process::exit(2);
                        }
                        if req.composition.is_some() {
                            eprintln!("framesmith render: duplicate --composition flag.");
                        }
                        req.composition = Some(args[i].clone());
                    }
                    "--start" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!("framesmith render: --start needs a value (seconds into the composition).");
                            std::process::exit(2);
                        }
                        if req.start.is_some() {
                            eprintln!("framesmith render: duplicate --start flag.");
                            std::process::exit(2);
                        }
                        req.start = Some(args[i].clone());
                    }
                    "--end" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!("framesmith render: --end needs a value (seconds into the composition).");
                            std::process::exit(2);
                        }
                        if req.end.is_some() {
                            eprintln!("framesmith render: duplicate --end flag.");
                            std::process::exit(2);
                        }
                        req.end = Some(args[i].clone());
                    }
                    "--variables" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!("framesmith render: --variables needs a value (a JSON object of string keys to scalar values).");
                            std::process::exit(2);
                        }
                        if req.variables.is_some() {
                            eprintln!("framesmith render: duplicate --variables flag.");
                            std::process::exit(2);
                        }
                        req.variables = Some(args[i].clone());
                    }
                    "--strict-variables" => {
                        if req.strict_variables {
                            eprintln!("framesmith render: duplicate --strict-variables flag.");
                            std::process::exit(2);
                        }
                        req.strict_variables = true;
                    }
                    "--theme" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!(
                                "framesmith render: --theme needs a value (a declared theme name)."
                            );
                            std::process::exit(2);
                        }
                        if req.theme.is_some() {
                            eprintln!("framesmith render: duplicate --theme flag.");
                            std::process::exit(2);
                        }
                        req.theme = Some(args[i].clone());
                    }
                    "--batch" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!(
                                "framesmith render: --batch needs a value (a JSON rows file)."
                            );
                            std::process::exit(2);
                        }
                        if req.batch.is_some() {
                            eprintln!("framesmith render: duplicate --batch flag.");
                            std::process::exit(2);
                        }
                        req.batch = Some(args[i].clone());
                    }
                    "--name-template" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!("framesmith render: --name-template needs a value (for example \"clip-{{title}}-{{index}}\").");
                            std::process::exit(2);
                        }
                        if req.name_template.is_some() {
                            eprintln!("framesmith render: duplicate --name-template flag.");
                            std::process::exit(2);
                        }
                        req.name_template = Some(args[i].clone());
                    }
                    "--jobs" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!("framesmith render: --jobs needs a value (a worker count from 1 to 64).");
                            std::process::exit(2);
                        }
                        if req.jobs.is_some() {
                            eprintln!("framesmith render: duplicate --jobs flag.");
                            std::process::exit(2);
                        }
                        req.jobs = Some(args[i].clone());
                    }
                    "--fail-fast" => {
                        if req.fail_fast {
                            eprintln!("framesmith render: duplicate --fail-fast flag.");
                            std::process::exit(2);
                        }
                        req.fail_fast = true;
                    }
                    other if other.starts_with('-') => {
                        eprintln!("framesmith render: unknown flag {other:?}.");
                        std::process::exit(2);
                    }
                    other => {
                        if folder.is_some() {
                            eprintln!(
                                "framesmith render: expected one <folder>, found extra {other:?}."
                            );
                            std::process::exit(2);
                        }
                        folder = Some(other.to_string());
                    }
                }
                i += 1;
            }
            if req.batch.is_none()
                && (req.name_template.is_some() || req.jobs.is_some() || req.fail_fast)
            {
                eprintln!("framesmith render: --name-template, --jobs, and --fail-fast need --batch (a JSON rows file).");
                std::process::exit(2);
            }
            match folder {
                Some(f) => commands::cmd_render_full(Path::new(&f), output.as_deref(), &req),
                None => {
                    eprintln!("framesmith render: missing <folder>.");
                    std::process::exit(2);
                }
            }
        }
        "compositions" => {
            let (folder, _, json) = parse_listing_args("compositions", &args[1..]);
            commands::cmd_compositions(&folder, json)
        }
        "timeline" => {
            let (folder, name, json) = parse_listing_args("timeline", &args[1..]);
            commands::cmd_timeline(&folder, name.as_deref(), json)
        }
        "info" => {
            let (folder, name, json) = parse_listing_args("info", &args[1..]);
            commands::cmd_info(&folder, name.as_deref(), json)
        }
        "waveform" => {
            let mut json = false;
            let mut composition: Option<String> = None;
            let mut buckets: Option<String> = None;
            let mut positionals: Vec<String> = Vec::new();
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--json" => {
                        if json {
                            waveform_usage_error("duplicate --json flag");
                        }
                        json = true;
                    }
                    "--composition" => {
                        i += 1;
                        if i >= args.len() {
                            waveform_usage_error(
                                "--composition needs a value (a composition name)",
                            );
                        }
                        if composition.is_some() {
                            waveform_usage_error("duplicate --composition flag");
                        }
                        composition = Some(args[i].clone());
                    }
                    "--buckets" => {
                        i += 1;
                        if i >= args.len() {
                            waveform_usage_error(
                                "--buckets needs a value (a bucket count of 1 or more)",
                            );
                        }
                        if buckets.is_some() {
                            waveform_usage_error("duplicate --buckets flag");
                        }
                        buckets = Some(args[i].clone());
                    }
                    other if other.starts_with('-') => {
                        waveform_usage_error(&format!("unknown flag {other:?}"));
                    }
                    other => positionals.push(other.to_string()),
                }
                i += 1;
            }
            if positionals.is_empty() || positionals.len() > 2 {
                waveform_usage_error("expected one <folder> plus an optional [name]");
            }
            let folder = PathBuf::from(&positionals[0]);
            let positional_name = positionals.get(1).cloned();
            let name = match (composition, positional_name) {
                (Some(f), Some(p)) if f != p => waveform_usage_error(&format!(
                    "--composition {f:?} disagrees with positional name {p:?}; pass one"
                )),
                (Some(f), _) => Some(f),
                (None, p) => p,
            };
            commands::cmd_waveform(
                &folder,
                &commands::WaveformRequest {
                    composition: name,
                    buckets,
                    json,
                },
            )
        }
        "-h" | "--help" | "help" => {
            println!(
                "framesmith — turn a small KDL composition folder into a playable MP4.\n\n\
                 commands:\n\
                 \x20 init <name> [--dir <parent>]  create a minimal valid project\n\
                 \x20 check [--json] [--backend cpu|gpu|auto] [--variables JSON] [--strict-variables] [--theme NAME] <folder>  validate without rendering (exit 0 valid, 1 invalid, 2 unsupported)\n\
                 \x20 errors [--json] [CODE]        explain every error code\n\
                 \x20 compositions [--json] <folder>  list every composition name plus the root\n\
                 \x20 timeline [--json] [--composition <name>] <folder> [<name>]  list every clip with id, track, label, hidden state, and window\n\
                 \x20 info [--json] [--composition <name>] <folder> [<name>]  summarize canvas, duration, and clip counts\n\
                 \x20 waveform [--json] [--composition <name>] [--buckets N] <folder> [<name>]  report per-bucket mix peaks plus the master peak\n\
                 \x20 render <folder> [-o <file>] [--composition <name>] [--start S] [--end E] [--backend cpu|gpu|auto] [--preset draft|standard|high] [--crf N] [--video-bitrate B] [--variables JSON] [--strict-variables] [--theme NAME] [--json]  validate, bootstrap FFmpeg, refuse or encode\n\
                 \x20 render <folder> --batch rows.json [--name-template TPL] [--jobs N] [--fail-fast]  one MP4 per row plus a per-row summary"
            );
            0
        }
        "-V" | "--version" => {
            println!("framesmith {}", env!("CARGO_PKG_VERSION"));
            0
        }
        other => {
            eprintln!("framesmith: unknown subcommand {other:?}.");
            std::process::exit(2);
        }
    };
    std::process::exit(code);
}
