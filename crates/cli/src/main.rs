//! Framesmith CLI — Sprint 1 (T001–T014: init/check/render/errors with
//! pinned FFmpeg self-download, CPU text/video rendering, sound
//! passthrough, and standard MP4 output).
//!
//! * `framesmith init <name> [--dir <parent>] [--example <starter>]` — create a minimal
//!   project, or start one from a bundled starter example.
//! * `framesmith check [--json] <folder>` — validate without rendering.
//! * `framesmith snapshot (--at T,.. | --every S | --from S --to E) --out <dir> <folder>`
//!   — draw frames as `snap-<time>s.png` without writing any MP4.
//! * `framesmith errors [--json] [CODE]` — explain error codes.
//! * `framesmith render <folder> [-o <out.mp4>] [--composition <name>] [--start S] [--end E]
//!   [--backend cpu|gpu|auto] [--preset draft|standard|high] [--crf N] [--video-bitrate B] [--json]
//!   [--variables JSON] [--strict-variables] [--theme NAME]
//!   [--format ..] [--prores-profile ..] [--resolution ..] [--supersample N]
//!   [--encoder auto|software|videotoolbox] [--keyint N] [--maxrate B] [--hdr off|pq|hlg|auto]`
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

mod catalog_cmd;
mod commands;
mod ml_cmd;
mod resume;
mod router;
mod skills;
mod starters;
mod storyboard;

use std::path::{Path, PathBuf};

fn usage() -> ! {
    eprintln!(
        "usage:\n\
         \x20 framesmith init <name> [--dir <parent>] [--example <starter>]\n\
         \x20 framesmith check [--json] [--backend cpu|gpu|auto] [--variables JSON] [--strict-variables] [--theme NAME] [--strict|--strict-all] <folder>\n\
         \x20 framesmith lint [--json] [--backend cpu|gpu|auto] [--variables JSON] [--strict-variables] [--theme NAME] [--strict|--strict-all] <folder>\n\
         \x20 framesmith snapshot [--json] [--composition <name>] [--backend cpu|gpu|auto] [--variables JSON] [--strict-variables] [--theme NAME] (--at <t,..> | --every <secs> | --from <s> --to <e>) --out <dir> <folder>\n\
         \x20 framesmith keyframes [--json] [--composition <name>] [--backend cpu|gpu|auto] [--variables JSON] [--strict-variables] [--theme NAME] --clip <id> --property <name> <folder>\n\
         \x20 framesmith compare [--json] [--composition <name>] [--at <secs>] [--backend cpu|gpu|auto] [--variables JSON] [--strict-variables] [--theme NAME] -o <out.png> <folderA> <folderB>\n\
         \x20 framesmith grade-compare [--json] [--composition <name>] [--at <secs>] [--backend cpu|gpu|auto] [--variables JSON] [--strict-variables] [--theme NAME] -o <out.png> <folder>\n\
         \x20 framesmith preview [--port <n>] [--composition <name>] [--foreground] <folder>\n\
         \x20 framesmith preview-status [<folder>]\n\
         \x20 framesmith preview-stop <folder>\n\
         \x20 framesmith preview-list\n\
         \x20 framesmith preview-stop-all\n\
         \x20 framesmith play [--port <n>] [--composition <name>] [--foreground] <folder>\n\
         \x20 framesmith doctor [--json] [<folder>]\n\
         \x20 framesmith models list [--json]\n\
         \x20 framesmith models install <name> [--yes] [--json]\n\
         \x20 framesmith transcribe <input> [--engine auto|NAME|stub] [--model <name>] [--language <lang>] [--to srt|vtt] [-o <out.json>]\n\
         \x20 framesmith tts <text...> [--voice <voice>] [--speed <f>] [--language <lang>] [--model <name>] [--engine auto|NAME|stub] [-o <out.wav>]\n\
         \x20 framesmith remove-background <input.png> [--quality fast|balanced|precise] [--model <name>] [--engine auto|NAME|stub] [-o <outdir>]\n\
         \x20 framesmith beats <input.wav> [-o <out.beats.json>]\n\
         \x20 framesmith benchmark [--json] [--out <dir>]\n\
         \x20 framesmith errors [--json] [CODE]
         \x20 framesmith catalog search <words...> [--json] [--registry <dir>]
         \x20 framesmith catalog show <name> [--json] [--registry <dir>]
         \x20 framesmith catalog report-miss <words...> [--registry <dir>] [--out <file>]
         \x20 framesmith catalog install <project> <name...> [--tag <tag>...] [--registry <dir>] [--vars <id=value>...] [--force]
         \x20 framesmith catalog meaning-search <words...> [--consent yes|no] [--registry <dir>]
         \x20 framesmith catalog meaning-revoke\n\
         \x20 framesmith compositions [--json] <folder>\n\
         \x20 framesmith timeline [--json] [--composition <name>] <folder> [<name>]\n\
         \x20 framesmith timeline move|trim|split|delete|set|duplicate|ids|plan|apply|undo ...  (timeline edit verbs; bare `timeline` still lists)\n\
         \x20 framesmith history (list|show|undo|restore|peek|pin|begin|end) [--json] [--diff] [--who <name>] [--exclusive] [--if-open] [--unpin] <folder> [<seq>]\n\
         \x20 framesmith skills list [--json]\n\
         \x20 framesmith skills install --host <host> [--dest <dir>]\n\
         \x20 framesmith skills check --host <host> [--dest <dir>] [--json]\n\
         \x20 framesmith skills update (--all | --core | --skill <name>) --host <host> [--dest <dir>] [--yes]\n\
         \x20 framesmith skills validate [--host <host>] [--dest <dir>] [--json]
         \x20 framesmith storyboard board [--at <t,..> | --every <secs> | --from <s> --to <e>] [--out <dir>] [--title <text>] [--composition <name>] [--backend cpu|gpu|auto] [--actor <name>] <folder>
         \x20 framesmith storyboard approve [--actor <name>] [--json] <folder>
         \x20 framesmith storyboard status [--json] <folder>\n\
         \x20 framesmith route --request <text> [<folder>] [--pref <k=v> ...] [--recipe <name>] [--shape single|batch|stills-only] [--workflow <name>] [--fresh] [--install --host <host> [--dest <dir>]] [--json]\n\
         \x20 framesmith resume [--json] [--words <text>] [--who <name>] <folder>\n\
         \x20 framesmith info [--json] [--composition <name>] <folder> [<name>]\n\
         \x20 framesmith waveform [--json] [--composition <name>] [--buckets N] <folder> [<name>]\n\
         \x20 framesmith render <folder> [-o <output>] [--composition <name>] [--start S] [--end E] [--backend cpu|gpu|auto] [--preset draft|standard|high] [--crf N] [--video-bitrate B] [--format mp4|hevc|webm|mov|prores|gif|pngseq|hls] [--prores-profile proxy|lt|standard|hq|4444] [--resolution source|1080p|720p|480p|square-1080|4k] [--supersample 1|2|3|4] [--encoder auto|software|videotoolbox] [--keyint N] [--maxrate B] [--hdr off|pq|hlg|auto] [--variables JSON] [--strict-variables] [--theme NAME] [--strict|--strict-all] [--json]\n\
         \x20 framesmith render <folder> --batch rows.json [--name-template TPL] [--jobs N] [--fail-fast]  one MP4 per row\n\
         \x20 framesmith grade-bake [--composition <name>] -o <out.cube> <folder>"
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
        "usage: framesmith check [--json] [--backend cpu|gpu|auto] [--variables JSON] [--strict-variables] [--theme NAME] [--strict|--strict-all] <folder>"
    );
    std::process::exit(2);
}

