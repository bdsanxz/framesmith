//! Framesmith CLI — Sprint 1 (T001–T014: init/check/render/errors with
//! pinned FFmpeg self-download, CPU text/video rendering, sound
//! passthrough, and standard MP4 output).
//!
//! * `framesmith init <name> [--dir <parent>]` — create a minimal project.
//! * `framesmith check [--json] <folder>` — validate without rendering.
//! * `framesmith errors [--json] [CODE]` — explain error codes.
//! * `framesmith render <folder> [-o <out.mp4>] [--composition <name>] [--start S] [--end E]
//!   [--backend cpu|gpu|auto] [--preset draft|standard|high] [--crf N] [--video-bitrate B] [--json]`
//!   — validate, bootstrap FFmpeg (pinned self-download or refusal),
//!   refuse or encode with staged progress.
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
         \x20 framesmith check [--json] [--backend cpu|gpu|auto] <folder>\n\
         \x20 framesmith errors [--json] [CODE]\n\
         \x20 framesmith compositions [--json] <folder>\n\
         \x20 framesmith timeline [--json] [--composition <name>] <folder> [<name>]\n\
         \x20 framesmith info [--json] [--composition <name>] <folder> [<name>]\n\
         \x20 framesmith render <folder> [-o <output.mp4>] [--composition <name>] [--start S] [--end E] [--backend cpu|gpu|auto] [--preset draft|standard|high] [--crf N] [--video-bitrate B] [--json]"
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
    eprintln!("usage: framesmith check [--json] <folder>");
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
                Some(f) => commands::cmd_check(Path::new(&f), json, backend.as_deref()),
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
        "-h" | "--help" | "help" => {
            println!(
                "framesmith — turn a small KDL composition folder into a playable MP4.\n\n\
                 commands:\n\
                 \x20 init <name> [--dir <parent>]  create a minimal valid project\n\
                 \x20 check [--json] [--backend cpu|gpu|auto] <folder>  validate without rendering (exit 0 valid, 1 invalid, 2 unsupported)\n\
                 \x20 errors [--json] [CODE]        explain every error code\n\
                 \x20 compositions [--json] <folder>  list every composition name plus the root\n\
                 \x20 timeline [--json] [--composition <name>] <folder> [<name>]  list every clip with id, track, label, hidden state, and window\n\
                 \x20 info [--json] [--composition <name>] <folder> [<name>]  summarize canvas, duration, and clip counts\n\
                 \x20 render <folder> [-o <file>] [--composition <name>] [--start S] [--end E] [--backend cpu|gpu|auto] [--preset draft|standard|high] [--crf N] [--video-bitrate B] [--json]  validate, bootstrap FFmpeg, refuse or encode"
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