fn lint_usage_error(msg: &str) -> ! {
    eprintln!("framesmith lint: {msg}.");
    eprintln!(
        "usage: framesmith lint [--json] [--backend cpu|gpu|auto] [--variables JSON] [--strict-variables] [--theme NAME] [--strict|--strict-all] <folder>"
    );
    std::process::exit(2);
}

fn waveform_usage_error(msg: &str) -> ! {
    eprintln!("framesmith waveform: {msg}.");
    eprintln!("usage: framesmith waveform [--json] [--composition <name>] [--buckets N] <folder> [<name>]");
    std::process::exit(2);
}

fn snapshot_usage_error(msg: &str) -> ! {
    eprintln!("framesmith snapshot: {msg}.");
    eprintln!("usage: framesmith snapshot [--json] [--composition <name>] [--backend cpu|gpu|auto] [--variables JSON] [--strict-variables] [--theme NAME] (--at <t,..> | --every <secs> | --from <s> --to <e>) --out <dir> <folder>");
    std::process::exit(2);
}

fn keyframes_usage_error(msg: &str) -> ! {
    eprintln!("framesmith keyframes: {msg}.");
    eprintln!("usage: framesmith keyframes [--json] [--composition <name>] [--backend cpu|gpu|auto] [--variables JSON] [--strict-variables] [--theme NAME] --clip <id> --property <name> <folder>");
    std::process::exit(2);
}

fn compare_usage_error(msg: &str) -> ! {
    eprintln!("framesmith compare: {msg}.");
    eprintln!("usage: framesmith compare [--json] [--composition <name>] [--at <secs>] [--backend cpu|gpu|auto] [--variables JSON] [--strict-variables] [--theme NAME] -o <out.png> <folderA> <folderB>");
    std::process::exit(2);
}

fn grade_compare_usage_error(msg: &str) -> ! {
    eprintln!("framesmith grade-compare: {msg}.");
    eprintln!("usage: framesmith grade-compare [--json] [--composition <name>] [--at <secs>] [--backend cpu|gpu|auto] [--variables JSON] [--strict-variables] [--theme NAME] -o <out.png> <folder>");
    std::process::exit(2);
}

fn doctor_usage_error(msg: &str) -> ! {
    eprintln!("framesmith doctor: {msg}.");
    eprintln!("usage: framesmith doctor [--json] [<folder>]");
    std::process::exit(2);
}

fn benchmark_usage_error(msg: &str) -> ! {
    eprintln!("framesmith benchmark: {msg}.");
    eprintln!("usage: framesmith benchmark [--json] [--out <dir>]");
    std::process::exit(2);
}

fn history_usage_error(msg: &str) -> ! {
    eprintln!("framesmith history: {msg}.");
    eprintln!("usage: framesmith history (list|show|undo|restore|peek|pin|begin|end) [--json] [--diff] [--who <name>] [--exclusive] [--if-open] [--unpin] <folder> [<seq>]");
    std::process::exit(2);
}

fn preview_usage_error(msg: &str) -> ! {
    eprintln!("framesmith preview: {msg}.");
    eprintln!(
        "usage: framesmith preview [--port <n>] [--composition <name>] [--foreground] <folder>"
    );
    std::process::exit(2);
}

fn play_usage_error(msg: &str) -> ! {
    eprintln!("framesmith play: {msg}.");
    eprintln!("usage: framesmith play [--port <n>] [--composition <name>] [--foreground] <folder>");
    std::process::exit(2);
}

fn preview_status_usage_error(msg: &str) -> ! {
    eprintln!("framesmith preview-status: {msg}.");
    eprintln!("usage: framesmith preview-status [<folder>]");
    std::process::exit(2);
}

fn preview_stop_usage_error(msg: &str) -> ! {
    eprintln!("framesmith preview-stop: {msg}.");
    eprintln!("usage: framesmith preview-stop <folder>");
    std::process::exit(2);
}

fn preview_list_usage_error(msg: &str) -> ! {
    eprintln!("framesmith preview-list: {msg}.");
    eprintln!("usage: framesmith preview-list");
    std::process::exit(2);
}

fn preview_stop_all_usage_error(msg: &str) -> ! {
    eprintln!("framesmith preview-stop-all: {msg}.");
    eprintln!("usage: framesmith preview-stop-all");
    std::process::exit(2);
}

fn preview_serve_usage_error(msg: &str) -> ! {
    eprintln!("framesmith preview-serve: {msg}.");
    eprintln!(
        "usage: framesmith preview-serve --port <n> --registry <file> [--composition <name>] <folder>"
    );
    std::process::exit(2);
}

fn errors_usage_error(msg: &str) -> ! {
    eprintln!("framesmith errors: {msg}.");
    eprintln!("usage: framesmith errors [--json] [CODE]");
    std::process::exit(2);
}

fn grade_bake_usage_error(msg: &str) -> ! {
    eprintln!("framesmith grade-bake: {msg}.");
    eprintln!("usage: framesmith grade-bake [--composition <name>] -o <out.cube> <folder>");
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
            let mut example: Option<String> = None;
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
                    "--example" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!("framesmith init: --example needs a value (a starter name).");
                            std::process::exit(2);
                        }
                        example = Some(args[i].clone());
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
                Some(n) => commands::cmd_init_with_example(&n, &dir, example.as_deref()),
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
            let mut strict = false;
            let mut strict_all = false;
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
                } else if *arg == "--strict" {
                    if strict {
                        check_usage_error("duplicate --strict flag");
                    }
                    strict = true;
                } else if *arg == "--strict-all" {
                    if strict_all {
                        check_usage_error("duplicate --strict-all flag");
                    }
                    strict_all = true;
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
            if strict && strict_all {
                check_usage_error("--strict and --strict-all are mutually exclusive");
            }
            let strict_mode = commands::resolve_strict_mode(strict, strict_all);
            match folder {
                // A bare `--json` with no folder is unparseable, not silent.
                Some(f) => commands::cmd_check_full_strict_themed(
                    Path::new(&f),
                    json,
                    backend.as_deref(),
                    variables.as_deref(),
                    strict_variables,
                    theme.as_deref(),
                    strict_mode,
                ),
                None => check_usage_error("expected exactly one <folder>"),
            }
        }
        "lint" => {
            let mut json = false;
            let mut backend: Option<String> = None;
            let mut variables: Option<String> = None;
            let mut strict_variables = false;
            let mut strict = false;
            let mut strict_all = false;
            let mut theme: Option<String> = None;
            let mut folder: Option<String> = None;
            let mut i = 1;
            while i < args.len() {
                let arg = &args[i];
                if *arg == "--json" {
                    if json {
                        lint_usage_error("duplicate --json flag");
                    }
                    json = true;
                } else if *arg == "--strict" {
                    if strict {
                        lint_usage_error("duplicate --strict flag");
                    }
                    strict = true;
                } else if *arg == "--strict-all" {
                    if strict_all {
                        lint_usage_error("duplicate --strict-all flag");
                    }
                    strict_all = true;
                } else if *arg == "--backend" {
                    i += 1;
                    if i >= args.len() {
                        lint_usage_error("--backend needs a value (cpu, gpu, or auto)");
                    }
                    if backend.is_some() {
                        lint_usage_error("duplicate --backend flag");
                    }
                    backend = Some(args[i].clone());
                } else if *arg == "--variables" {
                    i += 1;
                    if i >= args.len() {
                        lint_usage_error(
                            "--variables needs a value (a JSON object of string keys to scalar values)",
                        );
                    }
                    if variables.is_some() {
                        lint_usage_error("duplicate --variables flag");
                    }
                    variables = Some(args[i].clone());
                } else if *arg == "--strict-variables" {
                    if strict_variables {
                        lint_usage_error("duplicate --strict-variables flag");
                    }
                    strict_variables = true;
                } else if *arg == "--theme" {
                    i += 1;
                    if i >= args.len() {
                        lint_usage_error("--theme needs a value (a declared theme name)");
                    }
                    if theme.is_some() {
                        lint_usage_error("duplicate --theme flag");
                    }
                    theme = Some(args[i].clone());
                } else if arg.starts_with('-') {
                    lint_usage_error(&format!("unknown flag {arg:?}"));
                } else if folder.is_some() {
                    lint_usage_error(&format!(
                        "expected exactly one <folder>, found extra {arg:?}"
                    ));
                } else {
                    folder = Some(arg.clone());
                }
                i += 1;
            }
            if strict && strict_all {
                lint_usage_error("--strict and --strict-all are mutually exclusive");
            }
            let strict_mode = commands::resolve_strict_mode(strict, strict_all);
            match folder {
                // A bare `--json` with no folder is unparseable, not silent.
                Some(f) => commands::cmd_lint_full_strict_themed(
                    Path::new(&f),
                    json,
                    backend.as_deref(),
                    variables.as_deref(),
                    strict_variables,
                    theme.as_deref(),
                    strict_mode,
                ),
                None => lint_usage_error("expected exactly one <folder>"),
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
        "catalog" => catalog_cmd::cmd_catalog(&args[1..]),
        "skills" => skills::cmd_skills(&args[1..]),
        "storyboard" => storyboard::cmd_storyboard(&args[1..]),
        "route" => router::cmd_route(&args[1..]),
        "resume" => resume::cmd_resume(&args[1..]),
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
                    "--format" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!("framesmith render: --format needs a value (mp4, hevc, webm, mov, prores, gif, pngseq, or hls).");
                            std::process::exit(2);
                        }
                        if req.format.is_some() {
                            eprintln!("framesmith render: duplicate --format flag.");
                            std::process::exit(2);
                        }
                        req.format = Some(args[i].clone());
                    }
                    "--prores-profile" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!("framesmith render: --prores-profile needs a value (proxy, lt, standard, hq, or 4444).");
                            std::process::exit(2);
                        }
                        if req.prores_profile.is_some() {
                            eprintln!("framesmith render: duplicate --prores-profile flag.");
                            std::process::exit(2);
                        }
                        req.prores_profile = Some(args[i].clone());
                    }
                    "--resolution" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!("framesmith render: --resolution needs a value (source, 1080p, 720p, 480p, square-1080, or 4k).");
                            std::process::exit(2);
                        }
                        if req.resolution.is_some() {
                            eprintln!("framesmith render: duplicate --resolution flag.");
                            std::process::exit(2);
                        }
                        req.resolution = Some(args[i].clone());
                    }
                    "--supersample" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!("framesmith render: --supersample needs a value (a whole number from 1 to 4).");
                            std::process::exit(2);
                        }
                        if req.supersample.is_some() {
                            eprintln!("framesmith render: duplicate --supersample flag.");
                            std::process::exit(2);
                        }
                        req.supersample = Some(args[i].clone());
                    }
                    "--encoder" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!("framesmith render: --encoder needs a value (auto, software, or videotoolbox).");
                            std::process::exit(2);
                        }
                        if req.encoder.is_some() {
                            eprintln!("framesmith render: duplicate --encoder flag.");
                            std::process::exit(2);
                        }
                        req.encoder = Some(args[i].clone());
                    }
                    "--keyint" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!("framesmith render: --keyint needs a value (a positive whole number of frames, for example 60).");
                            std::process::exit(2);
                        }
                        if req.keyint.is_some() {
                            eprintln!("framesmith render: duplicate --keyint flag.");
                            std::process::exit(2);
                        }
                        req.keyint = Some(args[i].clone());
                    }
                    "--maxrate" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!("framesmith render: --maxrate needs a value (a peak bitrate, for example 8M).");
                            std::process::exit(2);
                        }
                        if req.maxrate.is_some() {
                            eprintln!("framesmith render: duplicate --maxrate flag.");
                            std::process::exit(2);
                        }
                        req.maxrate = Some(args[i].clone());
                    }
                    "--hdr" => {
                        i += 1;
                        if i >= args.len() {
                            eprintln!(
                                "framesmith render: --hdr needs a value (off, pq, hlg, or auto)."
                            );
                            std::process::exit(2);
                        }
                        if req.hdr.is_some() {
                            eprintln!("framesmith render: duplicate --hdr flag.");
                            std::process::exit(2);
                        }
                        req.hdr = Some(args[i].clone());
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
                    "--strict" => {
                        if req.strict {
                            eprintln!("framesmith render: duplicate --strict flag.");
                            std::process::exit(2);
                        }
                        req.strict = true;
                    }
                    "--strict-all" => {
                        if req.strict_all {
                            eprintln!("framesmith render: duplicate --strict-all flag.");
                            std::process::exit(2);
                        }
                        req.strict_all = true;
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
            if req.strict && req.strict_all {
                eprintln!("framesmith render: --strict and --strict-all are mutually exclusive.");
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
        "models" => ml_cmd::cmd_models(&args),
        "transcribe" => ml_cmd::cmd_transcribe(&args),
        "tts" => ml_cmd::cmd_tts(&args),
        "remove-background" => ml_cmd::cmd_remove_background(&args),
        "beats" => ml_cmd::cmd_beats(&args),
        "__ml-stub" => ml::stub::run_stub_verb(&args[1..]),
        "timeline" => {
            // Edit verbs live alongside the Sprint 6 listing: a verb
            // word in first position dispatches to the verb surface,
            // anything else keeps the listing parse byte-for-byte.
            if args.len() > 1 && commands::is_timeline_verb(&args[1]) {
                commands::cmd_timeline_verb(&args[1..])
            } else {
                let (folder, name, json) = parse_listing_args("timeline", &args[1..]);
                commands::cmd_timeline(&folder, name.as_deref(), json)
            }
        }
        "info" => {
            let (folder, name, json) = parse_listing_args("info", &args[1..]);
            commands::cmd_info(&folder, name.as_deref(), json)
        }
        "history" => {
            let mut req = commands::HistoryRequest::default();
            let mut verb: Option<String> = None;
            let mut positionals: Vec<String> = Vec::new();
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--json" => {
                        if req.json {
                            history_usage_error("duplicate --json flag");
                        }
                        req.json = true;
                    }
                    "--diff" => {
                        if req.diff {
                            history_usage_error("duplicate --diff flag");
                        }
                        req.diff = true;
                    }
                    "--exclusive" => {
                        if req.exclusive {
                            history_usage_error("duplicate --exclusive flag");
                        }
                        req.exclusive = true;
                    }
                    "--if-open" => {
                        if req.if_open {
                            history_usage_error("duplicate --if-open flag");
                        }
                        req.if_open = true;
                    }
                    "--unpin" => {
                        if req.unpin {
                            history_usage_error("duplicate --unpin flag");
                        }
                        req.unpin = true;
                    }
                    "--who" => {
                        i += 1;
                        if i >= args.len() {
                            history_usage_error("--who needs a value (an actor name)");
                        }
                        if req.who.is_some() {
                            history_usage_error("duplicate --who flag");
                        }
                        req.who = Some(args[i].clone());
                    }
                    other if other.starts_with('-') => {
                        history_usage_error(&format!("unknown flag {other:?}"));
                    }
                    other => {
                        if verb.is_none() {
                            verb = Some(other.to_string());
                        } else {
                            positionals.push(other.to_string());
                        }
                    }
                }
                i += 1;
            }
            let verb = match verb {
                Some(v) => v,
                None => history_usage_error(
                    "missing <verb> (list, show, undo, restore, peek, pin, begin, or end)",
                ),
            };
            match verb.as_str() {
                "list" | "show" | "undo" | "restore" | "peek" | "pin" | "begin" | "end" => {}
                other => history_usage_error(&format!(
                    "unknown verb {other:?}; expected list, show, undo, restore, peek, pin, begin, or end"
                )),
            }
            // Flag-to-verb shape: guards select writers, --diff names
            // changed files, --unpin releases a pin; anything else is
            // a usage error naming the mismatch.
            if req.diff && verb != "show" {
                history_usage_error("--diff needs the show verb (show --diff names changed files)");
            }
            if req.exclusive && verb != "begin" {
                history_usage_error("--exclusive needs the begin verb");
            }
            if req.if_open && verb != "begin" {
                history_usage_error("--if-open needs the begin verb");
            }
            if req.unpin && verb != "pin" {
                history_usage_error("--unpin needs the pin verb");
            }
            if req.who.is_some() && !matches!(verb.as_str(), "begin" | "end" | "undo") {
                history_usage_error("--who needs the begin, end, or undo verb");
            }
            if positionals.is_empty() {
                history_usage_error("missing <folder>");
            }
            if positionals.len() > 2 {
                history_usage_error(&format!(
                    "expected <folder> plus an optional [seq], found extra {:?}",
                    positionals[2]
                ));
            }
            req.verb = verb.clone();
            req.folder = PathBuf::from(&positionals[0]);
            if let Some(seq) = positionals.get(1) {
                req.seq = Some(seq.clone());
            }
            match verb.as_str() {
                "show" | "pin" | "restore" => {
                    if req.seq.is_none() {
                        history_usage_error(&format!("{verb} needs a turn number <seq>"));
                    }
                }
                "begin" | "end" => {
                    if req.who.is_none() {
                        history_usage_error(&format!("{verb} needs --who <name> (an actor name)"));
                    }
                    if req.seq.is_some() {
                        history_usage_error(&format!("{verb} takes no <seq>"));
                    }
                }
                "undo" => {
                    if req.who.is_some() && req.seq.is_some() {
                        history_usage_error("undo takes <seq> or --who <name>, not both; pass one");
                    }
                }
                _ => {
                    if req.seq.is_some() {
                        history_usage_error(&format!("{verb} takes no <seq>"));
                    }
                }
            }
            commands::cmd_history(&req)
        }
        "grade-bake" => {
            let mut composition: Option<String> = None;
            let mut output: Option<String> = None;
            let mut positionals: Vec<String> = Vec::new();
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--composition" => {
                        i += 1;
                        if i >= args.len() {
                            grade_bake_usage_error(
                                "--composition needs a value (a composition name)",
                            );
                        }
                        if composition.is_some() {
                            grade_bake_usage_error("duplicate --composition flag");
                        }
                        composition = Some(args[i].clone());
                    }
                    "-o" | "--output" => {
                        i += 1;
                        if i >= args.len() {
                            grade_bake_usage_error("-o needs a value (an output `.cube` path)");
                        }
                        if output.is_some() {
                            grade_bake_usage_error("duplicate -o flag");
                        }
                        output = Some(args[i].clone());
                    }
                    other if other.starts_with('-') => {
                        grade_bake_usage_error(&format!("unknown flag {other:?}"));
                    }
                    other => positionals.push(other.to_string()),
                }
                i += 1;
            }
            if positionals.len() != 1 {
                grade_bake_usage_error("expected exactly one <folder>");
            }
            match output {
                Some(o) => commands::cmd_grade_bake(
                    Path::new(&positionals[0]),
                    Path::new(&o),
                    composition.as_deref(),
                ),
                None => grade_bake_usage_error("missing -o <out.cube>"),
            }
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
        "snapshot" => {
            let mut req = commands::SnapshotRequest::default();
            let mut folder: Option<String> = None;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--json" => {
                        if req.json {
                            snapshot_usage_error("duplicate --json flag");
                        }
                        req.json = true;
                    }
                    "--composition" => {
                        i += 1;
                        if i >= args.len() {
                            snapshot_usage_error(
                                "--composition needs a value (a composition name)",
                            );
                        }
                        if req.composition.is_some() {
                            snapshot_usage_error("duplicate --composition flag");
                        }
                        req.composition = Some(args[i].clone());
                    }
                    "--backend" => {
                        i += 1;
                        if i >= args.len() {
                            snapshot_usage_error("--backend needs a value (cpu, gpu, or auto)");
                        }
                        if req.backend.is_some() {
                            snapshot_usage_error("duplicate --backend flag");
                        }
                        req.backend = Some(args[i].clone());
                    }
                    "--variables" => {
                        i += 1;
                        if i >= args.len() {
                            snapshot_usage_error(
                                "--variables needs a value (a JSON object of string keys to scalar values)",
                            );
                        }
                        if req.variables.is_some() {
                            snapshot_usage_error("duplicate --variables flag");
                        }
                        req.variables = Some(args[i].clone());
                    }
                    "--strict-variables" => {
                        if req.strict_variables {
                            snapshot_usage_error("duplicate --strict-variables flag");
                        }
                        req.strict_variables = true;
                    }
                    "--theme" => {
                        i += 1;
                        if i >= args.len() {
                            snapshot_usage_error("--theme needs a value (a declared theme name)");
                        }
                        if req.theme.is_some() {
                            snapshot_usage_error("duplicate --theme flag");
                        }
                        req.theme = Some(args[i].clone());
                    }
                    "--at" => {
                        i += 1;
                        if i >= args.len() {
                            snapshot_usage_error(
                                "--at needs a value (comma-separated seconds, for example 0,0.5,1)",
                            );
                        }
                        if req.at.is_some() {
                            snapshot_usage_error("duplicate --at flag");
                        }
                        req.at = Some(args[i].clone());
                    }
                    "--every" => {
                        i += 1;
                        if i >= args.len() {
                            snapshot_usage_error(
                                "--every needs a value (an interval in seconds, for example 0.5)",
                            );
                        }
                        if req.every.is_some() {
                            snapshot_usage_error("duplicate --every flag");
                        }
                        req.every = Some(args[i].clone());
                    }
                    "--from" => {
                        i += 1;
                        if i >= args.len() {
                            snapshot_usage_error(
                                "--from needs a value (seconds into the composition)",
                            );
                        }
                        if req.from.is_some() {
                            snapshot_usage_error("duplicate --from flag");
                        }
                        req.from = Some(args[i].clone());
                    }
                    "--to" => {
                        i += 1;
                        if i >= args.len() {
                            snapshot_usage_error(
                                "--to needs a value (seconds into the composition)",
                            );
                        }
                        if req.to.is_some() {
                            snapshot_usage_error("duplicate --to flag");
                        }
                        req.to = Some(args[i].clone());
                    }
                    "--out" => {
                        i += 1;
                        if i >= args.len() {
                            snapshot_usage_error("--out needs a value (an output directory)");
                        }
                        if req.out.is_some() {
                            snapshot_usage_error("duplicate --out flag");
                        }
                        req.out = Some(args[i].clone());
                    }
                    other if other.starts_with('-') => {
                        snapshot_usage_error(&format!("unknown flag {other:?}"));
                    }
                    other => {
                        if folder.is_some() {
                            snapshot_usage_error(&format!(
                                "expected exactly one <folder>, found extra {other:?}"
                            ));
                        }
                        folder = Some(other.to_string());
                    }
                }
                i += 1;
            }
            let folder = match folder {
                Some(f) => f,
                None => snapshot_usage_error("expected exactly one <folder>"),
            };
            if req.out.is_none() {
                snapshot_usage_error("missing --out <dir>");
            }
            let selectors = [
                req.at.is_some(),
                req.every.is_some(),
                req.from.is_some() || req.to.is_some(),
            ]
            .iter()
            .filter(|&&b| b)
            .count();
            if selectors == 0 {
                snapshot_usage_error("need one of --at, --every, or --from/--to");
            }
            if selectors > 1 {
                snapshot_usage_error("pass exactly one of --at, --every, or --from/--to");
            }
            if req.from.is_some() != req.to.is_some() {
                snapshot_usage_error("pass --from together with --to");
            }
            commands::cmd_snapshot_full(Path::new(&folder), &req)
        }
        "keyframes" => {
            let mut req = commands::KeyframesRequest {
                json: false,
                composition: None,
                backend: None,
                variables: None,
                strict_variables: false,
                theme: None,
                clip: None,
                property: None,
            };
            let mut folder: Option<String> = None;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--json" => {
                        if req.json {
                            keyframes_usage_error("duplicate --json flag");
                        }
                        req.json = true;
                    }
                    "--composition" => {
                        i += 1;
                        if i >= args.len() {
                            keyframes_usage_error(
                                "--composition needs a value (a composition name)",
                            );
                        }
                        if req.composition.is_some() {
                            keyframes_usage_error("duplicate --composition flag");
                        }
                        req.composition = Some(args[i].clone());
                    }
                    "--backend" => {
                        i += 1;
                        if i >= args.len() {
                            keyframes_usage_error("--backend needs a value (cpu, gpu, or auto)");
                        }
                        if req.backend.is_some() {
                            keyframes_usage_error("duplicate --backend flag");
                        }
                        req.backend = Some(args[i].clone());
                    }
                    "--variables" => {
                        i += 1;
                        if i >= args.len() {
                            keyframes_usage_error(
                                "--variables needs a value (a JSON object of string keys to scalar values)",
                            );
                        }
                        if req.variables.is_some() {
                            keyframes_usage_error("duplicate --variables flag");
                        }
                        req.variables = Some(args[i].clone());
                    }
                    "--strict-variables" => {
                        if req.strict_variables {
                            keyframes_usage_error("duplicate --strict-variables flag");
                        }
                        req.strict_variables = true;
                    }
                    "--theme" => {
                        i += 1;
                        if i >= args.len() {
                            keyframes_usage_error("--theme needs a value (a declared theme name)");
                        }
                        if req.theme.is_some() {
                            keyframes_usage_error("duplicate --theme flag");
                        }
                        req.theme = Some(args[i].clone());
                    }
                    "--clip" => {
                        i += 1;
                        if i >= args.len() {
                            keyframes_usage_error("--clip needs a value (a clip id or index)");
                        }
                        if req.clip.is_some() {
                            keyframes_usage_error("duplicate --clip flag");
                        }
                        req.clip = Some(args[i].clone());
                    }
                    "--property" => {
                        i += 1;
                        if i >= args.len() {
                            keyframes_usage_error(
                                "--property needs a value (a motion property, for example x)",
                            );
                        }
                        if req.property.is_some() {
                            keyframes_usage_error("duplicate --property flag");
                        }
                        req.property = Some(args[i].clone());
                    }
                    other if other.starts_with('-') => {
                        keyframes_usage_error(&format!("unknown flag {other:?}"));
                    }
                    other => {
                        if folder.is_some() {
                            keyframes_usage_error(&format!(
                                "expected exactly one <folder>, found extra {other:?}"
                            ));
                        }
                        folder = Some(other.to_string());
                    }
                }
                i += 1;
            }
            match folder {
                Some(f) => commands::cmd_keyframes_full(Path::new(&f), &req),
                None => keyframes_usage_error("expected exactly one <folder>"),
            }
        }
        "compare" => {
            let mut req = commands::CompareRequest {
                json: false,
                composition: None,
                at: None,
                backend: None,
                variables: None,
                strict_variables: false,
                theme: None,
                out: None,
            };
            let mut folders: Vec<String> = Vec::new();
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--json" => {
                        if req.json {
                            compare_usage_error("duplicate --json flag");
                        }
                        req.json = true;
                    }
                    "--composition" => {
                        i += 1;
                        if i >= args.len() {
                            compare_usage_error("--composition needs a value (a composition name)");
                        }
                        if req.composition.is_some() {
                            compare_usage_error("duplicate --composition flag");
                        }
                        req.composition = Some(args[i].clone());
                    }
                    "--at" => {
                        i += 1;
                        if i >= args.len() {
                            compare_usage_error(
                                "--at needs a value (seconds into the composition)",
                            );
                        }
                        if req.at.is_some() {
                            compare_usage_error("duplicate --at flag");
                        }
                        req.at = Some(args[i].clone());
                    }
                    "--backend" => {
                        i += 1;
                        if i >= args.len() {
                            compare_usage_error("--backend needs a value (cpu, gpu, or auto)");
                        }
                        if req.backend.is_some() {
                            compare_usage_error("duplicate --backend flag");
                        }
                        req.backend = Some(args[i].clone());
                    }
                    "--variables" => {
                        i += 1;
                        if i >= args.len() {
                            compare_usage_error(
                                "--variables needs a value (a JSON object of string keys to scalar values)",
                            );
                        }
                        if req.variables.is_some() {
                            compare_usage_error("duplicate --variables flag");
                        }
                        req.variables = Some(args[i].clone());
                    }
                    "--strict-variables" => {
                        if req.strict_variables {
                            compare_usage_error("duplicate --strict-variables flag");
                        }
                        req.strict_variables = true;
                    }
                    "--theme" => {
                        i += 1;
                        if i >= args.len() {
                            compare_usage_error("--theme needs a value (a declared theme name)");
                        }
                        if req.theme.is_some() {
                            compare_usage_error("duplicate --theme flag");
                        }
                        req.theme = Some(args[i].clone());
                    }
                    "-o" | "--output" => {
                        i += 1;
                        if i >= args.len() {
                            compare_usage_error("-o needs a value (an output `.png` path)");
                        }
                        if req.out.is_some() {
                            compare_usage_error("duplicate -o flag");
                        }
                        req.out = Some(args[i].clone());
                    }
                    other if other.starts_with('-') => {
                        compare_usage_error(&format!("unknown flag {other:?}"));
                    }
                    other => folders.push(other.to_string()),
                }
                i += 1;
            }
            if folders.len() != 2 {
                compare_usage_error("expected exactly two project folders <folderA> <folderB>");
            }
            commands::cmd_compare_full(Path::new(&folders[0]), Path::new(&folders[1]), &req)
        }
        "grade-compare" => {
            let mut req = commands::GradeCompareRequest {
                json: false,
                composition: None,
                at: None,
                backend: None,
                variables: None,
                strict_variables: false,
                theme: None,
                out: None,
            };
            let mut folder: Option<String> = None;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--json" => {
                        if req.json {
                            grade_compare_usage_error("duplicate --json flag");
                        }
                        req.json = true;
                    }
                    "--composition" => {
                        i += 1;
                        if i >= args.len() {
                            grade_compare_usage_error(
                                "--composition needs a value (a composition name)",
                            );
                        }
                        if req.composition.is_some() {
                            grade_compare_usage_error("duplicate --composition flag");
                        }
                        req.composition = Some(args[i].clone());
                    }
                    "--at" => {
                        i += 1;
                        if i >= args.len() {
                            grade_compare_usage_error(
                                "--at needs a value (seconds into the composition)",
                            );
                        }
                        if req.at.is_some() {
                            grade_compare_usage_error("duplicate --at flag");
                        }
                        req.at = Some(args[i].clone());
                    }
                    "--backend" => {
                        i += 1;
                        if i >= args.len() {
                            grade_compare_usage_error(
                                "--backend needs a value (cpu, gpu, or auto)",
                            );
                        }
                        if req.backend.is_some() {
                            grade_compare_usage_error("duplicate --backend flag");
                        }
                        req.backend = Some(args[i].clone());
                    }
                    "--variables" => {
                        i += 1;
                        if i >= args.len() {
                            grade_compare_usage_error(
                                "--variables needs a value (a JSON object of string keys to scalar values)",
                            );
                        }
                        if req.variables.is_some() {
                            grade_compare_usage_error("duplicate --variables flag");
                        }
                        req.variables = Some(args[i].clone());
                    }
                    "--strict-variables" => {
                        if req.strict_variables {
                            grade_compare_usage_error("duplicate --strict-variables flag");
                        }
                        req.strict_variables = true;
                    }
                    "--theme" => {
                        i += 1;
                        if i >= args.len() {
                            grade_compare_usage_error(
                                "--theme needs a value (a declared theme name)",
                            );
                        }
                        if req.theme.is_some() {
                            grade_compare_usage_error("duplicate --theme flag");
                        }
                        req.theme = Some(args[i].clone());
                    }
                    "-o" | "--output" => {
                        i += 1;
                        if i >= args.len() {
                            grade_compare_usage_error("-o needs a value (an output `.png` path)");
                        }
                        if req.out.is_some() {
                            grade_compare_usage_error("duplicate -o flag");
                        }
                        req.out = Some(args[i].clone());
                    }
                    other if other.starts_with('-') => {
                        grade_compare_usage_error(&format!("unknown flag {other:?}"));
                    }
                    other => {
                        if folder.is_some() {
                            grade_compare_usage_error(&format!(
                                "expected exactly one <folder>, found extra {other:?}"
                            ));
                        }
                        folder = Some(other.to_string());
                    }
                }
                i += 1;
            }
            match folder {
                Some(f) => commands::cmd_grade_compare_full(Path::new(&f), &req),
                None => grade_compare_usage_error("expected exactly one <folder>"),
            }
        }
        "preview" => {
            let mut req = commands::PreviewStartRequest::default();
            let mut folder: Option<String> = None;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--port" => {
                        i += 1;
                        if i >= args.len() {
                            preview_usage_error("--port needs a value (a port number)");
                        }
                        if req.port.is_some() {
                            preview_usage_error("duplicate --port flag");
                        }
                        req.port = Some(args[i].clone());
                    }
                    "--composition" => {
                        i += 1;
                        if i >= args.len() {
                            preview_usage_error("--composition needs a value (a composition name)");
                        }
                        if req.composition.is_some() {
                            preview_usage_error("duplicate --composition flag");
                        }
                        req.composition = Some(args[i].clone());
                    }
                    "--foreground" => {
                        if req.foreground {
                            preview_usage_error("duplicate --foreground flag");
                        }
                        req.foreground = true;
                    }
                    other if other.starts_with('-') => {
                        preview_usage_error(&format!("unknown flag {other:?}"));
                    }
                    other => {
                        if folder.is_some() {
                            preview_usage_error(&format!(
                                "expected exactly one <folder>, found extra {other:?}"
                            ));
                        }
                        folder = Some(other.to_string());
                    }
                }
                i += 1;
            }
            match folder {
                Some(f) => commands::cmd_preview(Path::new(&f), &req),
                None => preview_usage_error("missing <folder>"),
            }
        }
        "play" => {
            let mut req = commands::PreviewStartRequest::default();
            let mut folder: Option<String> = None;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--port" => {
                        i += 1;
                        if i >= args.len() {
                            play_usage_error("--port needs a value (a port number)");
                        }
                        if req.port.is_some() {
                            play_usage_error("duplicate --port flag");
                        }
                        req.port = Some(args[i].clone());
                    }
                    "--composition" => {
                        i += 1;
                        if i >= args.len() {
                            play_usage_error("--composition needs a value (a composition name)");
                        }
                        if req.composition.is_some() {
                            play_usage_error("duplicate --composition flag");
                        }
                        req.composition = Some(args[i].clone());
                    }
                    "--foreground" => {
                        if req.foreground {
                            play_usage_error("duplicate --foreground flag");
                        }
                        req.foreground = true;
                    }
                    other if other.starts_with('-') => {
                        play_usage_error(&format!("unknown flag {other:?}"));
                    }
                    other => {
                        if folder.is_some() {
                            play_usage_error(&format!(
                                "expected exactly one <folder>, found extra {other:?}"
                            ));
                        }
                        folder = Some(other.to_string());
                    }
                }
                i += 1;
            }
            match folder {
                Some(f) => commands::cmd_play(Path::new(&f), &req),
                None => play_usage_error("missing <folder>"),
            }
        }
        "preview-status" => {
            let mut folder: Option<String> = None;
            for arg in &args[1..] {
                if arg.starts_with('-') {
                    preview_status_usage_error(&format!("unknown flag {arg:?}"));
                } else if folder.is_some() {
                    preview_status_usage_error(&format!(
                        "expected at most one <folder>, found extra {arg:?}"
                    ));
                } else {
                    folder = Some(arg.clone());
                }
            }
            commands::cmd_preview_status(folder.as_deref().map(Path::new))
        }
        "preview-stop" => {
            let mut folder: Option<String> = None;
            for arg in &args[1..] {
                if arg.starts_with('-') {
                    preview_stop_usage_error(&format!("unknown flag {arg:?}"));
                } else if folder.is_some() {
                    preview_stop_usage_error(&format!(
                        "expected exactly one <folder>, found extra {arg:?}"
                    ));
                } else {
                    folder = Some(arg.clone());
                }
            }
            match folder {
                Some(f) => commands::cmd_preview_stop(Path::new(&f)),
                None => preview_stop_usage_error("missing <folder>"),
            }
        }
        "preview-list" => {
            if let Some(arg) = args.get(1) {
                preview_list_usage_error(&format!("expected no arguments, found extra {arg:?}"));
            }
            commands::cmd_preview_list()
        }
        "preview-stop-all" => {
            if let Some(arg) = args.get(1) {
                preview_stop_all_usage_error(&format!(
                    "expected no arguments, found extra {arg:?}"
                ));
            }
            commands::cmd_preview_stop_all()
        }
        "preview-serve" => {
            let mut port: Option<String> = None;
            let mut registry: Option<String> = None;
            let mut composition: Option<String> = None;
            let mut folder: Option<String> = None;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--port" => {
                        i += 1;
                        if i >= args.len() {
                            preview_serve_usage_error("--port needs a value (a port number)");
                        }
                        if port.is_some() {
                            preview_serve_usage_error("duplicate --port flag");
                        }
                        port = Some(args[i].clone());
                    }
                    "--registry" => {
                        i += 1;
                        if i >= args.len() {
                            preview_serve_usage_error("--registry needs a value (a registry file)");
                        }
                        if registry.is_some() {
                            preview_serve_usage_error("duplicate --registry flag");
                        }
                        registry = Some(args[i].clone());
                    }
                    "--composition" => {
                        i += 1;
                        if i >= args.len() {
                            preview_serve_usage_error(
                                "--composition needs a value (a composition name)",
                            );
                        }
                        if composition.is_some() {
                            preview_serve_usage_error("duplicate --composition flag");
                        }
                        composition = Some(args[i].clone());
                    }
                    other if other.starts_with('-') => {
                        preview_serve_usage_error(&format!("unknown flag {other:?}"));
                    }
                    other => {
                        if folder.is_some() {
                            preview_serve_usage_error(&format!(
                                "expected exactly one <folder>, found extra {other:?}"
                            ));
                        }
                        folder = Some(other.to_string());
                    }
                }
                i += 1;
            }
            let port: u16 = match port {
                Some(p) => p.parse().unwrap_or_else(|_| {
                    preview_serve_usage_error(&format!("--port needs a port number, found {p:?}"))
                }),
                None => preview_serve_usage_error("missing --port <n>"),
            };
            let registry = match registry {
                Some(r) => PathBuf::from(r),
                None => preview_serve_usage_error("missing --registry <file>"),
            };
            match folder {
                Some(f) => commands::cmd_preview_serve(
                    Path::new(&f),
                    port,
                    &registry,
                    composition.as_deref(),
                ),
                None => preview_serve_usage_error("missing <folder>"),
            }
        }
        "doctor" => {
            let mut json = false;
            let mut folder: Option<String> = None;
            for arg in &args[1..] {
                if *arg == "--json" {
                    if json {
                        doctor_usage_error("duplicate --json flag");
                    }
                    json = true;
                } else if arg.starts_with('-') {
                    doctor_usage_error(&format!("unknown flag {arg:?}"));
                } else if folder.is_some() {
                    doctor_usage_error(&format!(
                        "expected at most one <folder>, found extra {arg:?}"
                    ));
                } else {
                    folder = Some(arg.clone());
                }
            }
            commands::cmd_doctor(folder.as_deref().map(Path::new), json)
        }
        "benchmark" => {
            let mut req = commands::BenchmarkRequest {
                json: false,
                out: None,
            };
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--json" => {
                        if req.json {
                            benchmark_usage_error("duplicate --json flag");
                        }
                        req.json = true;
                    }
                    "--out" => {
                        i += 1;
                        if i >= args.len() {
                            benchmark_usage_error("--out needs a value (an output directory)");
                        }
                        if req.out.is_some() {
                            benchmark_usage_error("duplicate --out flag");
                        }
                        req.out = Some(args[i].clone());
                    }
                    other if other.starts_with('-') => {
                        benchmark_usage_error(&format!("unknown flag {other:?}"));
                    }
                    other => {
                        benchmark_usage_error(&format!(
                            "expected no <folder>, found extra {other:?}"
                        ));
                    }
                }
                i += 1;
            }
            commands::cmd_benchmark(&req)
        }
        "-h" | "--help" | "help" => {
            println!(
                "framesmith — turn a small KDL composition folder into a playable MP4.\n\n\
                 commands:\n\
                 \x20 init <name> [--dir <parent>]  create a minimal valid project\n\
                 \x20 check [--json] [--backend cpu|gpu|auto] [--variables JSON] [--strict-variables] [--theme NAME] [--strict|--strict-all] <folder>  validate without rendering (exit 0 valid, 1 invalid or gated, 2 unsupported)\n\
                 \x20 lint [--json] [--backend cpu|gpu|auto] [--variables JSON] [--strict-variables] [--theme NAME] [--strict|--strict-all] <folder>  report named-rule findings with fixes (same exits as check)\n\
                 \x20 errors [--json] [CODE]        explain every error code\n\
                 \x20 compositions [--json] <folder>  list every composition name plus the root\n\
                 \x20 timeline [--json] [--composition <name>] <folder> [<name>]  list every clip with id, track, label, hidden state, and window\n\
                 \x20 history (list|show|undo|restore|peek|pin|begin|end) [--json] [--diff] [--who <name>] [--exclusive] [--if-open] [--unpin] <folder> [<seq>]  share one guarded history across writers\n\
                 \x20 skills list [--json]  list every shipped skill with its purpose\n\
                 \x20 skills install --host <host> [--dest <dir>]  install the core skill set\n\
                 \x20 skills check --host <host> [--dest <dir>] [--json]  verify the install against the hashed manifest\n\
                 \x20 skills update (--all | --core | --skill <name>) --host <host> [--dest <dir>] [--yes]  restore drifted or missing skills\n\
                 \x20 skills validate [--host <host>] [--dest <dir>] [--json]  every skill document passes its own schema\n\
                 \x20 route --request <text> [<folder>]  interview, pitch five concepts, and name the workflow (install on demand with --install)\n\
                 \x20 storyboard board [--at <t,..> | --every <secs>] [--out <dir>] <folder>  present one board of locked stills in one page (film unchanged, gate armed)\n\
                 \x20 storyboard approve [--actor <name>] [--json] <folder>  record the maker approval (gate lands, motion and render proceed)\n\
                 \x20 storyboard status [--json] <folder>  read the gate back (missing reads as not approved)\n\
                 \x20 info [--json] [--composition <name>] <folder> [<name>]  summarize canvas, duration, and clip counts\n\
                 \x20 waveform [--json] [--composition <name>] [--buckets N] <folder> [<name>]  report per-bucket mix peaks plus the master peak\n\
                 \x20 render <folder> [-o <file>] [--composition <name>] [--start S] [--end E] [--backend cpu|gpu|auto] [--preset draft|standard|high] [--crf N] [--video-bitrate B] [--format ..] [--resolution ..] [--supersample N] [--variables JSON] [--strict-variables] [--theme NAME] [--strict|--strict-all] [--json]  validate, bootstrap FFmpeg, refuse or encode\n\
                 \x20 render <folder> --batch rows.json [--name-template TPL] [--jobs N] [--fail-fast]  one MP4 per row plus a per-row summary\n\
                 \x20 grade-bake [--composition <name>] -o <out.cube> <folder>  bake the composition grade to an equivalent `.cube`\n\
                 \x20 snapshot (--at <t,..> | --every <secs> | --from <s> --to <e>) --out <dir> [--composition <name>] <folder>  write snap-<time>s.png frames without an MP4\n\
                 \x20 keyframes --clip <id> --property <name> <folder>  list computed per-frame motion values\n\
                 \x20 compare -o <out.png> <folderA> <folderB>  join two renders side by side with an SSIM score\n\
                 \x20 grade-compare -o <out.png> <folder>  split one composition into ungraded and graded halves\n\
                 \x20 preview [--port <n>] [--composition <name>] [--foreground] <folder>  serve a live review page in the background and print its URL\n\
                 \x20 preview-status [<folder>]  name the project and port, or none\n\
                 \x20 preview-stop <folder>  end exactly that server\n\
                 \x20 preview-list  show every live server\n\
                 \x20 preview-stop-all  end all servers and report the count\n\
                 \x20 doctor [<folder>]  report environment and project health facts
                 \x20 models list|install <name> [--yes]  list known models or install one with consent\n\
                 \x20 transcribe <input>  transcribe speech to word-timed transcript JSON (optionally --to srt|vtt)\n\
                 \x20 tts <text...>  synthesize voice audio to WAV\n\
                 \x20 remove-background <input.png>  emit subject alpha plus a background plate\n\
                 \x20 beats <input.wav>  emit BPM plus a beat file (pure-Rust signal analysis)\n\\n\
                 \x20 benchmark [--out <dir>]  time bundled references (machine-specific)"
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
